//! 复用现有环境管理器；检测不会创建环境或改变系统默认版本。
use crate::{
    db::models::{EnvVar, RuntimeBinding},
    error::{AppError, AppResult},
};
use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    collections::HashSet,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub path: String,
    pub version: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub path: String,
    pub manager: String,
    pub manager_path: String,
    pub version: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentReport {
    pub tools: Vec<ToolInfo>,
    pub environments: Vec<EnvironmentInfo>,
    pub warnings: Vec<String>,
    pub scanned_at: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentAction {
    pub tool: String,
    pub action: String,
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub project_dir: String,
    #[serde(default)]
    pub runtime: RuntimeBinding,
    #[serde(default)]
    pub confirmed: bool,
}

pub fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}
pub fn invocation(program: &str, args: &[String]) -> String {
    format!(
        "& {} {}",
        ps_quote(program),
        args.iter()
            .map(|s| ps_quote(s))
            .collect::<Vec<_>>()
            .join(" ")
    )
}
fn command(program: &Path, args: &[&str]) -> Command {
    let ext = program.extension().and_then(|s| s.to_str()).unwrap_or("");
    let mut cmd = if ["cmd", "bat"].contains(&ext.to_ascii_lowercase().as_str()) {
        let mut c = Command::new("powershell.exe");
        c.args([
            "-NoLogo",
            "-NoProfile",
            "-Command",
            &invocation(
                &program.to_string_lossy(),
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            ),
        ]);
        c
    } else {
        let mut c = Command::new(program);
        c.args(args);
        c
    };
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}
/// 同时排空输出，避免管理器输出占满管道；超时后终止检测进程。
fn probe(program: &Path, args: &[&str], timeout: u64) -> AppResult<String> {
    let mut child = command(program, args)
        .spawn()
        .map_err(|e| AppError::exec(e.to_string()))?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let read = |mut p: Box<dyn Read + Send>| {
        let mut data = Vec::new();
        let _ = p.by_ref().take(2_000_000).read_to_end(&mut data);
        String::from_utf8_lossy(&data).into_owned()
    };
    let a = std::thread::spawn(move || read(Box::new(out)));
    let b = std::thread::spawn(move || read(Box::new(err)));
    let started = Instant::now();
    let status = loop {
        if let Some(s) = child
            .try_wait()
            .map_err(|e| AppError::exec(e.to_string()))?
        {
            break s;
        }
        if started.elapsed() > Duration::from_secs(timeout) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::exec("环境管理器检测超时"));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stdout = a.join().unwrap_or_default();
    let stderr = b.join().unwrap_or_default();
    if status.success() {
        Ok(if stdout.trim().is_empty() {
            stderr
        } else {
            stdout
        }
        .trim()
        .to_string())
    } else {
        Err(AppError::exec(format!(
            "工具检测失败：{}",
            stderr.chars().take(300).collect::<String>()
        )))
    }
}
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let mut directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if name == "conda" {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            let h = PathBuf::from(home);
            for dir in [
                "miniconda3/Scripts",
                "anaconda3/Scripts",
                "miniforge3/Scripts",
            ] {
                directories.push(h.join(dir));
            }
        }
    }
    for directory in directories {
        for ext in ["exe", "cmd", "bat"] {
            let path = directory.join(format!("{name}.{ext}"));
            if path.is_file() {
                if name == "conda" && ext != "exe" {
                    let mut ancestor = path.parent();
                    for _ in 0..4 {
                        if let Some(p) = ancestor {
                            let exe = p.join("Scripts/conda.exe");
                            if exe.is_file() {
                                return Some(exe);
                            }
                            ancestor = p.parent();
                        }
                    }
                }
                return Some(path);
            }
        }
    }
    None
}
fn profile(
    kind: &str,
    name: String,
    path: PathBuf,
    manager: &str,
    manager_path: &str,
    version: String,
) -> EnvironmentInfo {
    EnvironmentInfo {
        id: format!("{kind}:{}", path.to_string_lossy()),
        name,
        kind: kind.into(),
        path: path.to_string_lossy().into_owned(),
        manager: manager.into(),
        manager_path: manager_path.into(),
        version,
    }
}
pub fn discover(project: &str) -> EnvironmentReport {
    let mut report = EnvironmentReport {
        tools: vec![],
        environments: vec![],
        warnings: vec![],
        scanned_at: crate::db::models::now_ms(),
    };
    // 独立工具并发检测，每项有超时，避免长时间阻塞 IPC。
    let handles: Vec<_> = [
        "python", "py", "conda", "uv", "node", "npm", "pnpm", "yarn", "fnm", "nvm", "volta",
    ]
    .into_iter()
    .filter_map(|name| {
        find_tool(name).map(|path| {
            std::thread::spawn(move || {
                let version = probe(
                    &path,
                    if name == "nvm" {
                        &["version"]
                    } else {
                        &["--version"]
                    },
                    5,
                )
                .unwrap_or_else(|e| e.to_string());
                ToolInfo {
                    name: name.into(),
                    path: path.to_string_lossy().into_owned(),
                    version: version.chars().take(160).collect(),
                }
            })
        })
    })
    .collect();
    for h in handles {
        if let Ok(t) = h.join() {
            report.tools.push(t);
        }
    }
    for t in &report.tools {
        if ["python", "node"].contains(&t.name.as_str()) {
            report.environments.push(profile(
                &t.name,
                format!("系统 {}", t.name),
                PathBuf::from(&t.path),
                "PATH",
                "",
                t.version.clone(),
            ));
        }
    }
    if let Some(conda) = report.tools.iter().find(|t| t.name == "conda") {
        match probe(Path::new(&conda.path), &["env", "list", "--json"], 12).and_then(|s| {
            serde_json::from_str::<serde_json::Value>(&s)
                .map_err(|e| AppError::exec(format!("Conda 返回的数据无效：{e}")))
        }) {
            Ok(value) => {
                if let Some(envs) = value["envs"].as_array() {
                    for value in envs.iter().take(128) {
                        if let Some(p) = value.as_str() {
                            let path = PathBuf::from(p);
                            if path.is_dir() {
                                let name = path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
                                report.environments.push(profile(
                                    "conda",
                                    name,
                                    path,
                                    "conda",
                                    &conda.path,
                                    String::new(),
                                ));
                            }
                        }
                    }
                }
            }
            Err(e) => report.warnings.push(e.to_string()),
        }
    }
    if let Some(py) = report.tools.iter().find(|t| t.name == "py") {
        if let Ok(text) = probe(Path::new(&py.path), &["-0p"], 5) {
            for line in text.lines() {
                if let Some(start) = line.find(":\\") {
                    let path = PathBuf::from(line[start.saturating_sub(1)..].trim());
                    if path.is_file() {
                        report.environments.push(profile(
                            "python",
                            format!(
                                "Python {}",
                                path.parent()
                                    .and_then(Path::file_name)
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            ),
                            path,
                            "py",
                            "",
                            String::new(),
                        ));
                    }
                }
            }
        }
    }
    if !project.trim().is_empty() {
        let root = PathBuf::from(project);
        if root.is_dir() {
            for suffix in ["", ".venv", "venv", "env"] {
                let prefix = root.join(suffix);
                if prefix.join("pyvenv.cfg").is_file()
                    && prefix.join("Scripts/python.exe").is_file()
                {
                    report.environments.push(profile(
                        "venv",
                        format!(
                            "项目 {}",
                            if suffix.is_empty() {
                                "虚拟环境"
                            } else {
                                suffix
                            }
                        ),
                        prefix,
                        "venv",
                        "",
                        String::new(),
                    ));
                }
            }
        } else {
            report.warnings.push("指定项目目录不存在".into());
        }
    }
    let mut roots: Vec<(PathBuf, &str)> = vec![];
    if let Some(dir) = std::env::var_os("NVM_HOME") {
        roots.push((dir.into(), "nvm"));
    }
    if let Some(dir) = std::env::var_os("FNM_DIR") {
        roots.push((PathBuf::from(dir).join("node-versions"), "fnm"));
    }
    if let Some(dir) = std::env::var_os("APPDATA") {
        roots.push((PathBuf::from(dir).join("fnm/node-versions"), "fnm"));
    }
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        roots.push((PathBuf::from(&dir).join("fnm/node-versions"), "fnm"));
        roots.push((PathBuf::from(dir).join("Volta/tools/image/node"), "volta"));
    }
    if let Some(dir) = std::env::var_os("VOLTA_HOME") {
        roots.push((PathBuf::from(dir).join("tools/image/node"), "volta"));
    }
    for (root, manager) in roots {
        if let Ok(entries) = root.read_dir() {
            for entry in entries.flatten().take(128) {
                let path = entry.path().join(if manager == "fnm" {
                    "installation/node.exe"
                } else {
                    "node.exe"
                });
                if path.is_file() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    report.environments.push(profile(
                        "node",
                        name,
                        path,
                        manager,
                        "",
                        String::new(),
                    ));
                }
            }
        }
    }
    let mut seen = HashSet::new();
    report
        .environments
        .retain(|e| seen.insert((e.kind.clone(), e.path.to_ascii_lowercase())));
    report
}
pub fn validate_binding(binding: &RuntimeBinding) -> AppResult<()> {
    let path = Path::new(&binding.path);
    match binding.kind.as_str() {
        "" | "system" => Ok(()),
        "venv" if path.is_absolute() && path.join("Scripts/python.exe").is_file() => Ok(()),
        "python" | "node" if path.is_absolute() && path.is_file() => Ok(()),
        "conda"
            if path.is_absolute()
                && path.is_dir()
                && Path::new(&binding.manager_path).is_file()
                && Path::new(&binding.manager_path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("conda.exe") =>
        {
            Ok(())
        }
        _ => Err(AppError::validation(
            "所选运行环境不存在或已移动，请刷新环境并重新选择",
        )),
    }
}
pub fn apply_binding(
    cmd: &mut CommandBuilder,
    binding: &RuntimeBinding,
    env: &[EnvVar],
) -> AppResult<()> {
    let prefix = match binding.kind.as_str() {
        "venv" => {
            cmd.env("VIRTUAL_ENV", &binding.path);
            cmd.env_remove("PYTHONHOME");
            Path::new(&binding.path).join("Scripts")
        }
        "node" | "python" => Path::new(&binding.path)
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf(),
        _ => return Ok(()),
    };
    let inherited = env
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case("PATH"))
        .map(|e| e.value.clone())
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
    // 上层环境变量循环必须先执行，再应用绑定，保证环境选择拥有 PATH 优先权。
    cmd.env("PATH", format!("{};{inherited}", prefix.to_string_lossy()));
    Ok(())
}
pub fn action_command(req: &EnvironmentAction) -> AppResult<(String, Vec<String>)> {
    let tool = find_tool(&req.tool)
        .ok_or_else(|| AppError::validation(format!("未检测到 {}，请先安装并刷新", req.tool)))?;
    let mut args: Vec<String> = vec![];
    let valid_token = |s: &str| {
        !s.is_empty()
            && s.len() <= 200
            && !s.starts_with('-')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "@/._-=<>!~^[],+".contains(c))
    };
    if req.action == "install-package" && !valid_token(&req.package) {
        return Err(AppError::validation(
            "请输入单个包名，可附带版本号，不接受脚本或多个参数",
        ));
    }
    let valid_name = |s: &str| {
        !s.is_empty()
            && s.len() <= 60
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    };
    let valid_version = |s: &str| {
        !s.is_empty() && s.len() <= 24 && s.chars().all(|c| c.is_ascii_digit() || c == '.')
    };
    match (req.tool.as_str(), req.action.as_str()) {
        ("conda", "list-packages") | ("conda", "install-package") => {
            if req.runtime.kind != "conda" {
                return Err(AppError::validation("请先选择 Conda 环境"));
            }
            validate_binding(&req.runtime)?;
            args.extend([
                if req.action == "list-packages" {
                    "list"
                } else {
                    "install"
                }
                .into(),
                "-p".into(),
                req.runtime.path.clone(),
            ]);
            if req.action == "install-package" {
                args.extend(["-y".into(), req.package.clone()]);
            }
        }
        ("conda", "create-environment") => {
            if !valid_name(&req.name) || !valid_version(&req.version) {
                return Err(AppError::validation(
                    "环境名只接受字母数字、点、横线、下划线；Python 版本例如 3.12",
                ));
            }
            args.extend([
                "create".into(),
                "-y".into(),
                "-n".into(),
                req.name.clone(),
                format!("python={}", req.version),
            ]);
        }
        ("python", "create-environment") | ("uv", "create-environment") => {
            let cwd = crate::pty::resolve_working_directory(&req.project_dir, "")?;
            let prefix = Path::new(&cwd).join(".venv");
            if prefix.exists() {
                return Err(AppError::validation(
                    "项目中已有 .venv，请选择另一个目录，避免覆盖",
                ));
            }
            if req.tool == "python" {
                args.extend(["-m".into(), "venv".into()]);
            } else {
                args.push("venv".into());
            }
            args.push(prefix.to_string_lossy().into_owned());
        }
        ("python", "list-packages") | ("python", "install-package") => {
            args.extend([
                "-m".into(),
                "pip".into(),
                if req.action == "list-packages" {
                    "list"
                } else {
                    "install"
                }
                .into(),
            ]);
            if req.action == "install-package" {
                args.push(req.package.clone());
            }
        }
        ("npm", "list-packages") => args.extend(["list".into(), "--depth=0".into()]),
        ("pnpm", "list-packages") => args.extend(["list".into(), "--depth=0".into()]),
        ("yarn", "list-packages") => {
            return Err(AppError::validation(
                "不同 Yarn 版本的列表命令不同，请通过环境终端运行 yarn info / yarn list",
            ))
        }
        ("npm", "install-package") => {
            args.extend(["install".into(), "--".into(), req.package.clone()])
        }
        ("pnpm" | "yarn", "install-package") => {
            args.extend(["add".into(), "--".into(), req.package.clone()])
        }
        ("fnm" | "nvm", "install-version") => {
            if !valid_version(&req.version) {
                return Err(AppError::validation("Node 版本例如 22 或 22.15.0"));
            }
            args.extend(["install".into(), req.version.clone()]);
        }
        ("volta", "install-version") => {
            if !valid_version(&req.version) {
                return Err(AppError::validation("请输入有效的 Node 版本号"));
            }
            args.extend(["install".into(), format!("node@{}", req.version)]);
        }
        _ => return Err(AppError::validation("该管理器不支持此操作")),
    }
    let mut program = tool.to_string_lossy().into_owned();
    if req.tool == "python" && req.action != "create-environment" {
        match req.runtime.kind.as_str() {
            "venv" => {
                validate_binding(&req.runtime)?;
                program = Path::new(&req.runtime.path)
                    .join("Scripts/python.exe")
                    .to_string_lossy()
                    .into_owned();
            }
            "python" => {
                validate_binding(&req.runtime)?;
                program = req.runtime.path.clone();
            }
            "conda" => {
                validate_binding(&req.runtime)?;
                let python = Path::new(&req.runtime.path).join("python.exe");
                if !python.is_file() {
                    return Err(AppError::validation("该 Conda 环境没有安装 Python"));
                }
                program = python.to_string_lossy().into_owned();
            }
            _ => {}
        }
    }
    Ok((program, args))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn powershell_argument_quoting() {
        assert_eq!(
            invocation("C:/space folder/tool.cmd", &["a'b".into(), "x & y".into()]),
            "& 'C:/space folder/tool.cmd' 'a''b' 'x & y'"
        );
    }
    #[test]
    fn stale_runtime_is_rejected() {
        assert!(validate_binding(&RuntimeBinding {
            kind: "venv".into(),
            path: "missing".into(),
            manager_path: String::new()
        })
        .is_err());
        assert!(validate_binding(&RuntimeBinding::default()).is_ok());
    }
}

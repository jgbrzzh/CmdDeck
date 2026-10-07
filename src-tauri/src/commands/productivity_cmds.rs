use crate::{
    backups,
    db::models::*,
    error::{AppError, AppResult},
    monitor,
    productivity::{self, Layout, Productivity},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn get_productivity(state: State<'_, AppState>) -> AppResult<Productivity> {
    productivity::load(&state.db)
}
#[tauri::command]
pub fn save_productivity(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Productivity,
) -> AppResult<Productivity> {
    productivity::validate(&config)?;
    backups::snapshot(&state)?;
    productivity::save(&state.db, &config)?;
    state.emit(&app, "cmddeck://productivity-changed", config.clone());
    Ok(config)
}
#[tauri::command]
pub fn save_terminal_layout(
    state: State<'_, AppState>,
    workspace_id: String,
    layout: Layout,
) -> AppResult<()> {
    let mut p = productivity::load(&state.db)?;
    if !workspace_id.is_empty() && !p.workspaces.iter().any(|w| w.id == workspace_id) {
        return Err(AppError::validation("项目不存在"));
    }
    p.layouts.insert(workspace_id, layout);
    productivity::save(&state.db, &p)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    pub level: String,
    pub message: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    pub command: String,
    pub directory: String,
    pub checks: Vec<CheckItem>,
    pub can_run: bool,
}
fn check(items: &mut Vec<CheckItem>, level: &str, message: impl Into<String>) {
    items.push(CheckItem {
        level: level.into(),
        message: message.into(),
    });
}
#[tauri::command]
pub async fn preflight_preset(
    app: AppHandle,
    preset: Preset,
    args: HashMap<String, String>,
    workspace_id: Option<String>,
) -> AppResult<Preflight> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let mut items = vec![];
        let mut opt = match super::terminal_cmds::options(&preset, &args, "manual") {
            Ok(opt) => opt,
            Err(e) => {
                return Ok(Preflight {
                    command: String::new(),
                    directory: String::new(),
                    checks: vec![CheckItem {
                        level: "error".into(),
                        message: e.to_string(),
                    }],
                    can_run: false,
                })
            }
        };
        if preset.kind == "shell" && preset.program.is_empty() && preset.args.is_empty() {
            opt.kind = if preset.name.to_uppercase().contains("CMD") {
                "cmd".into()
            } else {
                st.settings().default_shell
            };
            opt.interactive = true;
            opt.use_shell = false;
        }
        let workspace = productivity::prepare(&st, &mut opt, workspace_id.as_deref())?;
        let dir = crate::pty::resolve_working_directory(
            &opt.working_dir,
            &st.settings().default_working_dir,
        );
        let directory = match dir {
            Ok(dir) => {
                check(&mut items, "ok", "工作目录存在");
                dir
            }
            Err(e) => {
                check(&mut items, "error", e.to_string());
                opt.working_dir.clone()
            }
        };
        if let Err(e) = crate::environments::validate_binding(&opt.runtime) {
            check(&mut items, "error", e.to_string());
        }
        let (program, argv) = opt.resolve();
        let command = join_command_line(&program, &argv);
        if executable_exists(&program, &directory, &opt.env, &opt.runtime) {
            check(&mut items, "ok", format!("解释器 / 程序可用：{program}"));
        } else {
            check(
                &mut items,
                "error",
                format!("找不到程序：{program}，请检查所选环境或 PATH"),
            );
        }
        let verdict = crate::security::check(&command, &st.settings());
        if verdict.level == "blocked" {
            check(&mut items, "error", verdict.reasons.join("；"));
        } else if verdict.requires_confirm || preset.confirm {
            check(&mut items, "warning", "执行时仍需二次确认");
        }
        if opt.elevated && !super::system_cmds::is_elevated()? {
            check(&mut items, "error", "此命令需要以管理员身份启动 CmdDeck");
        }
        if ["exe", "custom"].contains(&opt.kind.as_str()) && !st.settings().allow_unknown_exe {
            check(&mut items, "error", "设置尚未允许自定义 exe");
        }
        if st
            .pty
            .list()
            .iter()
            .filter(|t| t.status == "running")
            .count()
            >= st.settings().max_concurrent_sessions as usize
        {
            check(&mut items, "error", "并发会话已达到上限");
        }
        if let Some(w) = workspace {
            match monitor::ports() {
                Ok(ports) => {
                    for port in w.ports {
                        if let Some(owner) = ports.iter().find(|p| p.port == port) {
                            check(
                                &mut items,
                                "warning",
                                format!("项目端口 {port} 已被 PID {} 占用", owner.pid),
                            );
                        }
                    }
                }
                Err(e) => check(&mut items, "warning", e.to_string()),
            }
        }
        let can_run = !items.iter().any(|i| i.level == "error");
        Ok(Preflight {
            command,
            directory,
            checks: items,
            can_run,
        })
    })
    .await
    .map_err(|e| AppError::other(e.to_string()))?
}
fn executable_exists(program: &str, cwd: &str, env: &[EnvVar], runtime: &RuntimeBinding) -> bool {
    let path = std::path::Path::new(program);
    if path.is_absolute() {
        return path.is_file();
    }
    let inherited = env
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case("PATH"))
        .map(|e| e.value.clone())
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
    let mut dirs = vec![std::path::PathBuf::from(cwd)];
    match runtime.kind.as_str() {
        "venv" => dirs.push(std::path::Path::new(&runtime.path).join("Scripts")),
        "node" | "python" => {
            if let Some(p) = std::path::Path::new(&runtime.path).parent() {
                dirs.push(p.to_owned())
            }
        }
        _ => {}
    }
    dirs.extend(std::env::split_paths(&inherited));
    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
    dirs.iter().any(|d| {
        d.join(program).is_file()
            || (path.extension().is_none()
                && extensions
                    .split(';')
                    .any(|ext| d.join(format!("{program}{ext}")).is_file()))
    })
}
#[tauri::command]
pub async fn list_task_metrics(app: AppHandle) -> AppResult<Vec<monitor::TaskInfo>> {
    tauri::async_runtime::spawn_blocking(move || monitor::tasks(&app.state::<AppState>()))
        .await
        .map_err(|e| AppError::other(e.to_string()))?
}
#[tauri::command]
pub async fn list_listening_ports(app: AppHandle) -> AppResult<Vec<monitor::PortInfo>> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let mut ports = monitor::ports()?;
        for info in st.pty.list() {
            for pid in st.pty.process_ids(&info.session_id)? {
                for p in &mut ports {
                    if p.pid == pid {
                        p.session_id = info.session_id.clone();
                    }
                }
            }
        }
        Ok(ports)
    })
    .await
    .map_err(|e| AppError::other(e.to_string()))?
}
#[tauri::command]
pub fn open_local_service(app: AppHandle, port: u16) -> AppResult<()> {
    if port == 0 || !monitor::ports()?.iter().any(|p| p.port == port) {
        return Err(AppError::validation("此端口已停止监听，请刷新列表"));
    }
    app.opener()
        .open_url(format!("http://localhost:{port}"), None::<&str>)
        .map_err(|e| AppError::other(e.to_string()))
}
#[tauri::command]
pub fn list_config_backups(state: State<'_, AppState>) -> AppResult<Vec<backups::BackupInfo>> {
    backups::list(&state)
}
#[tauri::command]
pub fn create_config_backup(state: State<'_, AppState>) -> AppResult<String> {
    backups::snapshot(&state)
}
#[tauri::command]
pub fn restore_config_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    confirmed: bool,
) -> AppResult<ImportReport> {
    if !confirmed {
        return Err(AppError::blocked("恢复配置需要确认"));
    }
    if state
        .restoring
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return Err(AppError::validation("正在恢复另一份配置"));
    }
    struct RestoreGuard<'a>(&'a std::sync::atomic::AtomicBool);
    impl Drop for RestoreGuard<'_> {
        fn drop(&mut self) {
            self.0.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let _guard = RestoreGuard(&state.restoring);
    if state.pty.list().iter().any(|s| s.status == "running")
        || !state.active_workflows.read().is_empty()
    {
        return Err(AppError::validation("请先停止正在运行的任务，再恢复配置"));
    }
    let path = backups::path(&state, &id)?;
    let mut bundle: ExportBundle = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|e| AppError::validation(format!("备份损坏，未修改现有配置：{e}")))?;
    bundle.settings.data_dir = state.data_dir.to_string_lossy().into_owned();
    bundle.settings.first_run_done = true;
    backups::snapshot(&state)?;
    backups::restore_into(&state.db, &bundle)?;
    *state.settings.write() = bundle.settings.clone();
    let report = ImportReport {
        groups_added: bundle.groups.len() as i32,
        groups_updated: 0,
        presets_added: bundle.presets.len() as i32,
        presets_updated: 0,
        workflows_added: bundle.workflows.len() as i32,
        schedules_added: bundle.schedules.len() as i32,
        settings_imported: true,
        warnings: vec!["系统快捷键与开机启动偏好在重新启动后完整应用".into()],
    };
    state.emit(
        &app,
        crate::state::EV_PRESET_CHANGED,
        serde_json::json!({"reason":"restored"}),
    );
    state.emit(&app, crate::state::EV_SETTINGS_CHANGED, bundle.settings);
    let st = app.state::<AppState>();
    st.emit(
        &app,
        "cmddeck://productivity-changed",
        productivity::load(&st.db)?,
    );
    Ok(report)
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShareOptions {
    pub preset_ids: Vec<String>,
    pub include_environment: bool,
    pub include_paths: bool,
    pub include_settings: bool,
}
fn share(state: &AppState, opt: &ShareOptions) -> AppResult<ExportBundle> {
    let mut b = backups::bundle(state)?;
    b.presets.retain(|p| opt.preset_ids.contains(&p.id));
    for p in &mut b.presets {
        if !opt.include_environment {
            p.env.clear();
        }
        if !opt.include_paths {
            p.working_dir.clear();
            p.runtime = Default::default();
        }
    }
    // 分享时不夹带未选预设的工作流、定时任务或项目私有配置。
    b.workflows.clear();
    b.schedules.clear();
    b.productivity = Default::default();
    if !opt.include_settings {
        b.settings = AppSettings::default();
    }
    b.settings.data_dir.clear();
    if !opt.include_paths {
        b.settings.default_working_dir.clear();
        b.settings.shell_path.clear();
    }
    b.groups
        .retain(|g| b.presets.iter().any(|p| p.group_id == g.id));
    Ok(b)
}
#[tauri::command]
pub fn preview_config_export(
    state: State<'_, AppState>,
    options: ShareOptions,
) -> AppResult<String> {
    serde_json::to_string_pretty(&share(&state, &options)?)
        .map_err(|e| AppError::other(e.to_string()))
}
#[tauri::command]
pub fn export_shared_config(
    state: State<'_, AppState>,
    path: String,
    options: ShareOptions,
    preview: String,
) -> AppResult<()> {
    let bundle = share(&state, &options)?;
    let mut reviewed: ExportBundle = serde_json::from_str(&preview)
        .map_err(|e| AppError::validation(format!("导出预览无效：{e}")))?;
    // 导出时间变化不影响预览，其他字段必须与用户检查过的内容一致。
    reviewed.exported_at = bundle.exported_at;
    let current = serde_json::to_value(&bundle).map_err(|e| AppError::other(e.to_string()))?;
    if serde_json::to_value(&reviewed).map_err(|e| AppError::other(e.to_string()))? != current {
        return Err(AppError::validation(
            "配置或导出选项已变化，请更新预览后重新导出",
        ));
    }
    std::fs::write(path, preview)?;
    Ok(())
}
#[tauri::command]
pub fn get_history_output(state: State<'_, AppState>, id: i64) -> AppResult<String> {
    state
        .db
        .conn()
        .query_row(
            "SELECT output_tail FROM terminal_history WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| AppError::not_found(format!("历史日志不存在：{e}")))
}
#[tauri::command]
pub fn list_log_entries(state: State<'_, AppState>) -> AppResult<Vec<TerminalHistory>> {
    crate::db::audit::history_metadata(&state.db)
}
#[tauri::command]
pub fn export_task_log(
    state: State<'_, AppState>,
    session_id: Option<String>,
    history_id: Option<i64>,
    path: String,
) -> AppResult<()> {
    let raw = if let Some(id) = session_id {
        state.pty.snapshot(&id)?
    } else if let Some(id) = history_id {
        state.db.conn().query_row(
            "SELECT output_tail FROM terminal_history WHERE id=?1",
            [id],
            |r| r.get::<_, String>(0),
        )?
    } else {
        return Err(AppError::validation("请选择日志"));
    };
    std::fs::write(path, strip_ansi(&raw))?;
    Ok(())
}
pub fn strip_ansi(raw: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\)|[@-_])")
            .unwrap()
    })
    .replace_all(raw, "")
    .into_owned()
}
#[tauri::command]
pub fn test_task_notification(app: AppHandle) -> AppResult<()> {
    crate::notifications::show(
        &app,
        "",
        "CmdDeck 通知测试",
        "任务结束后可在这里收到通知，点击回到 CmdDeck。",
    )
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub url: String,
    pub published_at: String,
}
#[tauri::command]
pub async fn check_for_updates() -> AppResult<UpdateInfo> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("CmdDeck/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| AppError::other(e.to_string()))?;
    let response = client
        .get("https://api.github.com/repos/jgbrzzh/CmdDeck/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            let mut detail = e.to_string();
            let mut cause = std::error::Error::source(&e);
            while let Some(err) = cause {
                detail.push_str(&format!("；{err}"));
                cause = err.source();
            }
            AppError::io(format!(
                "无法连接 GitHub，请检查网络和系统代理后重试：{detail}"
            ))
        })?;
    if !response.status().is_success() {
        return Err(AppError::io(format!(
            "GitHub 更新检查失败（HTTP {}），可能尚无正式版本或请求受限",
            response.status()
        )));
    }
    if response.content_length().is_some_and(|n| n > 1024 * 1024) {
        return Err(AppError::io("版本信息过大"));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::io(format!("版本信息无法解析：{e}")))?;
    parse_update(&value)
}
fn parse_update(value: &serde_json::Value) -> AppResult<UpdateInfo> {
    let tag = value["tag_name"]
        .as_str()
        .ok_or_else(|| AppError::validation("版本信息缺少版本号"))?;
    let latest = semver::Version::parse(tag.trim_start_matches('v'))
        .map_err(|e| AppError::validation(e.to_string()))?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| AppError::validation(e.to_string()))?;
    let url = format!("https://github.com/jgbrzzh/CmdDeck/releases/tag/{tag}");
    Ok(UpdateInfo {
        current: current.to_string(),
        latest: latest.to_string(),
        available: latest > current,
        notes: value["body"].as_str().unwrap_or("").into(),
        url,
        published_at: value["published_at"].as_str().unwrap_or("").into(),
    })
}
#[tauri::command]
pub fn open_releases(app: AppHandle) -> AppResult<()> {
    app.opener()
        .open_url(
            "https://github.com/jgbrzzh/CmdDeck/releases/latest",
            None::<&str>,
        )
        .map_err(|e| AppError::other(e.to_string()))
}
#[cfg(test)]
mod tests {
    #[test]
    fn semantic_version_order() {
        let info = super::parse_update(&serde_json::json!({"tag_name":"v2.10.0","body":"changes"}))
            .unwrap();
        assert!(info.available);
        assert!(super::parse_update(&serde_json::json!({"tag_name":"invalid"})).is_err());
    }
    #[test]
    fn ansi_export_is_readable() {
        assert_eq!(
            super::strip_ansi("\x1b[31m中文\x1b[0m\x1b]0;title\x07"),
            "中文"
        );
    }
}

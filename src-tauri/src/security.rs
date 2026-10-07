//! 安全检查在后端执行；预设、批量和调度共用同一入口。
use crate::db::models::{AppSettings, SecurityVerdict};
use crate::error::{AppError, AppResult};
pub fn executable_path(program: &str, cwd: &str) -> AppResult<std::path::PathBuf> {
    let p = std::path::Path::new(program);
    let extensions = std::env::var("PATHEXT").unwrap_or(".EXE;.CMD;.BAT;.COM".into());
    let resolve = |base: std::path::PathBuf| -> Option<std::path::PathBuf> {
        if base.is_file() {
            return base.canonicalize().ok();
        }
        if base.extension().is_none() {
            for ext in extensions.split(';') {
                let test = base.with_extension(ext.trim_start_matches('.'));
                if test.is_file() {
                    return test.canonicalize().ok();
                }
            }
        }
        None
    };
    if p.is_absolute() {
        return resolve(p.to_owned()).ok_or_else(|| AppError::validation("程序路径不存在"));
    }
    if program.contains(['\\', '/']) {
        return resolve(std::path::Path::new(cwd).join(p))
            .ok_or_else(|| AppError::validation("相对程序路径不存在"));
    }
    if let Some(found) = resolve(std::path::Path::new(cwd).join(p)) {
        return Ok(found);
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        if let Some(found) = resolve(dir.join(p)) {
            return Ok(found);
        }
    }
    Err(AppError::validation(format!("无法找到程序 {program}")))
}
pub fn check_executable(program: &str, cwd: &str, settings: &AppSettings) -> AppResult<()> {
    if settings.allowed_executables.is_empty() {
        return Ok(());
    }
    let actual = executable_path(program, cwd)?;
    if settings
        .allowed_executables
        .iter()
        .filter_map(|p| std::path::Path::new(p).canonicalize().ok())
        .any(|p| {
            p.to_string_lossy()
                .eq_ignore_ascii_case(&actual.to_string_lossy())
        })
    {
        return Ok(());
    }
    Err(AppError::permission(format!(
        "程序不在执行白名单中：{}",
        actual.display()
    )))
}
pub fn check_input(data: &str, settings: &AppSettings) -> AppResult<()> {
    // 保留 Ctrl+C 和 xterm 对光标位置探测的回应，它们不能提交命令。
    static RESPONSE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let response = RESPONSE
        .get_or_init(|| regex::Regex::new(r"^\x1b\[[?>]?[0-9;]{1,64}(?:[Rcnt]|\$y)$").unwrap());
    if !settings.allow_interactive_input && data != "\u{3}" && !response.is_match(data) {
        return Err(AppError::permission(
            "终端默认为只读；需要输入或粘贴时，请在设置中开启交互输入",
        ));
    }
    Ok(())
}
pub fn check(text: &str, settings: &AppSettings) -> SecurityVerdict {
    let normalized = text.to_lowercase().replace('/', "\\");
    let mut v = SecurityVerdict::safe();
    for pattern in &settings.blacklist {
        let hit = if let Some(re) = pattern.strip_prefix("regex:") {
            regex::RegexBuilder::new(re)
                .case_insensitive(true)
                .build()
                .map(|r| r.is_match(text))
                .unwrap_or(true)
        } else {
            normalized.contains(
                &pattern
                    .to_lowercase()
                    .replace('/', "\\")
                    .replace("\\\\", "\\"),
            )
        };
        if !pattern.is_empty() && hit {
            v.matched.push(pattern.clone());
        }
    }
    if !v.matched.is_empty() {
        v.level = "blocked".into();
        v.reasons.push("命中黑名单，请在设置中检查规则".into());
        return v;
    }
    let risk = regex::Regex::new(r"(?i)\b(remove-item|del|erase|rmdir|rd|format|diskpart|shutdown|stop-process|taskkill|reg\s+delete|invoke-expression|iex)\b").unwrap();
    if risk.is_match(text) {
        v.level = "danger".into();
        v.reasons
            .push("命令包含删除、关机、终止进程或动态执行操作".into());
        v.requires_confirm = settings.confirm_dangerous;
    }
    v
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readonly_blocks_fragmented_input_paste_and_control_sequences() {
        let s = AppSettings::default();
        for input in [
            "d",
            "iskpart\r",
            "\x1b[200~echo unsafe\x1b[201~",
            "\r",
            "\x1b[A",
            "\u{4}",
        ] {
            assert!(check_input(input, &s).is_err());
        }
        assert!(check_input("\u{3}", &s).is_ok());
        assert!(check_input("\x1b[?12;25R", &s).is_ok());
        let mut enabled = s;
        enabled.allow_interactive_input = true;
        assert!(check_input("echo ok\r", &enabled).is_ok());
    }
    #[test]
    fn executable_allowlist_uses_canonical_paths() {
        let mut s = AppSettings::default();
        let exe = std::env::current_exe().unwrap();
        s.allowed_executables = vec![exe.to_string_lossy().into_owned()];
        assert!(check_executable(&exe.to_string_lossy(), "", &s).is_ok());
        assert!(check_executable("definitely-missing.exe", "", &s).is_err());
    }
    #[test]
    fn rules_and_danger() {
        let mut s = AppSettings::default();
        s.blacklist = vec!["diskpart".into(), "regex:secret.*delete".into()];
        assert_eq!(check("DISKPART", &s).level, "blocked");
        assert_eq!(check("secret --delete", &s).level, "blocked");
        assert!(check("Remove-Item test.txt", &s).requires_confirm);
        assert_eq!(check("echo hello", &s).level, "safe");
    }
}

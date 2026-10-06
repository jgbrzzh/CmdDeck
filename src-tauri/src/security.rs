//! 安全检查在后端执行；预设、批量和调度共用同一入口。
use crate::db::models::{AppSettings, SecurityVerdict};
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
    fn rules_and_danger() {
        let mut s = AppSettings::default();
        s.blacklist = vec!["diskpart".into(), "regex:secret.*delete".into()];
        assert_eq!(check("DISKPART", &s).level, "blocked");
        assert_eq!(check("secret --delete", &s).level, "blocked");
        assert!(check("Remove-Item test.txt", &s).requires_confirm);
        assert_eq!(check("echo hello", &s).level, "safe");
    }
}

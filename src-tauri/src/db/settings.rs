//! 应用设置仓储。
//!
//! 设置是一个整体对象，序列化成一行 JSON 存进 `settings` 表的 `key = 'app'`。
//! 这么做的好处是**加字段不用改表结构**——往 [`AppSettings`] 上加一个字段，
//! 老用户下次启动读到的 JSON 里没这个字段，serde 会用结构体里的默认值兜住。
//!
//! 反过来说，**读出来的 JSON 坏掉时必须退回默认值而不是报错**：
//! 设置坏了顶多影响外观，不该让用户进不去软件。

use rusqlite::params;

use super::models::{now_ms, AppSettings};
use super::{from_json, to_json, Db};
use crate::error::AppResult;

/// settings 表里存放全局设置的那一行的 key
const KEY_APP: &str = "app";

/// 读取设置。
///
/// 表里没有这一行、JSON 解析失败——都返回 [`AppSettings::default`]，不报错。
pub fn load(db: &Db) -> AppResult<AppSettings> {
    let conn = db.conn();
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![KEY_APP],
            |r| r.get(0),
        )
        .ok();

    match raw {
        Some(text) => Ok(from_json(&text, AppSettings::default())),
        None => Ok(AppSettings::default()),
    }
}

/// 保存设置（整行覆盖）
pub fn save(db: &Db, s: &AppSettings) -> AppResult<()> {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![KEY_APP, to_json(s), now_ms()],
    )?;
    Ok(())
}

/// 恢复默认设置：删掉那一行并返回默认值。
///
/// 删行而不是写一份默认值，是为了让"这份设置从来没被改过"和"被重置过"在库里表现一致。
pub fn reset(db: &Db) -> AppResult<AppSettings> {
    {
        let conn = db.conn();
        conn.execute("DELETE FROM settings WHERE key = ?1", params![KEY_APP])?;
    }
    Ok(AppSettings::default())
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_when_missing() {
        let db = Db::open_in_memory().unwrap();
        let s = load(&db).unwrap();
        assert_eq!(s.theme, "dark");
        assert_eq!(s.font_size, 14);
        assert!(!s.first_run_done);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let mut s = AppSettings::default();
        s.font_size = 18;
        s.ui_scale = 120;
        s.global_hotkey = "CommandOrControl+Alt+C".to_string();
        s.blacklist = vec!["format c:".to_string()];
        save(&db, &s).unwrap();

        let back = load(&db).unwrap();
        assert_eq!(back.font_size, 18);
        assert_eq!(back.ui_scale, 120);
        assert_eq!(back.global_hotkey, "CommandOrControl+Alt+C");
        assert_eq!(back.blacklist, vec!["format c:".to_string()]);
    }

    #[test]
    fn broken_json_falls_back_to_default() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.conn();
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('app', '完全不是JSON', 1)",
                [],
            )
            .unwrap();
        }
        let s = load(&db).unwrap();
        assert_eq!(s.font_size, 14, "坏数据应退回默认值");
    }

    #[test]
    fn reset_clears_row() {
        let db = Db::open_in_memory().unwrap();
        let mut s = AppSettings::default();
        s.font_size = 20;
        save(&db, &s).unwrap();

        let fresh = reset(&db).unwrap();
        assert_eq!(fresh.font_size, 14);
        assert_eq!(load(&db).unwrap().font_size, 14);
    }
}

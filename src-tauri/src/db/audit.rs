//! 审计日志与终端历史仓储。
//!
//! 两张表结构几乎一样（`audit_logs` / `terminal_history`），所以放在一起维护。
//! 两者的自增主键在**写入前一律是 0**，由数据库分配；读出来才是真实 ID。
//!
//! 清理策略：日志表会一直长大，所以除了「用户手动清空」之外还提供
//! [`prune`] / [`history_prune`]——只保留最近 N 条，由调用方在合适时机触发。

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Row};

use super::models::{AuditFilter, AuditLog, TerminalHistory};
use super::Db;
use crate::error::AppResult;

/// 审计日志默认返回条数
const DEFAULT_LIMIT: i64 = 200;
/// 审计日志返回条数上限（防止前端一次拉十万条把界面卡死）
const MAX_LIMIT: i64 = 5000;

/// 把 `limit` 规整到 `[1, MAX_LIMIT]`，`<= 0` 时用默认值
fn normalize_limit(limit: i64) -> i64 {
    if limit <= 0 {
        DEFAULT_LIMIT
    } else {
        limit.min(MAX_LIMIT)
    }
}

/// 动态 WHERE 条件用的小工具：压入一个值并返回它的占位符
fn bind(values: &mut Vec<Value>, v: Value) -> String {
    values.push(v);
    format!("?{}", values.len())
}

// ============================================================
// 审计日志
// ============================================================

/// 审计日志列清单
const AUDIT_COLUMNS: &str = "id, at, preset_id, preset_name, command, cwd, level, status, \
                             exit_code, message, duration_ms, source";

/// 把一行记录读成 [`AuditLog`]
fn row_to_audit(row: &Row<'_>) -> rusqlite::Result<AuditLog> {
    Ok(AuditLog {
        id: row.get(0)?,
        at: row.get(1)?,
        preset_id: row.get(2)?,
        preset_name: row.get(3)?,
        command: row.get(4)?,
        cwd: row.get(5)?,
        level: row.get(6)?,
        status: row.get(7)?,
        exit_code: row.get::<_, Option<i32>>(8)?,
        message: row.get(9)?,
        duration_ms: row.get(10)?,
        source: row.get(11)?,
    })
}

/// 写一条审计日志，返回自增 ID。
///
/// `log.id` 传什么都无所谓，数据库自己分配。
pub fn insert(db: &Db, log: &AuditLog) -> AppResult<i64> {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO audit_logs (at, preset_id, preset_name, command, cwd, level, status, \
         exit_code, message, duration_ms, source) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            log.at,
            log.preset_id,
            log.preset_name,
            log.command,
            log.cwd,
            log.level,
            log.status,
            log.exit_code,
            log.message,
            log.duration_ms,
            log.source,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 按条件查审计日志，按时间倒序（新的在最前）。
///
/// 过滤项：
/// * `keyword` → 预设名 / 命令 / 消息 任一包含
/// * `level`   → `> 0` 时按危险等级**精确匹配**（0 表示"全部"）
/// * `source`  → 运行来源精确匹配
/// * `limit`   → `<= 0` 用 200，上限 5000
pub fn list(db: &Db, filter: &AuditFilter) -> AppResult<Vec<AuditLog>> {
    let conn = db.conn();

    let mut sql = format!("SELECT {AUDIT_COLUMNS} FROM audit_logs WHERE 1=1");
    let mut values: Vec<Value> = Vec::new();

    let keyword = filter.keyword.trim();
    if !keyword.is_empty() {
        let p = bind(
            &mut values,
            Value::Text(format!(
                "%{}%",
                keyword
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )),
        );
        sql.push_str(&format!(
            " AND (preset_name LIKE {p} ESCAPE '\\' OR command LIKE {p} ESCAPE '\\' \
              OR message LIKE {p} ESCAPE '\\')"
        ));
    }

    if filter.level > 0 {
        let p = bind(&mut values, Value::Integer(filter.level as i64));
        sql.push_str(&format!(" AND level = {p}"));
    }

    if !filter.source.trim().is_empty() {
        let p = bind(&mut values, Value::Text(filter.source.trim().to_string()));
        sql.push_str(&format!(" AND source = {p}"));
    }

    sql.push_str(" ORDER BY at DESC, id DESC LIMIT ?");
    values.push(Value::Integer(normalize_limit(filter.limit)));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), row_to_audit)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 清空全部审计日志
pub fn clear(db: &Db) -> AppResult<()> {
    let conn = db.conn();
    conn.execute("DELETE FROM audit_logs", [])?;
    Ok(())
}

/// 只保留最近 `keep` 条审计日志，返回删除条数。
///
/// `keep <= 0` 等价于清空（一条不留）。
pub fn prune(db: &Db, keep: i64) -> AppResult<i64> {
    let conn = db.conn();
    if keep <= 0 {
        return Ok(conn.execute("DELETE FROM audit_logs", [])? as i64);
    }
    let n = conn.execute(
        "DELETE FROM audit_logs WHERE id NOT IN \
         (SELECT id FROM audit_logs ORDER BY id DESC LIMIT ?1)",
        params![keep],
    )?;
    Ok(n as i64)
}

/// [`prune`] 的别名，语义更明确：给运行时模块调用时不容易看错。
pub fn audit_prune(db: &Db, keep: i64) -> AppResult<i64> {
    prune(db, keep)
}

// ============================================================
// 终端历史
// ============================================================

/// 终端历史列清单
const HIST_COLUMNS: &str = "id, session_id, preset_id, preset_name, title, command, cwd, kind, \
                             started_at, ended_at, exit_code, output_tail";

/// 把一行记录读成 [`TerminalHistory`]
fn row_to_history(row: &Row<'_>) -> rusqlite::Result<TerminalHistory> {
    Ok(TerminalHistory {
        id: row.get(0)?,
        session_id: row.get(1)?,
        preset_id: row.get(2)?,
        preset_name: row.get(3)?,
        title: row.get(4)?,
        command: row.get(5)?,
        cwd: row.get(6)?,
        kind: row.get(7)?,
        started_at: row.get(8)?,
        ended_at: row.get(9)?,
        exit_code: row.get::<_, Option<i32>>(10)?,
        output_tail: row.get(11)?,
    })
}

/// 写一条终端历史，返回自增 ID
pub fn history_insert(db: &Db, h: &TerminalHistory) -> AppResult<i64> {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO terminal_history (session_id, preset_id, preset_name, title, command, cwd, \
         kind, started_at, ended_at, exit_code, output_tail) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            h.session_id,
            h.preset_id,
            h.preset_name,
            h.title,
            h.command,
            h.cwd,
            h.kind,
            h.started_at,
            h.ended_at,
            h.exit_code,
            h.output_tail,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 查终端历史。
///
/// `preset_id` 为空表示查全部；按开始时间倒序，`limit <= 0` 用 200，上限 5000。
pub fn history_list(db: &Db, preset_id: &str, limit: i64) -> AppResult<Vec<TerminalHistory>> {
    let conn = db.conn();

    let mut sql = format!("SELECT {HIST_COLUMNS} FROM terminal_history WHERE 1=1");
    let mut values: Vec<Value> = Vec::new();

    if !preset_id.trim().is_empty() {
        let p = bind(&mut values, Value::Text(preset_id.trim().to_string()));
        sql.push_str(&format!(" AND preset_id = {p}"));
    }
    sql.push_str(" ORDER BY started_at DESC, id DESC LIMIT ?");
    values.push(Value::Integer(normalize_limit(limit)));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), row_to_history)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 清空全部终端历史
pub fn history_clear(db: &Db) -> AppResult<()> {
    let conn = db.conn();
    conn.execute("DELETE FROM terminal_history", [])?;
    Ok(())
}

/// 只保留最近 `keep` 条终端历史，返回删除条数
pub fn history_prune(db: &Db, keep: i64) -> AppResult<i64> {
    let conn = db.conn();
    if keep <= 0 {
        return Ok(conn.execute("DELETE FROM terminal_history", [])? as i64);
    }
    let n = conn.execute(
        "DELETE FROM terminal_history WHERE id NOT IN \
         (SELECT id FROM terminal_history ORDER BY id DESC LIMIT ?1)",
        params![keep],
    )?;
    Ok(n as i64)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{now_ms, run_source};

    fn log(name: &str, level: i32, source: &str, at: i64) -> AuditLog {
        AuditLog {
            id: 0,
            at,
            preset_id: "p1".to_string(),
            preset_name: name.to_string(),
            command: format!("run {name}"),
            cwd: "C:\\".to_string(),
            level,
            status: "success".to_string(),
            exit_code: Some(0),
            message: String::new(),
            duration_ms: 12,
            source: source.to_string(),
        }
    }

    fn hist(preset_id: &str, started: i64) -> TerminalHistory {
        TerminalHistory {
            id: 0,
            session_id: format!("s{started}"),
            preset_id: preset_id.to_string(),
            preset_name: "历史预设".to_string(),
            title: "t".to_string(),
            command: "echo".to_string(),
            cwd: "C:\\".to_string(),
            kind: "powershell".to_string(),
            started_at: started,
            ended_at: started + 5,
            exit_code: Some(0),
            output_tail: "ok".to_string(),
        }
    }

    #[test]
    fn insert_and_list_newest_first() {
        let db = Db::open_in_memory().unwrap();
        let a = insert(&db, &log("第一条", 0, run_source::MANUAL, 1000)).unwrap();
        let b = insert(&db, &log("第二条", 1, run_source::BATCH, 2000)).unwrap();
        assert!(b > a, "ID 应自增");

        let all = list(&db, &AuditFilter::default()).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].preset_name, "第二条", "新的在前");
        assert_eq!(all[0].id, b);
    }

    #[test]
    fn filter_by_keyword_level_source() {
        let db = Db::open_in_memory().unwrap();
        insert(&db, &log("清理 DNS", 1, run_source::MANUAL, 1000)).unwrap();
        insert(&db, &log("删除进程", 2, run_source::SCHEDULE, 2000)).unwrap();
        insert(&db, &log("编译项目", 0, run_source::WORKFLOW, 3000)).unwrap();

        // 关键字
        let hit = list(
            &db,
            &AuditFilter {
                keyword: "DNS".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(hit.len(), 1);

        // 危险等级（>0 精确匹配）
        let danger = list(
            &db,
            &AuditFilter {
                level: 2,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(danger.len(), 1);
        assert_eq!(danger[0].preset_name, "删除进程");

        // level = 0 表示不过滤
        assert_eq!(list(&db, &AuditFilter::default()).unwrap().len(), 3);

        // 来源
        let sched = list(
            &db,
            &AuditFilter {
                source: run_source::SCHEDULE.into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(sched.len(), 1);
    }

    #[test]
    fn limit_is_clamped() {
        let db = Db::open_in_memory().unwrap();
        for i in 0..5 {
            insert(&db, &log(&format!("第{i}条"), 0, "manual", 1000 + i)).unwrap();
        }
        assert_eq!(
            list(
                &db,
                &AuditFilter {
                    limit: 0,
                    ..Default::default()
                }
            )
            .unwrap()
            .len(),
            5
        );
        assert_eq!(
            list(
                &db,
                &AuditFilter {
                    limit: 2,
                    ..Default::default()
                }
            )
            .unwrap()
            .len(),
            2
        );
        assert_eq!(
            list(
                &db,
                &AuditFilter {
                    limit: -1,
                    ..Default::default()
                }
            )
            .unwrap()
            .len(),
            5
        );
        assert_eq!(
            list(
                &db,
                &AuditFilter {
                    limit: 99999,
                    ..Default::default()
                }
            )
            .unwrap()
            .len(),
            5
        );
    }

    #[test]
    fn prune_keeps_newest() {
        let db = Db::open_in_memory().unwrap();
        for i in 0..10 {
            insert(&db, &log(&format!("第{i}条"), 0, "manual", 1000 + i)).unwrap();
        }
        let removed = prune(&db, 3).unwrap();
        assert_eq!(removed, 7);
        let left = list(&db, &AuditFilter::default()).unwrap();
        assert_eq!(left.len(), 3);
        assert_eq!(left[0].preset_name, "第9条");

        assert_eq!(audit_prune(&db, 1).unwrap(), 2);
        assert_eq!(audit_prune(&db, 0).unwrap(), 1);
        assert!(list(&db, &AuditFilter::default()).unwrap().is_empty());
    }

    #[test]
    fn clear_audit() {
        let db = Db::open_in_memory().unwrap();
        insert(&db, &log("x", 0, "manual", now_ms())).unwrap();
        clear(&db).unwrap();
        assert!(list(&db, &AuditFilter::default()).unwrap().is_empty());
    }

    #[test]
    fn history_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let id1 = history_insert(&db, &hist("p1", 1000)).unwrap();
        let id2 = history_insert(&db, &hist("p2", 2000)).unwrap();
        assert!(id2 > id1);

        let all = history_list(&db, "", 0).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].preset_id, "p2");

        let only_p1 = history_list(&db, "p1", 10).unwrap();
        assert_eq!(only_p1.len(), 1);
        assert_eq!(only_p1[0].id, id1);
        assert_eq!(only_p1[0].output_tail, "ok");
        assert_eq!(only_p1[0].exit_code, Some(0));
    }

    #[test]
    fn history_prune_and_clear() {
        let db = Db::open_in_memory().unwrap();
        for i in 0..6 {
            history_insert(&db, &hist("p1", 1000 + i)).unwrap();
        }
        assert_eq!(history_prune(&db, 2).unwrap(), 4);
        assert_eq!(history_list(&db, "", 0).unwrap().len(), 2);
        history_clear(&db).unwrap();
        assert!(history_list(&db, "", 0).unwrap().is_empty());
    }
}

//! 预设分组仓储。
//!
//! 分组本身只有 8 个字段，没有 JSON 列，所以这一层非常薄。
//! 真正的坑在删除：分组没了，但它下面的预设不能跟着消失，
//! 要把它们的 `group_id` 置空（变成"未分组"），再把预设数量返回给前端提示。

use rusqlite::{params, Row};

use super::models::{new_id, now_ms, Group};
use super::Db;
use crate::error::{AppError, AppResult};

/// 列清单，查询与写入共用
const COLUMNS: &str = "id, name, icon, color, sort_order, collapsed, created_at, updated_at";

/// 把一行记录读成 [`Group`]
fn row_to_group(row: &Row<'_>) -> rusqlite::Result<Group> {
    Ok(Group {
        id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        color: row.get(3)?,
        sort_order: row.get(4)?,
        collapsed: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

// ============================================================
// 查询
// ============================================================

/// 列出全部分组，按 `sort_order ASC, created_at ASC` 排序
pub fn list(db: &Db) -> AppResult<Vec<Group>> {
    let conn = db.conn();
    let sql = format!("SELECT {COLUMNS} FROM groups ORDER BY sort_order ASC, created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_group)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 按 ID 取一个分组
pub fn get(db: &Db, id: &str) -> AppResult<Group> {
    let conn = db.conn();
    let sql = format!("SELECT {COLUMNS} FROM groups WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(row_to_group(row)?),
        None => Err(AppError::not_found(format!(
            "分组不存在（ID：{id}），可能已被删除"
        ))),
    }
}

/// 分组总数
pub fn count(db: &Db) -> AppResult<i32> {
    let conn = db.conn();
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0))?;
    Ok(n as i32)
}

/// 判断分组是否存在
pub fn exists(db: &Db, id: &str) -> AppResult<bool> {
    if id.trim().is_empty() {
        return Ok(false);
    }
    let conn = db.conn();
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM groups WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// 下一个可用的 `sort_order`（没有分组时返回 0）
pub fn next_sort_order(db: &Db) -> AppResult<i32> {
    let conn = db.conn();
    let v: i64 = conn.query_row(
        "SELECT IFNULL(MAX(sort_order), 0) + 1 FROM groups",
        [],
        |r| r.get(0),
    )?;
    Ok(v as i32)
}

/// 找出重名的分组（`exclude_id` 用来排除自己）。
///
/// 命令层用它报"分组名称已存在"，避免把冲突判断塞进仓储层导致导入时误伤。
pub fn find_by_name(db: &Db, name: &str, exclude_id: &str) -> AppResult<Option<Group>> {
    let conn = db.conn();
    let sql = format!("SELECT {COLUMNS} FROM groups WHERE name = ?1 AND id <> ?2 LIMIT 1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![name, exclude_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(row_to_group(row)?)),
        None => Ok(None),
    }
}

// ============================================================
// 写入
// ============================================================

/// 校验分组
pub fn validate(g: &Group) -> AppResult<()> {
    if g.name.trim().is_empty() {
        return Err(AppError::validation("分组名称不能为空"));
    }
    if g.name.chars().count() > 50 {
        return Err(AppError::validation("分组名称过长（最多 50 个字符）"));
    }
    Ok(())
}

/// 保存（新增或更新）一个分组。
///
/// * `id` 为空 → 视为新增，自动生成 ID 并排到末尾；
/// * 更新时保留原有 `created_at`。
pub fn save(db: &Db, group: &Group) -> AppResult<Group> {
    validate(group)?;

    let conn = db.conn();
    let now = now_ms();
    let mut item = group.clone();

    let has_id = !item.id.trim().is_empty();
    let exists = has_id
        && conn
            .query_row(
                "SELECT 1 FROM groups WHERE id = ?1",
                params![item.id],
                |_| -> rusqlite::Result<()> { Ok(()) },
            )
            .is_ok();

    if exists {
        if let Ok(old) = conn.query_row(
            "SELECT created_at FROM groups WHERE id = ?1",
            params![item.id],
            |r| r.get::<_, i64>(0),
        ) {
            if old > 0 {
                item.created_at = old;
            }
        }
    } else {
        if !has_id {
            item.id = new_id();
            item.sort_order = conn.query_row(
                "SELECT IFNULL(MAX(sort_order), 0) + 1 FROM groups",
                [],
                |r| r.get(0),
            )?;
        }
        if item.created_at <= 0 {
            item.created_at = now;
        }
    }
    item.updated_at = now;

    let sql = format!(
        "INSERT INTO groups ({COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8) \
         ON CONFLICT(id) DO UPDATE SET \
           name=excluded.name, icon=excluded.icon, color=excluded.color, \
           sort_order=excluded.sort_order, collapsed=excluded.collapsed, \
           created_at=excluded.created_at, updated_at=excluded.updated_at"
    );
    conn.execute(
        &sql,
        params![
            item.id,
            item.name,
            item.icon,
            item.color,
            item.sort_order,
            if item.collapsed { 1i64 } else { 0i64 },
            item.created_at,
            item.updated_at,
        ],
    )?;
    Ok(item)
}

/// 删除一个分组，返回因此变成"未分组"的预设数量。
///
/// 顺序很重要：先把预设的 `group_id` 清空，再删分组记录，整个过程放在一个事务里。
pub fn delete(db: &Db, id: &str) -> AppResult<i32> {
    let conn = db.conn();
    let tx = conn.unchecked_transaction()?;

    let exists: i64 = tx.query_row(
        "SELECT COUNT(*) FROM groups WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if exists == 0 {
        return Err(AppError::not_found(format!(
            "分组不存在（ID：{id}），可能已被删除"
        )));
    }

    let moved = tx.execute(
        "UPDATE presets SET group_id = '' WHERE group_id = ?1",
        params![id],
    )?;
    tx.execute("DELETE FROM groups WHERE id = ?1", params![id])?;
    tx.commit()?;
    Ok(moved as i32)
}

/// 清空全部分组并把所有预设置为未分组（导入"覆盖模式"用）
pub fn clear(db: &Db) -> AppResult<usize> {
    let conn = db.conn();
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE presets SET group_id = ''", [])?;
    let n = tx.execute("DELETE FROM groups", [])?;
    tx.commit()?;
    Ok(n)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::presets;

    fn sample(name: &str) -> Group {
        let mut g = Group::default();
        g.id = String::new();
        g.name = name.to_string();
        g
    }

    #[test]
    fn save_list_get() {
        let db = Db::open_in_memory().unwrap();
        let g = save(&db, &sample("常用工具")).unwrap();
        assert!(!g.id.is_empty());
        assert_eq!(count(&db).unwrap(), 1);

        let mut edit = g.clone();
        edit.name = "常用工具2".to_string();
        edit.color = "#ff6b6b".to_string();
        let edit = save(&db, &edit).unwrap();
        assert_eq!(edit.color, "#ff6b6b");
        assert_eq!(edit.created_at, g.created_at, "created_at 不应被覆盖");

        assert_eq!(list(&db).unwrap().len(), 1);
        assert_eq!(get(&db, &g.id).unwrap().name, "常用工具2");
    }

    #[test]
    fn sort_order_increments() {
        let db = Db::open_in_memory().unwrap();
        let a = save(&db, &sample("A")).unwrap();
        let b = save(&db, &sample("B")).unwrap();
        assert!(b.sort_order > a.sort_order);
        assert_eq!(next_sort_order(&db).unwrap(), b.sort_order + 1);
        assert_eq!(list(&db).unwrap()[0].id, a.id);
    }

    #[test]
    fn delete_unbinds_presets() {
        let db = Db::open_in_memory().unwrap();
        let g = save(&db, &sample("系统运维")).unwrap();

        let mut p = crate::db::models::Preset::default();
        p.id = String::new();
        p.name = "清空 DNS".to_string();
        p.group_id = g.id.clone();
        let p = presets::save(&db, &p).unwrap();

        assert_eq!(delete(&db, &g.id).unwrap(), 1);
        assert_eq!(
            presets::get(&db, &p.id).unwrap().group_id,
            "",
            "预设应变成未分组"
        );
        assert!(get(&db, &g.id).is_err());
        assert!(delete(&db, &g.id).is_err());
    }

    #[test]
    fn duplicate_name_detection() {
        let db = Db::open_in_memory().unwrap();
        let a = save(&db, &sample("网络")).unwrap();
        assert!(find_by_name(&db, "网络", "").unwrap().is_some());
        assert!(
            find_by_name(&db, "网络", &a.id).unwrap().is_none(),
            "排除自己"
        );
        assert!(find_by_name(&db, "其它", "").unwrap().is_none());
    }

    #[test]
    fn empty_name_rejected() {
        let db = Db::open_in_memory().unwrap();
        let mut g = sample("x");
        g.name = "  ".to_string();
        assert!(save(&db, &g).is_err());
    }
}

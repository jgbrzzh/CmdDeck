//! 预设指令仓储。
//!
//! 表结构见 [`crate::db::migrations`] 的 v1 迁移。`args` / `env` / `tags` / `placeholder_args`
//! 四个复杂字段以 JSON 文本存储，读写时用 [`crate::db::to_json`] / [`crate::db::from_json`] 转换。
//!
//! ## 两条铁律
//!
//! 1. **所有用户输入都必须用 `?` 占位符绑定**，绝不把字符串拼进 SQL 文本。
//!    本文件里唯一被 `format!` 拼进 SQL 的部分是我们自己写的常量片段（列清单、排序），
//!    `keyword` 这类外部输入一律变成绑定参数。
//! 2. **读出来的 JSON 坏了不能报错**，一律退回空数组。老版本写坏的数据不应该
//!    导致用户整个软件打不开，那属于灾难性故障。

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, Row};

use super::models::{new_id, now_ms, EnvVar, Placeholder, Preset, PresetFilter};
use super::{from_json, to_json, Db};
use crate::error::{AppError, AppResult};

/// 列清单，查询与写入共用，保证字段顺序完全一致
const COLUMNS: &str = "id, name, kind, program, args, working_dir, env, use_shell, icon, \
                       group_id, tags, confirm, elevated, danger_level, notes, sort_order, \
                       favorite, hidden, shortcut, placeholder_args, run_count, last_run_at, \
                       created_at, updated_at";

/// 「最近使用」最多返回多少条
const RECENT_LIMIT: i64 = 50;

// ============================================================
// 行 → 结构体
// ============================================================

/// 把一行记录读成 [`Preset`]。
///
/// JSON 字段解析失败时退回空数组（`from_json` 内部已兜底），不会抛错。
fn row_to_preset(row: &Row<'_>) -> rusqlite::Result<Preset> {
    let args_json: String = row.get(4)?;
    let env_json: String = row.get(6)?;
    let tags_json: String = row.get(10)?;
    let ph_json: String = row.get(19)?;

    Ok(Preset {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        program: row.get(3)?,
        args: from_json(&args_json, Vec::new()),
        working_dir: row.get(5)?,
        env: from_json(&env_json, Vec::<EnvVar>::new()),
        use_shell: row.get::<_, i64>(7)? != 0,
        icon: row.get(8)?,
        group_id: row.get(9)?,
        tags: from_json(&tags_json, Vec::new()),
        confirm: row.get::<_, i64>(11)? != 0,
        elevated: row.get::<_, i64>(12)? != 0,
        danger_level: row.get(13)?,
        notes: row.get(14)?,
        sort_order: row.get(15)?,
        favorite: row.get::<_, i64>(16)? != 0,
        hidden: row.get::<_, i64>(17)? != 0,
        shortcut: row.get(18)?,
        placeholder_args: from_json(&ph_json, Vec::<Placeholder>::new()),
        run_count: row.get(20)?,
        last_run_at: row.get(21)?,
        created_at: row.get(22)?,
        updated_at: row.get(23)?,
    })
}

/// 把一个值压进参数数组，返回它的占位符（`?1`、`?2`……）。
///
/// 这是"动态拼 WHERE 条件但不拼用户数据"的标准写法。
fn bind(values: &mut Vec<Value>, v: Value) -> String {
    values.push(v);
    format!("?{}", values.len())
}

/// 判断预设是否已存在（调用方需自己保证已持有连接锁）
fn exists_locked(conn: &Connection, id: &str) -> bool {
    if id.trim().is_empty() {
        return false;
    }
    conn.query_row(
        "SELECT 1 FROM presets WHERE id = ?1",
        params![id],
        |_| -> rusqlite::Result<()> { Ok(()) },
    )
    .is_ok()
}

/// 把一条预设写进数据库（不存在则插入，存在则整行覆盖）。
fn write_preset(conn: &Connection, p: &Preset) -> AppResult<()> {
    let sql = format!(
        "INSERT INTO presets ({COLUMNS}) VALUES \
         (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24) \
         ON CONFLICT(id) DO UPDATE SET \
           name=excluded.name, kind=excluded.kind, program=excluded.program, args=excluded.args, \
           working_dir=excluded.working_dir, env=excluded.env, use_shell=excluded.use_shell, \
           icon=excluded.icon, group_id=excluded.group_id, tags=excluded.tags, \
           confirm=excluded.confirm, elevated=excluded.elevated, danger_level=excluded.danger_level, \
           notes=excluded.notes, sort_order=excluded.sort_order, favorite=excluded.favorite, \
           hidden=excluded.hidden, shortcut=excluded.shortcut, \
           placeholder_args=excluded.placeholder_args, run_count=excluded.run_count, \
           last_run_at=excluded.last_run_at, created_at=excluded.created_at, \
           updated_at=excluded.updated_at"
    );
    conn.execute(
        &sql,
        params![
            p.id,
            p.name,
            p.kind,
            p.program,
            to_json(&p.args),
            p.working_dir,
            to_json(&p.env),
            if p.use_shell { 1i64 } else { 0i64 },
            p.icon,
            p.group_id,
            to_json(&p.tags),
            if p.confirm { 1i64 } else { 0i64 },
            if p.elevated { 1i64 } else { 0i64 },
            p.danger_level,
            p.notes,
            p.sort_order,
            if p.favorite { 1i64 } else { 0i64 },
            if p.hidden { 1i64 } else { 0i64 },
            p.shortcut,
            to_json(&p.placeholder_args),
            p.run_count,
            p.last_run_at,
            p.created_at,
            p.updated_at,
        ],
    )?;
    Ok(())
}

// ============================================================
// 查询
// ============================================================

/// 按过滤条件列出预设。
///
/// 过滤项全部可选：
/// * `keyword`  → 名称 / 程序 / 参数 / 备注 / 标签 任一包含关键字（SQLite `LIKE` 对 ASCII 不敏感）
/// * `group_id` → 精确匹配分组
/// * `tag`      → 标签包含
/// * `kind`     → 精确匹配执行类型
/// * `only_favorite` → 只看收藏
/// * `only_recent`   → 只看跑过的，按最近优先，取前 50 条
/// * `include_hidden` 为 false 时排除隐藏项
///
/// 默认排序 `sort_order ASC, created_at ASC`（拖拽排序后顺序稳定）。
pub fn list(db: &Db, filter: &PresetFilter) -> AppResult<Vec<Preset>> {
    let conn = db.conn();

    let mut sql = format!("SELECT {COLUMNS} FROM presets WHERE 1=1");
    let mut values: Vec<Value> = Vec::new();

    // 关键字：五个字段任一命中即可
    let keyword = filter.keyword.trim();
    if !keyword.is_empty() {
        let p = bind(
            &mut values,
            Value::Text(format!("%{}%", escape_like(keyword))),
        );
        sql.push_str(&format!(
            " AND (name LIKE {p} ESCAPE '\\' OR program LIKE {p} ESCAPE '\\' \
              OR args LIKE {p} ESCAPE '\\' OR notes LIKE {p} ESCAPE '\\' \
              OR tags LIKE {p} ESCAPE '\\')"
        ));
    }

    if !filter.group_id.trim().is_empty() {
        let p = bind(&mut values, Value::Text(filter.group_id.trim().to_string()));
        sql.push_str(&format!(" AND group_id = {p}"));
    }

    let tag = filter.tag.trim();
    if !tag.is_empty() {
        let p = bind(&mut values, Value::Text(format!("%{}%", escape_like(tag))));
        sql.push_str(&format!(" AND tags LIKE {p} ESCAPE '\\'"));
    }

    if !filter.kind.trim().is_empty() {
        let p = bind(&mut values, Value::Text(filter.kind.trim().to_string()));
        sql.push_str(&format!(" AND kind = {p}"));
    }

    if filter.only_favorite {
        sql.push_str(" AND favorite = 1");
    }

    if !filter.include_hidden {
        sql.push_str(" AND hidden = 0");
    }

    if filter.only_recent {
        sql.push_str(" AND last_run_at > 0 ORDER BY last_run_at DESC, created_at ASC LIMIT ?");
        values.push(Value::Integer(RECENT_LIMIT));
    } else {
        sql.push_str(" ORDER BY sort_order ASC, created_at ASC");
    }

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), row_to_preset)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 按 ID 取一条预设，找不到时返回中文 `NotFound`
pub fn get(db: &Db, id: &str) -> AppResult<Preset> {
    let conn = db.conn();
    let sql = format!("SELECT {COLUMNS} FROM presets WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(row_to_preset(row)?),
        None => Err(AppError::not_found(format!(
            "预设不存在（ID：{id}），可能已被删除"
        ))),
    }
}

/// 判断预设是否存在
pub fn exists(db: &Db, id: &str) -> AppResult<bool> {
    if id.trim().is_empty() {
        return Ok(false);
    }
    let conn = db.conn();
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM presets WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// 预设总数
pub fn count(db: &Db) -> AppResult<i32> {
    let conn = db.conn();
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM presets", [], |r| r.get(0))?;
    Ok(n as i32)
}

// ============================================================
// 写入
// ============================================================

/// 校验预设的必填字段
pub fn validate(p: &Preset) -> AppResult<()> {
    if p.name.trim().is_empty() {
        return Err(AppError::validation("预设名称不能为空"));
    }
    if p.name.chars().count() > 100 {
        return Err(AppError::validation("预设名称过长（最多 100 个字符）"));
    }
    Ok(())
}

/// 保存（新增或更新）一条预设，返回落库后的对象。
///
/// 行为：
/// * `name` 为空直接报中文校验错误；
/// * `id` 为空或数据库里没有这条记录 → 视为新增，自动生成 `id`；
/// * 更新时**保留数据库里的 `created_at`**，只刷新 `updated_at`。
pub fn save(db: &Db, preset: &Preset) -> AppResult<Preset> {
    validate(preset)?;

    let conn = db.conn();
    let now = now_ms();
    let mut item = preset.clone();

    let has_id = !item.id.trim().is_empty();
    if has_id && exists_locked(&conn, &item.id) {
        // 更新：created_at 沿用旧值，防止前端传 0 把创建时间抹掉
        let old: Option<i64> = conn
            .query_row(
                "SELECT created_at FROM presets WHERE id = ?1",
                params![item.id],
                |r| r.get(0),
            )
            .ok();
        if let Some(c) = old {
            if c > 0 {
                item.created_at = c;
            }
        }
    } else {
        if !has_id {
            item.id = new_id();
        }
        if item.created_at <= 0 {
            item.created_at = now;
        }
    }
    item.updated_at = now;

    write_preset(&conn, &item)?;
    Ok(item)
}

/// 批量写入预设（导入 JSON 时用）。已存在的同 ID 记录会被整行覆盖。
///
/// 返回实际写入的条数。
pub fn insert_many(db: &Db, presets: &[Preset]) -> AppResult<i32> {
    if presets.is_empty() {
        return Ok(0);
    }
    let conn = db.conn();
    let tx = conn.unchecked_transaction()?;
    let mut n = 0i32;
    for p in presets {
        validate(p)?;
        let mut item = p.clone();
        if item.id.trim().is_empty() {
            item.id = new_id();
        }
        if item.created_at <= 0 {
            item.created_at = now_ms();
        }
        item.updated_at = now_ms();
        write_preset(&tx, &item)?;
        n += 1;
    }
    tx.commit()?;
    Ok(n)
}

/// 删除一条预设
pub fn delete(db: &Db, id: &str) -> AppResult<()> {
    let conn = db.conn();
    let affected = conn.execute("DELETE FROM presets WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(AppError::not_found(format!(
            "预设不存在（ID：{id}），可能已被删除"
        )));
    }
    Ok(())
}

/// 批量删除预设，返回真正删掉的条数（不存在的 ID 直接忽略，不报错）
pub fn delete_many(db: &Db, ids: &[String]) -> AppResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let conn = db.conn();
    let tx = conn.unchecked_transaction()?;
    let mut total = 0usize;
    for id in ids {
        total += tx.execute("DELETE FROM presets WHERE id = ?1", params![id])?;
    }
    tx.commit()?;
    Ok(total)
}

/// 清空全部预设（导入"覆盖模式"时用），返回清掉的条数
pub fn clear(db: &Db) -> AppResult<usize> {
    let conn = db.conn();
    let n = conn.execute("DELETE FROM presets", [])?;
    Ok(n)
}

// ============================================================
// 便捷操作
// ============================================================

/// 复制一条预设。
///
/// 副本会：换新 ID、名字追加「 副本」、清空运行次数与最近运行时间、取消收藏，
/// 并排到同一分组的末尾。
pub fn duplicate(db: &Db, id: &str) -> AppResult<Preset> {
    let src = get(db, id)?;
    let now = now_ms();

    let mut copy = src.clone();
    copy.id = new_id();
    copy.name = format!("{} 副本", src.name);
    copy.run_count = 0;
    copy.last_run_at = 0;
    copy.favorite = false;
    copy.created_at = now;
    copy.updated_at = now;
    copy.sort_order = next_sort_order(db, &src.group_id)?;

    save(db, &copy)
}

/// 收藏 / 取消收藏，返回更新后的预设
pub fn set_favorite(db: &Db, id: &str, favorite: bool) -> AppResult<Preset> {
    {
        let conn = db.conn();
        let affected = conn.execute(
            "UPDATE presets SET favorite = ?1, updated_at = ?2 WHERE id = ?3",
            params![if favorite { 1i64 } else { 0i64 }, now_ms(), id],
        )?;
        if affected == 0 {
            return Err(AppError::not_found(format!(
                "预设不存在（ID：{id}），可能已被删除"
            )));
        }
    }
    get(db, id)
}

/// 记一次运行：次数 +1，刷新最近运行时间，返回更新后的预设
pub fn record_run(db: &Db, id: &str) -> AppResult<Preset> {
    {
        let conn = db.conn();
        let now = now_ms();
        let affected = conn.execute(
            "UPDATE presets SET run_count = run_count + 1, last_run_at = ?1, updated_at = ?1 \
             WHERE id = ?2",
            params![now, id],
        )?;
        if affected == 0 {
            return Err(AppError::not_found(format!(
                "预设不存在（ID：{id}），可能已被删除"
            )));
        }
    }
    get(db, id)
}

/// 拖拽排序：按 `ids` 数组的先后顺序重写 `sort_order`（从 0 开始）。
pub fn reorder(db: &Db, ids: &[String]) -> AppResult<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let conn = db.conn();
    let tx = conn.unchecked_transaction()?;
    for (idx, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE presets SET sort_order = ?1, updated_at = ?2 WHERE id = ?3",
            params![idx as i64, now_ms(), id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// 把一条预设移动到指定分组的指定位置
pub fn move_to_group(db: &Db, id: &str, group_id: &str, sort_order: i32) -> AppResult<()> {
    let conn = db.conn();
    let affected = conn.execute(
        "UPDATE presets SET group_id = ?1, sort_order = ?2, updated_at = ?3 WHERE id = ?4",
        params![group_id, sort_order, now_ms(), id],
    )?;
    if affected == 0 {
        return Err(AppError::not_found(format!(
            "预设不存在（ID：{id}），可能已被删除"
        )));
    }
    Ok(())
}

/// 某个分组里下一个可用的 `sort_order`（分组为空时返回 0）
pub fn next_sort_order(db: &Db, group_id: &str) -> AppResult<i32> {
    let conn = db.conn();
    let v: i64 = conn.query_row(
        "SELECT IFNULL(MAX(sort_order), 0) + 1 FROM presets WHERE group_id = ?1",
        params![group_id],
        |r| r.get(0),
    )?;
    Ok(v as i32)
}

/// 把 `LIKE` 通配符转义掉，避免用户输入的 `%` / `_` 变成通配符。
///
/// 配合 SQL 里的 `ESCAPE '\'` 一起使用；注意 SQLite 默认**不**把反斜杠当转义符，
/// 少写 `ESCAPE` 的话这里转义出来的 `\%` 会被当成"反斜杠 + 任意字符"，搜不到东西。
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::preset_kind;

    fn sample(name: &str) -> Preset {
        let mut p = Preset::default();
        p.id = String::new();
        p.name = name.to_string();
        p.kind = preset_kind::POWERSHELL.to_string();
        p.program = "systeminfo".to_string();
        p.args = vec!["/fo".to_string(), "list".to_string()];
        p.tags = vec!["系统".to_string()];
        p
    }

    #[test]
    fn save_insert_then_update() {
        let db = Db::open_in_memory().unwrap();
        let saved = save(&db, &sample("测试预设")).unwrap();
        assert!(!saved.id.is_empty());
        assert_eq!(count(&db).unwrap(), 1);

        // 再存一次同一个 ID → 走更新，created_at 不变
        let mut again = saved.clone();
        again.name = "改名了".to_string();
        let again = save(&db, &again).unwrap();
        assert_eq!(again.name, "改名了");
        assert_eq!(again.created_at, saved.created_at);
        assert_eq!(count(&db).unwrap(), 1);
    }

    #[test]
    fn save_rejects_empty_name() {
        let db = Db::open_in_memory().unwrap();
        let mut p = sample("");
        p.name = "   ".to_string();
        let err = save(&db, &p).unwrap_err();
        assert!(err.to_string().contains("预设名称不能为空"));
    }

    #[test]
    fn get_and_delete() {
        let db = Db::open_in_memory().unwrap();
        let p = save(&db, &sample("待删除")).unwrap();
        assert!(exists(&db, &p.id).unwrap());
        assert_eq!(get(&db, &p.id).unwrap().name, "待删除");

        delete(&db, &p.id).unwrap();
        assert!(!exists(&db, &p.id).unwrap());
        assert!(get(&db, &p.id).is_err());
    }

    #[test]
    fn delete_many_reports_count() {
        let db = Db::open_in_memory().unwrap();
        let a = save(&db, &sample("A")).unwrap();
        let b = save(&db, &sample("B")).unwrap();
        let n = delete_many(&db, &[a.id.clone(), b.id.clone(), "不存在".to_string()]).unwrap();
        assert_eq!(n, 2);
        assert_eq!(count(&db).unwrap(), 0);
    }

    #[test]
    fn list_filters_and_sorts() {
        let db = Db::open_in_memory().unwrap();
        let mut a = sample("清空 DNS 缓存");
        a.args = vec!["ipconfig".into(), "/flushdns".into()];
        a.sort_order = 1;
        a.favorite = true;
        let a = save(&db, &a).unwrap();

        let mut b = sample("磁盘空间统计");
        b.group_id = "g1".to_string();
        b.sort_order = 0;
        b.hidden = true;
        let b = save(&db, &b).unwrap();

        // 默认：按 sort_order 升序
        let all = list(&db, &PresetFilter::default()).unwrap();
        assert_eq!(all.len(), 1, "隐藏项默认不返回");
        assert_eq!(all[0].id, a.id);

        // include_hidden
        let with_hidden = list(
            &db,
            &PresetFilter {
                include_hidden: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(with_hidden.len(), 2);
        assert_eq!(with_hidden[0].id, b.id, "sort_order 小的排前面");

        // 关键字（匹配到参数里的 flushdns）
        let hit = list(
            &db,
            &PresetFilter {
                keyword: "flushdns".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].id, a.id);

        // 只看收藏
        let fav = list(
            &db,
            &PresetFilter {
                only_favorite: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(fav.len(), 1);

        // 只看最近：没跑过就是空
        let recent = list(
            &db,
            &PresetFilter {
                only_recent: true,
                include_hidden: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(recent.is_empty());

        // 记一次运行后再看
        let a2 = record_run(&db, &a.id).unwrap();
        assert_eq!(a2.run_count, 1);
        assert!(a2.last_run_at > 0);
        let recent = list(
            &db,
            &PresetFilter {
                only_recent: true,
                include_hidden: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, a2.id);
    }

    #[test]
    fn list_by_group_and_tag() {
        let db = Db::open_in_memory().unwrap();
        let mut a = sample("分组内预设");
        a.group_id = "g1".to_string();
        a.tags = vec!["网络".to_string()];
        save(&db, &a).unwrap();

        let b = save(&db, &sample("无分组预设")).unwrap();

        let by_group = list(
            &db,
            &PresetFilter {
                group_id: "g1".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(by_group.len(), 1);

        let by_tag = list(
            &db,
            &PresetFilter {
                tag: "网络".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(by_tag.len(), 1);

        let none = list(&db, &PresetFilter::default()).unwrap();
        assert_eq!(none.len(), 2);
        assert!(none.iter().any(|p| p.id == b.id));
    }

    #[test]
    fn duplicate_resets_stats() {
        let db = Db::open_in_memory().unwrap();
        let p = save(&db, &sample("原件")).unwrap();
        let p = record_run(&db, &p.id).unwrap();
        let p = set_favorite(&db, &p.id, true).unwrap();

        let copy = duplicate(&db, &p.id).unwrap();
        assert_ne!(copy.id, p.id);
        assert_eq!(copy.name, "原件 副本");
        assert_eq!(copy.run_count, 0);
        assert_eq!(copy.last_run_at, 0);
        assert!(!copy.favorite);
        // 副本排在原件后面
        assert!(copy.sort_order > p.sort_order);
    }

    #[test]
    fn reorder_and_move() {
        let db = Db::open_in_memory().unwrap();
        let a = save(&db, &sample("A")).unwrap();
        let b = save(&db, &sample("B")).unwrap();
        let c = save(&db, &sample("C")).unwrap();

        reorder(&db, &[c.id.clone(), a.id.clone(), b.id.clone()]).unwrap();
        let ids: Vec<String> = list(&db, &PresetFilter::default())
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, vec![c.id.clone(), a.id.clone(), b.id.clone()]);

        move_to_group(&db, &a.id, "g9", 0).unwrap();
        assert_eq!(get(&db, &a.id).unwrap().group_id, "g9");
        assert_eq!(next_sort_order(&db, "g9").unwrap(), 1);
    }

    #[test]
    fn insert_many_and_clear() {
        let db = Db::open_in_memory().unwrap();
        let items: Vec<Preset> = (0..5).map(|i| sample(&format!("批量{i}"))).collect();
        assert_eq!(insert_many(&db, &items).unwrap(), 5);
        assert_eq!(count(&db).unwrap(), 5);
        assert_eq!(clear(&db).unwrap(), 5);
        assert_eq!(count(&db).unwrap(), 0);
    }

    #[test]
    fn broken_json_falls_back_to_empty() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.conn();
            conn.execute(
                "INSERT INTO presets (id, name, kind, args, tags, env, placeholder_args) \
                 VALUES ('x1', '坏数据', 'powershell', '不是JSON', '@@', '[]', '???')",
                [],
            )
            .unwrap();
        }
        let p = get(&db, "x1").unwrap();
        assert!(p.args.is_empty());
        assert!(p.tags.is_empty());
        assert!(p.env.is_empty());
        assert!(p.placeholder_args.is_empty());
    }

    #[test]
    fn keyword_wildcard_is_escaped() {
        let db = Db::open_in_memory().unwrap();
        save(&db, &sample("100% 完成率")).unwrap();
        save(&db, &sample("普通预设")).unwrap();
        let hits = list(
            &db,
            &PresetFilter {
                keyword: "100%".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1, "% 应该被当成普通字符");
    }
}

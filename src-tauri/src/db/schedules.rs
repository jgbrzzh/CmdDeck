//! 定时任务仓储。
//!
//! 表结构见 [`crate::db::migrations`] 的 v3 迁移。`args` / `weekdays` 两个
//! 复杂字段以 JSON 文本存储，读写时用 [`crate::db::to_json`] / [`crate::db::from_json`] 转换。
//!
//! ## 关于时区
//!
//! 项目**没有引入 chrono**（依赖已锁定），所以本文件自带一套最小的时间换算工具：
//! `ms_to_ymd_hm` / `ymd_hm_to_ms`，按 **UTC+8 固定偏移** 做换算。
//! 理由：CmdDeck 面向国内用户，UTC+8 覆盖绝大部分使用场景；跨时区用户会看到
//! "每天 09:00" 在本地略有偏移，属于可接受的次要问题（未来如需精确时区，
//! 可在设置里增加偏移量字段，这里只需把 `TZ_OFFSET_SECS` 换成变量即可）。

use rusqlite::{params, Row};

use super::models::{now_ms, Schedule};
use super::{from_json, to_json, Db};
use crate::error::{AppError, AppResult};

// ============================================================
// 本地时间工具（UTC+8 固定偏移）
// ============================================================

/// UTC+8 的秒偏移
const TZ_OFFSET_SECS: i64 = 8 * 3600;

/// 每分钟毫秒数
const MINUTE_MS: i64 = 60 * 1000;
/// 每天毫秒数
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// 把 Unix 毫秒换算成「本地」的年月日时分秒与星期几。
///
/// 返回 `(年, 月, 日, 时, 分, 星期)`，星期取值 1=周一 … 7=周日（与前端约定一致）。
/// 使用 Howard Hinnant 的 civil_from_days 算法，无需浮点，闰年闰月都精确。
fn ms_to_ymd_hm(ms: i64) -> (i64, i64, i64, i64, i64, i64) {
    // 1. 先加时区偏移，得到"本地墙上时钟"的秒
    let local_secs = ms.div_euclid(1000) + TZ_OFFSET_SECS;

    // 2. 天数与当天已过的秒数（floor div，负数时间戳也能正确处理）
    let days = local_secs.div_euclid(86_400);
    let secs_of_day = local_secs.rem_euclid(86_400);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;

    // 3. 天数 → 年月日
    let (y, m, d) = civil_from_days(days);

    // 4. 星期几：1970-01-01 是周四(4)，days 为 0 时输出 4
    let weekday = ((days + 3).rem_euclid(7)) + 1;

    (y, m, d, hh, mm, weekday)
}

/// 把「本地」的年月日时分秒换算回 Unix 毫秒。
fn ymd_hm_to_ms(y: i64, m: i64, d: i64, hh: i64, mm: i64) -> i64 {
    let days = days_from_civil(y, m, d);
    let local_secs = days * 86_400 + hh * 3600 + mm * 60;
    (local_secs - TZ_OFFSET_SECS) * 1000
}

/// 天数 → ���月日（Howard Hinnant 算法）。
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 年月日 → 天数（`civil_from_days` 的逆运算）。
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// 解析 "HH:mm"，成功返回 `(时, 分)`。格式非法返回 `None`。
fn parse_hhmm(text: &str) -> Option<(i64, i64)> {
    let t = text.trim();
    let (h_str, m_str) = t.split_once(':')?;
    if h_str.is_empty() || m_str.is_empty() {
        return None;
    }
    // 只允许纯数字，避免 "1a:30" 这类被误解析
    if !h_str.bytes().all(|b| b.is_ascii_digit()) || !m_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let h: i64 = h_str.parse().ok()?;
    let m: i64 = m_str.parse().ok()?;
    if !(0..=23).contains(&h) || !(0..=59).contains(&m) {
        return None;
    }
    Some((h, m))
}

/// 解析 "YYYY-MM-DD"，成功返回 `(年, 月, 日)`。格式非法或日期不存在返回 `None`。
fn parse_ymd(text: &str) -> Option<(i64, i64, i64)> {
    let t = text.trim();
    let parts: Vec<&str> = t.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    if !parts
        .iter()
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    if !(1970..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // 校验真实存在（排除 2025-02-30 这类）
    let (ry, rm, rd) = civil_from_days(days_from_civil(y, m, d));
    if (ry, rm, rd) != (y, m, d) {
        return None;
    }
    Some((y, m, d))
}

// ============================================================
// 下次触发时间计算（纯函数）
// ============================================================

/// 定时任务模式取值
pub mod mode {
    pub const INTERVAL: &str = "interval";
    pub const DAILY: &str = "daily";
    pub const WEEKLY: &str = "weekly";
    pub const ONCE: &str = "once";
    /// 全部合法取值
    pub const ALL: [&str; 4] = [INTERVAL, DAILY, WEEKLY, ONCE];
}

/// 根据模式计算下一次触发时间。
///
/// 返回值 0 表示"不再触发"（`once` 且时间已过、或输入非法）。
/// 这是一个**纯函数**：不读系统时间、不读数据库，方便单元测试。
///
/// 规则：
/// * `interval`：`from_ms + interval_minutes` 分钟（最小 1 分钟）；
/// * `daily`：下一个 "HH:mm" 时刻，今天已过则顺延到明天；
/// * `weekly`：`weekdays`（1=周一..7=周日）中最近的那一天的 "HH:mm"；
/// * `once`：`date` + `time` 那一刻的时间戳，已过则返回 0。
pub fn compute_next_run(s: &Schedule, from_ms: i64) -> i64 {
    match s.mode.as_str() {
        mode::INTERVAL => {
            let mins = if s.interval_minutes < 1 {
                1
            } else {
                s.interval_minutes
            };
            from_ms.saturating_add(mins.saturating_mul(MINUTE_MS))
        }
        mode::DAILY => {
            let Some((hh, mm)) = parse_hhmm(&s.time) else {
                return 0;
            };
            let (y, m, d, _now_h, _now_m, _) = ms_to_ymd_hm(from_ms);
            let today = ymd_hm_to_ms(y, m, d, hh, mm);
            if today > from_ms {
                today
            } else {
                // 今天这个时刻已经过了（或正好相等），顺延到明天同一时刻
                let (y2, m2, d2, _, _, _) = ms_to_ymd_hm(today + DAY_MS);
                ymd_hm_to_ms(y2, m2, d2, hh, mm)
            }
        }
        mode::WEEKLY => {
            let Some((hh, mm)) = parse_hhmm(&s.time) else {
                return 0;
            };
            // 归一化星期列表：过滤非法值、去重
            let mut days: Vec<i64> = s
                .weekdays
                .iter()
                .map(|d| *d as i64)
                .filter(|d| (1..=7).contains(d))
                .collect();
            days.sort_unstable();
            days.dedup();
            if days.is_empty() {
                return 0;
            }
            let (y, m, d, _, _, cur_wd) = ms_to_ymd_hm(from_ms);
            // 依次检查「今天、明天、…后天」，最多找 7 天
            for offset in 0..8i64 {
                let (cy, cm, cd, _, _, wd) = ms_to_ymd_hm(from_ms + offset * DAY_MS);
                if days.contains(&wd) {
                    let t = ymd_hm_to_ms(cy, cm, cd, hh, mm);
                    // 今天但时刻已过 → 继续往后找
                    if t > from_ms {
                        let _ = (y, m, d, cur_wd);
                        return t;
                    }
                }
            }
            0
        }
        mode::ONCE => {
            let Some((y, m, d)) = parse_ymd(&s.date) else {
                return 0;
            };
            // 未填 time 时按 00:00 处理
            let (hh, mm) = parse_hhmm(&s.time).unwrap_or((0, 0));
            let t = ymd_hm_to_ms(y, m, d, hh, mm);
            if t > from_ms {
                t
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// 校验定时任务并返回中文错误信息（`save` 时调用）。
///
/// 与 [`compute_next_run`] 的解析规则保持一致：这里只负责把"为什么非法"
/// 翻译成人能看懂的话。
pub fn validate(s: &Schedule) -> AppResult<()> {
    if s.name.trim().is_empty() {
        return Err(AppError::validation("定时任务名称不能为空，请填写一个名字"));
    }
    if s.preset_id.trim().is_empty() {
        return Err(AppError::validation("请为定时任务选择一个要执行的预设指令"));
    }
    match s.mode.as_str() {
        mode::INTERVAL => {
            if s.interval_minutes < 1 {
                return Err(AppError::validation("间隔模式的间隔分钟数必须大于等于 1"));
            }
        }
        mode::DAILY => {
            if parse_hhmm(&s.time).is_none() {
                return Err(AppError::validation(
                    "运行时刻格式不正确，应形如 09:00（小时 00-23，分钟 00-59）",
                ));
            }
        }
        mode::WEEKLY => {
            if parse_hhmm(&s.time).is_none() {
                return Err(AppError::validation(
                    "运行时刻格式不正确，应形如 09:00（小时 00-23，分钟 00-59）",
                ));
            }
            if !s.weekdays.iter().any(|d| (1..=7).contains(d)) {
                return Err(AppError::validation("每周模式至少要选择一个星期几"));
            }
        }
        mode::ONCE => {
            if parse_ymd(&s.date).is_none() {
                return Err(AppError::validation(
                    "单次模式的日期格式不正确，应形如 2025-12-31",
                ));
            }
            if !s.time.trim().is_empty() && parse_hhmm(&s.time).is_none() {
                return Err(AppError::validation(
                    "运行时刻格式不正确，应形如 09:00（小时 00-23，分钟 00-59）",
                ));
            }
        }
        other => {
            return Err(AppError::validation(format!(
                "未知的定时模式「{other}」，可选值：interval / daily / weekly / once"
            )));
        }
    }
    Ok(())
}

// ============================================================
// 仓储函数
// ============================================================

/// 列清单，查询与插入共用，保证字段顺序一致
const COLUMNS: &str = "id, name, preset_id, args, mode, interval_minutes, time, weekdays, date, \
                       enabled, next_run_at, last_run_at, last_status, created_at, updated_at";

/// 把一行记录读成 [`Schedule`]
fn row_to_schedule(row: &Row<'_>) -> rusqlite::Result<Schedule> {
    let args_json: String = row.get(3)?;
    let weekdays_json: String = row.get(7)?;
    Ok(Schedule {
        id: row.get(0)?,
        name: row.get(1)?,
        preset_id: row.get(2)?,
        args: from_json(&args_json, Default::default()),
        mode: row.get(4)?,
        interval_minutes: row.get(5)?,
        time: row.get(6)?,
        weekdays: from_json(&weekdays_json, Vec::new()),
        date: row.get(8)?,
        enabled: row.get::<_, i64>(9)? != 0,
        next_run_at: row.get(10)?,
        last_run_at: row.get(11)?,
        last_status: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

/// 列出全部定时任务，按「下次触发时间 → 创建时间」升序。
///
/// 排序优先用 `next_run_at`（用户最关心"谁先跑"），并列时按 `created_at` 保持稳定。
pub fn list(db: &Db) -> AppResult<Vec<Schedule>> {
    let conn = db.conn();
    let sql = format!("SELECT {COLUMNS} FROM schedules ORDER BY next_run_at ASC, created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_schedule)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 按 ID 取一条，找不到时返回中文 `NotFound`
pub fn get(db: &Db, id: &str) -> AppResult<Schedule> {
    let conn = db.conn();
    let sql = format!("SELECT {COLUMNS} FROM schedules WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(row_to_schedule(row)?),
        None => Err(AppError::not_found(format!(
            "定时任务不存在（ID：{id}），可能已被删除"
        ))),
    }
}

/// 保存（新增或更新）一条定时任务。
///
/// 行为：
/// * 走一遍 [`validate`]，非法直接返回中文校验错误；
/// * `id` 为空时自动生成，`created_at` 为 0 时补当前时间；
/// * **每次保存都按当前时间重算 `next_run_at`**，避免关掉程序几天后回来
///   一次性补跑一堆过期任务；
/// * `once` 模式若时间已过，`next_run_at` 记 0 并自动置为停用。
pub fn save(db: &Db, s: &Schedule) -> AppResult<Schedule> {
    validate(s)?;

    let now = now_ms();
    let mut item = s.clone();
    if item.id.trim().is_empty() {
        item.id = crate::db::models::new_id();
    }
    if item.created_at <= 0 {
        item.created_at = now;
    }
    item.updated_at = now;

    let next = if item.enabled {
        compute_next_run(&item, now)
    } else {
        0
    };
    item.next_run_at = next;
    // 单次任务时间已过：自动停用，避免调度器反复扫描
    if item.mode == mode::ONCE && next == 0 {
        item.enabled = false;
        item.last_status = "已执行完毕（单次任务）".to_string();
    }

    let conn = db.conn();
    let sql = "INSERT INTO schedules (id, name, preset_id, args, mode, interval_minutes, time, \
               weekdays, date, enabled, next_run_at, last_run_at, last_status, created_at, updated_at) \
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15) \
               ON CONFLICT(id) DO UPDATE SET \
                 name=excluded.name, preset_id=excluded.preset_id, args=excluded.args, \
                 mode=excluded.mode, interval_minutes=excluded.interval_minutes, time=excluded.time, \
                 weekdays=excluded.weekdays, date=excluded.date, enabled=excluded.enabled, \
                 next_run_at=excluded.next_run_at, last_run_at=excluded.last_run_at, \
                 last_status=excluded.last_status, updated_at=excluded.updated_at";
    conn.execute(
        sql,
        params![
            item.id,
            item.name,
            item.preset_id,
            to_json(&item.args),
            item.mode,
            item.interval_minutes,
            item.time,
            to_json(&item.weekdays),
            item.date,
            if item.enabled { 1i64 } else { 0i64 },
            item.next_run_at,
            item.last_run_at,
            item.last_status,
            item.created_at,
            item.updated_at,
        ],
    )?;
    Ok(item)
}

/// 删除一条定时任务
pub fn delete(db: &Db, id: &str) -> AppResult<()> {
    let conn = db.conn();
    let affected = conn.execute("DELETE FROM schedules WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(AppError::not_found(format!(
            "定时任务不存在（ID：{id}），可能已被删除"
        )));
    }
    Ok(())
}

/// 启用 / 停用一条定时任务。
///
/// 启用时会立即重算 `next_run_at`（以当前时间为基准）；
/// 停用时把 `next_run_at` 清零，调度器就不会再捡起它。
pub fn set_enabled(db: &Db, id: &str, enabled: bool) -> AppResult<()> {
    let mut s = get(db, id)?;
    s.enabled = enabled;
    if enabled {
        s.next_run_at = compute_next_run(&s, now_ms());
    } else {
        s.next_run_at = 0;
    }
    s.updated_at = now_ms();

    let conn = db.conn();
    conn.execute(
        "UPDATE schedules SET enabled = ?2, next_run_at = ?3, updated_at = ?4 WHERE id = ?1",
        params![
            id,
            if enabled { 1i64 } else { 0i64 },
            s.next_run_at,
            s.updated_at
        ],
    )?;
    Ok(())
}

/// 取出所有「已到点」的任务：已启用、`next_run_at > 0` 且 `next_run_at <= now`。
///
/// 这是调度线程每 20 秒调用一次的查询，必须走 `idx_sched_next` 索引。
pub fn due(db: &Db, now: i64) -> AppResult<Vec<Schedule>> {
    let conn = db.conn();
    let sql = format!(
        "SELECT {COLUMNS} FROM schedules \
         WHERE enabled = 1 AND next_run_at > 0 AND next_run_at <= ?1 \
         ORDER BY next_run_at ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![now], row_to_schedule)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 调度器在任务触发**之后**回写执行结果并推进下一次时间。
///
/// `next` 为 0 表示"没有下次了"（单次任务已执行完毕），此时同时把任务停用。
pub fn set_next_run(db: &Db, id: &str, next: i64, last_status: &str) -> AppResult<()> {
    let now = now_ms();
    let conn = db.conn();
    if next > 0 {
        conn.execute(
            "UPDATE schedules SET next_run_at = ?2, last_run_at = ?3, last_status = ?4, \
             updated_at = ?5 WHERE id = ?1",
            params![id, next, now, last_status, now],
        )?;
    } else {
        conn.execute(
            "UPDATE schedules SET next_run_at = 0, last_run_at = ?2, last_status = ?3, \
             enabled = 0, updated_at = ?4 WHERE id = ?1",
            params![id, now, last_status, now],
        )?;
    }
    Ok(())
}

// ============================================================
// 单元测试：时间换算 + 下次触发时间计算
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个测试用定时任务（默认 interval / 60 分钟）
    fn mk(mode: &str) -> Schedule {
        let mut s = Schedule::default();
        s.mode = mode.to_string();
        s
    }

    /// 已知时间点：`2025-06-15 12:00:00 UTC+8` = 2025-06-15T04:00:00Z
    const BASE_2025_06_15_NOON: i64 = 1_749_960_000_000;

    // ---- 时间换算 ----

    #[test]
    fn ms_to_ymd_hm_uses_utc_plus_8() {
        // 1750000000 秒 = 2025-06-15T04:00:00Z = 北京时间 12:00 周日(7)
        let (y, m, d, hh, mm, wd) = ms_to_ymd_hm(BASE_2025_06_15_NOON);
        assert_eq!((y, m, d, hh, mm), (2025, 6, 15, 12, 0));
        assert_eq!(wd, 7, "2025-06-15 是周日");
    }

    #[test]
    fn civil_roundtrip_covers_leap_year() {
        for &(y, m, d) in &[
            (2024, 2, 29),
            (2025, 1, 1),
            (2000, 2, 29),
            (1999, 12, 31),
            (2100, 3, 1),
        ] {
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d), "roundtrip {y}-{m}-{d}");
        }
    }

    #[test]
    fn parse_hhmm_rejects_bad_input() {
        assert_eq!(parse_hhmm("09:30"), Some((9, 30)));
        assert_eq!(parse_hhmm("00:00"), Some((0, 0)));
        assert_eq!(parse_hhmm("23:59"), Some((23, 59)));
        assert_eq!(parse_hhmm("24:00"), None, "小时越界");
        assert_eq!(parse_hhmm("9:60"), None, "分钟越界");
        assert_eq!(parse_hhmm("abc"), None);
        assert_eq!(parse_hhmm(""), None);
        assert_eq!(parse_hhmm("9"), None, "没有冒号");
    }

    #[test]
    fn parse_ymd_rejects_impossible_date() {
        assert_eq!(parse_ymd("2025-12-31"), Some((2025, 12, 31)));
        assert_eq!(parse_ymd("2025-02-30"), None, "2 月没有 30 号");
        assert_eq!(parse_ymd("2025-13-01"), None, "13 月不存在");
        assert_eq!(parse_ymd("2025-1-1"), None, "必须补零");
        assert_eq!(parse_ymd(""), None);
    }

    // ---- interval ----

    #[test]
    fn interval_adds_minutes() {
        let mut s = mk(mode::INTERVAL);
        s.interval_minutes = 30;
        assert_eq!(compute_next_run(&s, 1_000_000), 1_000_000 + 30 * 60 * 1000);
    }

    #[test]
    fn interval_falls_back_to_one_minute() {
        // 非法值（0 / 负数）不应产生 0 或负的下次时间
        let mut s = mk(mode::INTERVAL);
        s.interval_minutes = 0;
        assert_eq!(compute_next_run(&s, 1_000_000), 1_000_000 + 60_000);
        s.interval_minutes = -5;
        assert_eq!(compute_next_run(&s, 1_000_000), 1_000_000 + 60_000);
    }

    // ---- daily ----

    #[test]
    fn daily_today_before_time() {
        let mut s = mk(mode::DAILY);
        s.time = "18:30".to_string();
        // 基准是当地 12:00，18:30 还没到 → 就是今天
        let next = compute_next_run(&s, BASE_2025_06_15_NOON);
        let (y, m, d, hh, mm, _) = ms_to_ymd_hm(next);
        assert_eq!((y, m, d, hh, mm), (2025, 6, 15, 18, 30));
    }

    #[test]
    fn daily_today_after_time_rolls_to_tomorrow() {
        let mut s = mk(mode::DAILY);
        s.time = "08:00".to_string();
        // 基准是当地 12:00，08:00 已过 → 明天
        let next = compute_next_run(&s, BASE_2025_06_15_NOON);
        let (y, m, d, hh, mm, _) = ms_to_ymd_hm(next);
        assert_eq!((y, m, d, hh, mm), (2025, 6, 16, 8, 0));
    }

    #[test]
    fn daily_invalid_time_returns_zero() {
        let mut s = mk(mode::DAILY);
        s.time = "25:00".to_string();
        assert_eq!(compute_next_run(&s, BASE_2025_06_15_NOON), 0);
    }

    // ---- weekly ----

    #[test]
    fn weekly_same_day_later_time() {
        let mut s = mk(mode::WEEKLY);
        s.time = "22:00".to_string();
        s.weekdays = vec![7]; // 周日；基准日就是周日
        let next = compute_next_run(&s, BASE_2025_06_15_NOON);
        let (y, m, d, hh, mm, wd) = ms_to_ymd_hm(next);
        assert_eq!((y, m, d, hh, mm, wd), (2025, 6, 15, 22, 0, 7));
    }

    #[test]
    fn weekly_finds_nearest_future_day() {
        let mut s = mk(mode::WEEKLY);
        s.time = "09:00".to_string();
        s.weekdays = vec![1, 3]; // 周一、周三；基准日是周日
        let next = compute_next_run(&s, BASE_2025_06_15_NOON);
        let (y, m, d, hh, mm, wd) = ms_to_ymd_hm(next);
        assert_eq!(
            (y, m, d, hh, mm, wd),
            (2025, 6, 16, 9, 0, 1),
            "下一个是周一"
        );
    }

    #[test]
    fn weekly_skips_earlier_time_on_same_day() {
        let mut s = mk(mode::WEEKLY);
        s.time = "09:00".to_string();
        s.weekdays = vec![7]; // 周日，但 09:00 已过（现在 12:00）→ 顺延一周
        let next = compute_next_run(&s, BASE_2025_06_15_NOON);
        let (y, m, d, hh, mm, wd) = ms_to_ymd_hm(next);
        assert_eq!((y, m, d, hh, mm, wd), (2025, 6, 22, 9, 0, 7));
    }

    #[test]
    fn weekly_empty_or_invalid_weekdays_returns_zero() {
        let mut s = mk(mode::WEEKLY);
        s.time = "09:00".to_string();
        s.weekdays = vec![];
        assert_eq!(compute_next_run(&s, BASE_2025_06_15_NOON), 0);
        s.weekdays = vec![0, 8, 99];
        assert_eq!(compute_next_run(&s, BASE_2025_06_15_NOON), 0, "全部越界");
    }

    // ---- once ----

    #[test]
    fn once_future_date() {
        let mut s = mk(mode::ONCE);
        s.date = "2025-12-31".to_string();
        s.time = "23:59".to_string();
        let next = compute_next_run(&s, BASE_2025_06_15_NOON);
        let (y, m, d, hh, mm, _) = ms_to_ymd_hm(next);
        assert_eq!((y, m, d, hh, mm), (2025, 12, 31, 23, 59));
    }

    #[test]
    fn once_past_date_returns_zero() {
        let mut s = mk(mode::ONCE);
        s.date = "2020-01-01".to_string();
        s.time = "10:00".to_string();
        assert_eq!(
            compute_next_run(&s, BASE_2025_06_15_NOON),
            0,
            "已过去就不该再触发"
        );
    }

    #[test]
    fn once_missing_or_bad_date_returns_zero() {
        let mut s = mk(mode::ONCE);
        s.date = String::new();
        assert_eq!(compute_next_run(&s, BASE_2025_06_15_NOON), 0, "没有日期");
        s.date = "2025年12月31日".to_string();
        assert_eq!(
            compute_next_run(&s, BASE_2025_06_15_NOON),
            0,
            "中文日期不接受"
        );
    }

    #[test]
    fn unknown_mode_returns_zero() {
        let s = mk("hourly");
        assert_eq!(compute_next_run(&s, BASE_2025_06_15_NOON), 0);
    }

    // ---- 校验 ----

    #[test]
    fn validate_reports_chinese_errors() {
        let mut s = mk(mode::INTERVAL);
        s.name = "  ".to_string();
        s.preset_id = "p1".to_string();
        assert!(validate(&s).is_err());

        s.name = "备份".to_string();
        s.preset_id = String::new();
        assert!(validate(&s).is_err());

        s.preset_id = "p1".to_string();
        s.interval_minutes = 0;
        assert!(validate(&s).is_err());

        s.interval_minutes = 10;
        assert!(validate(&s).is_ok());

        s.mode = mode::WEEKLY.to_string();
        s.weekdays = vec![];
        assert!(validate(&s).is_err(), "每周模式必须选星期几");
        s.weekdays = vec![1];
        assert!(validate(&s).is_ok());
    }
}

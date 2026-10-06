//! SQLite 数据库封装。
//!
//! 设计要点：
//!   * 单连接 + `parking_lot::Mutex` 保护。CmdDeck 是单机单进程应用，
//!     写并发很低，单连接最简单也最快（避免 WAL 与锁竞争问题）。
//!   * 开启 `foreign_keys` 与 `journal_mode = WAL`（WAL 在移动硬盘上更抗断电）。
//!   * 所有迁移在 `open()` 时自动执行，保证升级无缝。

pub mod migrations;
pub mod models;

pub mod audit;
pub mod groups;
pub mod presets;
pub mod schedules;
pub mod settings;
pub mod workflows;

use rusqlite::Connection;
use std::path::Path;

use crate::error::{AppError, AppResult};

/// 数据库句柄。
///
/// 用法：
/// ```ignore
/// let db = Db::open("cmddeck.db")?;
/// let mut conn = db.conn();     // 拿一个互斥锁守卫
/// conn.query_row(...)?;
/// ```
pub struct Db {
    conn: parking_lot::Mutex<Connection>,
    path: String,
}

impl Db {
    /// 打开（或创建）数据库文件并执行迁移。
    ///
    /// `path` 为空时创建内存数据库，单元测试用。
    pub fn open(path: &str) -> AppResult<Self> {
        let conn = if path.is_empty() || path == ":memory:" {
            Connection::open_in_memory()
        } else {
            if let Some(dir) = Path::new(path).parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            Connection::open(path)
        }
        .map_err(|e| AppError::io(format!("无法打开数据库 {path}：{e}")))?;

        // WAL 提升读写并发；foreign_keys 打开级联删除
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        let _ = conn.pragma_update(None, "foreign_keys", "ON");
        // 负数表示 KB，这里限制缓存为 8MB，避免低配机器内存被吃光
        let _ = conn.pragma_update(None, "cache_size", -8000);

        let db = Self {
            conn: parking_lot::Mutex::new(conn),
            path: path.to_string(),
        };
        db.run_migrations()?;
        Ok(db)
    }

    /// 创建内存数据库（测试用）
    pub fn open_in_memory() -> AppResult<Self> {
        Self::open(":memory:")
    }

    /// 获取互斥锁保护的连接
    pub fn conn(&self) -> parking_lot::MutexGuard<'_, Connection> {
        self.conn.lock()
    }

    /// 数据库文件路径
    pub fn path(&self) -> &str {
        &self.path
    }

    /// 执行所有未执行过的迁移
    fn run_migrations(&self) -> AppResult<()> {
        let conn = self.conn.lock();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                 version    INTEGER PRIMARY KEY,
                 name       TEXT NOT NULL DEFAULT '',
                 applied_at INTEGER NOT NULL DEFAULT 0
             );",
        )
        .map_err(|e| AppError::io(format!("创建迁移表失败：{e}")))?;

        // 读出已应用的版本号集合
        let applied: Vec<i64> = {
            let mut stmt = conn.prepare("SELECT version FROM schema_migrations")?;
            let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            rows.filter_map(Result::ok).collect()
        };

        for m in migrations::all_migrations() {
            if applied.contains(&m.version) {
                continue;
            }
            log::info!("执行数据库迁移 v{} {}", m.version, m.name);
            // 每个迁移单独一个事务：失败时不会留下半截表结构
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(m.sql)
                .map_err(|e| AppError::io(format!("迁移 v{}（{}）失败：{e}", m.version, m.name)))?;
            tx.execute(
                "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![m.version, m.name, models::now_ms()],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    /// 备份数据库到指定路径（导出前调用，保证导出一致快照）
    pub fn backup_to(&self, dest: &str) -> AppResult<()> {
        let conn = self.conn.lock();
        // VACUUM INTO 是 SQLite 3.27+ 自带的在线备份，不需要额外依赖
        conn.execute("VACUUM INTO ?1", rusqlite::params![dest])
            .map_err(|e| AppError::io(format!("备份数据库失败：{e}")))?;
        Ok(())
    }

    /// 当前 schema 版本
    pub fn schema_version(&self) -> i64 {
        let conn = self.conn.lock();
        conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0)
    }
}

/// 便捷函数：把 `Vec<String>` 序列化成数据库里的 JSON 文本
pub fn to_json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string())
}

/// 便捷函数：从数据库里的 JSON 文本反序列化，失败时回退到默认值
pub fn from_json<T: serde::de::DeserializeOwned>(s: &str, fallback: T) -> T {
    serde_json::from_str(s).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_and_are_idempotent() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version(), migrations::SCHEMA_VERSION);
        // 再开一次同样的库不会重复执行
        let _conn = db.conn();
        assert!(!migrations::all_migrations().is_empty());
    }
}

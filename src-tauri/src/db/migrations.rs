//! 数据库表结构与迁移脚本。
//!
//! 迁移策略：脚本按序号递增执行，执行记录写入 `schema_migrations` 表。
//! 这样用户升级新版本时会自动补齐新表 / 新字段，不需要重新配置。

/// 当前 schema 版本号。新增迁移时 +1。
pub const SCHEMA_VERSION: i64 = 5;

/// 单条迁移：`version` 唯一，`sql` 可包含多条语句。
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

/// 全部迁移脚本，按 version 升序。
pub fn all_migrations() -> &'static [Migration] {
    &[
        Migration {
            version: 1,
            name: "初始化核心表",
            sql: r#"
            -- 预设分组
            CREATE TABLE IF NOT EXISTS groups (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                icon        TEXT NOT NULL DEFAULT 'folder',
                color       TEXT NOT NULL DEFAULT '',
                sort_order  INTEGER NOT NULL DEFAULT 0,
                collapsed   INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL DEFAULT 0,
                updated_at  INTEGER NOT NULL DEFAULT 0
            );

            -- 预设指令（核心表）
            CREATE TABLE IF NOT EXISTS presets (
                id              TEXT PRIMARY KEY,
                name            TEXT NOT NULL,
                kind            TEXT NOT NULL DEFAULT 'powershell',
                program         TEXT NOT NULL DEFAULT '',
                args            TEXT NOT NULL DEFAULT '[]',
                working_dir     TEXT NOT NULL DEFAULT '',
                env             TEXT NOT NULL DEFAULT '[]',
                use_shell       INTEGER NOT NULL DEFAULT 1,
                icon            TEXT NOT NULL DEFAULT 'terminal',
                group_id        TEXT NOT NULL DEFAULT '',
                tags            TEXT NOT NULL DEFAULT '[]',
                confirm         INTEGER NOT NULL DEFAULT 0,
                elevated        INTEGER NOT NULL DEFAULT 0,
                danger_level    INTEGER NOT NULL DEFAULT 0,
                notes           TEXT NOT NULL DEFAULT '',
                sort_order      INTEGER NOT NULL DEFAULT 0,
                favorite        INTEGER NOT NULL DEFAULT 0,
                hidden          INTEGER NOT NULL DEFAULT 0,
                shortcut        TEXT NOT NULL DEFAULT '',
                placeholder_args TEXT NOT NULL DEFAULT '[]',
                run_count       INTEGER NOT NULL DEFAULT 0,
                last_run_at     INTEGER NOT NULL DEFAULT 0,
                created_at      INTEGER NOT NULL DEFAULT 0,
                updated_at      INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_presets_group   ON presets(group_id);
            CREATE INDEX IF NOT EXISTS idx_presets_sort    ON presets(sort_order);
            CREATE INDEX IF NOT EXISTS idx_presets_fav     ON presets(favorite);
            CREATE INDEX IF NOT EXISTS idx_presets_recent  ON presets(last_run_at DESC);

            -- 设置：key-value 单行存储
            CREATE TABLE IF NOT EXISTS settings (
                key        TEXT PRIMARY KEY,
                value      TEXT NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT 0
            );

            -- 审计日志
            CREATE TABLE IF NOT EXISTS audit_logs (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                at          INTEGER NOT NULL,
                preset_id   TEXT NOT NULL DEFAULT '',
                preset_name TEXT NOT NULL DEFAULT '',
                command     TEXT NOT NULL DEFAULT '',
                cwd         TEXT NOT NULL DEFAULT '',
                level       INTEGER NOT NULL DEFAULT 0,
                status      TEXT NOT NULL DEFAULT '',
                exit_code   INTEGER,
                message     TEXT NOT NULL DEFAULT '',
                duration_ms INTEGER NOT NULL DEFAULT 0,
                source      TEXT NOT NULL DEFAULT 'manual'
            );

            CREATE INDEX IF NOT EXISTS idx_audit_at ON audit_logs(at DESC);
            CREATE INDEX IF NOT EXISTS idx_audit_p  ON audit_logs(preset_id);
            "#,
        },
        Migration {
            version: 2,
            name: "终端历史记录",
            sql: r#"
            CREATE TABLE IF NOT EXISTS terminal_history (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL DEFAULT '',
                preset_id   TEXT NOT NULL DEFAULT '',
                preset_name TEXT NOT NULL DEFAULT '',
                title       TEXT NOT NULL DEFAULT '',
                command     TEXT NOT NULL DEFAULT '',
                cwd         TEXT NOT NULL DEFAULT '',
                kind        TEXT NOT NULL DEFAULT '',
                started_at  INTEGER NOT NULL DEFAULT 0,
                ended_at    INTEGER NOT NULL DEFAULT 0,
                exit_code   INTEGER,
                output_tail TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_hist_started ON terminal_history(started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_hist_preset  ON terminal_history(preset_id);
            "#,
        },
        Migration {
            version: 3,
            name: "定时任务",
            sql: r#"
            CREATE TABLE IF NOT EXISTS schedules (
                id               TEXT PRIMARY KEY,
                name             TEXT NOT NULL,
                preset_id        TEXT NOT NULL DEFAULT '',
                args             TEXT NOT NULL DEFAULT '{}',
                mode             TEXT NOT NULL DEFAULT 'interval',
                interval_minutes INTEGER NOT NULL DEFAULT 60,
                time             TEXT NOT NULL DEFAULT '09:00',
                weekdays         TEXT NOT NULL DEFAULT '[]',
                date             TEXT NOT NULL DEFAULT '',
                enabled          INTEGER NOT NULL DEFAULT 1,
                next_run_at      INTEGER NOT NULL DEFAULT 0,
                last_run_at      INTEGER NOT NULL DEFAULT 0,
                last_status      TEXT NOT NULL DEFAULT '',
                created_at       INTEGER NOT NULL DEFAULT 0,
                updated_at       INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_sched_next ON schedules(next_run_at);
            "#,
        },
        Migration {
            version: 4,
            name: "工作流",
            sql: r#"
            CREATE TABLE IF NOT EXISTS workflows (
                id                TEXT PRIMARY KEY,
                name              TEXT NOT NULL,
                description       TEXT NOT NULL DEFAULT '',
                run_mode          TEXT NOT NULL DEFAULT 'serial',
                continue_on_error INTEGER NOT NULL DEFAULT 1,
                steps             TEXT NOT NULL DEFAULT '[]',
                enabled           INTEGER NOT NULL DEFAULT 1,
                created_at        INTEGER NOT NULL DEFAULT 0,
                updated_at        INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS workflow_runs (
                id            TEXT PRIMARY KEY,
                workflow_id   TEXT NOT NULL,
                workflow_name TEXT NOT NULL DEFAULT '',
                started_at    INTEGER NOT NULL DEFAULT 0,
                finished_at   INTEGER NOT NULL DEFAULT 0,
                status        TEXT NOT NULL DEFAULT 'running',
                steps         TEXT NOT NULL DEFAULT '[]',
                source        TEXT NOT NULL DEFAULT 'manual'
            );

            CREATE INDEX IF NOT EXISTS idx_wfrun_wf ON workflow_runs(workflow_id, started_at DESC);
            "#,
        },
        Migration {
            version: 5,
            name: "预设运行环境",
            sql: "ALTER TABLE presets ADD COLUMN runtime TEXT NOT NULL DEFAULT '{}';",
        },
    ]
}

/// 首次安装时写入的示例预设（与 `seed.rs` 的程序化种子互为补充；
/// 这里放"最基础的一条"，其余由 seed.rs 生成，方便用户一眼看懂数据长什么样）。
pub const MINIMAL_SEED_PRESET: &str = r#"{"name":"查看系统信息","kind":"powershell","args":["systeminfo"],"notes":"示例预设：查看本机 Windows 版本、补丁、内存等信息"}"#;

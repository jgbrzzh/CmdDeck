//! 全局共享状态。
//!
//! Tauri 的 `State<'_, AppState>` 会把这里的内容以 `Arc` 形式注入每个命令，
//! 因此数据库连接、PTY 会话管理器、设置缓存都集中放在这里。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};

use parking_lot::{Mutex, RwLock};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::models::AppSettings;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::pty::PtyManager;

/// 应用全局状态。
pub struct AppState {
    /// 数据库
    pub db: Db,
    /// 终端会话管理器
    pub pty: PtyManager,
    /// 设置缓存（内存态，DB 为准，这里只做读缓存）
    pub settings: RwLock<AppSettings>,
    /// 数据目录 `%APPDATA%\CmdDeck`
    pub data_dir: PathBuf,
    /// 当前已注册的全局快捷键（空 = 未注册）
    pub shortcut: RwLock<String>,
    /// 调度器线程是否在运行
    pub scheduler_running: AtomicBool,
    pub scheduler_epoch: AtomicU64,
    pub scheduler_control: Mutex<()>,
    /// 窗口是否正在退出（托盘最小化时为 true，阻止真正退出）
    pub exiting: AtomicBool,
    pub restoring: AtomicBool,
    pub integration_warnings: Mutex<Vec<String>>,
    /// 工作流线程池的运行计数（仅用于状态栏展示）
    pub active_workflows: RwLock<Vec<String>>,
}

impl AppState {
    /// 初始化全局状态：建数据目录、开数据库、读设置、创建 PTY 管理器。
    pub fn new(app: &AppHandle) -> AppResult<Self> {
        // ---- 数据目录：优先用 Tauri 的 app_data_dir，失败则退回 %APPDATA% ----
        let data_dir = std::env::var_os("CMDDECK_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                app.path().app_data_dir().unwrap_or_else(|_| {
                    let base = std::env::var("APPDATA")
                        .map(PathBuf::from)
                        .unwrap_or_else(|_| PathBuf::from("."));
                    base.join("CmdDeck")
                })
            });
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| AppError::io(format!("无法创建数据目录 {}：{e}", data_dir.display())))?;

        // ---- 数据库 ----
        let db_path = data_dir.join("cmddeck.db");
        let db = Db::open(&db_path.to_string_lossy())?;

        // ---- 设置 ----
        let mut settings = crate::db::settings::load(&db)?;
        settings.data_dir = data_dir.to_string_lossy().to_string();

        // ---- PTY 管理器 ----
        let pty = PtyManager::new(app.clone());

        Ok(Self {
            db,
            pty,
            settings: RwLock::new(settings),
            data_dir,
            shortcut: RwLock::new(String::new()),
            scheduler_running: AtomicBool::new(false),
            scheduler_epoch: AtomicU64::new(0),
            scheduler_control: Mutex::new(()),
            exiting: AtomicBool::new(false),
            restoring: AtomicBool::new(false),
            integration_warnings: Mutex::new(Vec::new()),
            active_workflows: RwLock::new(Vec::new()),
        })
    }

    /// 读取当前设置（返回副本，避免锁泄漏）
    pub fn settings(&self) -> AppSettings {
        self.settings.read().clone()
    }

    /// 写回设置缓存并落库
    pub fn set_settings(&self, s: &AppSettings) -> AppResult<()> {
        crate::db::settings::save(&self.db, s)?;
        *self.settings.write() = s.clone();
        Ok(())
    }

    /// 数据目录下的某个子目录，不存在则创建
    pub fn sub_dir(&self, name: &str) -> AppResult<PathBuf> {
        let dir = self.data_dir.join(name);
        std::fs::create_dir_all(&dir)
            .map_err(|e| AppError::io(format!("无法创建目录 {}：{e}", dir.display())))?;
        Ok(dir)
    }

    /// 广播事件给前端
    pub fn emit<E: serde::Serialize + Clone>(&self, app: &AppHandle, event: &str, payload: E) {
        if let Err(err) = app.emit(event, payload) {
            log::warn!("发送事件 {event} 失败：{err}");
        }
    }
}

// ============================================================
// 事件名常量（前后端必须一致，前端见 src/api/events.ts）
// ============================================================

/// 终端输出数据：`{ sessionId, data }`
pub const EV_PTY_DATA: &str = "cmddeck://pty-data";
/// 终端退出：`{ sessionId, exitCode, status }`
pub const EV_PTY_EXIT: &str = "cmddeck://pty-exit";
/// 新终端会话已创建：`TerminalInfo`
pub const EV_PTY_OPEN: &str = "cmddeck://pty-open";
/// 定时任务触发：`{ scheduleId, name, presetId, presetName, source, sessionId }`
pub const EV_SCHEDULE_FIRED: &str = "cmddeck://schedule-fired";
/// 工作流状态更新：`WorkflowRun`
pub const EV_WORKFLOW_UPDATE: &str = "cmddeck://workflow-update";
/// 请求打开快速启动面板（托盘 / 全局快捷键触发）
pub const EV_QUICK_LAUNCH: &str = "cmddeck://quick-launch";
/// 请求前端切换视图：`"presets" | "terminals" | "settings" | "audit"`
pub const EV_NAVIGATE: &str = "cmddeck://navigate";
/// 设置已变更：`AppSettings`
pub const EV_SETTINGS_CHANGED: &str = "cmddeck://settings-changed";
/// 预设数据已变更（导入 / 外部修改），前端需刷新列表：`{ reason }`
pub const EV_PRESET_CHANGED: &str = "cmddeck://preset-changed";

/// 调度器线程默认轮询间隔（秒）
pub const SCHEDULER_TICK_SECS: u64 = 20;

/// 供批量/工作流使用的参数类型别名
pub type ArgsMap = HashMap<String, String>;

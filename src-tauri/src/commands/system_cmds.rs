//! 设置、系统集成、导入导出相关的 Tauri 命令。
//!
//! 对应前端 `systemApi`（见 `src/api/index.ts`）。
//!
//! 这一层是"用户能直接感知到出错"的地方，所以：
//! * 每一处失败都翻译成中文，告诉用户**哪里错了、怎么改**；
//! * 涉及系统副作用的操作（注册快捷键、开机自启、打开文件夹）失败时**不写库**，
//!   避免出现"设置里写着已开启，系统里其实没开"这种鬼故事。

use std::path::{Path, PathBuf};

use rusqlite::params;
use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt as AutoStartExt;
use tauri_plugin_opener::OpenerExt;

use crate::db::models::{
    now_ms, AppInfo, AppSettings, ExportBundle, ImportMode, ImportReport, Schedule, ShellOption,
    Workflow,
};
use crate::db::{groups, presets, schedules, to_json, workflows};
use crate::error::{AppError, AppResult};
use crate::state::{AppState, EV_PRESET_CHANGED, EV_SETTINGS_CHANGED};

/// 应用版本号。
///
/// 与 `Cargo.toml` 的 `version` 同源，Tauri 配置里也是这个值。
/// `export_data` 没有 `AppHandle` 参数，所以这里用编译期常量而不是 `package_info()`。
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ============================================================
// 关于 / 设置
// ============================================================

/// 应用信息（「关于」弹窗用）
#[tauri::command]
pub fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    let info = app.package_info();
    let settings = state.settings();
    Ok(AppInfo {
        version: info.version.to_string(),
        name: info.name.to_string(),
        tauri_version: "2".to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        data_dir: state.data_dir.to_string_lossy().to_string(),
        db_path: state.db.path().to_string(),
        first_run: !settings.first_run_done,
        user_name: std::env::var("USERNAME").unwrap_or_else(|_| "当前用户".to_string()),
    })
}

/// 读取当前设置
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    Ok(state.settings())
}

/// 校验设置，返回中文错误（`Ok(())` 表示通过）
pub(crate) fn validate_settings(s: &AppSettings) -> AppResult<()> {
    if !(8..=48).contains(&s.font_size) {
        return Err(AppError::validation(format!(
            "终端字号必须在 8 到 48 之间，当前是 {}",
            s.font_size
        )));
    }
    if !(70..=200).contains(&s.ui_scale) {
        return Err(AppError::validation(format!(
            "界面缩放必须在 70% 到 200% 之间，当前是 {}%",
            s.ui_scale
        )));
    }
    if !(1..=64).contains(&s.max_concurrent_sessions) {
        return Err(AppError::validation(format!(
            "同时运行的终端数量必须在 1 到 64 之间，当前是 {}",
            s.max_concurrent_sessions
        )));
    }
    if !(50..=10_000).contains(&s.history_limit) {
        return Err(AppError::validation(format!(
            "历史记录条数必须在 50 到 10000 之间，当前是 {}",
            s.history_limit
        )));
    }
    if s.blacklist.len() > 200 {
        return Err(AppError::validation("黑名单条目过多（最多 200 条）"));
    }
    for raw in &s.blacklist {
        let text = raw.trim();
        if text.chars().count() > 200 {
            return Err(AppError::validation(format!(
                "黑名单条目过长（最多 200 个字符）：{text}"
            )));
        }
    }
    Ok(())
}

/// 保存设置。
///
/// 会顺带处理两件"有系统副作用"的事：
/// * `global_hotkey` 变了 → 重新注册全局快捷键（注册失败就不落库）
/// * `autostart` 变了 → 同步 Windows 开机启动开关
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    crate::backups::snapshot(&state)?;
    let mut s = settings;
    validate_settings(&s)?;

    // 清掉黑名单里的空行（用户在输入框里手滑多敲的空白）
    s.blacklist = s
        .blacklist
        .iter()
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect();
    // 数据目录由后端说了算，不接受前端传值
    s.data_dir = state.data_dir.to_string_lossy().to_string();

    let old = state.settings();

    // 先做有副作用的操作，失败就整个放弃——宁可让用户重试一次，
    // 也不能出现"设置里写着已注册、系统里其实没注册"
    if s.global_hotkey.trim() != old.global_hotkey.trim() {
        crate::apply_global_hotkey(&app, &state, &s.global_hotkey)?;
    }
    if s.autostart != old.autostart {
        if let Err(err) = apply_autostart(&app, s.autostart) {
            let _ = apply_autostart(&app, old.autostart);
            if s.global_hotkey.trim() != old.global_hotkey.trim() {
                let _ = crate::apply_global_hotkey(&app, &state, &old.global_hotkey);
            }
            return Err(err);
        }
    }

    if let Err(err) = state.set_settings(&s) {
        if s.autostart != old.autostart {
            let _ = apply_autostart(&app, old.autostart);
        }
        if s.global_hotkey.trim() != old.global_hotkey.trim() {
            let _ = crate::apply_global_hotkey(&app, &state, &old.global_hotkey);
        }
        return Err(err);
    }
    state.emit(&app, EV_SETTINGS_CHANGED, s.clone());
    Ok(s)
}

/// 恢复默认设置
#[tauri::command]
pub fn reset_settings(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppSettings> {
    crate::backups::snapshot(&state)?;
    let mut s = AppSettings::default();
    s.data_dir = state.data_dir.to_string_lossy().to_string();

    apply_autostart(&app, s.autostart)?;
    if let Err(e) = crate::apply_global_hotkey(&app, &state, &s.global_hotkey) {
        // 默认快捷键注册不上不应该卡住"恢复默认"，记个日志继续
        log::warn!("恢复默认设置时注册全局快捷键失败：{e}");
    }

    state.set_settings(&s)?;
    state.emit(&app, EV_SETTINGS_CHANGED, s.clone());
    Ok(s)
}

// ============================================================
// 打开路径
// ============================================================

/// 打开数据目录（资源管理器）
#[tauri::command]
pub fn open_data_dir(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let dir = state.data_dir.to_string_lossy().to_string();
    open_path_impl(&app, &dir)
}

/// 只打开固定项目主页，不接受任意地址或外部命令。
#[tauri::command]
pub fn open_project_repository(app: AppHandle) -> AppResult<()> {
    app.opener()
        .open_url("https://github.com/jgbrzzh/CmdDeck", None::<&str>)
        .map_err(|e| AppError::exec(format!("无法打开项目主页：{e}")))
}

/// 用系统默认程序打开文件或目录
#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> AppResult<()> {
    open_path_impl(&app, &path)
}

/// 在资源管理器中定位（选中）文件
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> AppResult<()> {
    let p = path.trim();
    if p.is_empty() {
        return Err(AppError::validation("路径不能为空"));
    }
    app.opener()
        .reveal_item_in_dir(p)
        .map_err(|e| AppError::exec(format!("无法在资源管理器中定位 {p}：{e}")))
}

/// 打开路径的公共实现
fn open_path_impl(app: &AppHandle, path: &str) -> AppResult<()> {
    let p = path.trim();
    if p.is_empty() {
        return Err(AppError::validation("路径不能为空"));
    }
    if !Path::new(p).exists() {
        return Err(AppError::not_found(format!("路径不存在：{p}")));
    }
    #[cfg(windows)]
    if Path::new(p).is_dir() {
        // 显式打开文件夹，避开 ShellExecute 的目录关联和 COM 定位失败弹窗。
        let windows =
            std::env::var_os("WINDIR").ok_or_else(|| AppError::exec("无法找到 Windows 目录"))?;
        std::process::Command::new(PathBuf::from(windows).join("explorer.exe"))
            .arg(p)
            .spawn()
            .map_err(|e| AppError::exec(format!("无法打开资源管理器 {p}：{e}")))?;
        return Ok(());
    }
    app.opener()
        .open_path(p, None::<&str>)
        .map_err(|e| AppError::exec(format!("无法打开 {p}：{e}")))
}

// ============================================================
// 导入 / 导出
// ============================================================

/// 导出全部配置为 JSON 文件，返回导出的预设数量。
#[tauri::command]
pub fn export_data(state: State<'_, AppState>, path: String) -> AppResult<i32> {
    let mut target = path.trim().to_string();
    if target.is_empty() {
        return Err(AppError::validation("导出路径不能为空"));
    }
    // 用户在保存框里习惯不带后缀，这里替他补上
    if !target.to_ascii_lowercase().ends_with(".json") {
        target.push_str(".json");
    }
    if let Some(parent) = Path::new(&target).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::io(format!("无法创建目录 {}：{e}", parent.display())))?;
        }
    }

    let mut settings = state.settings();
    settings.data_dir = state.data_dir.to_string_lossy().to_string();

    let preset_list = presets::list(
        &state.db,
        &crate::db::models::PresetFilter {
            include_hidden: true,
            ..Default::default()
        },
    )?;

    let bundle = ExportBundle {
        productivity: crate::productivity::load(&state.db)?,
        version: "1".to_string(),
        exported_at: now_ms(),
        app_version: app_version(),
        settings,
        groups: groups::list(&state.db)?,
        presets: preset_list.clone(),
        workflows: workflows::list(&state.db)?,
        schedules: schedules::list(&state.db)?,
    };

    let text = serde_json::to_string_pretty(&bundle)
        .map_err(|e| AppError::io(format!("序列化导出内容失败：{e}")))?;
    std::fs::write(&target, text)
        .map_err(|e| AppError::io(format!("写入文件 {target} 失败：{e}")))?;

    log::info!("已导出 {} 条预设到 {}", preset_list.len(), target);
    Ok(preset_list.len() as i32)
}

/// 导入 JSON 配置。
///
/// `mode` 取值：
/// * `replace` 覆盖：先清空现有数据，再整体导入（含设置）
/// * `merge`  合并：同 ID 以导入文件为准并刷新 `updated_at`，设置保持不变
/// * `append` 追加：只插入库里没有的 ID
#[tauri::command]
pub fn import_data(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    mode: String,
) -> AppResult<ImportReport> {
    let mode = match mode.trim().to_ascii_lowercase().as_str() {
        "replace" => ImportMode::Replace,
        "merge" => ImportMode::Merge,
        "append" => ImportMode::Append,
        other => {
            return Err(AppError::validation(format!(
                "导入模式无效（{other}），只能是 replace（覆盖）、merge（合并）或 append（追加）"
            )))
        }
    };

    let text = std::fs::read_to_string(&path)
        .map_err(|e| AppError::io(format!("无法读取文件 {path}：{e}")))?;

    // 标准格式优先；实在解析不了，再试一次"纯预设数组"（用户手写 / 从别处复制的情况）
    let bundle: ExportBundle = match serde_json::from_str::<ExportBundle>(&text) {
        Ok(b) => b,
        Err(first) => match serde_json::from_str::<Vec<crate::db::models::Preset>>(&text) {
            Ok(list) => ExportBundle {
                productivity: crate::productivity::Productivity::default(),
                version: "1".to_string(),
                exported_at: now_ms(),
                app_version: app_version(),
                settings: state.settings(),
                groups: Vec::new(),
                presets: list,
                workflows: Vec::new(),
                schedules: Vec::new(),
            },
            Err(_) => {
                return Err(AppError::validation(format!(
                    "{path} 不是有效的 CmdDeck 配置文件：{first}"
                )))
            }
        },
    };

    crate::productivity::validate(&bundle.productivity)?;
    validate_settings(&bundle.settings)?;
    crate::backups::snapshot(&state)?;
    let mut report = ImportReport {
        groups_added: 0,
        groups_updated: 0,
        presets_added: 0,
        presets_updated: 0,
        workflows_added: 0,
        schedules_added: 0,
        warnings: Vec::new(),
        settings_imported: false,
    };

    if mode == ImportMode::Replace {
        presets::clear(&state.db)?;
        groups::clear(&state.db)?;
        clear_automation(&state.db)?;
    }

    // ---- 分组必须先于预设导入，否则预设挂不到分组上 ----
    let mut known_groups: Vec<String> =
        groups::list(&state.db)?.into_iter().map(|g| g.id).collect();

    for mut g in bundle.groups {
        if g.name.trim().is_empty() {
            report
                .warnings
                .push(format!("分组「{}」没有名称，已跳过", g.id));
            continue;
        }
        if g.id.trim().is_empty() {
            g.id = crate::db::models::new_id();
        }
        let existed = groups::exists(&state.db, &g.id)?;
        if mode == ImportMode::Append && existed {
            continue;
        }
        groups::save(&state.db, &g)?;
        if existed {
            report.groups_updated += 1;
        } else {
            report.groups_added += 1;
        }
        if !known_groups.contains(&g.id) {
            known_groups.push(g.id);
        }
    }

    // ---- 预设 ----
    for mut p in bundle.presets {
        if p.name.trim().is_empty() {
            report
                .warnings
                .push(format!("有一条预设没有名称（ID：{}），已跳过", p.id));
            continue;
        }
        if p.id.trim().is_empty() {
            p.id = crate::db::models::new_id();
        }
        // 指向不存在分组的预设：降级成"未分组"，别让左侧导航出现幽灵分组
        if !p.group_id.trim().is_empty() && !known_groups.contains(&p.group_id) {
            report
                .warnings
                .push(format!("预设「{}」所属分组不存在，已放入未分组", p.name));
            p.group_id = String::new();
        }
        let existed = presets::exists(&state.db, &p.id)?;
        if mode == ImportMode::Append && existed {
            continue;
        }
        if mode == ImportMode::Merge && existed {
            p.updated_at = now_ms();
        }
        presets::save(&state.db, &p)?;
        if existed {
            report.presets_updated += 1;
        } else {
            report.presets_added += 1;
        }
    }

    // ---- 工作流与定时任务（表结构由 backend-automation 定义，这里只做数据搬运）----
    {
        let conn = state.db.conn();
        for w in bundle.workflows {
            if w.name.trim().is_empty() || w.id.trim().is_empty() {
                report
                    .warnings
                    .push("有一条工作流缺少名称或 ID，已跳过".to_string());
                continue;
            }
            let existed: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM workflows WHERE id = ?1",
                    params![w.id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if mode == ImportMode::Append && existed > 0 {
                continue;
            }
            upsert_workflow(&conn, &w)?;
            if existed == 0 {
                report.workflows_added += 1;
            }
        }
        for s in bundle.schedules {
            if s.name.trim().is_empty() || s.id.trim().is_empty() {
                report
                    .warnings
                    .push("有一条定时任务缺少名称或 ID，已跳过".to_string());
                continue;
            }
            let existed: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM schedules WHERE id = ?1",
                    params![s.id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if mode == ImportMode::Append && existed > 0 {
                continue;
            }
            upsert_schedule(&conn, &s)?;
            if existed == 0 {
                report.schedules_added += 1;
            }
        }
    }

    // ---- 设置：只有"覆盖"模式才动，免得合并几���预设把用户的外观设置也覆盖掉 ----
    if mode == ImportMode::Replace {
        crate::productivity::save(&state.db, &bundle.productivity)?;
        let mut incoming = bundle.settings;
        validate_settings(&incoming)?;
        incoming.data_dir = state.data_dir.to_string_lossy().to_string();
        // 引导页只该走一次：导入旧配置时如果带回 first_run_done=false，
        // 下次启动又会弹出欢迎流程
        incoming.first_run_done = true;
        state.set_settings(&incoming)?;
        report.settings_imported = true;
    } else {
        report
            .warnings
            .push("合并/追加模式不会导入设置，当前设置已保留".to_string());
    }

    state.emit(&app, EV_PRESET_CHANGED, json!({ "reason": "imported" }));
    state.emit(&app, EV_SETTINGS_CHANGED, state.settings());
    Ok(report)
}

/// 清空工作流与定时任务（导入"覆盖"模式用）
fn clear_automation(db: &crate::db::Db) -> AppResult<()> {
    let conn = db.conn();
    conn.execute("DELETE FROM workflow_runs", [])?;
    conn.execute("DELETE FROM workflows", [])?;
    conn.execute("DELETE FROM schedules", [])?;
    Ok(())
}

/// 写一条工作流（存在则整行覆盖）
fn upsert_workflow(conn: &rusqlite::Connection, w: &Workflow) -> AppResult<()> {
    conn.execute(
        "INSERT INTO workflows (id, name, description, run_mode, continue_on_error, steps, \
         enabled, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) \
         ON CONFLICT(id) DO UPDATE SET \
           name=excluded.name, description=excluded.description, run_mode=excluded.run_mode, \
           continue_on_error=excluded.continue_on_error, steps=excluded.steps, \
           enabled=excluded.enabled, updated_at=excluded.updated_at",
        params![
            w.id,
            w.name,
            w.description,
            w.run_mode,
            if w.continue_on_error { 1i64 } else { 0i64 },
            to_json(&w.steps),
            if w.enabled { 1i64 } else { 0i64 },
            w.created_at,
            now_ms(),
        ],
    )?;
    Ok(())
}

/// 写一条定时任务（存在则整行覆盖）
fn upsert_schedule(conn: &rusqlite::Connection, s: &Schedule) -> AppResult<()> {
    conn.execute(
        "INSERT INTO schedules (id, name, preset_id, args, mode, interval_minutes, time, \
         weekdays, date, enabled, next_run_at, last_run_at, last_status, created_at, updated_at) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15) \
         ON CONFLICT(id) DO UPDATE SET \
           name=excluded.name, preset_id=excluded.preset_id, args=excluded.args, \
           mode=excluded.mode, interval_minutes=excluded.interval_minutes, time=excluded.time, \
           weekdays=excluded.weekdays, date=excluded.date, enabled=excluded.enabled, \
           last_run_at=excluded.last_run_at, last_status=excluded.last_status, \
           updated_at=excluded.updated_at",
        params![
            s.id,
            s.name,
            s.preset_id,
            to_json(&s.args),
            s.mode,
            s.interval_minutes,
            s.time,
            to_json(&s.weekdays),
            s.date,
            if s.enabled { 1i64 } else { 0i64 },
            s.next_run_at,
            s.last_run_at,
            s.last_status,
            s.created_at,
            now_ms(),
        ],
    )?;
    Ok(())
}

// ============================================================
// 系统集成
// ============================================================

/// 返回操作系统和运行时的真实状态，而不是仅显示保存的偏好。
#[tauri::command]
pub fn get_integration_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<serde_json::Value> {
    let autostart = app
        .autolaunch()
        .is_enabled()
        .map_err(|e| AppError::other(e.to_string()))?;
    Ok(
        json!({"autostart":autostart,"tray":app.tray_by_id("cmddeck-tray").is_some(),"scheduler":state.scheduler_running.load(std::sync::atomic::Ordering::SeqCst)}),
    )
}

/// 设置全局快捷键（快速启动面板）。成功后才写进设置。
#[tauri::command]
pub fn set_global_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    accelerator: String,
) -> AppResult<()> {
    crate::apply_global_hotkey(&app, &state, &accelerator)?;

    let mut s = state.settings();
    s.global_hotkey = accelerator.trim().to_string();
    state.set_settings(&s)?;
    state.emit(&app, EV_SETTINGS_CHANGED, s);
    Ok(())
}

/// 开关"开机自启动"。
///
/// 形参里没有 `State`，但仍然从 `AppHandle` 里取全局状态把设置落库——
/// 否则用户在这里打开的自启动会在下次启动时被 `lib.rs` 里的对齐逻辑改回去。
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> AppResult<()> {
    let state = app.state::<AppState>();
    let mut s = state.settings();
    if let Err(err) = apply_autostart(&app, enabled) {
        let _ = apply_autostart(&app, s.autostart);
        return Err(err);
    }
    let old = s.autostart;
    s.autostart = enabled;
    if let Err(err) = state.set_settings(&s) {
        let _ = apply_autostart(&app, old);
        return Err(err);
    }
    state.emit(&app, EV_SETTINGS_CHANGED, s);
    Ok(())
}

/// 同步 Windows 开机启动开关
fn apply_autostart(app: &AppHandle, enabled: bool) -> AppResult<()> {
    let mgr = app.autolaunch();
    let res = if enabled { mgr.enable() } else { mgr.disable() };
    res.map_err(|e| AppError::other(format!("设置开机自启动失败：{e}")))
}

/// 探测本机可用的 Shell。
///
/// 顺序固定为 PowerShell → CMD → PowerShell 7 → Git Bash → WSL，
/// 因为这是国内用户的使用频率排序，前端下拉框直接按这个顺序渲染。
/// 找不到的也返回（`available = false`），让用户知道"装了但没在 PATH 里"。
#[tauri::command]
pub fn get_system_shells() -> AppResult<Vec<ShellOption>> {
    let win = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let out = vec![
        ShellOption {
            id: "powershell".to_string(),
            label: "Windows PowerShell 5.1".to_string(),
            path: first_existing(&[
                PathBuf::from(&win).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
                PathBuf::from(&win).join(r"SysWOW64\WindowsPowerShell\v1.0\powershell.exe"),
            ])
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
            args: Vec::new(),
            available: false,
        },
        ShellOption {
            id: "cmd".to_string(),
            label: "命令提示符 (CMD)".to_string(),
            path: std::env::var("COMSPEC")
                .ok()
                .filter(|p| Path::new(p).exists())
                .or_else(|| {
                    PathBuf::from(&win)
                        .join("System32")
                        .join("cmd.exe")
                        .exists()
                        .then(|| {
                            PathBuf::from(&win)
                                .join("System32")
                                .join("cmd.exe")
                                .to_string_lossy()
                                .to_string()
                        })
                })
                .unwrap_or_default(),
            args: Vec::new(),
            available: false,
        },
        ShellOption {
            id: "pwsh".to_string(),
            label: "PowerShell 7+ (pwsh)".to_string(),
            path: first_existing(&[
                PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe"),
                PathBuf::from(r"C:\Program Files\PowerShell\7-preview\pwsh.exe"),
            ])
            .or_else(|| search_in_path("pwsh.exe"))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
            args: Vec::new(),
            available: false,
        },
        ShellOption {
            id: "gitbash".to_string(),
            label: "Git Bash".to_string(),
            path: first_existing(&[
                PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"),
                PathBuf::from(r"C:\Program Files (x86)\Git\bin\bash.exe"),
                PathBuf::from(r"C:\Program Files\Git\usr\bin\bash.exe"),
            ])
            .or_else(|| search_in_path("bash.exe"))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
            args: vec!["-i".to_string()],
            available: false,
        },
        ShellOption {
            id: "wsl".to_string(),
            label: "WSL �� Linux".to_string(),
            path: PathBuf::from(&win)
                .join("System32")
                .join("wsl.exe")
                .exists()
                .then(|| {
                    PathBuf::from(&win)
                        .join("System32")
                        .join("wsl.exe")
                        .to_string_lossy()
                        .to_string()
                })
                .unwrap_or_default(),
            args: Vec::new(),
            available: false,
        },
    ];

    // available = 路径存在（路径为空的一律算不可用）
    Ok(out
        .into_iter()
        .map(|mut s| {
            s.available = !s.path.is_empty() && Path::new(&s.path).exists();
            s
        })
        .collect())
}

/// 返回第一个真实存在的路径
fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.exists()).cloned()
}

/// 在 `PATH` 环境变量里找可执行文件
fn search_in_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|p| p.exists())
}

/// 当前进程是否以管理员身份运行。
///
/// 主判据是 `net session`——这个命令需要「管理远程服务」权限，普通用户会直接报错。
/// 万一 `net` 被系统精简掉了，再用注册表里的服务账户 SID 兜底；两条路都不通就当普通用户。
#[tauri::command]
pub fn is_elevated() -> AppResult<bool> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        };
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(AppError::permission("无法读取当前用户权限"));
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut length = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        );
        CloseHandle(token);
        if ok == 0 {
            return Err(AppError::permission("无法读取管理员权限状态"));
        }
        Ok(elevation.TokenIsElevated != 0)
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

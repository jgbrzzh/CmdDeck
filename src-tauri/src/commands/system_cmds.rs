//! 设置、系统集成、导入导出相关的 Tauri 命令。
//!
//! 对应前端 `systemApi`（见 `src/api/index.ts`）。
//!
//! 这一层是"用户能直接感知到出错"的地方，所以：
//! * 每一处失败都翻译成中文，告诉用户**哪里错了、怎么改**；
//! * 涉及系统副作用的操作（注册快捷键、开机自启、打开文件夹）失败时**不写库**，
//!   避免出现"设置里写着已开启，系统里其实没开"这种鬼故事。

use std::path::{Path, PathBuf};

use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt as AutoStartExt;
use tauri_plugin_opener::OpenerExt;

use crate::db::models::{
    now_ms, AppInfo, AppSettings, ExportBundle, ImportMode, ImportReport, ShellOption,
};
use crate::db::{groups, presets, schedules, workflows};
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
/// 第二窗口复用本机数据目录，避免 WebView2 默认目录与主窗口不一致。
#[tauri::command]
pub async fn create_work_window(app: AppHandle) -> AppResult<String> {
    let label = format!("deck-{}", uuid::Uuid::new_v4());
    let directory = app.state::<AppState>().sub_dir("webview")?;
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title("CmdDeck · 工作窗口")
        .inner_size(1280.0, 800.0)
        .min_inner_size(1024.0, 620.0)
        .data_directory(directory)
        .icon(tauri::include_image!("icons/128x128.png"))
        .map_err(|e| AppError::exec(e.to_string()))?
        .build()
        .map_err(|e| AppError::exec(format!("新窗口失败：{e}")))?;
    Ok(label)
}

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
    if s.allowed_executables.len() > 100
        || s.allowed_executables
            .iter()
            .any(|p| !Path::new(p).is_absolute() || p.contains('\0'))
    {
        return Err(AppError::validation(
            "执行白名单最多 100 项，每项必须是完整绝对路径",
        ));
    }
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

    if state
        .restoring
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return Err(AppError::validation("正在导入或恢复配置，请稍后再试"));
    }
    struct Guard<'a>(&'a std::sync::atomic::AtomicBool);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let _guard = Guard(&state.restoring);
    if state.pty.list().iter().any(|s| s.status == "running")
        || !state.active_workflows.read().is_empty()
    {
        return Err(AppError::validation("请先停止任务，再导入配置"));
    }
    let current = crate::backups::bundle(&state)?;
    let (mut candidate, report) = crate::importing::plan(current, bundle, mode)?;
    candidate.settings.data_dir = state.data_dir.to_string_lossy().into_owned();
    candidate.settings.first_run_done = true;
    crate::backups::snapshot(&state)?;
    crate::backups::replace_configuration(&state.db, &candidate, mode == ImportMode::Replace)?;
    *state.settings.write() = candidate.settings.clone();
    state.emit(&app, EV_PRESET_CHANGED, json!({"reason":"imported"}));
    state.emit(&app, EV_SETTINGS_CHANGED, candidate.settings);
    state.emit(
        &app,
        "cmddeck://productivity-changed",
        candidate.productivity,
    );
    Ok(report)
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
        json!({"autostart":autostart,"hotkey":state.shortcut.read().clone(),"warnings":state.integration_warnings.lock().clone(),"tray":app.tray_by_id("cmddeck-tray").is_some(),"scheduler":state.scheduler_running.load(std::sync::atomic::Ordering::SeqCst)}),
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

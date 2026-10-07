//! CmdDeck 应用入口：装配插件、注册命令、初始化托盘与全局快捷键。

pub mod backups;
pub mod commands;
pub mod db;
#[cfg(windows)]
pub mod elevated;
pub mod environments;
pub mod error;
pub mod importing;
pub mod monitor;
pub mod notifications;
pub mod productivity;
pub mod pty;
pub mod security;
pub mod seed;
pub mod state;
pub mod tray;

use std::str::FromStr;
use std::sync::atomic::Ordering;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::error::AppResult;
use crate::state::AppState;

/// 应用启动
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // ---- 单实例：重复启动时激活已有窗口 ----
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        // ---- 全局快捷键（只注册一个：快速启动面板）----
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let state = app.state::<AppState>();
                    let registered = state.shortcut.read().clone();
                    if registered.is_empty() {
                        return;
                    }
                    // 只处理与当前设置一致的快捷键
                    let hit = Shortcut::from_str(&registered)
                        .map(|s| s == *shortcut)
                        .unwrap_or(false);
                    if hit {
                        crate::tray::toggle_quick_launch(app);
                    }
                })
                .build(),
        )
        // ---- 托盘图标 + 窗口关闭行为 ----
        .setup(|app| {
            eprintln!("[CmdDeck] 初始化数据库与全局状态");
            // 全局状态必须最先初始化：托盘菜单与快捷键都要读设置
            let state = AppState::new(app.handle())?;
            eprintln!("[CmdDeck] 数据目录：{}", state.data_dir.display());
            let webview_dir = state.data_dir.join("webview");
            std::fs::create_dir_all(&webview_dir)?;
            app.manage(state);
            eprintln!("[CmdDeck] 创建 WebView2 主窗口");
            tauri::WebviewWindowBuilder::from_config(app.handle(), &app.config().app.windows[0])?
                .icon(tauri::include_image!("icons/128x128.png"))?
                .data_directory(webview_dir)
                .build()?;
            eprintln!("[CmdDeck] WebView2 创建完成");

            // 托盘
            eprintln!("[CmdDeck] 创建托盘");
            tray::build(app.handle())?;
            eprintln!("[CmdDeck] 托盘创建完成");

            // 开机自启动开关要与系统实际状态对齐
            {
                let st = app.state::<AppState>();
                let want = st.settings().autostart;
                let mgr = app.autolaunch();
                let enabled = mgr.is_enabled().unwrap_or(false);
                if want != enabled {
                    if let Err(e) = if want { mgr.enable() } else { mgr.disable() } {
                        st.integration_warnings
                            .lock()
                            .push(format!("恢复开机启动设置失败：{e}；请在设置中重试"));
                    }
                }
            }

            // 自启动时隐藏主窗口
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            // 首次运行写入示例数据
            {
                let st = app.state::<AppState>();
                if !st.settings().first_run_done {
                    let _ = crate::seed::seed_default_data(app.handle());
                }
            }

            // 启动定时任务调度器 + 注册全局快捷键
            crate::commands::schedule_cmds::start_scheduler_internal(app.handle());
            crate::bootstrap_hotkey(app.handle());

            Ok(())
        })
        // ---- 注册全部前端命令 ----
        .invoke_handler(tauri::generate_handler![
            commands::productivity_cmds::get_productivity,
            commands::productivity_cmds::save_productivity,
            commands::productivity_cmds::save_terminal_layout,
            commands::productivity_cmds::preflight_preset,
            commands::productivity_cmds::list_task_metrics,
            commands::productivity_cmds::list_listening_ports,
            commands::productivity_cmds::open_local_service,
            commands::productivity_cmds::list_config_backups,
            commands::productivity_cmds::create_config_backup,
            commands::productivity_cmds::restore_config_backup,
            commands::productivity_cmds::preview_config_export,
            commands::productivity_cmds::export_shared_config,
            commands::productivity_cmds::get_history_output,
            commands::productivity_cmds::list_log_entries,
            commands::productivity_cmds::export_task_log,
            commands::productivity_cmds::test_task_notification,
            commands::productivity_cmds::check_for_updates,
            commands::productivity_cmds::open_releases,
            commands::environment_cmds::discover_environments,
            commands::environment_cmds::open_environment_terminal,
            commands::environment_cmds::run_environment_action,
            // 预设
            commands::preset_cmds::list_presets,
            commands::preset_cmds::get_preset,
            commands::preset_cmds::save_preset,
            commands::preset_cmds::delete_preset,
            commands::preset_cmds::delete_presets,
            commands::preset_cmds::duplicate_preset,
            commands::preset_cmds::reorder_presets,
            commands::preset_cmds::toggle_preset_favorite,
            commands::preset_cmds::record_preset_run,
            commands::preset_cmds::move_preset,
            commands::preset_cmds::scan_placeholders,
            commands::preset_cmds::preview_command,
            // 分组
            commands::group_cmds::list_groups,
            commands::group_cmds::save_group,
            commands::group_cmds::delete_group,
            // 终端
            commands::terminal_cmds::spawn_terminal,
            commands::terminal_cmds::open_shell,
            commands::terminal_cmds::run_preset,
            commands::terminal_cmds::write_terminal,
            commands::terminal_cmds::resize_terminal,
            commands::terminal_cmds::kill_terminal,
            commands::terminal_cmds::close_terminal,
            commands::terminal_cmds::get_terminal_snapshot,
            commands::terminal_cmds::list_terminal_sessions,
            commands::terminal_cmds::wait_terminal_exit,
            // 批量
            commands::batch_cmds::run_batch,
            // 定时
            commands::schedule_cmds::list_schedules,
            commands::schedule_cmds::save_schedule,
            commands::schedule_cmds::delete_schedule,
            commands::schedule_cmds::set_schedule_enabled,
            commands::schedule_cmds::start_scheduler,
            commands::schedule_cmds::stop_scheduler,
            commands::schedule_cmds::trigger_schedule_now,
            // 工作流
            commands::workflow_cmds::list_workflows,
            commands::workflow_cmds::save_workflow,
            commands::workflow_cmds::delete_workflow,
            commands::workflow_cmds::start_workflow,
            commands::workflow_cmds::list_workflow_runs,
            commands::workflow_cmds::cancel_workflow,
            // 设置 / 系统
            commands::system_cmds::get_app_info,
            commands::system_cmds::create_work_window,
            commands::system_cmds::get_settings,
            commands::system_cmds::save_settings,
            commands::system_cmds::reset_settings,
            commands::system_cmds::open_data_dir,
            commands::system_cmds::open_path,
            commands::system_cmds::open_project_repository,
            commands::system_cmds::reveal_path,
            commands::system_cmds::export_data,
            commands::system_cmds::import_data,
            commands::system_cmds::set_global_hotkey,
            commands::system_cmds::set_autostart,
            commands::system_cmds::get_integration_status,
            commands::system_cmds::get_system_shells,
            commands::system_cmds::is_elevated,
            commands::seed_cmds::seed_default_data,
            // 安全
            commands::security_cmds::check_command,
            commands::security_cmds::get_blacklist,
            commands::security_cmds::add_blacklist,
            commands::security_cmds::remove_blacklist,
            // 审计与历史
            commands::audit_cmds::list_audit_logs,
            commands::audit_cmds::clear_audit_logs,
            commands::audit_cmds::list_terminal_history,
            commands::audit_cmds::clear_terminal_history,
        ])
        // ---- 关闭主窗口时最小化到托盘 ----
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let minimize = state.settings().minimize_to_tray;
                if window.label() == "main" && minimize && !state.exiting.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                } else if window.label() == "main" {
                    state.exiting.store(true, Ordering::SeqCst);
                    state.pty.kill_all();
                    app.exit(0);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("CmdDeck 启动失败，请检查 tauri.conf.json 配置");
}

/// 全局快捷键的注册 / 注销。供 `commands::system_cmds::set_global_hotkey` 调用。
pub fn apply_global_hotkey(
    app: &tauri::AppHandle,
    state: &AppState,
    accelerator: &str,
) -> AppResult<()> {
    let gs = app.global_shortcut();

    let old = state.shortcut.read().clone();
    let acc = accelerator.trim();
    if acc == old {
        return Ok(());
    }
    if acc.is_empty() {
        if !old.is_empty() {
            gs.unregister(old.as_str())
                .map_err(|e| crate::error::AppError::other(e.to_string()))?;
        }
        *state.shortcut.write() = String::new();
        return Ok(());
    }

    // 校验格式后再注册，避免运行期崩溃
    let parsed = tauri_plugin_global_shortcut::Shortcut::from_str(acc)
        .map_err(|e| crate::error::AppError::validation(format!("快捷键格式无效：{e}")))?;

    gs.register(parsed).map_err(|e| {
        crate::error::AppError::other(format!("注册全局快捷键失败（可能已被其它软件占用）：{e}"))
    })?;

    // 新快捷键注册成功后再移除旧快捷键，失败时旧配置仍然可用。
    if !old.is_empty() {
        let _ = gs.unregister(old.as_str());
    }
    *state.shortcut.write() = acc.to_string();
    Ok(())
}

/// 应用启动时把设置里的快捷键注册上去
pub fn bootstrap_hotkey(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let acc = state.settings().global_hotkey.clone();
    if let Err(err) = apply_global_hotkey(app, &state, &acc) {
        log::warn!("启动时注册全局快捷键失败：{err}");
        state.integration_warnings.lock().push(err.to_string());
    }
}

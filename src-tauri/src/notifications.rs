//! 直接使用 Windows Toast 回调：点击通知回到对应终端。
use crate::{
    db::models::TerminalInfo,
    error::{AppError, AppResult},
    state::AppState,
};
use tauri::{AppHandle, Emitter, Manager};
pub fn completed(app: &AppHandle, info: &TerminalInfo) {
    let result = (|| -> AppResult<()> {
        let p = crate::productivity::load(&app.state::<AppState>().db)?;
        if p.notification_mode == "off"
            || info.status == "killed"
            || (p.notification_mode == "failure" && info.exit_code == Some(0))
            || crate::db::models::now_ms() - info.started_at
                < (p.notification_min_seconds as i64) * 1000
        {
            return Ok(());
        }
        show(
            app,
            &info.session_id,
            &info.title,
            &format!(
                "任务{} · 退出码 {}",
                if info.exit_code == Some(0) {
                    "完成"
                } else {
                    "失败"
                },
                info.exit_code.unwrap_or(-1)
            ),
        )
    })();
    if let Err(e) = result {
        log::warn!("任务通知失败：{e}");
    }
}
pub fn show(app: &AppHandle, session_id: &str, title: &str, body: &str) -> AppResult<()> {
    #[cfg(windows)]
    {
        use tauri_winrt_notification::Toast;
        let handle = app.clone();
        let id = session_id.to_owned();
        // Win32 应用没有 MSIX 包身份，按通知库的 unpackaged 示例注册当前用户 AUMID。
        // 只写本应用的通知名称和图标，不调整权限或系统通知开关。
        let app_id = app.config().identifier.as_str();
        let icon = app
            .state::<AppState>()
            .data_dir
            .join("notification-icon.png");
        if !icon.exists() {
            std::fs::write(&icon, include_bytes!("../icons/128x128.png"))?;
        }
        let key = windows_registry::CURRENT_USER
            .create(format!(r"SOFTWARE\Classes\AppUserModelId\{app_id}"))
            .map_err(|e| AppError::other(format!("注册当前用户通知入口失败：{e}")))?;
        key.set_string("DisplayName", "CmdDeck")
            .and_then(|_| key.set_string("IconBackgroundColor", "0"))
            .and_then(|_| key.set_string("IconUri", &icon.to_string_lossy()))
            .map_err(|e| AppError::other(format!("保存通知名称和图标失败：{e}")))?;
        Toast::new(app_id)
            .title(title)
            .text1(body)
            .on_activated(move |_| {
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
                let _ = handle.emit(
                    "cmddeck://notification-click",
                    serde_json::json!({"sessionId":id}),
                );
                Ok(())
            })
            .show()
            .map_err(|e| AppError::other(format!("发送 Windows 通知失败：{e}")))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, session_id, title, body);
        Err(AppError::other("系统通知目前支持 Windows"))
    }
}

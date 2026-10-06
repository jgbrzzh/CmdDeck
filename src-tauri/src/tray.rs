use crate::{error::AppResult, state::*};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
pub fn toggle_quick_launch(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    app.state::<AppState>().emit(app, EV_QUICK_LAUNCH, ());
}
pub fn build(app: &AppHandle) -> AppResult<()> {
    let show = MenuItem::with_id(app, "show", "打开 CmdDeck", true, None::<&str>)
        .map_err(|e| crate::error::AppError::other(e.to_string()))?;
    let quit = MenuItem::with_id(app, "quit", "退出并停止任务", true, None::<&str>)
        .map_err(|e| crate::error::AppError::other(e.to_string()))?;
    let menu = Menu::with_items(app, &[&show, &quit])
        .map_err(|e| crate::error::AppError::other(e.to_string()))?;
    let mut builder = TrayIconBuilder::with_id("cmddeck-tray")
        .tooltip("CmdDeck 控制台中心")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => toggle_quick_launch(app),
            "quit" => {
                let st = app.state::<AppState>();
                st.exiting.store(true, std::sync::atomic::Ordering::SeqCst);
                st.pty.kill_all();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_quick_launch(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .build(app)
        .map_err(|e| crate::error::AppError::other(e.to_string()))?;
    Ok(())
}

use crate::{
    db::{models::*, presets, schedules},
    error::AppResult,
    state::*,
};
use serde_json::json;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};
#[tauri::command]
pub fn list_schedules(state: State<'_, AppState>) -> AppResult<Vec<Schedule>> {
    schedules::list(&state.db)
}
#[tauri::command]
pub fn save_schedule(state: State<'_, AppState>, schedule: Schedule) -> AppResult<Schedule> {
    presets::get(&state.db, &schedule.preset_id)?;
    schedules::save(&state.db, &schedule)
}
#[tauri::command]
pub fn delete_schedule(state: State<'_, AppState>, id: String) -> AppResult<()> {
    schedules::delete(&state.db, &id)
}
#[tauri::command]
pub fn set_schedule_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> AppResult<()> {
    schedules::set_enabled(&state.db, &id, enabled)
}
fn fire(app: &AppHandle, s: &Schedule) -> AppResult<String> {
    let st = app.state::<AppState>();
    let next = if s.mode == "once" {
        0
    } else {
        schedules::compute_next_run(s, now_ms())
    };
    let result = super::terminal_cmds::run_internal(app, &s.preset_id, &s.args, "schedule", false);
    let status = result
        .as_ref()
        .map(|_| "已启动".to_string())
        .unwrap_or_else(|e| e.to_string());
    schedules::set_next_run(&st.db, &s.id, next, &status)?;
    match result {
        Ok(info) => {
            st.emit(app,EV_SCHEDULE_FIRED,json!({"scheduleId":s.id,"name":s.name,"presetId":s.preset_id,"presetName":info.preset_name,"source":"schedule","sessionId":info.session_id}));
            let a = app.clone();
            let sid = info.session_id.clone();
            let schedule_id = s.id.clone();
            std::thread::spawn(move || loop {
                match super::terminal_cmds::wait_internal(&a, &sid, 1000) {
                    Ok(Some(code)) => {
                        let _ = a.state::<AppState>().db.conn().execute(
                            "UPDATE schedules SET last_status=?2 WHERE id=?1",
                            rusqlite::params![schedule_id, format!("退出码 {code}")],
                        );
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            });
            Ok(info.session_id)
        }
        Err(e) => Err(e),
    }
}
#[tauri::command]
pub fn trigger_schedule_now(app: AppHandle, id: String) -> AppResult<String> {
    let s = schedules::get(&app.state::<AppState>().db, &id)?;
    fire(&app, &s)
}
pub fn start_scheduler_internal(app: &AppHandle) -> bool {
    let st = app.state::<AppState>();
    let _control = st.scheduler_control.lock();
    if st.scheduler_running.swap(true, Ordering::SeqCst) {
        return false;
    }
    let epoch = st.scheduler_epoch.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || loop {
        let st = app.state::<AppState>();
        if !st.scheduler_running.load(Ordering::SeqCst)
            || st.exiting.load(Ordering::SeqCst)
            || st.scheduler_epoch.load(Ordering::SeqCst) != epoch
        {
            break;
        }
        if let Ok(due) = schedules::due(&st.db, now_ms()) {
            for s in due {
                let _ = fire(&app, &s);
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    });
    true
}
#[tauri::command]
pub fn start_scheduler(app: AppHandle) -> bool {
    start_scheduler_internal(&app)
}
#[tauri::command]
pub fn stop_scheduler(state: State<'_, AppState>) -> bool {
    let _control = state.scheduler_control.lock();
    state.scheduler_epoch.fetch_add(1, Ordering::SeqCst);
    state.scheduler_running.swap(false, Ordering::SeqCst)
}

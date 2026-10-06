use crate::{
    db::models::*,
    error::{AppError, AppResult},
    state::AppState,
};
use std::collections::HashMap;
use tauri::State;
#[tauri::command]
pub fn check_command(
    state: State<'_, AppState>,
    preset: Preset,
    args: HashMap<String, String>,
) -> AppResult<SecurityVerdict> {
    let opt = super::terminal_cmds::options(&preset, &args, "manual")?;
    let (p, a) = opt.resolve();
    let mut v = crate::security::check(&join_command_line(&p, &a), &state.settings());
    if preset.confirm {
        v.requires_confirm = true;
    }
    Ok(v)
}
#[tauri::command]
pub fn get_blacklist(state: State<'_, AppState>) -> Vec<String> {
    state.settings().blacklist
}
#[tauri::command]
pub fn add_blacklist(state: State<'_, AppState>, pattern: String) -> AppResult<Vec<String>> {
    if pattern.trim().is_empty() {
        return Err(AppError::validation("规则不能为空"));
    }
    if let Some(re) = pattern.strip_prefix("regex:") {
        regex::Regex::new(re).map_err(|e| AppError::validation(e.to_string()))?;
    }
    let mut s = state.settings();
    if !s.blacklist.contains(&pattern) {
        s.blacklist.push(pattern);
    }
    state.set_settings(&s)?;
    Ok(s.blacklist)
}
#[tauri::command]
pub fn remove_blacklist(state: State<'_, AppState>, pattern: String) -> AppResult<Vec<String>> {
    let mut s = state.settings();
    s.blacklist.retain(|p| p != &pattern);
    state.set_settings(&s)?;
    Ok(s.blacklist)
}

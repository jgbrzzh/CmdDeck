use crate::{
    db::models::*,
    error::{AppError, AppResult},
};
use std::collections::HashMap;
use tauri::AppHandle;
#[tauri::command]
pub async fn run_batch(
    app: AppHandle,
    preset_ids: Vec<String>,
    args_map: HashMap<String, String>,
    concurrency: usize,
    continue_on_error: bool,
) -> AppResult<BatchResult> {
    if preset_ids.len() > 100 || concurrency == 0 || concurrency > 12 {
        return Err(AppError::validation("批量最多 100 条，并发数为 1 到 12"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut result = BatchResult {
            run_id: new_id(),
            sessions: vec![],
            blocked: vec![],
            started_at: now_ms(),
        };
        for ids in preset_ids.chunks(concurrency) {
            let mut current = vec![];
            let mut failed = false;
            for id in ids {
                match super::terminal_cmds::run_internal(&app, id, &args_map, "batch", false) {
                    Ok(info) => {
                        current.push(info.session_id.clone());
                        result.sessions.push(info);
                    }
                    Err(e) => {
                        result.blocked.push(format!("{id}: {e}"));
                        failed = true;
                    }
                }
            }
            for id in current {
                loop {
                    if let Some(code) = super::terminal_cmds::wait_internal(&app, &id, 1000)? {
                        failed |= code != 0;
                        break;
                    }
                }
            }
            if failed && !continue_on_error {
                break;
            }
        }
        Ok(result)
    })
    .await
    .map_err(|e| AppError::other(e.to_string()))?
}

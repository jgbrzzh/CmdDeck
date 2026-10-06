//! 审计日志与终端历史相关的 Tauri 命令。
//!
//! 对应前端 `auditApi`（见 `src/api/index.ts`）。

use tauri::State;

use crate::db::audit;
use crate::db::models::{AuditFilter, AuditLog, TerminalHistory};
use crate::error::AppResult;
use crate::state::AppState;

/// 查询审计日志（`filter` 可为 null）
#[tauri::command]
pub fn list_audit_logs(
    state: State<'_, AppState>,
    filter: Option<AuditFilter>,
) -> AppResult<Vec<AuditLog>> {
    let f = filter.unwrap_or_default();
    audit::list(&state.db, &f)
}

/// 清空审计日志
#[tauri::command]
pub fn clear_audit_logs(state: State<'_, AppState>) -> AppResult<()> {
    audit::clear(&state.db)
}

/// 查询终端历史。`preset_id` 传空串表示查全部。
#[tauri::command]
pub fn list_terminal_history(
    state: State<'_, AppState>,
    preset_id: String,
    limit: i64,
) -> AppResult<Vec<TerminalHistory>> {
    audit::history_list(&state.db, &preset_id, limit)
}

/// 清空终端历史
#[tauri::command]
pub fn clear_terminal_history(state: State<'_, AppState>) -> AppResult<()> {
    audit::history_clear(&state.db)
}

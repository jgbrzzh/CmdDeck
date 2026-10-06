//! 首次启动写入示例数据的命令。
//!
//! 对应前端 `systemApi.seedDefaults()`。
//! 真正的数据在 [`crate::seed`] 里，这里只是一层薄壳。

use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::state::AppState;

/// 写入示例预设，返回实际写入的条数。
///
/// 幂等：数据库里已经有预设时直接返回 0，不会重复写入。
#[tauri::command]
pub fn seed_default_data(app: AppHandle, state: State<'_, AppState>) -> AppResult<i32> {
    // 强制读一次数据库，避免调用方只用到 state 而触发"未使用"告警
    let _ = &state.db;
    crate::seed::seed_default_data(&app)
}

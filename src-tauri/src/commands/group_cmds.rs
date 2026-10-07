//! 分组相关的 Tauri 命令。
//!
//! 对应前端 `groupApi`（见 `src/api/index.ts`）。

use serde_json::json;
use tauri::{AppHandle, State};

use crate::db::groups;
use crate::db::models::{new_id, now_ms, Group};
use crate::error::{AppError, AppResult};
use crate::state::{AppState, EV_PRESET_CHANGED};

/// 列出全部分组
#[tauri::command]
pub fn list_groups(state: State<'_, AppState>) -> AppResult<Vec<Group>> {
    groups::list(&state.db)
}

/// 保存（新增或更新）分组。
///
/// 重名会直接报 `Conflict`，因为左侧导航里两个同名文件夹用户根本分不清。
#[tauri::command]
pub fn save_group(app: AppHandle, state: State<'_, AppState>, group: Group) -> AppResult<Group> {
    crate::backups::snapshot(&state)?;
    if group.name.trim().is_empty() {
        return Err(AppError::validation("分组名称不能为空"));
    }
    let name = group.name.trim().to_string();

    // 排除自己之后还撞名才算冲突（改名时当然要允许和自己同名）
    let mut exclude = group.id.clone();
    if exclude.trim().is_empty() {
        // 还没落库，用一个不可能存在的 ID 占位
        exclude = "__new__".to_string();
    }
    if let Some(old) = groups::find_by_name(&state.db, &name, &exclude)? {
        return Err(AppError::conflict(format!(
            "已经有一个叫「{name}」的分组了，换个名字吧（原 ID：{}）",
            old.id
        )));
    }

    let mut item = group;
    item.name = name;
    let is_new = item.id.trim().is_empty();
    if is_new {
        item.id = new_id();
        item.created_at = now_ms();
        item.sort_order = groups::next_sort_order(&state.db)?;
    }

    let saved = groups::save(&state.db, &item)?;
    let reason = if is_new {
        "group-created"
    } else {
        "group-saved"
    };
    state.emit(&app, EV_PRESET_CHANGED, json!({ "reason": reason }));
    Ok(saved)
}

/// 删除分组，返回被移出该分组的预设数量（前端据此提示"已把 N 条预设移到未分组"）
#[tauri::command]
pub fn delete_group(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<i32> {
    crate::backups::snapshot(&state)?;
    let moved = groups::delete(&state.db, &id)?;
    state.emit(
        &app,
        EV_PRESET_CHANGED,
        json!({ "reason": "group-deleted" }),
    );
    Ok(moved)
}

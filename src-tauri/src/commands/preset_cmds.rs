//! 预设指令相关的 Tauri 命令。
//!
//! 对应前端 `presetApi`（见 `src/api/index.ts`）。
//! 形参名必须与前端 `invoke` 传的 key 一致（Tauri 2 会把 camelCase 自动转 snake_case，
//! 例如前端传 `presetId` 对应 Rust 的 `preset_id`）。

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::json;
use tauri::{AppHandle, State};

use crate::db::models::{join_command_line, Placeholder, Preset, PresetFilter};
use crate::db::presets;
use crate::error::AppResult;
use crate::state::{AppState, EV_PRESET_CHANGED};

/// 占位符正则：`{{key}}`，允许 key 里出现字母、数字、下划线、连字符与常用汉字。
///
/// 编译一次就缓存起来——这条命令在用户打字时会高频触发，不能每次都重新编译。
fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\{\{\s*([A-Za-z0-9_\-\u4e00-\u9fa5]+)\s*\}\}").expect("占位符正则常量应当合法")
    })
}

/// 把文本里的 `{{key}}` 替换成给定值，没给值的替换成空串。
///
/// 未提供的占位符之所以替换成空串而不是保留原样：预览命令行的场景下，
/// 留个 `{{port}}` 在界面上会让人以为程序没跑起来。
pub fn apply_placeholders(text: &str, args: &HashMap<String, String>) -> String {
    placeholder_re()
        .replace_all(text, |caps: &regex::Captures| {
            let key = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            args.get(key).cloned().unwrap_or_default()
        })
        .into_owned()
}

// ============================================================
// 增删改查
// ============================================================

/// 列出预设（`filter` 可为 null，表示不过滤）
#[tauri::command]
pub fn list_presets(
    state: State<'_, AppState>,
    filter: Option<PresetFilter>,
) -> AppResult<Vec<Preset>> {
    let f = filter.unwrap_or_default();
    presets::list(&state.db, &f)
}

/// 取一条预设
#[tauri::command]
pub fn get_preset(state: State<'_, AppState>, id: String) -> AppResult<Preset> {
    presets::get(&state.db, &id)
}

/// 保存预设（新增或更新）。
///
/// 新增的预设会自动排到所属分组的末尾，省得前端算 `sortOrder`。
#[tauri::command]
pub fn save_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    preset: Preset,
) -> AppResult<Preset> {
    // 判断是新增还是更新（前端新建时 id 为空串）
    let is_new = preset.id.trim().is_empty() || !presets::exists(&state.db, &preset.id)?;

    let mut item = preset;
    if is_new {
        item.sort_order = presets::next_sort_order(&state.db, &item.group_id)?;
    }

    let saved = presets::save(&state.db, &item)?;
    state.emit(&app, EV_PRESET_CHANGED, json!({ "reason": "saved" }));
    Ok(saved)
}

/// 删除一条预设
#[tauri::command]
pub fn delete_preset(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    presets::delete(&state.db, &id)?;
    state.emit(&app, EV_PRESET_CHANGED, json!({ "reason": "deleted" }));
    Ok(())
}

/// 批量删除预设
#[tauri::command]
pub fn delete_presets(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<()> {
    let n = presets::delete_many(&state.db, &ids)?;
    state.emit(
        &app,
        EV_PRESET_CHANGED,
        json!({ "reason": "deleted", "count": n }),
    );
    Ok(())
}

/// 复制一条预设
#[tauri::command]
pub fn duplicate_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Preset> {
    let copy = presets::duplicate(&state.db, &id)?;
    state.emit(&app, EV_PRESET_CHANGED, json!({ "reason": "duplicated" }));
    Ok(copy)
}

// ============================================================
// 排序 / 分组 / 收藏
// ============================================================

/// 拖拽排序：按给定顺序重写 `sort_order`
#[tauri::command]
pub fn reorder_presets(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<()> {
    presets::reorder(&state.db, &ids)
}

/// 收藏 / 取消收藏（读当前值取反）
#[tauri::command]
pub fn toggle_preset_favorite(state: State<'_, AppState>, id: String) -> AppResult<Preset> {
    let cur = presets::get(&state.db, &id)?;
    presets::set_favorite(&state.db, &id, !cur.favorite)
}

/// 记一次运行（次数 +1、刷新最近使用时间）
#[tauri::command]
pub fn record_preset_run(state: State<'_, AppState>, id: String) -> AppResult<Preset> {
    presets::record_run(&state.db, &id)
}

/// 移动预设到指定分组的指定位置
#[tauri::command]
pub fn move_preset(
    state: State<'_, AppState>,
    id: String,
    group_id: String,
    sort_order: i32,
) -> AppResult<()> {
    presets::move_to_group(&state.db, &id, &group_id, sort_order)
}

// ============================================================
// 占位符
// ============================================================

/// 扫描文本里的 `{{key}}` 占位符，按**首次出现顺序**去重返回。
///
/// 只负责"发现"，标签、是否必填、候选值这些由用户在预设编辑器里补。
#[tauri::command]
pub fn scan_placeholders(state: State<'_, AppState>, text: String) -> AppResult<Vec<Placeholder>> {
    // 抑制未使用告警：state 目前没用到，但前端调用签名保持一致
    let _ = &state;

    let re = placeholder_re();
    let mut out: Vec<Placeholder> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for cap in re.captures_iter(&text) {
        let Some(m) = cap.get(1) else { continue };
        let key = m.as_str().to_string();
        if seen.iter().any(|k| k == &key) {
            continue;
        }
        seen.push(key.clone());
        out.push(Placeholder {
            key: key.clone(),
            label: key,
            input_type: "text".to_string(),
            default_value: String::new(),
            options: Vec::new(),
            required: false,
            help: String::new(),
        });
    }
    Ok(out)
}

/// 预览最终命令行：把占位符填上，返回一行可读文本。
///
/// 前端在"运行前确认"弹窗里展示它，让用户看清到底要执行什么。
#[tauri::command]
pub fn preview_command(
    state: State<'_, AppState>,
    preset: Preset,
    args: HashMap<String, String>,
) -> AppResult<String> {
    let _ = &state;

    let mut values = args;
    for field in std::iter::once(&preset.program)
        .chain(preset.args.iter())
        .chain(std::iter::once(&preset.working_dir))
        .chain(preset.env.iter().map(|e| &e.value))
    {
        for cap in placeholder_re().captures_iter(field) {
            values.entry(cap[1].to_string()).or_default();
        }
    }
    let opt = super::terminal_cmds::options(&preset, &values, "manual")?;
    let (program, filled) = opt.resolve();
    Ok(join_command_line(&program, &filled))
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn apply_placeholders_fills_and_blanks() {
        let m = map(&[("port", "8080")]);
        assert_eq!(apply_placeholders("--port {{port}}", &m), "--port 8080");
        assert_eq!(apply_placeholders("--port {{port}}", &map(&[])), "--port ");
        assert_eq!(apply_placeholders("无占位符", &m), "无占位符");
        // 大小写敏感：{{Port}} 不是 {{port}}
        assert_eq!(apply_placeholders("{{Port}}", &m), "");
    }

    #[test]
    fn placeholder_regex_handles_spaces_and_chinese() {
        let re = placeholder_re();
        let caps: Vec<String> = re
            .captures_iter("{{a}} {{ b }} {{中文-1}}")
            .map(|c| c.get(1).unwrap().as_str().to_string())
            .collect();
        assert_eq!(caps, vec!["a", "b", "中文-1"]);
    }
}

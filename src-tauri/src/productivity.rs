//! 项目与界面配置：SQLite 保存；布局仅恢复视图，绝不自动执行命令。
use crate::{
    db::{models::*, Db},
    error::{AppError, AppResult},
    state::AppState,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub directory: String,
    pub python: RuntimeBinding,
    pub node: RuntimeBinding,
    pub env: Vec<EnvVar>,
    pub preset_ids: Vec<String>,
    pub ports: Vec<u16>,
}
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedTab {
    pub title: String,
    pub kind: String,
    pub cwd: String,
    pub preset_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layout {
    pub mode: String,
    pub tabs: Vec<SavedTab>,
    pub active: usize,
    pub secondary: usize,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            mode: "single".into(),
            tabs: vec![],
            active: 0,
            secondary: 1,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Productivity {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: String,
    pub layouts: std::collections::BTreeMap<String, Layout>,
    pub notification_mode: String,
    pub notification_min_seconds: u32,
    pub restore_layout: bool,
    pub log_max_megabytes: u32,
}
impl Default for Productivity {
    fn default() -> Self {
        Self {
            workspaces: vec![],
            active_workspace: String::new(),
            layouts: Default::default(),
            notification_mode: "off".into(),
            notification_min_seconds: 10,
            restore_layout: true,
            log_max_megabytes: 128,
        }
    }
}
pub fn load(db: &Db) -> AppResult<Productivity> {
    let raw: Option<String> = db
        .conn()
        .query_row(
            "SELECT value FROM settings WHERE key='productivity'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| {
        serde_json::from_str(&s)
            .map_err(|e| AppError::io(format!("项目配置损坏，请从备份恢复：{e}")))
    })
    .unwrap_or(Ok(Productivity::default()))
}
use rusqlite::OptionalExtension;
pub fn save(db: &Db, p: &Productivity) -> AppResult<()> {
    validate(p)?;
    db.conn().execute("INSERT INTO settings(key,value,updated_at) VALUES('productivity',?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",params![serde_json::to_string(p).map_err(|e|AppError::other(e.to_string()))?,now_ms()])?;
    Ok(())
}
pub fn validate(p: &Productivity) -> AppResult<()> {
    if !(4..=2048).contains(&p.log_max_megabytes) {
        return Err(AppError::validation("历史日志总容量应为 4～2048 MB"));
    }
    if !["off", "all", "failure"].contains(&p.notification_mode.as_str())
        || p.notification_min_seconds > 86400
    {
        return Err(AppError::validation("通知设置无效"));
    }
    if p.workspaces.len() > 100 || p.layouts.len() > 101 {
        return Err(AppError::validation("项目数量最多 100 个"));
    }
    let mut ids = std::collections::HashSet::new();
    for w in &p.workspaces {
        if w.id.is_empty() || w.name.trim().is_empty() || !ids.insert(&w.id) {
            return Err(AppError::validation("项目名称、ID 不能为空或重复"));
        }
        if w.directory.is_empty() || !std::path::Path::new(&w.directory).is_absolute() {
            return Err(AppError::validation("项目目录必须是绝对路径"));
        }
        if w.ports.contains(&0) || w.ports.len() > 32 {
            return Err(AppError::validation("项目端口应为 1～65535，最多 32 个"));
        }
        if !["", "system", "python", "venv", "conda"].contains(&w.python.kind.as_str())
            || !["", "system", "node"].contains(&w.node.kind.as_str())
        {
            return Err(AppError::validation("Python / Node 环境类型不匹配"));
        }
        for e in &w.env {
            if e.name.is_empty() || e.name.contains(['=', '\0']) || e.value.contains('\0') {
                return Err(AppError::validation("环境变量无效"));
            }
        }
    }
    if !p.active_workspace.is_empty() && !ids.contains(&p.active_workspace) {
        return Err(AppError::validation("当前项目不存在"));
    }
    for l in p.layouts.values() {
        if !["single", "columns", "rows"].contains(&l.mode.as_str()) || l.tabs.len() > 64 {
            return Err(AppError::validation("终端布局无效或标签超过 64 个"));
        }
    }
    Ok(())
}
/// 预设优先于项目；自动化只使用预设设置，不受当前窗口选中项目影响。
pub fn prepare(
    state: &AppState,
    opt: &mut SpawnOptions,
    workspace_id: Option<&str>,
) -> AppResult<Option<Workspace>> {
    let config = load(&state.db)?;
    let id = workspace_id.unwrap_or("");
    if id.is_empty() {
        return Ok(None);
    }
    let w = config
        .workspaces
        .into_iter()
        .find(|w| w.id == id)
        .ok_or_else(|| AppError::validation("项目已删除，请重新选择"))?;
    if opt.working_dir.trim().is_empty() {
        opt.working_dir = w.directory.clone();
    }
    if opt.runtime.kind.is_empty() {
        opt.runtime = if opt.kind == "node" {
            w.node.clone()
        } else {
            w.python.clone()
        };
    }
    let mut env = w.env.clone();
    env.retain(|a| !opt.env.iter().any(|b| a.name.eq_ignore_ascii_case(&b.name)));
    env.extend(opt.env.clone());
    opt.env = env;
    Ok(Some(w))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_and_old_database_defaults() {
        let db = Db::open_in_memory().unwrap();
        let mut p = load(&db).unwrap();
        assert!(p.workspaces.is_empty());
        p.notification_mode = "failure".into();
        save(&db, &p).unwrap();
        assert_eq!(load(&db).unwrap().notification_mode, "failure");
    }
    #[test]
    fn rejects_invalid_layout_and_project() {
        let mut p = Productivity::default();
        p.active_workspace = "missing".into();
        assert!(validate(&p).is_err());
        p.active_workspace.clear();
        p.layouts.insert(
            "".into(),
            Layout {
                mode: "invalid".into(),
                ..Default::default()
            },
        );
        assert!(validate(&p).is_err());
    }
}

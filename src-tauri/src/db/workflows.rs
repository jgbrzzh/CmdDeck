use super::{from_json, models::*, to_json, Db};
use crate::error::{AppError, AppResult};
use rusqlite::params;
pub fn list(db: &Db) -> AppResult<Vec<Workflow>> {
    let conn = db.conn();
    let mut stmt=conn.prepare("SELECT id,name,description,run_mode,continue_on_error,steps,enabled,created_at,updated_at FROM workflows ORDER BY updated_at DESC")?;
    let rows = stmt.query_map([], |r| {
        Ok(Workflow {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            run_mode: r.get(3)?,
            continue_on_error: r.get::<_, i64>(4)? != 0,
            steps: from_json(&r.get::<_, String>(5)?, vec![]),
            enabled: r.get::<_, i64>(6)? != 0,
            created_at: r.get(7)?,
            updated_at: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
pub fn get(db: &Db, id: &str) -> AppResult<Workflow> {
    list(db)?
        .into_iter()
        .find(|w| w.id == id)
        .ok_or_else(|| AppError::not_found("工作流不存在"))
}
pub fn save(db: &Db, w: &Workflow) -> AppResult<Workflow> {
    if w.name.trim().is_empty() || w.steps.is_empty() {
        return Err(AppError::validation("填写工作流名称并添加步骤"));
    }
    if !["serial", "parallel"].contains(&w.run_mode.as_str()) {
        return Err(AppError::validation("运行模式无效"));
    }
    if w.run_mode == "parallel" && w.steps.iter().any(|s| s.condition != "always") {
        return Err(AppError::validation("并行工作流不支持依赖上一步结果"));
    }
    for s in &w.steps {
        if s.delay_ms < 0 || !["always", "on_success", "on_fail"].contains(&s.condition.as_str()) {
            return Err(AppError::validation("步骤条件或延迟无效"));
        }
        if s.enabled {
            super::presets::get(db, &s.preset_id)?;
        }
    }
    let mut w = w.clone();
    if w.id.is_empty() {
        w.id = new_id();
    }
    if w.created_at == 0 {
        w.created_at = now_ms();
    }
    w.updated_at = now_ms();
    db.conn().execute("INSERT INTO workflows(id,name,description,run_mode,continue_on_error,steps,enabled,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,run_mode=excluded.run_mode,continue_on_error=excluded.continue_on_error,steps=excluded.steps,enabled=excluded.enabled,updated_at=excluded.updated_at",params![w.id,w.name,w.description,w.run_mode,w.continue_on_error,to_json(&w.steps),w.enabled,w.created_at,w.updated_at])?;
    Ok(w)
}
pub fn delete(db: &Db, id: &str) -> AppResult<()> {
    db.conn()
        .execute("DELETE FROM workflows WHERE id=?1", [id])?;
    Ok(())
}
pub fn save_run(db: &Db, r: &WorkflowRun) -> AppResult<()> {
    db.conn().execute("INSERT INTO workflow_runs(id,workflow_id,workflow_name,started_at,finished_at,status,steps,source) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET finished_at=excluded.finished_at,status=excluded.status,steps=excluded.steps",params![r.id,r.workflow_id,r.workflow_name,r.started_at,r.finished_at,r.status,to_json(&r.steps),r.source])?;
    Ok(())
}
pub fn list_runs(db: &Db, id: &str, limit: i64) -> AppResult<Vec<WorkflowRun>> {
    let conn = db.conn();
    let mut stmt=conn.prepare("SELECT id,workflow_id,workflow_name,started_at,finished_at,status,steps,source FROM workflow_runs WHERE (?1='' OR workflow_id=?1) ORDER BY started_at DESC LIMIT ?2")?;
    let rows = stmt.query_map(params![id, limit.clamp(1, 500)], |r| {
        Ok(WorkflowRun {
            id: r.get(0)?,
            workflow_id: r.get(1)?,
            workflow_name: r.get(2)?,
            started_at: r.get(3)?,
            finished_at: r.get(4)?,
            status: r.get(5)?,
            steps: from_json(&r.get::<_, String>(6)?, vec![]),
            source: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

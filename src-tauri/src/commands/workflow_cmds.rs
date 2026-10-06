use crate::{
    db::{models::*, workflows},
    error::{AppError, AppResult},
    state::*,
};
use tauri::{AppHandle, Manager, State};
#[tauri::command]
pub fn list_workflows(state: State<'_, AppState>) -> AppResult<Vec<Workflow>> {
    workflows::list(&state.db)
}
#[tauri::command]
pub fn save_workflow(state: State<'_, AppState>, workflow: Workflow) -> AppResult<Workflow> {
    workflows::save(&state.db, &workflow)
}
#[tauri::command]
pub fn delete_workflow(state: State<'_, AppState>, id: String) -> AppResult<()> {
    workflows::delete(&state.db, &id)
}
#[tauri::command]
pub fn list_workflow_runs(
    state: State<'_, AppState>,
    workflow_id: String,
    limit: Option<i64>,
) -> AppResult<Vec<WorkflowRun>> {
    workflows::list_runs(&state.db, &workflow_id, limit.unwrap_or(20))
}
fn publish(app: &AppHandle, r: &WorkflowRun) {
    let st = app.state::<AppState>();
    if let Err(e) = workflows::save_run(&st.db, r) {
        log::error!("工作流记录失败：{e}");
    }
    st.emit(app, EV_WORKFLOW_UPDATE, r.clone());
}
fn canceled(app: &AppHandle, id: &str) -> bool {
    !app.state::<AppState>()
        .active_workflows
        .read()
        .iter()
        .any(|v| v == id)
}
fn execute(app: &AppHandle, run_id: &str, step: WorkflowStep) -> WorkflowStepResult {
    let mut r = WorkflowStepResult {
        step_id: step.id,
        step_name: step.name,
        preset_name: String::new(),
        status: "skipped".into(),
        session_id: String::new(),
        exit_code: None,
        message: String::new(),
        started_at: 0,
        finished_at: 0,
    };
    if !step.enabled {
        return r;
    }
    let delay = std::time::Instant::now();
    while delay.elapsed().as_millis() < (step.delay_ms.max(0) as u128) {
        if canceled(app, run_id) {
            return r;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if canceled(app, run_id) {
        return r;
    }
    r.started_at = now_ms();
    match super::terminal_cmds::run_internal(app, &step.preset_id, &step.args, "workflow", false) {
        Ok(info) => {
            r.session_id = info.session_id;
            r.preset_name = info.preset_name;
            r.status = "running".into();
            loop {
                if canceled(app, run_id) {
                    let _ = app.state::<AppState>().pty.kill(&r.session_id);
                    r.status = "canceled".into();
                    break;
                }
                match super::terminal_cmds::wait_internal(app, &r.session_id, 100) {
                    Ok(Some(code)) => {
                        r.exit_code = Some(code);
                        r.status = if code == 0 { "success" } else { "failed" }.into();
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        r.status = "failed".into();
                        r.message = e.to_string();
                        break;
                    }
                }
            }
        }
        Err(e) => {
            r.status = "blocked".into();
            r.message = e.to_string();
        }
    }
    r.finished_at = now_ms();
    r
}
#[tauri::command]
pub fn start_workflow(
    app: AppHandle,
    id: String,
    source: Option<String>,
) -> AppResult<WorkflowRun> {
    let st = app.state::<AppState>();
    let w = workflows::get(&st.db, &id)?;
    if !w.enabled {
        return Err(AppError::validation("工作流已停用"));
    }
    let mut r = WorkflowRun {
        id: new_id(),
        workflow_id: w.id,
        workflow_name: w.name,
        started_at: now_ms(),
        finished_at: 0,
        status: "running".into(),
        steps: vec![],
        source: source.unwrap_or("manual".into()),
    };
    st.active_workflows.write().push(r.id.clone());
    publish(&app, &r);
    let initial = r.clone();
    std::thread::spawn(move || {
        if w.run_mode == "parallel" {
            // 并行最多 4 步一组，避免一次创建过多线程。
            for chunk in w.steps.chunks(4) {
                if canceled(&app, &r.id) {
                    break;
                }
                let handles: Vec<_> = chunk
                    .iter()
                    .cloned()
                    .map(|s| {
                        let a = app.clone();
                        let id = r.id.clone();
                        std::thread::spawn(move || execute(&a, &id, s))
                    })
                    .collect();
                for h in handles {
                    if let Ok(result) = h.join() {
                        r.steps.push(result);
                        publish(&app, &r);
                    }
                }
                if !w.continue_on_error
                    && r.steps
                        .iter()
                        .any(|s| s.status == "failed" || s.status == "blocked")
                {
                    break;
                }
            }
        } else {
            let mut previous = true;
            for mut s in w.steps {
                if canceled(&app, &r.id) {
                    break;
                }
                if s.condition == "on_success" && !previous || s.condition == "on_fail" && previous
                {
                    s.enabled = false;
                }
                let result = execute(&app, &r.id, s);
                if result.status != "skipped" {
                    previous = result.status == "success";
                }
                r.steps.push(result);
                publish(&app, &r);
                if !previous && !w.continue_on_error {
                    break;
                }
            }
        }
        r.status = if canceled(&app, &r.id) {
            "canceled"
        } else if r
            .steps
            .iter()
            .any(|s| s.status == "failed" || s.status == "blocked")
        {
            "failed"
        } else {
            "success"
        }
        .into();
        r.finished_at = now_ms();
        publish(&app, &r);
        app.state::<AppState>()
            .active_workflows
            .write()
            .retain(|id| id != &r.id);
    });
    Ok(initial)
}
#[tauri::command]
pub fn cancel_workflow(state: State<'_, AppState>, run_id: String) {
    state.active_workflows.write().retain(|id| id != &run_id);
}

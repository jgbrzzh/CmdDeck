use crate::{
    db::models::*,
    environments::*,
    error::{AppError, AppResult},
    state::AppState,
};
use tauri::{AppHandle, Manager};
#[tauri::command]
pub async fn discover_environments(project_dir: Option<String>) -> AppResult<EnvironmentReport> {
    tauri::async_runtime::spawn_blocking(move || discover(&project_dir.unwrap_or_default()))
        .await
        .map_err(|e| AppError::other(e.to_string()))
}
#[tauri::command]
pub fn open_environment_terminal(
    app: AppHandle,
    runtime: RuntimeBinding,
    project_dir: String,
) -> AppResult<TerminalInfo> {
    let st = app.state::<AppState>();
    let kind = st.settings().default_shell;
    let opt = SpawnOptions {
        runtime,
        title: "环境终端".into(),
        kind,
        program: String::new(),
        args: vec![],
        preset_id: String::new(),
        working_dir: project_dir,
        env: vec![],
        use_shell: false,
        elevated: false,
        interactive: true,
        cols: 120,
        rows: 30,
        source: "environment".into(),
    };
    super::terminal_cmds::spawn_terminal(st, opt, Some(false))
}
#[tauri::command]
pub fn run_environment_action(
    app: AppHandle,
    request: EnvironmentAction,
) -> AppResult<TerminalInfo> {
    if request.action != "list-packages" && !request.confirmed {
        return Err(AppError::blocked("创建环境、安装版本和安装包需要用户确认"));
    }
    let (program, args) = action_command(&request)?;
    let runtime = if request.tool == "python" && request.action != "create-environment" {
        request.runtime
    } else if ["npm", "pnpm", "yarn"].contains(&request.tool.as_str()) {
        request.runtime
    } else {
        RuntimeBinding::default()
    };
    let opt = SpawnOptions {
        runtime,
        title: format!("{} · {}", request.tool, request.action),
        kind: "powershell".into(),
        program: invocation(&program, &args),
        args: vec![],
        preset_id: String::new(),
        working_dir: request.project_dir,
        env: if request.tool == "python" {
            vec![EnvVar {
                name: "PIP_DISABLE_PIP_VERSION_CHECK".into(),
                value: "1".into(),
            }]
        } else {
            vec![]
        },
        use_shell: true,
        elevated: false,
        interactive: false,
        cols: 120,
        rows: 30,
        source: "environment".into(),
    };
    super::terminal_cmds::spawn_terminal(app.state::<AppState>(), opt, Some(request.confirmed))
}

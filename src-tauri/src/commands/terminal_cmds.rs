use crate::{
    db::{self, models::*},
    error::{AppError, AppResult},
    state::AppState,
};
use std::collections::HashMap;
use tauri::{AppHandle, Manager, State};

pub fn options(
    p: &Preset,
    args: &HashMap<String, String>,
    source: &str,
) -> AppResult<SpawnOptions> {
    static PLACEHOLDER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = PLACEHOLDER.get_or_init(|| regex::Regex::new(r"\{\{\s*([^{}\s]+)\s*\}\}").unwrap());
    let fill = |text: &str| -> AppResult<String> {
        for c in re.captures_iter(text) {
            if !args.contains_key(&c[1]) {
                return Err(AppError::validation(format!("请填写参数 {}", &c[1])));
            }
        }
        Ok(re
            .replace_all(text, |c: &regex::Captures| {
                args.get(&c[1]).cloned().unwrap_or_default()
            })
            .into_owned())
    };
    Ok(SpawnOptions {
        runtime: p.runtime.clone(),
        preset_id: p.id.clone(),
        title: p.name.clone(),
        kind: p.kind.clone(),
        program: fill(&p.program)?,
        args: p.args.iter().map(|a| fill(a)).collect::<AppResult<_>>()?,
        working_dir: fill(&p.working_dir)?,
        env: p
            .env
            .iter()
            .map(|e| {
                Ok(EnvVar {
                    name: e.name.clone(),
                    value: fill(&e.value)?,
                })
            })
            .collect::<AppResult<_>>()?,
        use_shell: p.use_shell,
        elevated: p.elevated,
        interactive: false,
        cols: 120,
        rows: 30,
        source: source.into(),
    })
}
fn validate_spawn(state: &AppState, opt: &SpawnOptions, confirmed: bool) -> AppResult<()> {
    crate::environments::validate_binding(&opt.runtime)?;
    let settings = state.settings();
    let (program, args) = opt.resolve();
    let text = join_command_line(&program, &args);
    let verdict = crate::security::check(&text, &settings);
    if verdict.level == "blocked" || (verdict.requires_confirm && !confirmed) {
        if settings.audit_enabled {
            let _ = db::audit::insert(
                &state.db,
                &AuditLog {
                    id: 0,
                    at: now_ms(),
                    preset_id: opt.preset_id.clone(),
                    preset_name: opt.title.clone(),
                    command: text,
                    cwd: opt.working_dir.clone(),
                    level: 2,
                    status: "blocked".into(),
                    exit_code: None,
                    message: verdict.reasons.join("；"),
                    duration_ms: 0,
                    source: opt.source.clone(),
                },
            );
        }
        return Err(AppError::blocked(if verdict.level == "blocked" {
            verdict.reasons.join("；")
        } else {
            "此命令必须由用户确认后运行".into()
        }));
    }
    if ["exe", "custom"].contains(&opt.kind.as_str()) && !settings.allow_unknown_exe {
        return Err(AppError::permission("请在设置中开启自定义 exe 执行权限"));
    }
    if opt.elevated && !super::system_cmds::is_elevated()? {
        return Err(AppError::permission(
            "内嵌管理员终端需要先以管理员身份启动 CmdDeck，请关闭后右键选择以管理员身份运行",
        ));
    }
    Ok(())
}
pub fn run_internal(
    app: &AppHandle,
    id: &str,
    args: &HashMap<String, String>,
    source: &str,
    confirmed: bool,
) -> AppResult<TerminalInfo> {
    run_in_workspace(app, id, args, source, confirmed, None)
}
fn run_in_workspace(
    app: &AppHandle,
    id: &str,
    args: &HashMap<String, String>,
    source: &str,
    confirmed: bool,
    workspace_id: Option<&str>,
) -> AppResult<TerminalInfo> {
    let st = app.state::<AppState>();
    let p = db::presets::get(&st.db, id)?;
    if p.confirm && !confirmed {
        return Err(AppError::blocked("此预设要求人工确认，自动运行已阻止"));
    }
    let mut opt = options(&p, args, source)?;
    if p.kind == "shell" && p.program.is_empty() && p.args.is_empty() {
        opt.kind = if p.name.to_uppercase().contains("CMD") {
            "cmd".into()
        } else {
            st.settings().default_shell
        };
        opt.interactive = true;
        opt.use_shell = false;
    }
    crate::productivity::prepare(&st, &mut opt, workspace_id)?;
    validate_spawn(&st, &opt, confirmed)?;
    let info = st.pty.spawn_in_workspace(opt, workspace_id.unwrap_or(""))?;
    db::presets::record_run(&st.db, id)?;
    Ok(info)
}
#[tauri::command]
pub fn run_preset(
    app: AppHandle,
    preset_id: String,
    args: HashMap<String, String>,
    source: Option<String>,
    confirmed: Option<bool>,
    workspace_id: Option<String>,
) -> AppResult<TerminalInfo> {
    run_in_workspace(
        &app,
        &preset_id,
        &args,
        source.as_deref().unwrap_or("manual"),
        confirmed.unwrap_or(false),
        workspace_id.as_deref(),
    )
}
#[tauri::command]
pub fn spawn_terminal(
    state: State<'_, AppState>,
    options: SpawnOptions,
    confirmed: Option<bool>,
) -> AppResult<TerminalInfo> {
    validate_spawn(&state, &options, confirmed.unwrap_or(false))?;
    state.pty.spawn(options)
}
#[tauri::command]
pub fn open_shell(
    state: State<'_, AppState>,
    kind: String,
    workspace_id: Option<String>,
) -> AppResult<TerminalInfo> {
    let mut p = Preset::default();
    p.id.clear();
    p.name = kind.clone();
    p.kind = kind;
    p.use_shell = false;
    let mut opt = options(&p, &HashMap::new(), "manual")?;
    opt.interactive = true;
    if !state.settings().shell_path.is_empty() {
        opt.program = state.settings().shell_path;
    }
    crate::productivity::prepare(&state, &mut opt, workspace_id.as_deref())?;
    validate_spawn(&state, &opt, false)?;
    state
        .pty
        .spawn_in_workspace(opt, workspace_id.as_deref().unwrap_or(""))
}
#[tauri::command]
pub fn write_terminal(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> AppResult<()> {
    state.pty.write(&session_id, &data)
}
#[tauri::command]
pub fn resize_terminal(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> AppResult<()> {
    state.pty.resize(&session_id, cols, rows)
}
#[tauri::command]
pub fn kill_terminal(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.pty.kill(&session_id)
}
#[tauri::command]
pub fn close_terminal(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.pty.close(&session_id)
}
#[tauri::command]
pub fn get_terminal_snapshot(state: State<'_, AppState>, session_id: String) -> AppResult<String> {
    state.pty.snapshot(&session_id)
}
#[tauri::command]
pub fn list_terminal_sessions(state: State<'_, AppState>) -> Vec<TerminalInfo> {
    state.pty.list()
}
pub fn wait_internal(app: &AppHandle, id: &str, timeout: u64) -> AppResult<Option<i32>> {
    let s = app.state::<AppState>().pty.get(id)?;
    let start = std::time::Instant::now();
    loop {
        if let Some(code) = s.info.lock().exit_code {
            return Ok(Some(code));
        }
        if start.elapsed().as_millis() >= timeout as u128 {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}
#[tauri::command]
pub async fn wait_terminal_exit(
    app: AppHandle,
    session_id: String,
    timeout_ms: Option<u64>,
) -> AppResult<Option<i32>> {
    tauri::async_runtime::spawn_blocking(move || {
        wait_internal(&app, &session_id, timeout_ms.unwrap_or(60000))
    })
    .await
    .map_err(|e| AppError::other(e.to_string()))?
}

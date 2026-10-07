//! ConPTY 会话：读取、等待和输入独立，避免长命令阻塞界面。
use crate::{
    db::{audit, models::*},
    error::{AppError, AppResult},
    state::*,
};
use parking_lot::Mutex;
#[cfg(not(windows))]
use portable_pty::ChildKiller;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde_json::json;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::Arc,
};
use tauri::{AppHandle, Manager};

pub struct Session {
    pub info: Mutex<TerminalInfo>,
    output: Mutex<String>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Box<dyn Write + Send>>,
    #[cfg(not(windows))]
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    #[cfg(windows)]
    process: std::os::windows::io::OwnedHandle,
    #[cfg(windows)]
    job: Option<OwnedHandle>,
}
pub struct PtyManager {
    app: AppHandle,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}
fn exec_error(e: impl std::fmt::Display) -> AppError {
    AppError::exec(e.to_string())
}
#[cfg(windows)]
fn process_job(process: &OwnedHandle) -> AppResult<Option<OwnedHandle>> {
    use windows_sys::Win32::System::{JobObjects::*, Threading::*};
    unsafe {
        let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if raw.is_null() {
            return Err(exec_error(std::io::Error::last_os_error()));
        }
        let job = OwnedHandle::from_raw_handle(raw);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            raw,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            std::mem::size_of_val(&limits) as u32,
        ) == 0
        {
            let err = std::io::Error::last_os_error();
            TerminateProcess(process.as_raw_handle(), 1);
            return Err(exec_error(err));
        }
        if AssignProcessToJobObject(raw, process.as_raw_handle()) == 0 {
            let err = std::io::Error::last_os_error();
            let mut code = 0;
            if GetExitCodeProcess(process.as_raw_handle(), &mut code) != 0 && code != 259 {
                return Ok(None);
            }
            TerminateProcess(process.as_raw_handle(), 1);
            return Err(exec_error(format!("无法集中管理子进程：{err}")));
        }
        Ok(Some(job))
    }
}
/// 顺序：预设目录、全局默认目录、用户主目录。避免从安装目录运行相对路径。
pub fn resolve_working_directory(preset: &str, default: &str) -> AppResult<String> {
    let chosen = if !preset.trim().is_empty() {
        preset.trim().to_owned()
    } else if !default.trim().is_empty() {
        default.trim().to_owned()
    } else {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map_err(|_| AppError::validation("无法找到用户主目录，请在设置中指定默认工作目录"))?
    };
    let path = std::path::Path::new(&chosen);
    if !path.is_absolute() {
        return Err(AppError::validation(
            "工作目录需要使用绝对路径，请通过选择文件夹按钮设置",
        ));
    }
    if !path.is_dir() {
        return Err(AppError::validation(format!(
            "工作目录不存在或不是文件夹：{chosen}"
        )));
    }
    Ok(chosen)
}

#[cfg(test)]
mod directory_tests {
    use super::*;
    #[test]
    fn directory_precedence_and_missing_path() {
        let temp = std::env::temp_dir();
        let text = temp.to_string_lossy();
        assert_eq!(resolve_working_directory(&text, "invalid").unwrap(), text);
        assert_eq!(resolve_working_directory("", &text).unwrap(), text);
        assert!(resolve_working_directory("relative/path", "").is_err());
        assert!(resolve_working_directory(
            &temp
                .join("cmddeck-nonexistent-directory-924981")
                .to_string_lossy(),
            ""
        )
        .is_err());
        assert!(!resolve_working_directory("", "").unwrap().is_empty());
    }
}
impl PtyManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            sessions: Mutex::new(HashMap::new()),
        }
    }
    pub fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        self.sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::not_found("终端会话不存在"))
    }
    pub fn list(&self) -> Vec<TerminalInfo> {
        self.sessions
            .lock()
            .values()
            .map(|s| s.info.lock().clone())
            .collect()
    }
    pub fn snapshot(&self, id: &str) -> AppResult<String> {
        Ok(self.get(id)?.output.lock().clone())
    }
    pub fn write(&self, id: &str, data: &str) -> AppResult<()> {
        let s = self.get(id)?;
        let mut w = s.writer.lock();
        w.write_all(data.as_bytes())?;
        w.flush()?;
        Ok(())
    }
    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> AppResult<()> {
        let s = self.get(id)?;
        s.master
            .lock()
            .as_ref()
            .ok_or_else(|| AppError::validation("终端已退出"))?
            .resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(exec_error)?;
        let mut i = s.info.lock();
        i.cols = cols;
        i.rows = rows;
        Ok(())
    }
    pub fn kill(&self, id: &str) -> AppResult<()> {
        let s = self.get(id)?;
        if s.info.lock().status == "running" {
            // portable-pty 0.9 的 Windows clone_killer 把 BOOL 成功判据反写；
            // 使用拥有生命周期的进程句柄直接调用 Win32，避免误报“操作成功”。
            s.info.lock().status = "killed".into();
            #[cfg(windows)]
            {
                use windows_sys::Win32::System::JobObjects::TerminateJobObject;
                use windows_sys::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess};
                unsafe {
                    let stopped = if let Some(job) = &s.job {
                        TerminateJobObject(job.as_raw_handle(), 1)
                    } else {
                        TerminateProcess(s.process.as_raw_handle(), 1)
                    };
                    if stopped == 0 {
                        let mut code = 259;
                        if GetExitCodeProcess(s.process.as_raw_handle(), &mut code) == 0
                            || code == 259
                        {
                            s.info.lock().status = "running".into();
                            return Err(exec_error(std::io::Error::last_os_error()));
                        }
                    }
                }
            }
            #[cfg(not(windows))]
            s.killer.lock().kill().map_err(exec_error)?;
        }
        Ok(())
    }
    pub fn close(&self, id: &str) -> AppResult<()> {
        self.kill(id)?;
        self.sessions.lock().remove(id);
        Ok(())
    }
    pub fn kill_all(&self) {
        for s in self.list() {
            let _ = self.kill(&s.session_id);
        }
    }
    pub fn spawn(&self, opt: SpawnOptions) -> AppResult<TerminalInfo> {
        // 插入会话之前持锁检查数量，避免并发启动绕过上限。
        let mut sessions = self.sessions.lock();
        let state = self.app.state::<AppState>();
        let settings = state.settings();
        if sessions
            .values()
            .filter(|s| s.info.lock().exit_code.is_none())
            .count()
            >= settings.max_concurrent_sessions as usize
        {
            return Err(AppError::validation(
                "运行中的终端已达到上限，请先停止部分任务",
            ));
        }
        // 已退出且没有窗口保留的后台会话最多保留 128 条，历史仍在 SQLite 中。
        if sessions.len() > 128 {
            sessions.retain(|_, s| s.info.lock().exit_code.is_none());
        }
        let (program, args) = opt.resolve();
        if program.is_empty() {
            return Err(AppError::validation("请填写可执行程序路径"));
        }
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: opt.rows.max(1),
                cols: opt.cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(exec_error)?;
        let mut cmd = CommandBuilder::new(&program);
        cmd.args(&args);
        let cwd = resolve_working_directory(&opt.working_dir, &settings.default_working_dir)?;
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("PYTHONIOENCODING", "utf-8");
        if opt.runtime.kind == "conda" || opt.source == "environment" {
            // Conda run 会写临时批处理；使用应用可写目录，避免受限 TEMP 导致启动失败。
            let temp = state.sub_dir("tmp")?;
            for key in ["TEMP", "TMP"] {
                if !opt.env.iter().any(|e| e.name.eq_ignore_ascii_case(key)) {
                    cmd.env(key, &temp);
                }
            }
        }
        for env in &opt.env {
            cmd.env(&env.name, &env.value);
        }
        crate::environments::apply_binding(&mut cmd, &opt.runtime, &opt.env)?;
        let mut child = pair.slave.spawn_command(cmd).map_err(exec_error)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(exec_error)?;
        let writer = pair.master.take_writer().map_err(exec_error)?;
        let preset_name = if opt.preset_id.is_empty() {
            String::new()
        } else {
            crate::db::presets::get(&state.db, &opt.preset_id)
                .map(|p| p.name)
                .unwrap_or_default()
        };
        let info = TerminalInfo {
            session_id: new_id(),
            title: if opt.title.is_empty() {
                program.clone()
            } else {
                opt.title.clone()
            },
            preset_id: opt.preset_id.clone(),
            preset_name,
            kind: opt.kind.clone(),
            command: join_command_line(&program, &args),
            cwd,
            started_at: now_ms(),
            status: "running".into(),
            exit_code: None,
            cols: opt.cols,
            rows: opt.rows,
            elevated: opt.elevated,
            temporary: opt.source != "manual",
        };
        #[cfg(windows)]
        let process = unsafe {
            BorrowedHandle::borrow_raw(
                child
                    .as_raw_handle()
                    .ok_or_else(|| AppError::exec("无法获取进程句柄"))?,
            )
            .try_clone_to_owned()?
        };
        #[cfg(windows)]
        let job = process_job(&process)?;
        let session = Arc::new(Session {
            info: Mutex::new(info.clone()),
            output: Mutex::new(String::new()),
            master: Mutex::new(Some(pair.master)),
            writer: Mutex::new(writer),
            #[cfg(not(windows))]
            killer: Mutex::new(child.clone_killer()),
            #[cfg(windows)]
            process,
            #[cfg(windows)]
            job,
        });
        sessions.insert(info.session_id.clone(), session.clone());
        drop(sessions);
        state.emit(&self.app, EV_PTY_OPEN, info.clone());
        let app = self.app.clone();
        let read_session = session.clone();
        let encoding = if settings.encoding == "gbk" {
            encoding_rs::GBK
        } else {
            encoding_rs::UTF_8
        };
        let read_thread = std::thread::spawn(move || {
            // 增量解码保存跨 read 的中文多字节字符。
            let mut decoder = encoding.new_decoder();
            let mut buf = [0u8; 8192];
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(n) => n,
                    Err(_) => 0,
                };
                let mut text = String::with_capacity(n * 4 + 16);
                let _ = decoder.decode_to_string(&buf[..n], &mut text, n == 0);
                if !text.is_empty() {
                    let mut out = read_session.output.lock();
                    out.push_str(&text);
                    if out.len() > 2_000_000 {
                        let mut cut = out.len() - 1_000_000;
                        while !out.is_char_boundary(cut) {
                            cut += 1;
                        }
                        out.drain(..cut);
                    }
                    drop(out);
                    app.state::<AppState>().emit(
                        &app,
                        EV_PTY_DATA,
                        json!({"sessionId": read_session.info.lock().session_id, "data": text}),
                    );
                }
                if n == 0 {
                    break;
                }
            }
        });
        let app = self.app.clone();
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
            #[cfg(windows)]
            if let Some(job) = &session.job {
                unsafe {
                    windows_sys::Win32::System::JobObjects::TerminateJobObject(
                        job.as_raw_handle(),
                        1,
                    );
                }
            }
            // 进程退出后关闭 ConPTY，使输出管道发送 EOF，再等读取线程排空。
            session.master.lock().take();
            let _ = read_thread.join();
            let i = {
                let mut i = session.info.lock();
                i.exit_code = Some(code);
                if i.status != "killed" {
                    i.status = "exited".into();
                }
                i.clone()
            };
            let st = app.state::<AppState>();
            let settings = st.settings();
            if settings.audit_enabled {
                let _ = audit::insert(
                    &st.db,
                    &AuditLog {
                        id: 0,
                        at: now_ms(),
                        preset_id: i.preset_id.clone(),
                        preset_name: i.preset_name.clone(),
                        command: i.command.clone(),
                        cwd: i.cwd.clone(),
                        level: 0,
                        status: if i.status == "killed" {
                            "killed"
                        } else if code == 0 {
                            "success"
                        } else {
                            "failed"
                        }
                        .into(),
                        exit_code: Some(code),
                        message: String::new(),
                        duration_ms: now_ms() - i.started_at,
                        source: opt.source,
                    },
                );
            }
            if settings.history_enabled {
                let _ = audit::history_insert(
                    &st.db,
                    &TerminalHistory {
                        id: 0,
                        session_id: i.session_id.clone(),
                        preset_id: i.preset_id.clone(),
                        preset_name: i.preset_name.clone(),
                        title: i.title.clone(),
                        command: i.command.clone(),
                        cwd: i.cwd.clone(),
                        kind: i.kind.clone(),
                        started_at: i.started_at,
                        ended_at: now_ms(),
                        exit_code: Some(code),
                        output_tail: session.output.lock().clone(),
                    },
                );
                let _ = audit::history_prune(&st.db, settings.history_limit as i64);
            }
            st.emit(
                &app,
                EV_PTY_EXIT,
                json!({"sessionId":i.session_id,"exitCode":code,"status":i.status}),
            );
        });
        Ok(info)
    }
}

//! 单任务 UAC 工作者：不启动第二个 GUI，通过本机随机端口和 256 位一次性令牌传输 ConPTY。
//! 连接仅绑定 127.0.0.1；每次只执行已确认的一个请求，断开连接即终止进程树。
#![cfg(windows)]
use crate::{
    db::models::SpawnOptions,
    error::{AppError, AppResult},
};
use parking_lot::{Condvar, Mutex};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::windows::io::{AsRawHandle, BorrowedHandle},
    sync::Arc,
    time::{Duration, Instant},
};
const MAX_FRAME: usize = 1024 * 1024;
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub options: SpawnOptions,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub temp: String,
}
#[derive(Serialize, Deserialize)]
enum Event {
    Ready(u32),
    Output(Vec<u8>),
    Exit(i32),
    Error(String),
    Input(Vec<u8>),
    Resize(u16, u16),
    Kill,
}
fn send<T: Serialize>(stream: &mut TcpStream, value: &T) -> std::io::Result<()> {
    let data = serde_json::to_vec(value)?;
    if data.len() > MAX_FRAME {
        return Err(std::io::Error::other("管理员终端消息过大"));
    }
    stream.write_all(&(data.len() as u32).to_le_bytes())?;
    stream.write_all(&data)?;
    stream.flush()
}
fn receive<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> std::io::Result<T> {
    let mut size = [0u8; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err(std::io::Error::other("管理员终端消息长度无效"));
    }
    let mut data = vec![0; size];
    stream.read_exact(&mut data)?;
    Ok(serde_json::from_slice(&data)?)
}
pub struct Remote {
    stream: Mutex<TcpStream>,
    exit: Mutex<Option<i32>>,
    wake: Condvar,
    pub pid: u32,
}
impl Remote {
    pub fn kill(&self) -> AppResult<()> {
        send(&mut self.stream.lock(), &Event::Kill)?;
        Ok(())
    }
    pub fn resize(&self, cols: u16, rows: u16) -> AppResult<()> {
        send(&mut self.stream.lock(), &Event::Resize(cols, rows))?;
        Ok(())
    }
    pub fn wait(&self) -> i32 {
        let mut code = self.exit.lock();
        while code.is_none() {
            self.wake.wait(&mut code);
        }
        code.unwrap_or(-1)
    }
    fn finish(&self, code: i32) {
        *self.exit.lock() = Some(code);
        self.wake.notify_all();
    }
}
pub struct RemoteReader {
    stream: TcpStream,
    remote: Arc<Remote>,
    pending: Vec<u8>,
    offset: usize,
}
impl Read for RemoteReader {
    fn read(&mut self, dest: &mut [u8]) -> std::io::Result<usize> {
        if dest.is_empty() {
            return Ok(0);
        }
        while self.offset == self.pending.len() {
            match receive::<Event>(&mut self.stream) {
                Ok(Event::Output(data)) => {
                    self.pending = data;
                    self.offset = 0;
                }
                Ok(Event::Exit(code)) => {
                    self.remote.finish(code);
                    return Ok(0);
                }
                _ => {
                    self.remote.finish(-1);
                    return Ok(0);
                }
            }
        }
        let n = dest.len().min(self.pending.len() - self.offset);
        dest[..n].copy_from_slice(&self.pending[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
}
pub struct RemoteWriter(pub Arc<Remote>);
impl Write for RemoteWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        for chunk in data.chunks(8192) {
            send(&mut self.0.stream.lock(), &Event::Input(chunk.to_vec()))?;
        }
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn authenticated_peer(stream: &mut TcpStream, token: &str) -> AppResult<bool> {
    // Windows accept 可能继承监听器的非阻塞模式，认证与帧读取需要阻塞 I/O。
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    Ok(receive::<String>(stream).ok().as_deref() == Some(token))
}
pub fn spawn(
    request: Request,
) -> AppResult<(Arc<Remote>, Box<dyn Read + Send>, Box<dyn Write + Send>)> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Com::{
            CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
        },
        UI::Shell::{
            ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
            SHELLEXECUTEINFOW,
        },
    };
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let args = format!(
        "--elevated-worker {} {token}",
        listener.local_addr()?.port()
    );
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let exe = wide(&std::env::current_exe()?.to_string_lossy());
    let args = wide(&args);
    let verb = wide("runas");
    unsafe {
        let initialized = CoInitializeEx(
            std::ptr::null(),
            COINIT_APARTMENTTHREADED as u32 | COINIT_DISABLE_OLE1DDE as u32,
        ) >= 0;
        let mut launch: SHELLEXECUTEINFOW = std::mem::zeroed();
        launch.cbSize = std::mem::size_of_val(&launch) as u32;
        launch.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
        launch.lpVerb = verb.as_ptr();
        launch.lpFile = exe.as_ptr();
        launch.lpParameters = args.as_ptr();
        launch.nShow = 0;
        let success = ShellExecuteExW(&mut launch);
        let failure = std::io::Error::last_os_error();
        if initialized {
            CoUninitialize();
        }
        if success == 0 {
            return Err(AppError::permission(format!(
                "管理员启动失败或 UAC 已取消：{}",
                failure
            )));
        }
        if !launch.hProcess.is_null() {
            CloseHandle(launch.hProcess);
        }
    }
    let start = Instant::now();
    let mut stream = loop {
        if start.elapsed() > Duration::from_secs(30) {
            return Err(AppError::exec("管理员终端连接超时，请重试"));
        }
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                if !authenticated_peer(&mut stream, &token)? {
                    continue;
                }
                break stream;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() > Duration::from_secs(30) {
                    return Err(AppError::exec("管理员终端连接超时，请重试"));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e.into()),
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    send(&mut stream, &request)?;
    let pid = match receive::<Event>(&mut stream)? {
        Event::Ready(pid) => pid,
        Event::Error(e) => return Err(AppError::exec(e)),
        _ => return Err(AppError::exec("管理员终端响应无效")),
    };
    stream.set_read_timeout(None)?;
    let remote = Arc::new(Remote {
        stream: Mutex::new(stream.try_clone()?),
        exit: Mutex::new(None),
        wake: Condvar::new(),
        pid,
    });
    Ok((
        remote.clone(),
        Box::new(RemoteReader {
            stream,
            remote: remote.clone(),
            pending: vec![],
            offset: 0,
        }),
        Box::new(RemoteWriter(remote)),
    ))
}
/// 程序入口在装配 Tauri 前分流，避免单实例插件拦截提权进程。
pub fn worker_entry() -> bool {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) != Some("--elevated-worker") {
        return false;
    }
    let result = (|| -> AppResult<()> {
        if !crate::commands::system_cmds::is_elevated()? {
            return Err(AppError::permission("工作者需要管理员身份"));
        }
        let port = args
            .get(2)
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|p| *p > 0)
            .ok_or_else(|| AppError::validation("端口无效"))?;
        let token = args
            .get(3)
            .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| AppError::validation("连接令牌无效"))?;
        let mut stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_secs(5),
        )?;
        send(&mut stream, token)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let request: Request = receive(&mut stream)?;
        stream.set_read_timeout(None)?;
        if let Err(e) = run_worker(request, &mut stream) {
            let _ = send(&mut stream, &Event::Error(e.to_string()));
            return Err(e);
        }
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("管理员工作者失败：{e}");
    }
    true
}
fn run_worker(request: Request, stream: &mut TcpStream) -> AppResult<()> {
    let pair = native_pty_system()
        .openpty(PtySize {
            cols: request.options.cols.max(1),
            rows: request.options.rows.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::exec(e.to_string()))?;
    let mut cmd = CommandBuilder::new(&request.program);
    cmd.args(&request.args);
    cmd.cwd(&request.cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("PYTHONIOENCODING", "utf-8");
    if !request.temp.is_empty() {
        cmd.env("TEMP", &request.temp);
        cmd.env("TMP", &request.temp);
    }
    for var in &request.options.env {
        cmd.env(&var.name, &var.value);
    }
    crate::environments::apply_binding(&mut cmd, &request.options.runtime, &request.options.env)?;
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::exec(e.to_string()))?;
    drop(pair.slave);
    let process = unsafe {
        BorrowedHandle::borrow_raw(
            child
                .as_raw_handle()
                .ok_or_else(|| AppError::exec("无法获取进程"))?,
        )
        .try_clone_to_owned()?
    };
    // 极短任务可能在加入 Job 前退出，此时保留输出与退出码而非误报启动失败。
    let job = crate::pty::process_job(&process)?.map(Arc::new);
    let process = Arc::new(process);
    let pid =
        unsafe { windows_sys::Win32::System::Threading::GetProcessId(process.as_raw_handle()) };
    send(stream, &Event::Ready(pid))?;
    let master = Arc::new(Mutex::new(Some(pair.master)));
    let mut reader = master
        .lock()
        .as_ref()
        .unwrap()
        .try_clone_reader()
        .map_err(|e| AppError::exec(e.to_string()))?;
    let writer = Arc::new(Mutex::new(
        master
            .lock()
            .as_ref()
            .unwrap()
            .take_writer()
            .map_err(|e| AppError::exec(e.to_string()))?,
    ));
    let output = Arc::new(Mutex::new(stream.try_clone()?));
    let outgoing = output.clone();
    let reading = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            if send(&mut outgoing.lock(), &Event::Output(buf[..n].to_vec())).is_err() {
                break;
            }
        }
    });
    let mut input = stream.try_clone()?;
    let control = job.clone();
    let control_process = process.clone();
    let resize = master.clone();
    std::thread::spawn(move || loop {
        match receive::<Event>(&mut input) {
            Ok(Event::Input(data)) => {
                if writer.lock().write_all(&data).is_err() {
                    break;
                }
            }
            Ok(Event::Resize(cols, rows)) => {
                if let Some(m) = resize.lock().as_ref() {
                    let _ = m.resize(PtySize {
                        cols: cols.max(1),
                        rows: rows.max(1),
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
            }
            Ok(Event::Kill) | Err(_) => {
                unsafe {
                    if let Some(job) = &control {
                        windows_sys::Win32::System::JobObjects::TerminateJobObject(
                            job.as_raw_handle(),
                            1,
                        );
                    } else {
                        windows_sys::Win32::System::Threading::TerminateProcess(
                            control_process.as_raw_handle(),
                            1,
                        );
                    }
                }
                break;
            }
            _ => break,
        }
    });
    let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
    unsafe {
        if let Some(job) = &job {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(job.as_raw_handle(), 1);
        }
    }
    master.lock().take();
    let _ = reading.join();
    send(&mut output.lock(), &Event::Exit(code))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authentication_waits_for_peer_frame_on_nonblocking_listener() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let peer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            send(&mut client, &"test-token").unwrap();
        });
        assert!(authenticated_peer(&mut server, "test-token").unwrap());
        peer.join().unwrap();
    }
    #[test]
    fn worker_streams_real_conpty_output_and_exit_without_second_gui() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let request = Request {
            options: crate::commands::terminal_cmds::options(
                &crate::db::models::Preset::default(),
                &Default::default(),
                "manual",
            )
            .unwrap(),
            program: std::env::var("COMSPEC").unwrap_or("cmd.exe".into()),
            args: vec!["/d".into(), "/c".into(), "echo WORKER_BRIDGE_OK".into()],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            temp: String::new(),
        };
        let worker = std::thread::spawn(move || run_worker(request, &mut server));
        assert!(matches!(receive::<Event>(&mut client).unwrap(),Event::Ready(pid) if pid>0));
        send(&mut client, &Event::Resize(100, 25)).unwrap();
        let mut output = Vec::new();
        loop {
            match receive::<Event>(&mut client).unwrap() {
                Event::Output(data) => {
                    if data.windows(4).any(|w| w == b"\x1b[6n") {
                        send(&mut client, &Event::Input(b"\x1b[1;1R".to_vec())).unwrap();
                    }
                    output.extend(data);
                }
                Event::Exit(code) => {
                    assert_eq!(code, 0);
                    break;
                }
                _ => panic!("工作者返回意外事件"),
            }
        }
        assert!(String::from_utf8_lossy(&output).contains("WORKER_BRIDGE_OK"));
        worker.join().unwrap().unwrap();
    }
    #[test]
    fn protocol_rejects_oversized_frames_before_allocation() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut writer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut reader, _) = listener.accept().unwrap();
        writer
            .write_all(&((MAX_FRAME + 1) as u32).to_le_bytes())
            .unwrap();
        assert!(receive::<Event>(&mut reader).is_err());
    }
}

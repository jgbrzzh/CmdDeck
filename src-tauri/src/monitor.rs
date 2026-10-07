//! Win32 只读进程与 TCP 监听端口采样；只允许停止 CmdDeck 自己管理的任务。
use crate::{
    db::models::TerminalInfo,
    error::{AppError, AppResult},
    state::AppState,
};
use serde::Serialize;
use std::{collections::HashMap, sync::OnceLock, time::Instant};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    pub address: String,
    pub port: u16,
    pub pid: u32,
    pub session_id: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInfo {
    pub terminal: TerminalInfo,
    pub pids: Vec<u32>,
    pub memory_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
    pub ports: Vec<PortInfo>,
}

#[cfg(windows)]
pub fn ports() -> AppResult<Vec<PortInfo>> {
    use windows_sys::Win32::NetworkManagement::IpHelper::*;
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
    let mut result = vec![];
    for af in [AF_INET, AF_INET6] {
        let mut size = 0u32;
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                af as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            );
        }
        if size < 4 {
            continue;
        }
        // u32 缓冲保证结构体对齐；表大小可能在两次调用间增长，最多重试三次。
        for _ in 0..3 {
            let mut buf = vec![0u32; (size as usize).div_ceil(4)];
            let code = unsafe {
                GetExtendedTcpTable(
                    buf.as_mut_ptr() as _,
                    &mut size,
                    0,
                    af as u32,
                    TCP_TABLE_OWNER_PID_LISTENER,
                    0,
                )
            };
            if code == 122 {
                continue;
            }
            if code != 0 {
                return Err(AppError::io(format!(
                    "读取 TCP 监听端口失败：Windows 错误 {code}"
                )));
            }
            let count = buf[0] as usize;
            let bytes = buf.len() * 4;
            let stride = if af == AF_INET {
                std::mem::size_of::<MIB_TCPROW_OWNER_PID>()
            } else {
                std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>()
            };
            if count > (bytes - 4) / stride {
                return Err(AppError::io("TCP 端口表大小无效"));
            }
            for i in 0..count {
                let ptr = unsafe { (buf.as_ptr() as *const u8).add(4 + i * stride) };
                let (address, port, pid) = unsafe {
                    if af == AF_INET {
                        let row = std::ptr::read_unaligned(ptr as *const MIB_TCPROW_OWNER_PID);
                        (
                            std::net::Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()).to_string(),
                            u16::from_be(row.dwLocalPort as u16),
                            row.dwOwningPid,
                        )
                    } else {
                        let row = std::ptr::read_unaligned(ptr as *const MIB_TCP6ROW_OWNER_PID);
                        (
                            std::net::Ipv6Addr::from(row.ucLocalAddr).to_string(),
                            u16::from_be(row.dwLocalPort as u16),
                            row.dwOwningPid,
                        )
                    }
                };
                result.push(PortInfo {
                    address,
                    port,
                    pid,
                    session_id: String::new(),
                });
            }
            break;
        }
    }
    result.sort_by_key(|p| (p.port, p.pid));
    Ok(result)
}
#[cfg(not(windows))]
pub fn ports() -> AppResult<Vec<PortInfo>> {
    Err(AppError::other("端口面板目前支持 Windows"))
}

#[cfg(windows)]
fn sample(pids: &[u32]) -> Option<(u64, u64)> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::{ProcessStatus::*, Threading::*},
    };
    let (mut mem, mut ticks, mut found) = (0u64, 0u64, false);
    for pid in pids {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, *pid);
            if handle.is_null() {
                continue;
            }
            let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            counters.cb = std::mem::size_of_val(&counters) as u32;
            let mut create: FILETIME = std::mem::zeroed();
            let mut exit = create;
            let mut kernel = create;
            let mut user = create;
            if GetProcessMemoryInfo(handle, &mut counters, counters.cb) != 0
                && GetProcessTimes(handle, &mut create, &mut exit, &mut kernel, &mut user) != 0
            {
                mem += counters.WorkingSetSize as u64;
                ticks += ((kernel.dwHighDateTime as u64) << 32 | kernel.dwLowDateTime as u64)
                    + ((user.dwHighDateTime as u64) << 32 | user.dwLowDateTime as u64);
                found = true;
            }
            CloseHandle(handle);
        }
    }
    found.then_some((mem, ticks))
}
#[cfg(not(windows))]
fn sample(_pids: &[u32]) -> Option<(u64, u64)> {
    None
}
pub fn tasks(state: &AppState) -> AppResult<Vec<TaskInfo>> {
    static PREVIOUS: OnceLock<parking_lot::Mutex<HashMap<String, (Instant, u64)>>> =
        OnceLock::new();
    let mut previous = PREVIOUS.get_or_init(Default::default).lock();
    let now = Instant::now();
    let mut endpoints = ports()?;
    let mut result = vec![];
    let all = state.pty.list();
    previous.retain(|id, _| {
        all.iter()
            .any(|i| i.session_id == *id && i.status == "running")
    });
    for info in all {
        let pids = state.pty.process_ids(&info.session_id)?;
        let measurement = if info.status == "running" {
            sample(&pids)
        } else {
            None
        };
        let (mut memory, mut cpu) = (None, None);
        if let Some((mem, ticks)) = measurement {
            memory = Some(mem);
            if let Some((time, old)) = previous.insert(info.session_id.clone(), (now, ticks)) {
                let elapsed = now.duration_since(time).as_secs_f64();
                if elapsed > 0.1 {
                    cpu = Some(
                        (ticks.saturating_sub(old) as f64 / 10_000_000.0 / elapsed * 100.0)
                            .max(0.0),
                    );
                }
            }
        }
        let mut owned = vec![];
        for p in &mut endpoints {
            if pids.contains(&p.pid) {
                p.session_id = info.session_id.clone();
                owned.push(p.clone());
            }
        }
        result.push(TaskInfo {
            terminal: info,
            pids,
            memory_bytes: memory,
            cpu_percent: cpu,
            ports: owned,
        });
    }
    result.sort_by_key(|t| std::cmp::Reverse(t.terminal.started_at));
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn identifies_a_real_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let entries = super::ports().unwrap();
        assert!(entries
            .iter()
            .any(|p| p.port == port && p.pid == std::process::id()));
    }
    #[cfg(windows)]
    #[test]
    fn samples_current_process() {
        let (memory, _) = super::sample(&[std::process::id()]).unwrap();
        assert!(memory > 0);
    }
}

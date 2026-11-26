use std::fs;
use std::net::Ipv6Addr;
use std::sync::atomic::{AtomicU64, Ordering};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::{command, run_checked, s, Backend};
use crate::net::{ApplyOptions, NetError, Target};

pub const CAN_RELAUNCH: bool = true;
pub const SUPPORTS_PERSISTENT: bool = true;
pub const SUPPORTS_NODAD: bool = false;

static SCRIPT_SEQ: AtomicU64 = AtomicU64::new(0);

struct WindowsBackend;

static BACKEND: WindowsBackend = WindowsBackend;

pub fn backend() -> &'static dyn Backend {
    &BACKEND
}

fn store(opts: ApplyOptions) -> &'static str {
    if opts.persistent {
        "persistent"
    } else {
        "active"
    }
}

fn add_line(target: &Target, addr: Ipv6Addr, opts: ApplyOptions) -> String {
    format!(
        "interface ipv6 add address interface={} address={}/{} store={}",
        target.index,
        addr,
        target.prefix_len,
        store(opts)
    )
}

fn remove_line(target: &Target, addr: Ipv6Addr, store: &str) -> String {
    format!(
        "interface ipv6 delete address interface={} address={} store={}",
        target.index, addr, store
    )
}

fn remove_args(target: &Target, addr: Ipv6Addr, store: &str) -> Vec<String> {
    vec![
        s("interface"),
        s("ipv6"),
        s("delete"),
        s("address"),
        format!("interface={}", target.index),
        format!("address={}", addr),
        format!("store={}", store),
    ]
}

fn run_script(lines: &[String]) {
    let seq = SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("proxyv6-mmo-{}-{}.netsh", std::process::id(), seq));
    let mut body = lines.join("\r\n");
    body.push_str("\r\n");
    if fs::write(&path, body).is_err() {
        return;
    }
    let _ = command("netsh")
        .arg("-f")
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .output();
    let _ = fs::remove_file(&path);
}

fn netsh(args: &[String]) -> Result<(), NetError> {
    run_checked("netsh", args)
}

impl Backend for WindowsBackend {
    fn is_elevated(&self) -> bool {
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut ret_len = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut TOKEN_ELEVATION as *mut _,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret_len,
            );
            CloseHandle(token);
            ok != 0 && elevation.TokenIsElevated != 0
        }
    }

    fn check_tools(&self) -> Result<(), NetError> {
        run_checked("netsh", &[s("interface"), s("ipv6"), s("show"), s("interfaces")])
            .map_err(|e| NetError::Command(format!("Không tìm thấy hoặc không chạy được netsh: {e}")))
    }

    fn add_one(&self, target: &Target, addr: Ipv6Addr, opts: ApplyOptions) -> Result<(), NetError> {
        netsh(&[
            s("interface"),
            s("ipv6"),
            s("add"),
            s("address"),
            format!("interface={}", target.index),
            format!("address={}/{}", addr, target.prefix_len),
            format!("store={}", store(opts)),
        ])
    }

    fn remove_one(&self, target: &Target, addr: Ipv6Addr) -> Result<(), NetError> {
        let persistent = netsh(&remove_args(target, addr, "persistent"));
        let active = netsh(&remove_args(target, addr, "active"));
        match (persistent, active) {
            (_, Ok(())) | (Ok(()), _) => Ok(()),
            (_, Err(e)) => Err(e),
        }
    }

    fn add_batch(&self, target: &Target, addrs: &[Ipv6Addr], opts: ApplyOptions) {
        let lines: Vec<String> = addrs.iter().map(|a| add_line(target, *a, opts)).collect();
        run_script(&lines);
    }

    fn remove_batch(&self, target: &Target, addrs: &[Ipv6Addr]) {
        let lines: Vec<String> = addrs
            .iter()
            .flat_map(|a| {
                [
                    remove_line(target, *a, "persistent"),
                    remove_line(target, *a, "active"),
                ]
            })
            .collect();
        run_script(&lines);
    }
}

pub fn elevation_hint() -> String {
    "Hãy chạy ứng dụng bằng quyền Administrator (chuột phải > Run as administrator) hoặc bấm nút \"Chạy lại với quyền Admin\"."
        .to_string()
}

pub fn relaunch_elevated() -> Result<(), NetError> {
    let exe = std::env::current_exe().map_err(|e| NetError::Command(e.to_string()))?;
    let exe_str = exe.to_string_lossy().replace('\'', "''");
    let script = format!("Start-Process -FilePath '{exe_str}' -Verb RunAs");
    let out = command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .output()
        .map_err(|e| NetError::Spawn("powershell".into(), e.to_string()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(NetError::Command(
            "Người dùng đã từ chối UAC hoặc không thể nâng quyền".into(),
        ))
    }
}

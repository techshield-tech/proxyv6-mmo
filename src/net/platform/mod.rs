use std::net::Ipv6Addr;
use std::process::{Command, Output, Stdio};

use super::interfaces::addrs_on_index;
use super::{ApplyOptions, NetError, Target};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(windows)]
use windows as imp;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("Nền tảng này chưa được hỗ trợ (chỉ hỗ trợ Windows, Linux, macOS)");

pub const CHUNK_SIZE: usize = 200;

pub trait Backend: Sync {
    fn is_elevated(&self) -> bool;
    fn check_tools(&self) -> Result<(), NetError>;
    fn add_one(&self, target: &Target, addr: Ipv6Addr, opts: ApplyOptions) -> Result<(), NetError>;
    fn remove_one(&self, target: &Target, addr: Ipv6Addr) -> Result<(), NetError>;

    fn add_batch(&self, target: &Target, addrs: &[Ipv6Addr], opts: ApplyOptions) {
        for a in addrs {
            let _ = self.add_one(target, *a, opts);
        }
    }

    fn remove_batch(&self, target: &Target, addrs: &[Ipv6Addr]) {
        for a in addrs {
            let _ = self.remove_one(target, *a);
        }
    }
}

pub fn backend() -> &'static dyn Backend {
    imp::backend()
}

pub fn is_elevated() -> bool {
    backend().is_elevated()
}

pub fn check_tools() -> Result<(), NetError> {
    backend().check_tools()
}

pub fn elevation_hint() -> String {
    imp::elevation_hint()
}

pub fn can_relaunch_elevated() -> bool {
    imp::CAN_RELAUNCH
}

pub fn relaunch_elevated() -> Result<(), NetError> {
    imp::relaunch_elevated()
}

pub fn supports_persistent() -> bool {
    imp::SUPPORTS_PERSISTENT
}

pub fn supports_nodad() -> bool {
    imp::SUPPORTS_NODAD
}

pub struct Outcome {
    pub addr: Ipv6Addr,
    pub result: Result<(), String>,
}

pub fn add_chunk(target: &Target, chunk: &[Ipv6Addr], opts: ApplyOptions) -> Vec<Outcome> {
    let be = backend();
    be.add_batch(target, chunk, opts);
    let present = addrs_on_index(target.index);
    chunk
        .iter()
        .map(|&addr| {
            let result = if present.contains(&addr) {
                Ok(())
            } else {
                be.add_one(target, addr, opts).map_err(|e| e.to_string())
            };
            Outcome { addr, result }
        })
        .collect()
}

pub fn remove_chunk(target: &Target, chunk: &[Ipv6Addr]) -> Vec<Outcome> {
    let be = backend();
    be.remove_batch(target, chunk);
    let present = addrs_on_index(target.index);
    chunk
        .iter()
        .map(|&addr| {
            let result = if !present.contains(&addr) {
                Ok(())
            } else {
                be.remove_one(target, addr).map_err(|e| e.to_string())
            };
            Outcome { addr, result }
        })
        .collect()
}

pub(crate) fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

pub(crate) fn command(program: &str) -> Command {
    let cmd = Command::new(program);
    #[cfg(windows)]
    let cmd = {
        use std::os::windows::process::CommandExt;
        let mut cmd = cmd;
        cmd.creation_flags(0x0800_0000);
        cmd
    };
    cmd
}

pub(crate) fn run(program: &str, args: &[String]) -> Result<Output, NetError> {
    command(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| NetError::Spawn(program.to_string(), e.to_string()))
}

pub(crate) fn run_checked(program: &str, args: &[String]) -> Result<(), NetError> {
    let out = run(program, args)?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = decode(&out.stderr);
    let stdout = decode(&out.stdout);
    let msg = if !stderr.is_empty() { stderr } else { stdout };
    let msg = if msg.is_empty() {
        format!("Lệnh `{program}` thất bại (mã {})", out.status.code().unwrap_or(-1))
    } else {
        msg.lines().next().unwrap_or("").to_string()
    };
    Err(NetError::Command(msg))
}

pub(crate) fn s(v: &str) -> String {
    v.to_string()
}

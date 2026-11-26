use std::io::Write;
use std::net::Ipv6Addr;
use std::process::Stdio;

use super::{command, run_checked, s, Backend};
use crate::net::{ApplyOptions, NetError, Target};

pub const CAN_RELAUNCH: bool = false;
pub const SUPPORTS_PERSISTENT: bool = false;
pub const SUPPORTS_NODAD: bool = true;

struct LinuxBackend;

static BACKEND: LinuxBackend = LinuxBackend;

pub fn backend() -> &'static dyn Backend {
    &BACKEND
}

fn run_batch(lines: &[String]) {
    let child = command("ip")
        .args(["-6", "-force", "-batch", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return };
    if let Some(mut stdin) = child.stdin.take() {
        let mut body = lines.join("\n");
        body.push('\n');
        let _ = stdin.write_all(body.as_bytes());
    }
    let _ = child.wait();
}

impl Backend for LinuxBackend {
    fn is_elevated(&self) -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    fn check_tools(&self) -> Result<(), NetError> {
        run_checked("ip", &[s("-V")]).map_err(|e| {
            NetError::Command(format!(
                "Không tìm thấy lệnh `ip` (cài gói iproute2): {e}"
            ))
        })
    }

    fn add_one(&self, target: &Target, addr: Ipv6Addr, opts: ApplyOptions) -> Result<(), NetError> {
        let mut args = vec![
            s("-6"),
            s("addr"),
            s("add"),
            format!("{}/{}", addr, target.prefix_len),
            s("dev"),
            target.name.clone(),
        ];
        if opts.nodad {
            args.push(s("nodad"));
        }
        run_checked("ip", &args)
    }

    fn remove_one(&self, target: &Target, addr: Ipv6Addr) -> Result<(), NetError> {
        run_checked(
            "ip",
            &[
                s("-6"),
                s("addr"),
                s("del"),
                format!("{}/{}", addr, target.prefix_len),
                s("dev"),
                target.name.clone(),
            ],
        )
    }

    fn add_batch(&self, target: &Target, addrs: &[Ipv6Addr], opts: ApplyOptions) {
        let nodad = if opts.nodad { " nodad" } else { "" };
        let lines: Vec<String> = addrs
            .iter()
            .map(|a| format!("addr add {}/{} dev {}{}", a, target.prefix_len, target.name, nodad))
            .collect();
        run_batch(&lines);
    }

    fn remove_batch(&self, target: &Target, addrs: &[Ipv6Addr]) {
        let lines: Vec<String> = addrs
            .iter()
            .map(|a| format!("addr del {}/{} dev {}", a, target.prefix_len, target.name))
            .collect();
        run_batch(&lines);
    }
}

pub fn elevation_hint() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "./proxyv6-mmo".into());
    format!("Hãy chạy lại bằng root: `sudo -E {exe}` hoặc `pkexec env DISPLAY=$DISPLAY XAUTHORITY=$XAUTHORITY {exe}`.")
}

pub fn relaunch_elevated() -> Result<(), NetError> {
    Err(NetError::Command(elevation_hint()))
}

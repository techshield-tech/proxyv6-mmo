use std::net::Ipv6Addr;

use super::{run_checked, s, Backend};
use crate::net::{ApplyOptions, NetError, Target};

pub const CAN_RELAUNCH: bool = false;
pub const SUPPORTS_PERSISTENT: bool = false;
pub const SUPPORTS_NODAD: bool = false;

struct MacBackend;

static BACKEND: MacBackend = MacBackend;

pub fn backend() -> &'static dyn Backend {
    &BACKEND
}

impl Backend for MacBackend {
    fn is_elevated(&self) -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    fn check_tools(&self) -> Result<(), NetError> {
        run_checked("ifconfig", &[s("-l")])
            .map_err(|e| NetError::Command(format!("Không chạy được ifconfig: {e}")))
    }

    fn add_one(&self, target: &Target, addr: Ipv6Addr, _opts: ApplyOptions) -> Result<(), NetError> {
        run_checked(
            "ifconfig",
            &[
                target.name.clone(),
                s("inet6"),
                addr.to_string(),
                s("prefixlen"),
                target.prefix_len.to_string(),
                s("alias"),
            ],
        )
    }

    fn remove_one(&self, target: &Target, addr: Ipv6Addr) -> Result<(), NetError> {
        run_checked(
            "ifconfig",
            &[target.name.clone(), s("inet6"), addr.to_string(), s("-alias")],
        )
    }
}

pub fn elevation_hint() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "./proxyv6-mmo".into());
    format!("Hãy chạy lại bằng quyền root: `sudo {exe}`.")
}

pub fn relaunch_elevated() -> Result<(), NetError> {
    Err(NetError::Command(elevation_hint()))
}

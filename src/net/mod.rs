pub mod generator;
pub mod interfaces;
pub mod platform;
pub mod public;

use std::net::Ipv6Addr;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("{0}")]
    Invalid(String),
    #[error("Không thể chạy lệnh `{0}`: {1}")]
    Spawn(String, String),
    #[error("{0}")]
    Command(String),
}

#[derive(Clone, Debug)]
pub struct Target {
    pub index: u32,
    pub name: String,
    pub prefix_len: u8,
}

#[derive(Clone, Copy, Debug, Default)]
#[allow(dead_code)]
pub struct ApplyOptions {
    pub persistent: bool,
    pub nodad: bool,
}

pub fn is_global_unicast(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xe000) == 0x2000
}

pub fn mask_for(prefix_len: u8) -> u128 {
    match prefix_len {
        0 => 0,
        128.. => u128::MAX,
        n => u128::MAX << (128 - n as u32),
    }
}

pub fn network_of(addr: Ipv6Addr, prefix_len: u8) -> Ipv6Addr {
    Ipv6Addr::from(u128::from(addr) & mask_for(prefix_len))
}

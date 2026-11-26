use std::collections::HashSet;
use std::net::Ipv6Addr;

use rand::Rng;

use super::{mask_for, network_of, NetError};

pub const MAX_COUNT: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Random,
    Sequential,
}

#[derive(Clone, Debug)]
pub struct GenParams {
    pub prefix: Ipv6Addr,
    pub prefix_len: u8,
    pub count: usize,
    pub mode: Mode,
    pub start_offset: u128,
}

pub fn parse_prefix(text: &str) -> Result<(Ipv6Addr, u8), NetError> {
    let text = text.trim();
    let (addr_part, len_part) = match text.split_once('/') {
        Some((a, l)) => (a.trim(), Some(l.trim())),
        None => (text, None),
    };
    let addr: Ipv6Addr = addr_part
        .parse()
        .map_err(|_| NetError::Invalid("Prefix IPv6 không hợp lệ".into()))?;
    let len = match len_part {
        Some(l) => l
            .parse::<u8>()
            .map_err(|_| NetError::Invalid("Độ dài prefix không hợp lệ".into()))?,
        None => 64,
    };
    if !(16..=120).contains(&len) {
        return Err(NetError::Invalid("Độ dài prefix phải trong khoảng 16..120".into()));
    }
    Ok((network_of(addr, len), len))
}

pub fn parse_offset(text: &str) -> Result<u128, NetError> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(0);
    }
    let parsed = if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u128::from_str_radix(hex, 16)
    } else {
        t.parse::<u128>()
    };
    parsed.map_err(|_| NetError::Invalid("Offset bắt đầu không hợp lệ (số thập phân hoặc 0x...)".into()))
}

pub fn generate(params: &GenParams, exclude: &HashSet<Ipv6Addr>) -> Result<Vec<Ipv6Addr>, NetError> {
    if params.count == 0 || params.count > MAX_COUNT {
        return Err(NetError::Invalid(format!("Số lượng phải trong khoảng 1..{MAX_COUNT}")));
    }
    if !(16..=120).contains(&params.prefix_len) {
        return Err(NetError::Invalid("Độ dài prefix phải trong khoảng 16..120".into()));
    }
    let host_bits = 128 - params.prefix_len as u32;
    let host_mask = !mask_for(params.prefix_len);
    let base = u128::from(params.prefix) & mask_for(params.prefix_len);
    let capacity: u128 = if host_bits >= 64 { u128::MAX } else { (1u128 << host_bits) - 2 };
    if capacity < (params.count as u128).saturating_add(exclude.len() as u128) {
        return Err(NetError::Invalid(format!(
            "Prefix /{} quá nhỏ để sinh {} địa chỉ",
            params.prefix_len, params.count
        )));
    }

    let mut seen: HashSet<u128> = HashSet::with_capacity(params.count * 2);
    let mut out = Vec::with_capacity(params.count);
    let mut accept = |host: u128, out: &mut Vec<Ipv6Addr>| -> bool {
        if host <= 1 {
            return false;
        }
        let addr = Ipv6Addr::from(base | host);
        if exclude.contains(&addr) || !seen.insert(host) {
            return false;
        }
        out.push(addr);
        true
    };

    match params.mode {
        Mode::Random => {
            let mut rng = rand::rng();
            let mut attempts: u64 = 0;
            let limit = (params.count as u64).saturating_mul(50).saturating_add(10_000);
            while out.len() < params.count {
                attempts += 1;
                if attempts > limit {
                    return Err(NetError::Invalid(
                        "Không thể sinh đủ địa chỉ duy nhất, hãy giảm số lượng".into(),
                    ));
                }
                let host: u128 = rng.random::<u128>() & host_mask;
                accept(host, &mut out);
            }
        }
        Mode::Sequential => {
            let mut host = params.start_offset;
            if host > host_mask {
                return Err(NetError::Invalid(
                    "Offset bắt đầu vượt quá không gian host của prefix".into(),
                ));
            }
            while out.len() < params.count {
                if host > host_mask {
                    return Err(NetError::Invalid(
                        "Hết không gian địa chỉ trong prefix khi sinh tuần tự".into(),
                    ));
                }
                accept(host, &mut out);
                host += 1;
            }
        }
    }
    Ok(out)
}

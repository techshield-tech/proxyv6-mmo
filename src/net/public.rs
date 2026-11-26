use std::io::{Read, Write};
use std::net::{IpAddr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use super::interfaces::{list_adapters, Adapter};
use super::is_global_unicast;

const ROUTE_PROBES: [&str; 2] = ["[2606:4700:4700::1111]:53", "[2001:4860:4860::8888]:53"];
const HTTP_HOSTS: [&str; 3] = ["api6.ipify.org", "ipv6.icanhazip.com", "v6.ident.me"];
const MAX_RESPONSE: u64 = 16 * 1024;

#[derive(Clone, Debug)]
pub struct PublicInfo {
    pub route: Option<Ipv6Addr>,
    pub public: Result<Ipv6Addr, String>,
}

impl PublicInfo {
    pub fn addrs(&self) -> Vec<Ipv6Addr> {
        let mut out = Vec::new();
        if let Ok(a) = &self.public {
            out.push(*a);
        }
        if let Some(r) = self.route {
            if !out.contains(&r) {
                out.push(r);
            }
        }
        out
    }

    pub fn matches(&self, addr: &Ipv6Addr) -> bool {
        self.public.as_ref().ok() == Some(addr) || self.route.as_ref() == Some(addr)
    }
}

pub fn route_source() -> Option<Ipv6Addr> {
    let socket = UdpSocket::bind("[::]:0").ok()?;
    for probe in ROUTE_PROBES {
        if socket.connect(probe).is_err() {
            continue;
        }
        if let Ok(SocketAddr::V6(local)) = socket.local_addr() {
            if is_global_unicast(local.ip()) {
                return Some(*local.ip());
            }
        }
    }
    None
}

fn http_get(host: &str, timeout: Duration) -> Result<Ipv6Addr, String> {
    let addrs: Vec<SocketAddr> = (host, 80)
        .to_socket_addrs()
        .map_err(|e| format!("không phân giải được {host}: {e}"))?
        .filter(SocketAddr::is_ipv6)
        .collect();
    if addrs.is_empty() {
        return Err(format!("{host} không có địa chỉ IPv6"));
    }
    let mut last = String::new();
    for addr in addrs {
        match request(host, addr, timeout) {
            Ok(ip) => return Ok(ip),
            Err(e) => last = e,
        }
    }
    Err(format!("{host}: {last}"))
}

fn request(host: &str, addr: SocketAddr, timeout: Duration) -> Result<Ipv6Addr, String> {
    let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("không kết nối được: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let req = format!("GET / HTTP/1.1\r\nHost: {host}\r\nUser-Agent: proxyv6-mmo\r\nAccept: text/plain\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).map_err(|e| format!("không gửi được yêu cầu: {e}"))?;
    let mut buf = Vec::new();
    let read = stream.take(MAX_RESPONSE).read_to_end(&mut buf);
    if buf.is_empty() {
        return Err(match read {
            Err(e) => format!("không đọc được phản hồi: {e}"),
            Ok(_) => "phản hồi rỗng".to_string(),
        });
    }
    let text = String::from_utf8_lossy(&buf);
    let (head, body) = text.split_once("\r\n\r\n").ok_or("phản hồi không hợp lệ")?;
    let status = head.lines().next().unwrap_or_default();
    if !status.split_whitespace().nth(1).is_some_and(|c| c == "200") {
        return Err(format!("mã trạng thái lạ: {status}"));
    }
    body.split_whitespace()
        .find_map(|t| match t.parse::<IpAddr>() {
            Ok(IpAddr::V6(ip)) => Some(ip),
            _ => None,
        })
        .ok_or_else(|| "nội dung không phải địa chỉ IPv6".to_string())
}

pub fn fetch_public(timeout: Duration, route: Option<Ipv6Addr>) -> Result<Ipv6Addr, String> {
    if !list_adapters().iter().any(Adapter::has_global) {
        return Err("Máy chưa có địa chỉ IPv6 public (chỉ có link-local fe80::/loopback). \
                    Hãy bật IPv6 trên modem/router và card mạng."
            .to_string());
    }
    if route.is_none() {
        return Err("Có địa chỉ IPv6 nhưng không có route IPv6 mặc định (gateway) — \
                    kiểm tra cấu hình IPv6 trên router."
            .to_string());
    }
    let mut errors = Vec::new();
    for host in HTTP_HOSTS {
        match http_get(host, timeout) {
            Ok(ip) => return Ok(ip),
            Err(e) => errors.push(e),
        }
    }
    Err(format!(
        "Có IPv6 nhưng không truy cập được dịch vụ kiểm tra IP qua IPv6 ({})",
        errors.join("; ")
    ))
}

pub fn detect(timeout: Duration) -> PublicInfo {
    let route = route_source();
    PublicInfo {
        route,
        public: fetch_public(timeout, route),
    }
}

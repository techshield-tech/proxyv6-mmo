use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::proxy::{Credentials, ProxyStats};

const MAX_RESPONSE: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct CheckUrl {
    pub host: String,
    pub path: String,
}

pub fn parse_check_url(url: &str) -> Result<CheckUrl, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Chưa nhập URL kiểm tra IP".into());
    }
    let rest = match url.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => rest,
        Some((scheme, _)) if scheme.eq_ignore_ascii_case("https") => {
            return Err("URL kiểm tra IP chỉ hỗ trợ http://, không hỗ trợ https://".into());
        }
        Some((scheme, _)) => return Err(format!("URL kiểm tra IP không hỗ trợ giao thức {scheme}://")),
        None => url,
    };
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() || authority.contains('@') || authority.contains(char::is_whitespace) {
        return Err("URL kiểm tra IP không hợp lệ".into());
    }
    let tail = &rest[end..];
    let path = if tail.is_empty() {
        "/".to_string()
    } else if tail.starts_with('?') {
        format!("/{tail}")
    } else {
        tail.to_string()
    };
    Ok(CheckUrl {
        host: authority.to_string(),
        path,
    })
}

fn b64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

fn build_request(url: &CheckUrl, auth: Option<&Credentials>) -> Vec<u8> {
    let mut req = format!(
        "GET http://{host}{path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: proxyv6-mmo\r\nConnection: close\r\n",
        host = url.host,
        path = url.path
    );
    if let Some(c) = auth {
        let token = b64_encode(format!("{}:{}", c.user, c.pass).as_bytes());
        req.push_str(&format!("Proxy-Authorization: Basic {token}\r\n"));
    }
    req.push_str("\r\n");
    req.into_bytes()
}

fn dechunk(body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let pos = rest.windows(2).position(|w| w == b"\r\n")?;
        let size_text = std::str::from_utf8(&rest[..pos]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[pos + 2..];
        if size == 0 {
            return Some(out);
        }
        if rest.len() < size {
            return None;
        }
        out.extend_from_slice(&rest[..size]);
        rest = rest.get(size + 2..)?;
    }
}

fn parse_response(raw: &[u8]) -> Result<IpAddr, String> {
    let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Err("phản hồi không hợp lệ".into());
    };
    let head = String::from_utf8_lossy(&raw[..end]);
    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or("");
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| "phản hồi không hợp lệ".to_string())?;
    match code {
        200 => {}
        407 => return Err("407 sai xác thực".into()),
        c => return Err(format!("HTTP {c}")),
    }
    let chunked = lines.any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    let body = &raw[end + 4..];
    let body = if chunked { dechunk(body).unwrap_or_default() } else { body.to_vec() };
    let text = String::from_utf8_lossy(&body);
    text.trim()
        .parse::<IpAddr>()
        .map_err(|_| "phản hồi không phải IP".to_string())
}

fn is_timeout(e: &std::io::Error) -> bool {
    matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock)
}

pub fn test_one(endpoint: SocketAddr, request: &[u8], timeout: Duration) -> Result<(u32, IpAddr), String> {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut stream = TcpStream::connect_timeout(&endpoint, timeout).map_err(|e| {
        if is_timeout(&e) {
            "timeout".to_string()
        } else {
            format!("không kết nối được proxy: {e}")
        }
    })?;
    let _ = stream.set_nodelay(true);
    let remaining = |now: Instant| deadline.saturating_duration_since(now).max(Duration::from_millis(1));
    let _ = stream.set_write_timeout(Some(remaining(Instant::now())));
    stream.write_all(request).map_err(|e| {
        if is_timeout(&e) {
            "timeout".to_string()
        } else {
            format!("lỗi gửi: {e}")
        }
    })?;
    let mut raw: Vec<u8> = Vec::with_capacity(1024);
    let mut buf = [0u8; 2048];
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("timeout".into());
        }
        let _ = stream.set_read_timeout(Some(remaining(now)));
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > MAX_RESPONSE {
                    return Err("phản hồi quá lớn".into());
                }
            }
            Err(e) if is_timeout(&e) => return Err("timeout".into()),
            Err(e) if e.kind() == ErrorKind::ConnectionReset && !raw.is_empty() => break,
            Err(e) => return Err(format!("lỗi đọc: {e}")),
        }
    }
    if raw.is_empty() {
        return Err("proxy đóng kết nối".into());
    }
    let ip = parse_response(&raw)?;
    let ms = start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
    Ok((ms, ip))
}

pub fn wait_ready(stats: &ProxyStats, cancel: &AtomicBool, max: Duration) -> bool {
    let deadline = Instant::now() + max;
    while !stats.ready.load(Ordering::Relaxed) {
        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

pub struct TestJob {
    pub pairs: Vec<(Ipv6Addr, u16)>,
    pub host: Ipv4Addr,
    pub url: CheckUrl,
    pub auth: Option<Credentials>,
    pub threads: usize,
    pub timeout: Duration,
    pub cancel: Arc<AtomicBool>,
}

pub fn run<F>(job: TestJob, on_result: F)
where
    F: Fn(Ipv6Addr, Result<(u32, IpAddr), String>) + Send + Sync + 'static,
{
    let request = Arc::new(build_request(&job.url, job.auth.as_ref()));
    let pairs = Arc::new(job.pairs);
    let next = Arc::new(AtomicUsize::new(0));
    let on_result = Arc::new(on_result);
    let threads = job.threads.clamp(1, 64).min(pairs.len().max(1));
    let mut handles = Vec::with_capacity(threads);
    for _ in 0..threads {
        let pairs = Arc::clone(&pairs);
        let request = Arc::clone(&request);
        let next = Arc::clone(&next);
        let cancel = Arc::clone(&job.cancel);
        let on_result = Arc::clone(&on_result);
        let host = job.host;
        let timeout = job.timeout;
        handles.push(std::thread::spawn(move || loop {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let i = next.fetch_add(1, Ordering::Relaxed);
            let Some(&(addr, port)) = pairs.get(i) else { break };
            let endpoint = SocketAddr::V4(SocketAddrV4::new(host, port));
            on_result(addr, test_one(endpoint, &request, timeout));
        }));
    }
    for h in handles {
        let _ = h.join();
    }
}

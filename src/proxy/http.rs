use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::{connect_v6, relay, ProxyError, Session, HANDSHAKE_TIMEOUT};

const MAX_HEAD: usize = 64 * 1024;

struct Request {
    method: String,
    target: String,
    version: String,
    headers: Vec<Vec<u8>>,
    leftover: Vec<u8>,
}

struct Dest {
    host: String,
    port: u16,
    forward: Option<Forward>,
}

struct Forward {
    path: String,
    authority: String,
}

pub async fn handle(mut client: TcpStream, session: &Session) -> Result<(), ProxyError> {
    let req = match timeout(HANDSHAKE_TIMEOUT, read_request(&mut client)).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            let _ = status(&mut client, 400, "Bad Request", "").await;
            return Err(e);
        }
        Err(_) => return Err(ProxyError::Timeout),
    };

    if let Some(cred) = &session.auth {
        if !authorized(&req, &cred.user, &cred.pass) {
            let _ = status(
                &mut client,
                407,
                "Proxy Authentication Required",
                "Proxy-Authenticate: Basic realm=\"proxy\"\r\n",
            )
            .await;
            return Err(ProxyError::Auth);
        }
    }

    let dest = match parse_dest(&req) {
        Ok(d) => d,
        Err(e) => {
            let _ = status(&mut client, 400, "Bad Request", "").await;
            return Err(e);
        }
    };

    let mut upstream = match connect_v6(session.src, &dest.host, dest.port).await {
        Ok(u) => u,
        Err(e) => {
            let _ = match e {
                ProxyError::Timeout => status(&mut client, 504, "Gateway Timeout", "").await,
                _ => status(&mut client, 502, "Bad Gateway", "").await,
            };
            return Err(e);
        }
    };

    match &dest.forward {
        None => {
            client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await?;
        }
        Some(fwd) => {
            let head = build_head(&req, fwd);
            upstream.write_all(&head).await?;
        }
    }
    if !req.leftover.is_empty() {
        upstream.write_all(&req.leftover).await?;
    }
    relay(client, upstream).await;
    Ok(())
}

async fn status(client: &mut TcpStream, code: u16, reason: &str, extra: &str) -> std::io::Result<()> {
    let text = format!("HTTP/1.1 {code} {reason}\r\n{extra}Content-Length: 0\r\nConnection: close\r\n\r\n");
    client.write_all(text.as_bytes()).await
}

async fn read_request(client: &mut TcpStream) -> Result<Request, ProxyError> {
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    let mut chunk = [0u8; 4096];
    let mut scanned = 0usize;
    let end = loop {
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Err(ProxyError::Bad("kết nối đóng sớm"));
        }
        buf.extend_from_slice(&chunk[..n]);
        let from = scanned.saturating_sub(3);
        if let Some(pos) = buf[from..].windows(4).position(|w| w == b"\r\n\r\n") {
            break from + pos;
        }
        scanned = buf.len();
        if buf.len() > MAX_HEAD {
            return Err(ProxyError::Bad("header quá lớn"));
        }
    };
    let leftover = buf[end + 4..].to_vec();
    let mut lines = buf[..end].split(|&b| b == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l));
    let line = lines.next().ok_or(ProxyError::Bad("thiếu request line"))?;
    let line = std::str::from_utf8(line).map_err(|_| ProxyError::Bad("request line không hợp lệ"))?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(ProxyError::Bad("request line không hợp lệ"));
    };
    if !version.starts_with("HTTP/") {
        return Err(ProxyError::Bad("không phải HTTP"));
    }
    let headers = lines.filter(|l| !l.is_empty()).map(|l| l.to_vec()).collect();
    Ok(Request {
        method: method.to_string(),
        target: target.to_string(),
        version: version.to_string(),
        headers,
        leftover,
    })
}

fn header_name(line: &[u8]) -> &[u8] {
    let name = match line.iter().position(|&b| b == b':') {
        Some(i) => &line[..i],
        None => line,
    };
    name.trim_ascii()
}

fn header_value(line: &[u8]) -> &[u8] {
    match line.iter().position(|&b| b == b':') {
        Some(i) => line[i + 1..].trim_ascii(),
        None => &[],
    }
}

fn authorized(req: &Request, user: &str, pass: &str) -> bool {
    for line in &req.headers {
        if !header_name(line).eq_ignore_ascii_case(b"proxy-authorization") {
            continue;
        }
        let Ok(value) = std::str::from_utf8(header_value(line)) else { continue };
        let Some((scheme, token)) = value.split_once(char::is_whitespace) else { continue };
        if !scheme.eq_ignore_ascii_case("basic") {
            continue;
        }
        let Some(decoded) = b64_decode(token.trim()) else { continue };
        let Some(i) = decoded.iter().position(|&b| b == b':') else { continue };
        if decoded[..i] == *user.as_bytes() && decoded[i + 1..] == *pass.as_bytes() {
            return true;
        }
    }
    false
}

fn b64_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0u32;
    for c in input.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn split_host_port(s: &str, default_port: u16) -> Result<(String, u16), ProxyError> {
    let bad = ProxyError::Bad("địa chỉ đích không hợp lệ");
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (h, after) = rest.split_once(']').ok_or(ProxyError::Bad("địa chỉ đích không hợp lệ"))?;
        match after.strip_prefix(':') {
            Some(p) => (h, Some(p)),
            None if after.is_empty() => (h, None),
            None => return Err(bad),
        }
    } else {
        match s.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (s, None),
        }
    };
    if host.is_empty() {
        return Err(bad);
    }
    let port = match port {
        Some(p) => p.parse::<u16>().map_err(|_| ProxyError::Bad("port đích không hợp lệ"))?,
        None => default_port,
    };
    Ok((host.to_string(), port))
}

fn parse_dest(req: &Request) -> Result<Dest, ProxyError> {
    if req.method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = split_host_port(&req.target, 443)?;
        return Ok(Dest {
            host,
            port,
            forward: None,
        });
    }
    let rest = match req.target.get(..7) {
        Some(p) if p.eq_ignore_ascii_case("http://") => &req.target[7..],
        _ => return Err(ProxyError::Bad("chỉ hỗ trợ URI tuyệt đối http://")),
    };
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = rest[..end].rsplit('@').next().unwrap_or("");
    let tail = &rest[end..];
    let path = if tail.is_empty() {
        "/".to_string()
    } else if tail.starts_with('?') {
        format!("/{tail}")
    } else {
        tail.to_string()
    };
    let (host, port) = split_host_port(authority, 80)?;
    Ok(Dest {
        host,
        port,
        forward: Some(Forward {
            path,
            authority: authority.to_string(),
        }),
    })
}

fn build_head(req: &Request, fwd: &Forward) -> Vec<u8> {
    let mut out = Vec::with_capacity(512);
    out.extend_from_slice(format!("{} {} {}\r\n", req.method, fwd.path, req.version).as_bytes());
    let mut has_host = false;
    for line in &req.headers {
        let name = header_name(line);
        if name.eq_ignore_ascii_case(b"proxy-authorization")
            || name.eq_ignore_ascii_case(b"proxy-connection")
            || name.eq_ignore_ascii_case(b"connection")
        {
            continue;
        }
        if name.eq_ignore_ascii_case(b"host") {
            has_host = true;
        }
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    if !has_host {
        out.extend_from_slice(format!("Host: {}\r\n", fwd.authority).as_bytes());
    }
    out.extend_from_slice(b"Connection: close\r\n\r\n");
    out
}

use std::io::ErrorKind;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::{connect_v6, relay, ProxyError, Session, HANDSHAKE_TIMEOUT};

const VER: u8 = 0x05;
const METHOD_NONE: u8 = 0x00;
const METHOD_USERPASS: u8 = 0x02;
const METHOD_REJECT: u8 = 0xFF;

const REP_FAILURE: u8 = 0x01;
const REP_NET_UNREACHABLE: u8 = 0x03;
const REP_HOST_UNREACHABLE: u8 = 0x04;
const REP_REFUSED: u8 = 0x05;
const REP_TTL_EXPIRED: u8 = 0x06;
const REP_CMD_UNSUPPORTED: u8 = 0x07;
const REP_ATYP_UNSUPPORTED: u8 = 0x08;

pub async fn handle(mut client: TcpStream, session: &Session) -> Result<(), ProxyError> {
    let (host, port) = match timeout(HANDSHAKE_TIMEOUT, negotiate(&mut client, session)).await {
        Ok(r) => r?,
        Err(_) => return Err(ProxyError::Timeout),
    };
    let upstream = match connect_v6(session.src, &host, port).await {
        Ok(u) => u,
        Err(e) => {
            let _ = reply(&mut client, reply_code(&e), None).await;
            return Err(e);
        }
    };
    let bound = upstream.local_addr().ok();
    reply(&mut client, 0x00, bound).await?;
    relay(client, upstream).await;
    Ok(())
}

async fn negotiate(client: &mut TcpStream, session: &Session) -> Result<(String, u16), ProxyError> {
    let mut head = [0u8; 2];
    client.read_exact(&mut head).await?;
    if head[0] != VER {
        return Err(ProxyError::Bad("không phải SOCKS5"));
    }
    let mut methods = vec![0u8; head[1] as usize];
    client.read_exact(&mut methods).await?;
    let wanted = if session.auth.is_some() { METHOD_USERPASS } else { METHOD_NONE };
    if !methods.contains(&wanted) {
        client.write_all(&[VER, METHOD_REJECT]).await?;
        return Err(ProxyError::Bad("không có phương thức xác thực phù hợp"));
    }
    client.write_all(&[VER, wanted]).await?;

    if let Some(cred) = &session.auth {
        let mut ver = [0u8; 2];
        client.read_exact(&mut ver).await?;
        let mut user = vec![0u8; ver[1] as usize];
        client.read_exact(&mut user).await?;
        let mut plen = [0u8; 1];
        client.read_exact(&mut plen).await?;
        let mut pass = vec![0u8; plen[0] as usize];
        client.read_exact(&mut pass).await?;
        if ver[0] != 0x01 || user != cred.user.as_bytes() || pass != cred.pass.as_bytes() {
            client.write_all(&[0x01, 0x01]).await?;
            return Err(ProxyError::Auth);
        }
        client.write_all(&[0x01, 0x00]).await?;
    }

    let mut req = [0u8; 4];
    client.read_exact(&mut req).await?;
    if req[0] != VER {
        return Err(ProxyError::Bad("sai phiên bản yêu cầu"));
    }
    let host = match req[3] {
        0x01 => {
            let mut b = [0u8; 4];
            client.read_exact(&mut b).await?;
            Ipv4Addr::from(b).to_string()
        }
        0x03 => {
            let mut len = [0u8; 1];
            client.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            client.read_exact(&mut name).await?;
            match String::from_utf8(name) {
                Ok(s) if !s.is_empty() => s,
                _ => {
                    reply(client, REP_FAILURE, None).await?;
                    return Err(ProxyError::Bad("tên miền không hợp lệ"));
                }
            }
        }
        0x04 => {
            let mut b = [0u8; 16];
            client.read_exact(&mut b).await?;
            Ipv6Addr::from(b).to_string()
        }
        _ => {
            reply(client, REP_ATYP_UNSUPPORTED, None).await?;
            return Err(ProxyError::Bad("kiểu địa chỉ không hỗ trợ"));
        }
    };
    let mut port = [0u8; 2];
    client.read_exact(&mut port).await?;
    if req[1] != 0x01 {
        reply(client, REP_CMD_UNSUPPORTED, None).await?;
        return Err(ProxyError::Bad("chỉ hỗ trợ lệnh CONNECT"));
    }
    Ok((host, u16::from_be_bytes(port)))
}

async fn reply(client: &mut TcpStream, code: u8, bound: Option<SocketAddr>) -> std::io::Result<()> {
    let mut out = vec![VER, code, 0x00];
    match bound {
        Some(SocketAddr::V6(a)) => {
            out.push(0x04);
            out.extend_from_slice(&a.ip().octets());
            out.extend_from_slice(&a.port().to_be_bytes());
        }
        Some(SocketAddr::V4(a)) => {
            out.push(0x01);
            out.extend_from_slice(&a.ip().octets());
            out.extend_from_slice(&a.port().to_be_bytes());
        }
        None => {
            out.push(0x01);
            out.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        }
    }
    client.write_all(&out).await
}

fn reply_code(e: &ProxyError) -> u8 {
    match e {
        ProxyError::NoV6(_) | ProxyError::Resolve(..) => REP_HOST_UNREACHABLE,
        ProxyError::Timeout => REP_TTL_EXPIRED,
        ProxyError::Connect(io) => match io.kind() {
            ErrorKind::ConnectionRefused => REP_REFUSED,
            ErrorKind::NetworkUnreachable => REP_NET_UNREACHABLE,
            ErrorKind::HostUnreachable => REP_HOST_UNREACHABLE,
            _ => REP_FAILURE,
        },
        _ => REP_FAILURE,
    }
}

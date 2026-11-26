mod http;
mod socks;

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use eframe::egui;
use thiserror::Error;
use tokio::io::copy_bidirectional;
use tokio::net::{lookup_host, TcpListener, TcpSocket, TcpStream};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

use crate::app::{LogLevel, Msg};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_LOGGED_BIND_ERRORS: usize = 20;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("Cấu hình proxy không hợp lệ: {0}")]
    Config(String),
    #[error("Không có bản ghi IPv6 cho {0}")]
    NoV6(String),
    #[error("Không phân giải được {0}: {1}")]
    Resolve(String, String),
    #[error("Kết nối quá thời gian")]
    Timeout,
    #[error("Kết nối thất bại: {0}")]
    Connect(io::Error),
    #[error("Yêu cầu không hợp lệ: {0}")]
    Bad(&'static str),
    #[error("Sai thông tin xác thực")]
    Auth,
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Clone, Debug)]
pub struct Credentials {
    pub user: String,
    pub pass: String,
}

#[derive(Clone, Debug)]
pub struct ProxyConfig {
    pub bind: Ipv4Addr,
    pub start_port: u16,
    pub auth: Option<Credentials>,
    pub addrs: Vec<Ipv6Addr>,
}

#[derive(Default)]
pub struct ProxyStats {
    pub bound: AtomicUsize,
    pub failed: AtomicUsize,
    pub ready: AtomicBool,
    pub active: AtomicUsize,
    pub total: AtomicU64,
}

struct ActiveGuard(Arc<ProxyStats>);

impl ActiveGuard {
    fn new(stats: &Arc<ProxyStats>) -> Self {
        stats.active.fetch_add(1, Ordering::Relaxed);
        stats.total.fetch_add(1, Ordering::Relaxed);
        Self(Arc::clone(stats))
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::Relaxed);
    }
}

pub struct Session {
    pub src: Ipv6Addr,
    pub auth: Option<Arc<Credentials>>,
    pub stats: Arc<ProxyStats>,
}

pub struct ProxyHandle {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    pub stats: Arc<ProxyStats>,
    pub bind: Ipv4Addr,
    pub start_port: u16,
    pub addrs: Vec<Ipv6Addr>,
}

pub fn port_range(start_port: u16, count: usize) -> Result<(u16, u16), String> {
    if count == 0 {
        return Err("Chưa có địa chỉ IPv6 nào ở trạng thái Đã thêm".into());
    }
    let last = start_port as usize + count - 1;
    if start_port == 0 || last > u16::MAX as usize {
        return Err(format!(
            "Dải cổng {start_port}-{last} vượt quá 65535, hãy giảm port bắt đầu hoặc số địa chỉ"
        ));
    }
    Ok((start_port, last as u16))
}

impl ProxyHandle {
    pub fn start(cfg: ProxyConfig, tx: Sender<Msg>, ctx: egui::Context) -> Result<Self, ProxyError> {
        port_range(cfg.start_port, cfg.addrs.len()).map_err(ProxyError::Config)?;
        raise_fd_limit();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(2, 8))
            .thread_name("proxy-worker")
            .enable_all()
            .build()?;
        let stats = Arc::new(ProxyStats::default());
        let (stop_tx, stop_rx) = oneshot::channel();
        let bind = cfg.bind;
        let start_port = cfg.start_port;
        let addrs = cfg.addrs.clone();
        let run_stats = Arc::clone(&stats);
        let thread = std::thread::Builder::new()
            .name("proxy-main".into())
            .spawn(move || {
                rt.block_on(run(cfg, run_stats, tx, ctx, stop_rx));
                rt.shutdown_timeout(Duration::from_secs(2));
            })?;
        Ok(ProxyHandle {
            stop: Some(stop_tx),
            thread: Some(thread),
            stats,
            bind,
            start_port,
            addrs,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.stats.ready.load(Ordering::Relaxed)
    }
}

impl Drop for ProxyHandle {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn run(cfg: ProxyConfig, stats: Arc<ProxyStats>, tx: Sender<Msg>, ctx: egui::Context, stop: oneshot::Receiver<()>) {
    let auth = cfg.auth.clone().map(Arc::new);
    let mut logged = 0usize;
    for (i, addr) in cfg.addrs.iter().enumerate() {
        let Some(port) = u16::try_from(i).ok().and_then(|o| cfg.start_port.checked_add(o)) else {
            stats.failed.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        match TcpListener::bind(SocketAddrV4::new(cfg.bind, port)).await {
            Ok(listener) => {
                stats.bound.fetch_add(1, Ordering::Relaxed);
                let session = Arc::new(Session {
                    src: *addr,
                    auth: auth.clone(),
                    stats: Arc::clone(&stats),
                });
                tokio::spawn(accept_loop(listener, session));
            }
            Err(e) => {
                stats.failed.fetch_add(1, Ordering::Relaxed);
                if logged < MAX_LOGGED_BIND_ERRORS {
                    logged += 1;
                    let _ = tx.send(Msg::Log(
                        LogLevel::Error,
                        format!("Proxy: không mở được {}:{port} cho {addr}: {e}", cfg.bind),
                    ));
                }
            }
        }
    }
    let bound = stats.bound.load(Ordering::Relaxed);
    let failed = stats.failed.load(Ordering::Relaxed);
    if failed > logged {
        let _ = tx.send(Msg::Log(
            LogLevel::Warn,
            format!("Proxy: ... và {} lỗi mở cổng khác (đã rút gọn)", failed - logged),
        ));
    }
    let level = if failed == 0 { LogLevel::Ok } else { LogLevel::Warn };
    let _ = tx.send(Msg::Log(
        level,
        format!("Proxy: đang lắng nghe {bound} cổng trên {}, lỗi {failed}", cfg.bind),
    ));
    stats.ready.store(true, Ordering::Relaxed);
    ctx.request_repaint();
    let _ = stop.await;
}

async fn accept_loop(listener: TcpListener, session: Arc<Session>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(serve(stream, Arc::clone(&session)));
            }
            Err(_) => sleep(Duration::from_millis(100)).await,
        }
    }
}

async fn serve(client: TcpStream, session: Arc<Session>) {
    let _guard = ActiveGuard::new(&session.stats);
    let _ = client.set_nodelay(true);
    let mut first = [0u8; 1];
    match timeout(HANDSHAKE_TIMEOUT, client.peek(&mut first)).await {
        Ok(Ok(n)) if n > 0 => {}
        _ => return,
    }
    let _ = if first[0] == 0x05 {
        socks::handle(client, &session).await
    } else {
        http::handle(client, &session).await
    };
}

async fn resolve_v6(host: &str, port: u16) -> Result<Vec<SocketAddrV6>, ProxyError> {
    let host = host.trim_matches(|c| c == '[' || c == ']');
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V6(v6) => Ok(vec![SocketAddrV6::new(v6, port, 0, 0)]),
            IpAddr::V4(_) => Err(ProxyError::NoV6(host.to_string())),
        };
    }
    let found: Vec<SocketAddrV6> = lookup_host((host, port))
        .await
        .map_err(|e| ProxyError::Resolve(host.to_string(), e.to_string()))?
        .filter_map(|a| match a {
            SocketAddr::V6(v6) => Some(v6),
            SocketAddr::V4(_) => None,
        })
        .collect();
    if found.is_empty() {
        Err(ProxyError::NoV6(host.to_string()))
    } else {
        Ok(found)
    }
}

async fn connect_any(src: Ipv6Addr, targets: Vec<SocketAddrV6>) -> io::Result<TcpStream> {
    let mut last: Option<io::Error> = None;
    for target in targets {
        let socket = TcpSocket::new_v6()?;
        socket.bind(SocketAddr::V6(SocketAddrV6::new(src, 0, 0, 0)))?;
        match socket.connect(SocketAddr::V6(target)).await {
            Ok(stream) => return Ok(stream),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::from(io::ErrorKind::AddrNotAvailable)))
}

pub async fn connect_v6(src: Ipv6Addr, host: &str, port: u16) -> Result<TcpStream, ProxyError> {
    let targets = resolve_v6(host, port).await?;
    match timeout(CONNECT_TIMEOUT, connect_any(src, targets)).await {
        Ok(Ok(stream)) => {
            let _ = stream.set_nodelay(true);
            Ok(stream)
        }
        Ok(Err(e)) => Err(ProxyError::Connect(e)),
        Err(_) => Err(ProxyError::Timeout),
    }
}

pub async fn relay(mut a: TcpStream, mut b: TcpStream) {
    let _ = copy_bidirectional(&mut a, &mut b).await;
}

#[cfg(unix)]
fn raise_fd_limit() {
    unsafe {
        let mut lim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            return;
        }
        for cap in [1_048_576u64, 65_536, 10_240] {
            let target = std::cmp::min(lim.rlim_max, cap as libc::rlim_t);
            if target <= lim.rlim_cur {
                return;
            }
            let new = libc::rlimit {
                rlim_cur: target,
                rlim_max: lim.rlim_max,
            };
            if libc::setrlimit(libc::RLIMIT_NOFILE, &new) == 0 {
                return;
            }
        }
    }
}

#[cfg(not(unix))]
fn raise_fd_limit() {}

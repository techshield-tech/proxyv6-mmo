use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32};

use crate::net::generator::{self, GenParams, Mode};
use crate::net::interfaces::{self, Adapter, V6Entry};
use crate::net::platform::{self, CHUNK_SIZE};
use crate::net::public::{self, PublicInfo};
use crate::net::{network_of, ApplyOptions, Target};
use crate::proxy::{self, Credentials, ProxyConfig, ProxyHandle};
use crate::storage::{self, Persisted, Settings, Tracked};
use crate::tester::{self, TestJob};
use crate::theme::{self, Palette};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Generate,
    Proxy,
    Logs,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Added,
    Failed,
    Removed,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Pending => "Chờ",
            Status::Added => "Đã thêm",
            Status::Failed => "Thất bại",
            Status::Removed => "Đã gỡ",
        }
    }

    pub fn color(self, p: &Palette) -> Color32 {
        match self {
            Status::Pending => p.weak,
            Status::Added => p.success,
            Status::Failed => p.error,
            Status::Removed => p.warn,
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Status::Pending => 0,
            Status::Added => 1,
            Status::Failed => 2,
            Status::Removed => 3,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum TestState {
    None,
    Running,
    Ok,
    Fail(String),
}

pub struct Row {
    pub addr: Ipv6Addr,
    pub text: String,
    pub status: Status,
    pub selected: bool,
    pub test: TestState,
    pub latency_ms: Option<u32>,
    pub egress: Option<IpAddr>,
    pub error: Option<String>,
    pub adapter_index: u32,
    pub adapter_name: String,
    pub prefix_len: u8,
}

impl Row {
    pub fn test_rank(&self) -> u8 {
        match self.test {
            TestState::None => 0,
            TestState::Running => 1,
            TestState::Ok => 2,
            TestState::Fail(_) => 3,
        }
    }

    fn new(addr: Ipv6Addr, status: Status, adapter_index: u32, adapter_name: String, prefix_len: u8) -> Self {
        Self {
            addr,
            text: addr.to_string(),
            status,
            selected: false,
            test: TestState::None,
            latency_ms: None,
            egress: None,
            error: None,
            adapter_index,
            adapter_name,
            prefix_len,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Ok,
    Warn,
    Error,
}

pub struct LogEntry {
    pub time: String,
    pub level: LogLevel,
    pub text: String,
}

pub enum Msg {
    Log(LogLevel, String),
    Outcomes {
        remove: bool,
        items: Vec<(Ipv6Addr, Result<(), String>)>,
    },
    OpDone,
    TestResult(Ipv6Addr, Result<(u32, IpAddr), String>),
    TestDone,
    PublicV6 { info: PublicInfo, select: bool, force: bool },
    ProxyStopped,
    Relaunch(Result<(), String>),
    Export(Result<Option<(PathBuf, usize)>, String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Apply,
    Remove,
}

pub struct Op {
    pub kind: OpKind,
    pub total: usize,
    pub done: usize,
    pub ok: usize,
    pub fail: usize,
    pub started: Instant,
    pub cancel: Arc<AtomicBool>,
    pub cancelling: bool,
    pub targets: Vec<Target>,
    pub addr_target: HashMap<Ipv6Addr, usize>,
}

pub struct TestRun {
    pub total: usize,
    pub done: usize,
    pub ok: usize,
    pub mismatch: usize,
    pub cancel: Arc<AtomicBool>,
    pub cancelling: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Error,
}

pub struct Toast {
    pub text: String,
    pub level: ToastLevel,
    pub at: Instant,
}

impl Toast {
    pub fn life(&self) -> Duration {
        match self.level {
            ToastLevel::Error => Duration::from_millis(6000),
            _ => Duration::from_millis(2500),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    RemoveSelected,
    RemoveAll,
    StopProxy,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResultAction {
    FilterRows(&'static str),
    OpenLogs,
}

pub struct LastResult {
    pub text: String,
    pub level: LogLevel,
    pub action: Option<ResultAction>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Addr,
    Status,
    Test,
    Latency,
}

#[derive(Default, Clone, Copy)]
pub struct Counts {
    pub added: usize,
    pub selected: usize,
    pub eligible: usize,
    pub selected_eligible: usize,
    pub selected_added: usize,
    pub test_failed: usize,
}

impl Counts {
    pub fn apply_n(&self) -> usize {
        if self.selected_eligible > 0 {
            self.selected_eligible
        } else {
            self.eligible
        }
    }

    pub fn test_n(&self) -> usize {
        if self.selected_added > 0 {
            self.selected_added
        } else {
            self.added
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportScope {
    All,
    Selected,
    Added,
}

impl ExportScope {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => ExportScope::All,
            1 => ExportScope::Selected,
            _ => ExportScope::Added,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            ExportScope::All => 0,
            ExportScope::Selected => 1,
            ExportScope::Added => 2,
        }
    }
}

#[derive(Default)]
pub struct Eligibility {
    pub blocked: Vec<String>,
    pub warnings: Vec<String>,
}

impl Eligibility {
    pub fn allowed(&self) -> bool {
        self.blocked.is_empty()
    }
}

pub struct App {
    pub ctx: egui::Context,
    pub pal: Palette,
    pub settings: Settings,
    pub settings_dirty: bool,
    pub adapters: Vec<Adapter>,
    pub selected_adapter: Option<String>,
    pub only_global: bool,
    pub prefix_text: String,
    pub custom_prefix: bool,
    pub mode: Mode,
    pub offset_text: String,
    pub form_error: Option<String>,
    pub rows: Vec<Row>,
    pub by_addr: HashMap<Ipv6Addr, usize>,
    pub visible: Vec<usize>,
    pub visible_dirty: bool,
    pub filter: String,
    pub tab: Tab,
    pub logs: Vec<LogEntry>,
    pub tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
    pub op: Option<Op>,
    pub test: Option<TestRun>,
    pub elevated: bool,
    pub tools_error: Option<String>,
    pub export_scope: ExportScope,
    pub clipboard: Option<arboard::Clipboard>,
    pub toast: Option<Toast>,
    pub font_path: Option<&'static str>,
    pub proxy: Option<ProxyHandle>,
    pub proxy_error: Option<String>,
    pub public_v6: Option<PublicInfo>,
    pub detecting_public: bool,
    pub user_chose_adapter: bool,
    pub pending_confirm: Option<Confirm>,
    pub last_result: Option<LastResult>,
    pub unseen_errors: usize,
    pub logs_errors_only: bool,
    pub stopping_proxy: bool,
    pub start_after_stop: bool,
    pub relaunching: bool,
    pub exporting: bool,
    pub settings_dirty_at: Option<Instant>,
    pub counts: Counts,
    pub port_map: HashMap<Ipv6Addr, u16>,
    pub port_map_dirty: bool,
    pub port_map_key: Option<(bool, u16, usize)>,
    pub sort: Option<(SortKey, bool)>,
    pub last_clicked: Option<usize>,
    pub logo: Option<egui::TextureHandle>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        let font_path = theme::install_fonts(&ctx);
        let persisted = storage::load();
        let settings = persisted.settings.clone();
        let mode = if settings.sequential { Mode::Sequential } else { Mode::Random };
        let offset_text = if settings.offset_text.trim().is_empty() {
            "0x1000".to_string()
        } else {
            settings.offset_text.clone()
        };
        let export_scope = ExportScope::from_u8(settings.export_scope);
        let only_global = settings.only_global;
        theme::apply(&ctx, settings.dark);
        let (tx, rx) = channel();
        let elevated = platform::is_elevated();
        let tools_error = platform::check_tools().err().map(|e| e.to_string());

        let mut app = App {
            ctx,
            pal: Palette::get(settings.dark),
            settings,
            settings_dirty: false,
            adapters: Vec::new(),
            selected_adapter: None,
            only_global,
            prefix_text: String::new(),
            custom_prefix: false,
            mode,
            offset_text,
            form_error: None,
            rows: Vec::new(),
            by_addr: HashMap::new(),
            visible: Vec::new(),
            visible_dirty: true,
            filter: String::new(),
            tab: Tab::Generate,
            logs: Vec::new(),
            tx,
            rx,
            op: None,
            test: None,
            elevated,
            tools_error,
            export_scope,
            clipboard: None,
            toast: None,
            font_path,
            proxy: None,
            proxy_error: None,
            public_v6: None,
            detecting_public: false,
            user_chose_adapter: false,
            pending_confirm: None,
            last_result: None,
            unseen_errors: 0,
            logs_errors_only: false,
            stopping_proxy: false,
            start_after_stop: false,
            relaunching: false,
            exporting: false,
            settings_dirty_at: None,
            counts: Counts::default(),
            port_map: HashMap::new(),
            port_map_dirty: true,
            port_map_key: None,
            sort: None,
            last_clicked: None,
            logo: None,
        };
        app.logo = theme::load_logo(&app.ctx);

        app.log(LogLevel::Info, "Khởi động ProxyV6 MMO");
        match app.font_path {
            Some(p) => app.log(LogLevel::Info, format!("Đã nạp font hệ thống: {p}")),
            None => app.log(LogLevel::Warn, "Không tìm thấy font hệ thống, dùng font mặc định (có thể thiếu dấu tiếng Việt)"),
        }
        if app.elevated {
            app.log(LogLevel::Ok, "Đang chạy với quyền quản trị (admin/root)");
        } else {
            app.log(LogLevel::Warn, "Chưa có quyền admin/root: không thể thêm hoặc gỡ địa chỉ");
        }
        if let Some(err) = app.tools_error.clone() {
            app.log(LogLevel::Error, err);
        }
        app.refresh_adapters();
        if app.settings.custom_prefix && !app.settings.prefix_text.trim().is_empty() {
            app.custom_prefix = true;
            app.prefix_text = app.settings.prefix_text.clone();
        }
        app.restore_tracked(persisted);
        if app.added_count() > 0 {
            app.start_proxy();
        }
        app.detect_public(true, false);
        app
    }

    pub fn log(&mut self, level: LogLevel, text: impl Into<String>) {
        let entry = LogEntry {
            time: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            text: text.into(),
        };
        if level == LogLevel::Error && self.tab != Tab::Logs {
            self.unseen_errors += 1;
        }
        self.logs.push(entry);
        if self.logs.len() > 5000 {
            self.logs.drain(0..500);
        }
    }

    pub fn toast_level(&mut self, text: impl Into<String>, level: ToastLevel) {
        self.toast = Some(Toast {
            text: text.into(),
            level,
            at: Instant::now(),
        });
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast_level(text, ToastLevel::Info);
    }

    pub fn toast_ok(&mut self, text: impl Into<String>) {
        self.toast_level(text, ToastLevel::Success);
    }

    pub fn toast_err(&mut self, text: impl Into<String>) {
        self.toast_level(text, ToastLevel::Error);
    }

    pub fn adapter(&self) -> Option<&Adapter> {
        let name = self.selected_adapter.as_ref()?;
        self.adapters.iter().find(|a| &a.name == name)
    }

    pub fn refresh_adapters(&mut self) {
        self.adapters = interfaces::list_adapters();
        let still_there = self
            .selected_adapter
            .as_ref()
            .map(|n| self.adapters.iter().any(|a| &a.name == n))
            .unwrap_or(false);
        if !still_there {
            let preferred = self
                .public_v6
                .as_ref()
                .and_then(|i| i.addrs().into_iter().find_map(|a| self.adapter_owning(a)))
                .map(|(a, _)| a.name.clone());
            let pick = preferred.or_else(|| {
                self.adapters
                    .iter()
                    .find(|a| a.has_global() && a.up)
                    .or_else(|| self.adapters.iter().find(|a| a.has_global()))
                    .or_else(|| self.adapters.first())
                    .map(|a| a.name.clone())
            });
            self.select_adapter(pick);
        }
    }

    pub fn adapter_owning(&self, addr: Ipv6Addr) -> Option<(&Adapter, &V6Entry)> {
        self.adapters
            .iter()
            .find_map(|a| a.ipv6.iter().find(|e| e.addr == addr).map(|e| (a, e)))
    }

    fn owner_info(&self, addr: Ipv6Addr) -> Option<(String, String, u8)> {
        self.adapter_owning(addr).map(|(a, e)| {
            let len = if e.prefix_len == 0 || e.prefix_len >= 128 { 64 } else { e.prefix_len };
            (a.name.clone(), a.display_name().to_string(), len)
        })
    }

    pub fn detect_public(&mut self, auto_select: bool, force: bool) {
        if self.detecting_public {
            return;
        }
        self.detecting_public = true;
        self.log(LogLevel::Info, "Đang phát hiện IPv6 public...");
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let info = public::detect(Duration::from_secs(5));
            let _ = tx.send(Msg::PublicV6 {
                info,
                select: auto_select,
                force,
            });
            ctx.request_repaint();
        });
    }

    fn handle_public(&mut self, info: PublicInfo, select: bool, force: bool) {
        self.detecting_public = false;
        let mut target: Option<(Ipv6Addr, String, u8)> = None;
        let route_owner = info.route.and_then(|r| self.owner_info(r).map(|o| (r, o)));
        match &info.public {
            Ok(addr) => {
                if let Some((name, display, len)) = self.owner_info(*addr) {
                    self.log(LogLevel::Ok, format!("Phát hiện IPv6 public: {addr} trên card {display}"));
                    target = Some((*addr, name, len));
                } else if let Some((r, (name, display, len))) = route_owner {
                    self.log(
                        LogLevel::Warn,
                        format!(
                            "IPv6 public {addr} không nằm trên card nào (có thể NAT66, ULA hoặc tunnel). \
                             Dùng địa chỉ nguồn route {r} trên card {display}; địa chỉ tạo từ prefix cục bộ có thể không ra Internet bằng IP public này"
                        ),
                    );
                    target = Some((r, name, len));
                } else {
                    self.log(
                        LogLevel::Warn,
                        format!(
                            "IPv6 public {addr} không nằm trên card nào (có thể NAT66, ULA hoặc tunnel). \
                             Địa chỉ tạo từ prefix cục bộ có thể không ra Internet bằng IP public này"
                        ),
                    );
                }
            }
            Err(e) => {
                if let Some((r, (name, display, len))) = route_owner {
                    self.log(
                        LogLevel::Warn,
                        format!("{e}. Dùng địa chỉ nguồn route {r} trên card {display}"),
                    );
                    target = Some((r, name, len));
                } else if let Some(r) = info.route {
                    self.log(
                        LogLevel::Warn,
                        format!("{e}. Địa chỉ nguồn route {r} không nằm trên card nào"),
                    );
                } else {
                    self.log(LogLevel::Error, format!("Không phát hiện được IPv6 public: {e}"));
                }
            }
        }
        let may_select = force || (!self.user_chose_adapter && !self.custom_prefix);
        if select && may_select {
            if let Some((_, name, _)) = target {
                if force || self.selected_adapter.as_deref() != Some(name.as_str()) {
                    self.select_adapter(Some(name));
                }
            }
        }
        self.public_v6 = Some(info);
    }

    pub fn select_adapter(&mut self, name: Option<String>) {
        self.selected_adapter = name;
        self.use_detected_prefix();
    }

    pub fn use_detected_prefix(&mut self) {
        self.custom_prefix = false;
        let detected = self.adapter().and_then(|a| a.detected_prefix());
        self.prefix_text = match detected {
            Some(d) => format!("{}/{}", d.network, d.prefix_len),
            None => String::new(),
        };
    }

    fn restore_tracked(&mut self, persisted: Persisted) {
        if persisted.tracked.is_empty() {
            return;
        }
        let existing = interfaces::all_existing_v6();
        let mut kept = 0usize;
        let mut dropped = 0usize;
        for t in persisted.tracked {
            if existing.contains(&t.addr) {
                self.rows.push(Row::new(t.addr, Status::Added, t.adapter_index, t.adapter_name, t.prefix_len));
                kept += 1;
            } else {
                dropped += 1;
            }
        }
        self.rebuild_index();
        if kept > 0 {
            self.log(LogLevel::Info, format!("Khôi phục {kept} địa chỉ đã thêm từ phiên trước"));
        }
        if dropped > 0 {
            self.log(
                LogLevel::Warn,
                format!("Bỏ {dropped} địa chỉ đã lưu vì không còn trên card mạng (có thể do khởi động lại)"),
            );
        }
        self.persist();
    }

    pub fn persist(&mut self) {
        self.settings_dirty_at = None;
        let tracked = self
            .rows
            .iter()
            .filter(|r| r.status == Status::Added)
            .map(|r| Tracked {
                addr: r.addr,
                adapter_index: r.adapter_index,
                adapter_name: r.adapter_name.clone(),
                prefix_len: r.prefix_len,
            })
            .collect();
        let mut settings = self.settings.clone();
        settings.sequential = self.mode == Mode::Sequential;
        settings.offset_text = self.offset_text.clone();
        settings.export_scope = self.export_scope.to_u8();
        settings.only_global = self.only_global;
        settings.custom_prefix = self.custom_prefix;
        settings.prefix_text = if self.custom_prefix { self.prefix_text.clone() } else { String::new() };
        let data = Persisted { tracked, settings };
        if let Err(e) = storage::save(&data) {
            self.log(LogLevel::Error, format!("Không lưu được trạng thái: {e}"));
        }
    }

    pub fn rebuild_index(&mut self) {
        self.by_addr = self.rows.iter().enumerate().map(|(i, r)| (r.addr, i)).collect();
        self.visible_dirty = true;
        self.port_map_dirty = true;
        self.last_clicked = None;
    }

    pub fn recompute_visible(&mut self) {
        let needle = self.filter.trim().to_lowercase();
        let status_hit = [Status::Pending, Status::Added, Status::Failed, Status::Removed]
            .map(|s| needle.is_empty() || s.label().to_lowercase().contains(&needle));
        let test_hit = ["", "đang thử", "sống", "chết"].map(|l| !needle.is_empty() && !l.is_empty() && l.contains(&needle));
        let mut visible: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                needle.is_empty()
                    || r.text.contains(&needle)
                    || status_hit[r.status.rank() as usize]
                    || test_hit[r.test_rank() as usize]
            })
            .map(|(i, _)| i)
            .collect();
        if let Some((key, asc)) = self.sort {
            let rows = &self.rows;
            visible.sort_by(|&x, &y| {
                let (rx, ry) = (&rows[x], &rows[y]);
                let ord = match key {
                    SortKey::Addr => rx.addr.cmp(&ry.addr),
                    SortKey::Status => rx.status.rank().cmp(&ry.status.rank()).then(rx.addr.cmp(&ry.addr)),
                    SortKey::Test => rx.test_rank().cmp(&ry.test_rank()).then(rx.addr.cmp(&ry.addr)),
                    SortKey::Latency => rx
                        .latency_ms
                        .unwrap_or(u32::MAX)
                        .cmp(&ry.latency_ms.unwrap_or(u32::MAX))
                        .then(rx.addr.cmp(&ry.addr)),
                };
                if asc {
                    ord
                } else {
                    ord.reverse()
                }
            });
        }
        self.visible = visible;
        self.visible_dirty = false;
    }

    pub fn refresh_counts(&mut self) {
        let mut c = Counts::default();
        for r in &self.rows {
            let added = r.status == Status::Added;
            let eligible = matches!(r.status, Status::Pending | Status::Failed | Status::Removed);
            if added {
                c.added += 1;
                if matches!(r.test, TestState::Fail(_)) {
                    c.test_failed += 1;
                }
            }
            if eligible {
                c.eligible += 1;
            }
            if r.selected {
                c.selected += 1;
                if added {
                    c.selected_added += 1;
                }
                if eligible {
                    c.selected_eligible += 1;
                }
            }
        }
        self.counts = c;
    }

    pub fn added_count(&self) -> usize {
        self.rows.iter().filter(|r| r.status == Status::Added).count()
    }

    pub fn selected_count(&self) -> usize {
        self.rows.iter().filter(|r| r.selected).count()
    }

    pub fn effective_prefix_text(&self) -> String {
        if self.custom_prefix {
            return self.prefix_text.clone();
        }
        match self.adapter().and_then(|a| a.detected_prefix()) {
            Some(d) => format!("{}/{}", d.network, d.prefix_len),
            None => String::new(),
        }
    }

    pub fn can_apply(&self) -> bool {
        self.elevated && self.tools_error.is_none()
    }

    pub fn eligibility(&self) -> Eligibility {
        let mut e = Eligibility::default();
        let Some(a) = self.adapter() else {
            e.blocked.push("Hãy chọn một card mạng ở thanh bên trái".into());
            return e;
        };
        if !a.up {
            e.blocked.push("Card mạng đang tắt, hãy bật card mạng trước khi tạo địa chỉ".into());
        }
        if !a.has_global() {
            e.blocked.push(
                "Router/ISP không cấp IPv6 global cho card này (chỉ có link-local hoặc ULA) nên không thể tạo địa chỉ dùng được. Hãy bật IPv6 trên router/modem hoặc chọn card khác"
                    .into(),
            );
        } else if !a.v6_gateway {
            e.blocked.push(
                "Card mạng có IPv6 global nhưng không có gateway/route IPv6 mặc định, địa chỉ tạo ra sẽ không ra được Internet".into(),
            );
        }
        if self.custom_prefix {
            if let (Ok((prefix, len)), Some(d)) = (generator::parse_prefix(&self.prefix_text), a.detected_prefix()) {
                if len < d.prefix_len || network_of(prefix, d.prefix_len) != d.network {
                    e.blocked.push(format!(
                        "Prefix tùy chỉnh nằm ngoài prefix router cấp cho card này ({}/{})",
                        d.network, d.prefix_len
                    ));
                }
            }
        }
        if a.only_single_host_global() {
            e.warnings.push(
                "Card mạng chỉ có một địa chỉ global /128 (DHCPv6 đơn lẻ): prefix on-link không rõ, ISP có thể không route các địa chỉ thêm vào"
                    .into(),
            );
        }
        if let Some(info) = &self.public_v6 {
            if let Ok(addr) = &info.public {
                if self.adapter_owning(*addr).is_none() {
                    e.warnings.push(format!(
                        "IPv6 public {addr} không nằm trên card nào (có thể NAT66 hoặc tunnel), địa chỉ tạo ra có thể không ra Internet bằng IP public này"
                    ));
                }
            }
        }
        if !self.elevated {
            e.warnings.push(format!(
                "Thiếu quyền admin/root nên chưa thể áp dụng vào card. {}",
                platform::elevation_hint()
            ));
        }
        if let Some(t) = &self.tools_error {
            e.warnings.push(format!("Không thể áp dụng vào card: {t}"));
        }
        e
    }

    pub fn busy(&self) -> bool {
        self.op.is_some()
    }

    pub fn generate(&mut self, then_apply: bool) {
        self.form_error = None;
        if self.busy() {
            self.log(LogLevel::Warn, "Đang có tác vụ chạy, vui lòng chờ");
            return;
        }
        let Some(adapter) = self.adapter().cloned() else {
            self.form_error = Some("Hãy chọn một card mạng ở thanh bên trái".into());
            return;
        };
        let elig = self.eligibility();
        if !elig.allowed() {
            self.form_error = Some(elig.blocked.join("
"));
            return;
        }
        let (prefix, prefix_len) = match generator::parse_prefix(&self.effective_prefix_text()) {
            Ok(v) => v,
            Err(e) => {
                self.form_error = Some(e.to_string());
                return;
            }
        };
        let start_offset = if self.mode == Mode::Sequential {
            match generator::parse_offset(&self.offset_text) {
                Ok(v) => v,
                Err(e) => {
                    self.form_error = Some(e.to_string());
                    return;
                }
            }
        } else {
            0
        };
        let mut exclude: HashSet<Ipv6Addr> = interfaces::all_existing_v6();
        exclude.extend(self.rows.iter().filter(|r| r.status == Status::Added).map(|r| r.addr));
        let params = GenParams {
            prefix,
            prefix_len,
            count: self.settings.count,
            mode: self.mode,
            start_offset,
        };
        let addrs = match generator::generate(&params, &exclude) {
            Ok(v) => v,
            Err(e) => {
                self.form_error = Some(e.to_string());
                return;
            }
        };
        self.rows.retain(|r| r.status == Status::Added);
        let first_new = self.rows.len();
        for a in &addrs {
            self.rows
                .push(Row::new(*a, Status::Pending, adapter.index, adapter.name.clone(), prefix_len));
        }
        self.rebuild_index();
        self.log(
            LogLevel::Ok,
            format!(
                "Đã tạo {} địa chỉ từ {}/{} ({})",
                addrs.len(),
                prefix,
                prefix_len,
                if self.mode == Mode::Random { "ngẫu nhiên" } else { "tuần tự" }
            ),
        );
        self.tab = Tab::Proxy;
        if then_apply {
            let idxs: Vec<usize> = (first_new..self.rows.len()).collect();
            self.start_apply(idxs);
        }
    }

    pub fn apply_targets(&self) -> Vec<usize> {
        let eligible = |r: &Row| matches!(r.status, Status::Pending | Status::Failed | Status::Removed);
        let selected: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selected && eligible(r))
            .map(|(i, _)| i)
            .collect();
        if !selected.is_empty() {
            return selected;
        }
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| eligible(r))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn remove_targets(&self, only_selected: bool) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.status == Status::Added && (!only_selected || r.selected))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn retest_targets(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.status == Status::Added && matches!(r.test, TestState::Fail(_)))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn card_names(&self, idxs: &[usize]) -> String {
        let mut names: Vec<String> = Vec::new();
        for &i in idxs {
            let r = &self.rows[i];
            let n = self
                .adapters
                .iter()
                .find(|a| a.name == r.adapter_name)
                .map(|a| a.display_name().to_string())
                .unwrap_or_else(|| r.adapter_name.clone());
            if !names.contains(&n) {
                names.push(n);
            }
        }
        names.join(", ")
    }

    pub fn apply_block_reason(&self) -> Option<String> {
        if self.busy() {
            return Some("Đang có tác vụ chạy, vui lòng chờ".into());
        }
        if !self.elevated {
            return Some(platform::elevation_hint());
        }
        self.tools_error.clone()
    }

    pub fn run_confirm(&mut self, c: Confirm) {
        match c {
            Confirm::RemoveSelected => {
                let t = self.remove_targets(true);
                self.start_remove(t);
            }
            Confirm::RemoveAll => {
                let t = self.remove_targets(false);
                self.start_remove(t);
            }
            Confirm::StopProxy => self.stop_proxy(),
        }
    }

    pub fn cancel_op(&mut self) {
        if let Some(op) = &mut self.op {
            op.cancelling = true;
            op.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn cancel_test(&mut self) {
        if let Some(t) = &mut self.test {
            t.cancelling = true;
            t.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn test_targets(&self) -> Vec<usize> {
        let selected: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selected && r.status == Status::Added)
            .map(|(i, _)| i)
            .collect();
        if !selected.is_empty() {
            return selected;
        }
        self.remove_targets(false)
    }

    fn precheck(&mut self) -> bool {
        if self.op.is_some() {
            self.log(LogLevel::Warn, "Đang có tác vụ chạy, vui lòng chờ");
            return false;
        }
        if !self.elevated {
            self.log(LogLevel::Error, format!("Thiếu quyền admin/root. {}", platform::elevation_hint()));
            self.toast_err("Cần quyền admin/root");
            return false;
        }
        if let Some(e) = &self.tools_error {
            let e = e.clone();
            self.log(LogLevel::Error, e);
            return false;
        }
        true
    }

    fn group_rows(&self, idxs: &[usize]) -> Vec<(Target, Vec<Ipv6Addr>)> {
        let mut groups: Vec<(Target, Vec<Ipv6Addr>)> = Vec::new();
        for &i in idxs {
            let r = &self.rows[i];
            match groups
                .iter_mut()
                .find(|(t, _)| t.index == r.adapter_index && t.name == r.adapter_name && t.prefix_len == r.prefix_len)
            {
                Some((_, v)) => v.push(r.addr),
                None => groups.push((
                    Target {
                        index: r.adapter_index,
                        name: r.adapter_name.clone(),
                        prefix_len: r.prefix_len,
                    },
                    vec![r.addr],
                )),
            }
        }
        groups
    }

    pub fn start_apply(&mut self, idxs: Vec<usize>) {
        if idxs.is_empty() {
            self.log(LogLevel::Warn, "Không có địa chỉ nào để áp dụng");
            return;
        }
        if !self.precheck() {
            return;
        }
        let groups = self.group_rows(&idxs);
        let opts = ApplyOptions {
            persistent: self.settings.persistent && platform::supports_persistent(),
            nodad: self.settings.nodad && platform::supports_nodad(),
        };
        self.spawn_op(OpKind::Apply, idxs.len(), groups, opts);
    }

    pub fn start_remove(&mut self, idxs: Vec<usize>) {
        if idxs.is_empty() {
            self.log(LogLevel::Warn, "Không có địa chỉ nào để gỡ");
            return;
        }
        if !self.precheck() {
            return;
        }
        let groups = self.group_rows(&idxs);
        self.spawn_op(OpKind::Remove, idxs.len(), groups, ApplyOptions::default());
    }

    fn spawn_op(&mut self, kind: OpKind, total: usize, groups: Vec<(Target, Vec<Ipv6Addr>)>, opts: ApplyOptions) {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut targets: Vec<Target> = Vec::with_capacity(groups.len());
        let mut addr_target: HashMap<Ipv6Addr, usize> = HashMap::new();
        for (t, addrs) in &groups {
            let ti = targets.len();
            targets.push(t.clone());
            for a in addrs {
                addr_target.insert(*a, ti);
            }
        }
        self.last_result = None;
        self.op = Some(Op {
            kind,
            total,
            done: 0,
            ok: 0,
            fail: 0,
            started: Instant::now(),
            cancel: Arc::clone(&cancel),
            cancelling: false,
            targets,
            addr_target,
        });
        let verb = if kind == OpKind::Apply { "Áp dụng" } else { "Gỡ" };
        self.log(LogLevel::Info, format!("{verb} {total} địa chỉ..."));
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            'outer: for (target, addrs) in groups {
                for chunk in addrs.chunks(CHUNK_SIZE) {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = tx.send(Msg::Log(LogLevel::Warn, "Đã dừng theo yêu cầu".into()));
                        break 'outer;
                    }
                    let outcomes = if kind == OpKind::Apply {
                        platform::add_chunk(&target, chunk, opts)
                    } else {
                        platform::remove_chunk(&target, chunk)
                    };
                    let items = outcomes.into_iter().map(|o| (o.addr, o.result)).collect();
                    let _ = tx.send(Msg::Outcomes {
                        remove: kind == OpKind::Remove,
                        items,
                    });
                    ctx.request_repaint();
                }
            }
            let _ = tx.send(Msg::OpDone);
            ctx.request_repaint();
        });
    }

    pub fn start_test(&mut self, idxs: Vec<usize>) {
        if self.test.is_some() {
            self.log(LogLevel::Warn, "Đang kiểm tra, vui lòng chờ");
            return;
        }
        if idxs.is_empty() {
            self.log(LogLevel::Warn, "Không có proxy nào để kiểm tra");
            return;
        }
        let url_text = self.settings.ip_check_url.clone();
        let url = match tester::parse_check_url(&url_text) {
            Ok(u) => u,
            Err(e) => {
                self.log(LogLevel::Error, e.clone());
                self.toast_err(e);
                return;
            }
        };
        let auth = match self.proxy_credentials() {
            Ok(a) => a,
            Err(e) => {
                self.log(LogLevel::Error, e.clone());
                self.toast_err(e);
                return;
            }
        };
        if self.proxy.is_none() {
            if self.stopping_proxy {
                self.log(LogLevel::Warn, "Proxy đang dừng, hãy thử lại sau giây lát");
                self.toast_err("Proxy đang dừng, thử lại sau giây lát");
                return;
            }
            self.start_proxy();
        }
        self.ensure_port_map();
        let Some(handle) = &self.proxy else {
            let msg = self
                .proxy_error
                .clone()
                .unwrap_or_else(|| "Không khởi động được proxy".to_string());
            self.log(LogLevel::Error, format!("Không thể kiểm tra proxy: {msg}"));
            self.toast_err("Không khởi động được proxy");
            return;
        };
        let stats = Arc::clone(&handle.stats);
        let bind = handle.bind;
        let host = if bind.is_unspecified() { Ipv4Addr::LOCALHOST } else { bind };
        let pairs: Vec<(Ipv6Addr, u16)> = idxs
            .iter()
            .filter_map(|&i| {
                let addr = self.rows[i].addr;
                self.port_map.get(&addr).map(|&p| (addr, p))
            })
            .collect();
        if pairs.is_empty() {
            self.log(LogLevel::Warn, "Không có địa chỉ nào nằm trong danh sách proxy đang chạy");
            return;
        }
        for (addr, _) in &pairs {
            if let Some(&i) = self.by_addr.get(addr) {
                let r = &mut self.rows[i];
                r.test = TestState::Running;
                r.latency_ms = None;
                r.egress = None;
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.last_result = None;
        self.test = Some(TestRun {
            total: pairs.len(),
            done: 0,
            ok: 0,
            mismatch: 0,
            cancel: Arc::clone(&cancel),
            cancelling: false,
        });
        let threads = self.settings.test_threads;
        let timeout = Duration::from_secs(self.settings.test_timeout_secs.max(1));
        self.log(
            LogLevel::Info,
            format!("Kiểm tra {} proxy qua {} ({} luồng)", pairs.len(), url_text.trim(), threads),
        );
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            if tester::wait_ready(&stats, &cancel, Duration::from_secs(10)) {
                let tx2 = tx.clone();
                let ctx2 = ctx.clone();
                tester::run(
                    TestJob {
                        pairs,
                        host,
                        url,
                        auth,
                        threads,
                        timeout,
                        cancel,
                    },
                    move |addr, res| {
                        let _ = tx2.send(Msg::TestResult(addr, res));
                        ctx2.request_repaint();
                    },
                );
            } else if !cancel.load(Ordering::Relaxed) {
                let _ = tx.send(Msg::Log(LogLevel::Error, "Proxy chưa sẵn sàng sau 10 giây".into()));
            }
            let _ = tx.send(Msg::TestDone);
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self) {
        let mut changed = false;
        while let Ok(msg) = self.rx.try_recv() {
            changed = true;
            match msg {
                Msg::Log(level, text) => self.log(level, text),
                Msg::Outcomes { remove, items } => self.handle_outcomes(remove, items),
                Msg::OpDone => self.finish_op(),
                Msg::TestResult(addr, res) => self.handle_test(addr, res),
                Msg::TestDone => self.finish_test(),
                Msg::PublicV6 { info, select, force } => self.handle_public(info, select, force),
                Msg::ProxyStopped => self.handle_proxy_stopped(),
                Msg::Relaunch(res) => self.handle_relaunch(res),
                Msg::Export(res) => self.handle_export(res),
            }
        }
        if changed {
            self.visible_dirty = true;
        }
    }

    fn handle_outcomes(&mut self, remove: bool, items: Vec<(Ipv6Addr, Result<(), String>)>) {
        let mut logs: Vec<(LogLevel, String)> = Vec::new();
        let mut restore: Vec<Row> = Vec::new();
        for (addr, res) in items {
            let ok = res.is_ok();
            if let Some(&i) = self.by_addr.get(&addr) {
                let row = &mut self.rows[i];
                match (remove, res) {
                    (false, Ok(())) => {
                        row.status = Status::Added;
                        row.error = None;
                        logs.push((LogLevel::Ok, format!("Đã thêm {addr}")));
                    }
                    (false, Err(e)) => {
                        row.status = Status::Failed;
                        logs.push((LogLevel::Error, format!("Thêm {addr} thất bại: {e}")));
                        row.error = Some(e);
                    }
                    (true, Ok(())) => {
                        row.status = Status::Removed;
                        row.selected = false;
                        row.test = TestState::None;
                        row.latency_ms = None;
                        row.egress = None;
                        row.error = None;
                        logs.push((LogLevel::Ok, format!("Đã gỡ {addr}")));
                    }
                    (true, Err(e)) => {
                        logs.push((LogLevel::Error, format!("Gỡ {addr} thất bại: {e}")));
                        row.error = Some(e);
                    }
                }
            } else {
                let meta = self
                    .op
                    .as_ref()
                    .and_then(|op| op.addr_target.get(&addr).map(|&t| op.targets[t].clone()));
                match (remove, res, meta) {
                    (false, Ok(()), Some(t)) => {
                        restore.push(Row::new(addr, Status::Added, t.index, t.name, t.prefix_len));
                        logs.push((
                            LogLevel::Warn,
                            format!("{addr} đã được thêm vào card nhưng dòng trong danh sách bị mất, đã khôi phục"),
                        ));
                    }
                    (false, Ok(()), None) => {
                        logs.push((
                            LogLevel::Error,
                            format!("{addr} đã được thêm vào card nhưng không còn trong danh sách, hãy gỡ thủ công"),
                        ));
                    }
                    (_, Err(e), _) => {
                        logs.push((LogLevel::Error, format!("Kết quả cho {addr} (không còn trong danh sách): {e}")));
                    }
                    (true, Ok(()), _) => {
                        logs.push((LogLevel::Info, format!("Đã gỡ {addr} (không còn trong danh sách)")));
                    }
                }
            }
            if let Some(op) = &mut self.op {
                op.done += 1;
                if ok {
                    op.ok += 1;
                } else {
                    op.fail += 1;
                }
            }
        }
        if !restore.is_empty() {
            self.rows.extend(restore);
            self.rebuild_index();
        }
        self.port_map_dirty = true;
        let total = logs.len();
        for (level, text) in logs.into_iter().take(200) {
            self.log(level, text);
        }
        if total > 200 {
            self.log(LogLevel::Info, format!("... và {} dòng log khác (đã rút gọn)", total - 200));
        }
    }

    fn finish_op(&mut self) {
        if let Some(op) = self.op.take() {
            let apply = op.kind == OpKind::Apply;
            let verb = if apply { "Áp dụng" } else { "Gỡ" };
            let level = if op.fail == 0 { LogLevel::Ok } else { LogLevel::Warn };
            let secs = op.started.elapsed().as_secs_f32();
            self.log(
                level,
                format!(
                    "{verb} xong: {} thành công, {} thất bại ({:.1}s)",
                    op.ok, op.fail, secs
                ),
            );
            let stopped = if op.cancelling { " (đã dừng)" } else { "" };
            let summary = format!("{verb} xong{stopped}: {} OK, {} lỗi", op.ok, op.fail);
            let action = if op.fail == 0 {
                None
            } else if apply {
                Some(ResultAction::FilterRows(Status::Failed.label()))
            } else {
                Some(ResultAction::OpenLogs)
            };
            if op.fail == 0 {
                self.toast_ok(summary.clone());
            } else {
                self.toast_err(summary.clone());
            }
            self.last_result = Some(LastResult {
                text: format!("{summary} ({secs:.1}s)"),
                level,
                action,
            });
        }
        self.refresh_adapters();
        self.persist();
        self.sync_proxy();
        self.visible_dirty = true;
    }

    fn sync_proxy(&mut self) {
        if self.added_count() == 0 {
            self.start_after_stop = false;
            self.stop_proxy();
            return;
        }
        if self.proxy_stale() {
            self.stop_proxy();
        }
        if self.proxy.is_none() {
            self.start_proxy();
        }
    }

    pub fn proxy_stale(&self) -> bool {
        match &self.proxy {
            Some(h) => !self
                .rows
                .iter()
                .filter(|r| r.status == Status::Added)
                .map(|r| r.addr)
                .eq(h.addrs.iter().copied()),
            None => false,
        }
    }

    fn handle_test(&mut self, addr: Ipv6Addr, res: Result<(u32, IpAddr), String>) {
        let mut warn: Option<String> = None;
        if let Some(&i) = self.by_addr.get(&addr) {
            let row = &mut self.rows[i];
            match res {
                Ok((ms, ip)) => {
                    row.test = TestState::Ok;
                    row.latency_ms = Some(ms);
                    row.egress = Some(ip);
                    if let Some(t) = &mut self.test {
                        t.ok += 1;
                        if ip != IpAddr::V6(addr) {
                            t.mismatch += 1;
                            if t.mismatch <= 3 {
                                warn = Some(format!("IP ra {ip} khác địa chỉ IPv6 {addr} (có thể NAT66)"));
                            }
                        }
                    }
                }
                Err(e) => {
                    row.test = TestState::Fail(e);
                    row.latency_ms = None;
                    row.egress = None;
                }
            }
        }
        if let Some(t) = &mut self.test {
            t.done += 1;
        }
        if let Some(w) = warn {
            self.log(LogLevel::Warn, w);
        }
    }

    fn finish_test(&mut self) {
        for r in &mut self.rows {
            if r.test == TestState::Running {
                r.test = TestState::None;
            }
        }
        if let Some(t) = self.test.take() {
            let level = if t.ok == t.done { LogLevel::Ok } else { LogLevel::Warn };
            self.log(level, format!("Kiểm tra proxy xong: {}/{} proxy sống", t.ok, t.done));
            if t.mismatch > 0 {
                self.log(
                    LogLevel::Warn,
                    format!("{} proxy có IP ra khác địa chỉ IPv6 được gán (có thể NAT66)", t.mismatch),
                );
            }
            if t.done == 0 {
                if !t.cancelling {
                    self.toast_err("Proxy chưa sẵn sàng, chưa kiểm tra được");
                }
            } else {
                let dead = t.done.saturating_sub(t.ok);
                let stopped = if t.cancelling { " (đã dừng)" } else { "" };
                let summary = format!("Kiểm tra xong{stopped}: {}/{} proxy sống, {dead} lỗi", t.ok, t.done);
                if dead == 0 {
                    self.toast_ok(summary.clone());
                } else {
                    self.toast_err(summary.clone());
                }
                self.last_result = Some(LastResult {
                    text: summary,
                    level,
                    action: if dead > 0 { Some(ResultAction::FilterRows("Chết")) } else { None },
                });
            }
        }
    }

    pub fn clear_non_added(&mut self) {
        let only_selected = self.selected_count() > 0;
        let before = self.rows.len();
        self.rows
            .retain(|r| r.status == Status::Added || (only_selected && !r.selected));
        self.last_clicked = None;
        let removed = before - self.rows.len();
        self.rebuild_index();
        self.log(LogLevel::Info, format!("Đã xóa {removed} dòng khỏi danh sách (không ảnh hưởng card mạng)"));
    }

    fn in_export_scope(&self, r: &Row) -> bool {
        match self.export_scope {
            ExportScope::All => true,
            ExportScope::Selected => r.selected,
            ExportScope::Added => r.status == Status::Added,
        }
    }

    pub fn export_count(&self) -> usize {
        let fmt = self.settings.export_format;
        self.rows
            .iter()
            .filter(|r| self.in_export_scope(r))
            .filter(|r| fmt < 3 || self.port_map.contains_key(&r.addr))
            .count()
    }

    pub fn export_text(&self) -> (String, usize) {
        let fmt = self.settings.export_format;
        let port = self.settings.export_port.trim().to_string();
        let host = if fmt >= 3 { self.proxy_export_host() } else { Ipv4Addr::LOCALHOST };
        let ports = &self.port_map;
        let lines: Vec<String> = self
            .rows
            .iter()
            .filter(|r| self.in_export_scope(r))
            .filter_map(|r| match fmt {
                1 => Some(format!("[{}]:{}", r.addr, port)),
                2 => Some(format!("{}/{}", r.addr, r.prefix_len)),
                3 => ports.get(&r.addr).map(|p| format!("{host}:{p}")),
                4 => ports
                    .get(&r.addr)
                    .map(|p| format!("{host}:{p}:{}:{}", self.settings.proxy_user, self.settings.proxy_pass)),
                _ => Some(r.addr.to_string()),
            })
            .collect();
        (lines.join("\n"), lines.len())
    }

    pub fn export_valid(&self) -> Result<(), String> {
        let fmt = self.settings.export_format;
        if fmt == 1 && self.settings.export_port.trim().parse::<u16>().is_err() {
            return Err("Cổng không hợp lệ (0-65535)".into());
        }
        if fmt >= 3 {
            self.proxy_bind_ip()?;
            proxy::port_range(self.proxy_start_port(), self.proxy_export_len())?;
        }
        if fmt == 4 {
            if !self.settings.proxy_auth {
                return Err("Hãy bật xác thực ở phần Proxy để xuất ip:port:user:pass".into());
            }
            self.proxy_credentials()?;
        }
        Ok(())
    }

    pub fn proxy_bind_ip(&self) -> Result<Ipv4Addr, String> {
        match &self.proxy {
            Some(h) => Ok(h.bind),
            None => self
                .settings
                .proxy_bind
                .trim()
                .parse::<Ipv4Addr>()
                .map_err(|_| "Địa chỉ bind IPv4 của proxy không hợp lệ".to_string()),
        }
    }

    pub fn proxy_start_port(&self) -> u16 {
        match &self.proxy {
            Some(h) => h.start_port,
            None => self.settings.proxy_start_port,
        }
    }

    pub fn proxy_addrs(&self) -> Vec<Ipv6Addr> {
        self.rows
            .iter()
            .filter(|r| r.status == Status::Added)
            .map(|r| r.addr)
            .collect()
    }

    pub fn proxy_export_len(&self) -> usize {
        match &self.proxy {
            Some(h) => h.addrs.len(),
            None => self.added_count(),
        }
    }

    pub fn proxy_export_addrs(&self) -> Vec<Ipv6Addr> {
        match &self.proxy {
            Some(h) => h.addrs.clone(),
            None => self.proxy_addrs(),
        }
    }

    pub fn proxy_credentials(&self) -> Result<Option<Credentials>, String> {
        if !self.settings.proxy_auth {
            return Ok(None);
        }
        let user = self.settings.proxy_user.trim();
        let pass = self.settings.proxy_pass.as_str();
        if user.is_empty() || pass.is_empty() {
            return Err("Hãy nhập tên đăng nhập và mật khẩu cho proxy".into());
        }
        if user.contains(':') {
            return Err("Tên đăng nhập proxy không được chứa dấu ':'".into());
        }
        Ok(Some(Credentials {
            user: user.to_string(),
            pass: pass.to_string(),
        }))
    }

    pub fn proxy_export_host(&self) -> Ipv4Addr {
        let bind = self.proxy_bind_ip().unwrap_or(Ipv4Addr::LOCALHOST);
        if !bind.is_unspecified() {
            return bind;
        }
        self.adapter()
            .and_then(|a| a.ipv4.iter().map(|(ip, _)| *ip).find(|ip| !ip.is_link_local()))
            .unwrap_or(Ipv4Addr::LOCALHOST)
    }

    pub fn ensure_port_map(&mut self) {
        let key = (
            self.proxy.is_some(),
            self.proxy_start_port(),
            self.proxy.as_ref().map_or(0, |h| h.addrs.len()),
        );
        if self.port_map_dirty || self.port_map_key != Some(key) {
            self.port_map = self.compute_port_map();
            self.port_map_key = Some(key);
            self.port_map_dirty = false;
        }
    }

    fn compute_port_map(&self) -> HashMap<Ipv6Addr, u16> {
        let start = self.proxy_start_port();
        self.proxy_export_addrs()
            .into_iter()
            .enumerate()
            .filter_map(|(i, a)| {
                let port = u16::try_from(i).ok().and_then(|o| start.checked_add(o))?;
                Some((a, port))
            })
            .collect()
    }

    pub fn start_proxy(&mut self) {
        self.proxy_error = None;
        if self.proxy.is_some() {
            return;
        }
        if self.stopping_proxy {
            self.start_after_stop = true;
            return;
        }
        let result = (|| -> Result<ProxyConfig, String> {
            let bind = self.proxy_bind_ip()?;
            let addrs = self.proxy_addrs();
            let start_port = self.settings.proxy_start_port;
            proxy::port_range(start_port, addrs.len())?;
            let auth = self.proxy_credentials()?;
            Ok(ProxyConfig {
                bind,
                start_port,
                auth,
                addrs,
            })
        })();
        let cfg = match result {
            Ok(c) => c,
            Err(e) => {
                self.proxy_error = Some(e);
                return;
            }
        };
        let (bind, count, first, last) = (
            cfg.bind,
            cfg.addrs.len(),
            cfg.start_port,
            cfg.start_port as usize + cfg.addrs.len() - 1,
        );
        match ProxyHandle::start(cfg, self.tx.clone(), self.ctx.clone()) {
            Ok(h) => {
                self.proxy = Some(h);
                self.port_map_dirty = true;
                self.log(
                    LogLevel::Info,
                    format!("Khởi động proxy: {count} địa chỉ, {bind} cổng {first}-{last}"),
                );
                self.persist();
            }
            Err(e) => {
                self.log(LogLevel::Error, format!("Không khởi động được proxy: {e}"));
                self.proxy_error = Some(e.to_string());
            }
        }
    }

    pub fn stop_proxy(&mut self) {
        let Some(handle) = self.proxy.take() else { return };
        self.port_map_dirty = true;
        self.stopping_proxy = true;
        self.log(LogLevel::Info, "Đang dừng proxy...");
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            drop(handle);
            let _ = tx.send(Msg::ProxyStopped);
            ctx.request_repaint();
        });
    }

    fn handle_proxy_stopped(&mut self) {
        self.stopping_proxy = false;
        self.log(LogLevel::Info, "Đã dừng proxy");
        if self.start_after_stop {
            self.start_after_stop = false;
            if self.added_count() > 0 {
                self.start_proxy();
            }
        }
    }

    pub fn copy_text(&mut self, text: String) -> bool {
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }
        match self.clipboard.as_mut() {
            Some(cb) => cb.set_text(text).is_ok(),
            None => {
                self.ctx.copy_text(text);
                true
            }
        }
    }

    pub fn copy_export(&mut self) {
        self.ensure_port_map();
        if let Err(e) = self.export_valid() {
            self.toast_err(e);
            return;
        }
        let (text, n) = self.export_text();
        if n == 0 {
            self.toast_err("Không có địa chỉ để xuất");
            return;
        }
        if self.copy_text(text) {
            self.toast_ok(format!("Đã sao chép {n} địa chỉ"));
            self.log(LogLevel::Info, format!("Đã sao chép {n} địa chỉ vào clipboard"));
        } else {
            self.toast_err("Không truy cập được clipboard");
        }
    }

    pub fn save_export(&mut self) {
        if self.exporting {
            return;
        }
        self.ensure_port_map();
        if let Err(e) = self.export_valid() {
            self.toast_err(e);
            return;
        }
        let (text, n) = self.export_text();
        if n == 0 {
            self.toast_err("Không có địa chỉ để xuất");
            return;
        }
        self.exporting = true;
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Lưu danh sách IPv6")
                .set_file_name("ipv6.txt")
                .add_filter("Văn bản", &["txt"])
                .save_file();
            let res = match picked {
                None => Ok(None),
                Some(path) => std::fs::write(&path, text)
                    .map(|()| Some((path, n)))
                    .map_err(|e| e.to_string()),
            };
            let _ = tx.send(Msg::Export(res));
            ctx.request_repaint();
        });
    }

    fn handle_export(&mut self, res: Result<Option<(PathBuf, usize)>, String>) {
        self.exporting = false;
        match res {
            Ok(Some((path, n))) => {
                self.toast_ok(format!("Đã lưu {n} địa chỉ"));
                self.log(LogLevel::Ok, format!("Đã lưu {n} địa chỉ vào {}", path.display()));
            }
            Ok(None) => {}
            Err(e) => {
                self.log(LogLevel::Error, format!("Không lưu được file: {e}"));
                self.toast_err("Lưu file thất bại");
            }
        }
    }

    pub fn relaunch_admin(&mut self) {
        if self.relaunching {
            return;
        }
        self.relaunching = true;
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let res = platform::relaunch_elevated().map_err(|e| e.to_string());
            let _ = tx.send(Msg::Relaunch(res));
            ctx.request_repaint();
        });
    }

    fn handle_relaunch(&mut self, res: Result<(), String>) {
        self.relaunching = false;
        match res {
            Ok(()) => {
                self.log(LogLevel::Info, "Đang mở lại với quyền admin...");
                self.persist();
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => {
                self.log(LogLevel::Error, e);
                self.toast_err("Không thể nâng quyền");
            }
        }
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.settings.dark = dark;
        self.pal = Palette::get(dark);
        theme::apply(&self.ctx, dark);
        self.settings_dirty = true;
    }
}

impl App {
    fn flush_settings(&mut self) {
        if self.settings_dirty {
            self.settings_dirty = false;
            self.settings_dirty_at = Some(Instant::now());
        }
        if let Some(at) = self.settings_dirty_at {
            let wait = Duration::from_millis(500);
            let elapsed = at.elapsed();
            if elapsed >= wait {
                self.persist();
            } else {
                self.ctx.request_repaint_after(wait - elapsed);
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        self.refresh_counts();
        self.ensure_port_map();
        if self.visible_dirty {
            self.recompute_visible();
        }
        let toast_expired = self.toast.as_ref().map(|t| t.at.elapsed() > t.life());
        match toast_expired {
            Some(true) => self.toast = None,
            Some(false) => self.ctx.request_repaint_after(Duration::from_millis(250)),
            None => {}
        }
        if self.op.is_some() || self.test.is_some() {
            self.ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.proxy.is_some() {
            self.ctx.request_repaint_after(Duration::from_millis(500));
        }
        if self.tab == Tab::Logs {
            self.unseen_errors = 0;
        }
        self.draw(ui);
        self.flush_settings();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.proxy = None;
        self.persist();
    }
}

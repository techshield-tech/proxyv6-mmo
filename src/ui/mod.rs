mod addresses;
mod generate;
mod logs;
mod settings;
mod sidebar;

use eframe::egui::{
    self, Align, Align2, Button, Color32, CornerRadius, Frame, Layout, Margin, Response, RichText, Sense, Stroke,
    StrokeKind, Vec2,
};

use crate::app::{App, Confirm, Tab, ToastLevel};
use crate::net::platform;
use crate::theme::Palette;

pub fn card_pad<R>(ui: &mut egui::Ui, p: &Palette, pad: i8, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(pad))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_pad(ui, p, 16, add)
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.16))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(color));
        });
}

pub fn dot_label(ui: &mut egui::Ui, color: Color32, text: RichText) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
        ui.label(text);
    })
    .response
}

pub fn title(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(15.5).strong().color(p.text));
}

pub fn hint(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).small().color(p.weak));
}

fn press_feedback(ui: &egui::Ui, resp: &Response, hover: Color32, press: Color32) {
    if !resp.enabled() {
        return;
    }
    let color = if resp.is_pointer_button_down_on() {
        press
    } else if resp.hovered() {
        hover
    } else {
        return;
    };
    ui.painter().rect_filled(resp.rect, CornerRadius::same(8), color);
}

fn filled_button(ui: &mut egui::Ui, fill: Color32, enabled: bool, text: &str) -> Response {
    let resp = ui.add_enabled(
        enabled,
        Button::new(RichText::new(text.to_string()).strong().color(Color32::WHITE))
            .fill(fill)
            .stroke(Stroke::NONE)
            .min_size(Vec2::new(0.0, 32.0)),
    );
    press_feedback(ui, &resp, Color32::from_white_alpha(28), Color32::from_black_alpha(48));
    resp
}

pub fn primary(ui: &mut egui::Ui, p: &Palette, enabled: bool, text: &str) -> Response {
    filled_button(ui, p.accent_fill, enabled, text)
}

pub fn danger(ui: &mut egui::Ui, p: &Palette, enabled: bool, text: &str) -> Response {
    filled_button(ui, p.danger_fill, enabled, text)
}

pub fn plain_button(text: &str) -> Button<'static> {
    Button::new(text.to_string()).min_size(Vec2::new(0.0, 32.0))
}

fn tab_button(ui: &mut egui::Ui, p: &Palette, text: String, selected: bool, alert: bool) -> Response {
    let rich = if selected {
        RichText::new(text).strong().color(Color32::WHITE)
    } else if alert {
        RichText::new(text).strong().color(p.error)
    } else {
        RichText::new(text).color(p.weak)
    };
    let fill = if selected { p.accent_fill } else { Color32::TRANSPARENT };
    let resp = ui.add(
        Button::new(rich)
            .min_size(Vec2::new(0.0, 30.0))
            .stroke(Stroke::NONE)
            .fill(fill),
    );
    if selected {
        press_feedback(ui, &resp, Color32::from_white_alpha(28), Color32::from_black_alpha(48));
    } else {
        press_feedback(ui, &resp, p.accent.gamma_multiply(0.16), p.accent.gamma_multiply(0.3));
    }
    resp
}

impl App {
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;

        egui::Panel::top("header")
            .frame(
                Frame::new()
                    .fill(p.panel)
                    .stroke(Stroke::new(1.0, p.border))
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| self.header(ui));

        if !self.elevated {
            egui::Panel::top("banner")
                .frame(
                    Frame::new()
                        .fill(p.warn.gamma_multiply(if p.dark { 0.16 } else { 0.18 }))
                        .stroke(Stroke::new(1.0, p.warn.gamma_multiply(0.6)))
                        .inner_margin(Margin::symmetric(16, 8)),
                )
                .show(ui, |ui| self.banner(ui));
        }

        egui::Panel::bottom("status")
            .frame(
                Frame::new()
                    .fill(p.panel)
                    .stroke(Stroke::new(1.0, p.border))
                    .inner_margin(Margin::symmetric(16, 6)),
            )
            .show(ui, |ui| self.status_bar(ui));

        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(260.0)
            .size_range(220.0..=320.0)
            .frame(
                Frame::new()
                    .fill(p.panel)
                    .stroke(Stroke::new(1.0, p.border))
                    .inner_margin(Margin::same(14)),
            )
            .show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg).inner_margin(Margin::same(16)))
            .show(ui, |ui| match self.tab {
                Tab::Generate => self.generate_tab(ui),
                Tab::Proxy => self.proxy_tab(ui),
                Tab::Logs => self.logs_tab(ui),
                Tab::Settings => self.settings_tab(ui),
            });

        self.draw_confirm(ui);
        self.draw_toast(ui);
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        ui.horizontal(|ui| {
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(26.0)));
                ui.add_space(2.0);
            }
            ui.label(RichText::new("ProxyV6 MMO").size(20.0).strong().color(p.accent));
            ui.add_space(18.0);
            let tabs = [
                (Tab::Generate, "Tạo địa chỉ"),
                (Tab::Proxy, "Địa chỉ & Proxy"),
                (Tab::Logs, "Nhật ký"),
                (Tab::Settings, "Cài đặt"),
            ];
            for (tab, label) in tabs {
                let selected = self.tab == tab;
                let mut alert = false;
                let text = match tab {
                    Tab::Proxy if !self.rows.is_empty() => format!("{label} ({})", self.rows.len()),
                    Tab::Logs if self.unseen_errors > 0 => {
                        alert = true;
                        format!("{label} ({} lỗi)", self.unseen_errors)
                    }
                    _ => label.to_string(),
                };
                if tab_button(ui, &p, text, selected, alert).clicked() {
                    self.tab = tab;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if self.settings.dark {
                    "Chuyển sang giao diện sáng"
                } else {
                    "Chuyển sang giao diện tối"
                };
                if ui.add(plain_button(label)).clicked() {
                    let dark = !self.settings.dark;
                    self.set_dark(dark);
                }
            });
        });
    }

    fn banner(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Chưa có quyền quản trị.").strong().color(p.warn));
            ui.label(RichText::new(platform::elevation_hint()).color(p.text));
            if platform::can_relaunch_elevated() {
                if self.relaunching {
                    ui.spinner();
                    ui.label(RichText::new("Đang chờ xác nhận quyền...").color(p.text));
                } else if primary(ui, &p, true, "Chạy lại với quyền Admin").clicked() {
                    self.relaunch_admin();
                }
            } else if ui.add(plain_button("Sao chép lệnh sudo")).clicked() {
                let exe = std::env::current_exe()
                    .map(|e| e.display().to_string())
                    .unwrap_or_else(|_| "./proxyv6-mmo".into());
                let ok = self.copy_text(format!("sudo -E {exe}"));
                if ok {
                    self.toast_ok("Đã sao chép lệnh");
                } else {
                    self.toast_err("Không truy cập được clipboard");
                }
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        ui.horizontal(|ui| {
            if self.elevated {
                dot_label(ui, p.success, RichText::new("Quyền: Admin/Root").small());
            } else {
                dot_label(ui, p.warn, RichText::new("Quyền: Thường (không đủ)").small());
            }
            ui.separator();
            let adapter = self
                .adapter()
                .map(|a| a.display_name().to_string())
                .unwrap_or_else(|| "chưa chọn".into());
            ui.label(RichText::new(format!("Card mạng: {adapter}")).small().color(p.weak));
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "Đang hoạt động: {} địa chỉ · Danh sách: {}",
                    self.counts.added,
                    self.rows.len()
                ))
                .small()
                .color(p.weak),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(op) = &self.op {
                    ui.label(
                        RichText::new(format!("Đang xử lý {}/{}", op.done, op.total))
                            .small()
                            .color(p.accent),
                    );
                    ui.spinner();
                }
                if let Some(t) = &self.test {
                    ui.label(
                        RichText::new(format!("Đang kiểm tra {}/{}", t.done, t.total))
                            .small()
                            .color(p.info),
                    );
                    ui.spinner();
                }
                if self.stopping_proxy {
                    ui.label(RichText::new("Đang dừng proxy...").small().color(p.warn));
                    ui.spinner();
                }
                if self.detecting_public {
                    ui.label(RichText::new("Đang phát hiện IPv6 public…").small().color(p.weak));
                    ui.spinner();
                }
            });
        });
    }

    fn draw_confirm(&mut self, ui: &mut egui::Ui) {
        let Some(c) = self.pending_confirm else { return };
        let p = self.pal;
        let (heading, body, action) = match c {
            Confirm::RemoveSelected | Confirm::RemoveAll => {
                let only_selected = c == Confirm::RemoveSelected;
                let t = self.remove_targets(only_selected);
                let names = self.card_names(&t);
                let what = if only_selected { "đã chọn" } else { "đã thêm" };
                (
                    format!("Gỡ {} địa chỉ {what}?", t.len()),
                    format!(
                        "{} địa chỉ sẽ bị gỡ khỏi card mạng: {names}. Các proxy tương ứng sẽ dừng hoạt động.",
                        t.len()
                    ),
                    format!("Gỡ {} địa chỉ", t.len()),
                )
            }
            Confirm::StopProxy => {
                let (target, active) = match &self.proxy {
                    Some(h) => {
                        let last = h.start_port as usize + h.addrs.len().saturating_sub(1);
                        let active = h.stats.active.load(std::sync::atomic::Ordering::Relaxed);
                        (format!("{}:{}-{} ({} cổng)", h.bind, h.start_port, last, h.addrs.len()), active)
                    }
                    None => (String::new(), 0),
                };
                (
                    "Dừng proxy?".to_string(),
                    format!("Dừng proxy tại {target}. {active} kết nối đang hoạt động sẽ bị ngắt."),
                    "Dừng proxy".to_string(),
                )
            }
        };
        let mut decision: Option<bool> = None;
        let resp = egui::Modal::new(egui::Id::unique("confirm-modal")).show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new(heading).size(16.0).strong());
            ui.add_space(6.0);
            ui.label(body);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if danger(ui, &p, true, &action).clicked() {
                    decision = Some(true);
                }
                if ui.add(plain_button("Hủy")).clicked() {
                    decision = Some(false);
                }
            });
        });
        if decision.is_none() && resp.should_close() {
            decision = Some(false);
        }
        match decision {
            Some(true) => {
                self.pending_confirm = None;
                self.run_confirm(c);
            }
            Some(false) => self.pending_confirm = None,
            None => {}
        }
    }

    fn draw_toast(&mut self, ui: &mut egui::Ui) {
        let Some(t) = &self.toast else { return };
        let fill = match t.level {
            ToastLevel::Info => {
                if self.pal.dark {
                    Color32::from_rgb(52, 58, 84)
                } else {
                    Color32::from_rgb(30, 36, 56)
                }
            }
            ToastLevel::Success => Color32::from_rgb(4, 120, 87),
            ToastLevel::Error => Color32::from_rgb(185, 28, 28),
        };
        let text = t.text.clone();
        let resp = egui::Area::new(egui::Id::unique("toast"))
            .anchor(Align2::CENTER_BOTTOM, [0.0, -48.0])
            .order(egui::Order::Foreground)
            .interactable(true)
            .sense(Sense::click())
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(fill)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(16, 9))
                    .show(ui, |ui| {
                        ui.label(RichText::new(text).color(Color32::WHITE));
                    });
            });
        if resp.response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.response.clicked() {
            self.toast = None;
        }
    }
}

pub fn outline(ui: &egui::Ui, rect: egui::Rect, color: Color32, width: f32) {
    ui.painter()
        .rect_stroke(rect, CornerRadius::same(12), Stroke::new(width, color), StrokeKind::Inside);
}

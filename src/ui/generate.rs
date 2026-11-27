use eframe::egui::{self, DragValue, Response, RichText, TextEdit};

use std::net::Ipv6Addr;

use super::{badge, card, hint, plain_button, primary, title};
use crate::app::App;
use crate::net::generator::{self, Mode, MAX_COUNT};
use crate::net::interfaces::V6Entry;
use crate::net::{network_of, platform};
use crate::theme;

fn enter_pressed(ui: &egui::Ui, resp: &Response) -> bool {
    resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

struct PrefixGroup {
    network: Ipv6Addr,
    prefix_len: u8,
    count: usize,
    global: bool,
    public: bool,
    temporary: usize,
    tentative: usize,
}

fn v6_prefix_groups(entries: &[V6Entry], public: Option<&crate::net::public::PublicInfo>) -> Vec<PrefixGroup> {
    let mut groups: Vec<PrefixGroup> = Vec::new();
    for e in entries {
        let len = if e.prefix_len == 0 || e.prefix_len >= 128 { 64 } else { e.prefix_len };
        let network = network_of(e.addr, len);
        let idx = match groups.iter().position(|g| g.network == network && g.prefix_len == len) {
            Some(i) => i,
            None => {
                groups.push(PrefixGroup {
                    network,
                    prefix_len: len,
                    count: 0,
                    global: e.is_global(),
                    public: false,
                    temporary: 0,
                    tentative: 0,
                });
                groups.len() - 1
            }
        };
        let g = &mut groups[idx];
        g.count += 1;
        g.public |= public.is_some_and(|i| i.matches(&e.addr));
        g.temporary += e.temporary as usize;
        g.tentative += e.tentative as usize;
    }
    groups
}

impl App {
    pub fn generate_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            let p = self.pal;
            ui.label(
                RichText::new("1 Chọn card → 2 Tạo → 3 Áp dụng → 4 Chạy proxy → 5 Kiểm tra → 6 Xuất")
                    .small()
                    .color(p.weak),
            );
            self.adapter_detail(ui);
            self.generate_form(ui);
        });
    }

    fn adapter_detail(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let Some(a) = self.adapter().cloned() else {
            card(ui, &p, |ui| {
                title(ui, &p, "Chưa chọn card mạng");
                hint(ui, &p, "Chọn một card mạng ở thanh bên trái để bắt đầu.");
            });
            return;
        };
        let public_info = self.public_v6.clone();
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                title(ui, &p, a.display_name());
                badge(ui, a.kind.label(), p.info);
                if a.up {
                    badge(ui, "Bật", p.success);
                } else {
                    badge(ui, "Tắt", p.weak);
                }
                if a.has_global() {
                    badge(ui, "IPv6 global", p.accent);
                }
            });
            if !a.description.is_empty() {
                hint(ui, &p, &a.description);
            }
            ui.add_space(4.0);
            egui::Grid::new("adapter-grid").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                ui.label(RichText::new("Tên hệ thống").color(p.weak));
                ui.label(RichText::new(&a.name).monospace());
                ui.end_row();
                ui.label(RichText::new("Chỉ số (index)").color(p.weak));
                ui.label(RichText::new(a.index.to_string()).monospace());
                ui.end_row();
                ui.label(RichText::new("MAC").color(p.weak));
                ui.label(RichText::new(a.mac.clone().unwrap_or_else(|| "không rõ".into())).monospace());
                ui.end_row();
                ui.label(RichText::new("IPv4").color(p.weak));
                if a.ipv4.is_empty() {
                    ui.label(RichText::new("không có").color(p.weak));
                } else {
                    ui.vertical(|ui| {
                        for (ip, len) in &a.ipv4 {
                            ui.label(RichText::new(format!("{ip}/{len}")).monospace());
                        }
                    });
                }
                ui.end_row();
                ui.label(RichText::new("IPv6").color(p.weak));
                if a.ipv6.is_empty() {
                    ui.label(RichText::new("không có").color(p.weak));
                } else {
                    let groups = v6_prefix_groups(&a.ipv6, public_info.as_ref());
                    ui.vertical(|ui| {
                        for g in &groups {
                            ui.horizontal(|ui| {
                                let text = format!("{}/{}", g.network, g.prefix_len);
                                let color = if g.global { p.accent } else { p.text };
                                ui.label(RichText::new(text).monospace().color(color));
                                hint(ui, &p, &format!("{} địa chỉ", g.count));
                                if g.global {
                                    badge(ui, "global", p.accent);
                                }
                                if g.public {
                                    badge(ui, "public", p.success);
                                }
                                if g.temporary > 0 {
                                    badge(ui, &format!("tạm thời {}", g.temporary), p.warn);
                                }
                                if g.tentative > 0 {
                                    badge(ui, &format!("chờ xác nhận {}", g.tentative), p.weak);
                                }
                            });
                        }
                    });
                }
                ui.end_row();
            });
        });
    }

    fn generate_form(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let busy = self.busy();
        card(ui, &p, |ui| {
            title(ui, &p, "Tham số tạo địa chỉ");
            ui.add_space(2.0);

            let elig = self.eligibility();
            let mut enter_generate = false;
            let detected = self.adapter().and_then(|a| a.detected_prefix());
            let detecting = self.detecting_public;

            ui.label(RichText::new("Prefix").color(p.weak));
            ui.horizontal_wrapped(|ui| {
                if self.custom_prefix {
                    let resp = ui.add(
                        TextEdit::singleline(&mut self.prefix_text)
                            .hint_text("2001:db8:1:2::/64")
                            .font(egui::TextStyle::Monospace)
                            .desired_width(320.0),
                    );
                    if resp.changed() {
                        self.settings_dirty = true;
                    }
                    if enter_pressed(ui, &resp) {
                        enter_generate = true;
                    }
                } else {
                    let text = detected
                        .as_ref()
                        .map(|d| format!("{}/{}", d.network, d.prefix_len))
                        .unwrap_or_else(|| "chưa có".to_string());
                    let color = if detected.is_some() { p.accent } else { p.weak };
                    ui.label(RichText::new(text).monospace().color(color));
                    if detected.is_some() {
                        badge(ui, "tự động", p.info);
                    }
                }
                if ui.add_enabled(!detecting, plain_button("Tự phát hiện IPv6 public")).clicked() {
                    self.detect_public(true, true);
                }
                if detecting {
                    ui.spinner();
                }
            });
            if ui
                .checkbox(&mut self.custom_prefix, "Tùy chỉnh prefix (nâng cao, phải nằm trong prefix router cấp)")
                .changed()
            {
                self.settings_dirty = true;
            }
            if self.custom_prefix && !self.prefix_text.trim().is_empty() {
                if let Err(e) = generator::parse_prefix(&self.prefix_text) {
                    ui.label(RichText::new(e.to_string()).small().color(p.error));
                }
            }
            if let Some(info) = &self.public_v6 {
                let card_of = |addr| {
                    self.adapter_owning(addr)
                        .map(|(a, _)| a.display_name().to_string())
                        .unwrap_or_else(|| "không thuộc card nào".to_string())
                };
                match (&info.public, info.route) {
                    (Ok(addr), _) => {
                        ui.label(
                            RichText::new(format!("IPv6 public: {addr} (card mạng: {})", card_of(*addr))).color(p.accent),
                        );
                    }
                    (Err(e), Some(route)) => {
                        ui.label(
                            RichText::new(format!("IPv6 nguồn route: {route} (card mạng: {}) · {e}", card_of(route)))
                                .color(p.warn),
                        );
                    }
                    (Err(e), None) => {
                        ui.label(RichText::new(e.as_str()).color(p.warn));
                    }
                }
            }

            ui.add_space(6.0);
            ui.label(RichText::new("Số lượng").color(p.weak));
            ui.horizontal(|ui| {
                let resp = ui.add(DragValue::new(&mut self.settings.count).range(1..=MAX_COUNT).speed(1.0));
                if resp.changed() {
                    self.settings_dirty = true;
                }
                if enter_pressed(ui, &resp) {
                    enter_generate = true;
                }
                for preset in [10usize, 100, 500, 1000, 5000, 10000] {
                    if ui.add(plain_button(&preset.to_string())).clicked() {
                        self.settings.count = preset;
                        self.settings_dirty = true;
                    }
                }
            });

            ui.add_space(6.0);
            ui.label(RichText::new("Chế độ").color(p.weak));
            ui.horizontal(|ui| {
                let before = self.mode;
                ui.selectable_value(&mut self.mode, Mode::Random, "Ngẫu nhiên (Interface ID)");
                ui.selectable_value(&mut self.mode, Mode::Sequential, "Tuần tự từ offset");
                if before != self.mode {
                    self.settings_dirty = true;
                }
            });
            if self.mode == Mode::Sequential {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Offset bắt đầu").color(p.weak));
                    let resp = ui.add(
                        TextEdit::singleline(&mut self.offset_text)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(160.0),
                    );
                    if resp.changed() {
                        self.settings_dirty = true;
                    }
                    if enter_pressed(ui, &resp) {
                        enter_generate = true;
                    }
                    hint(ui, &p, "thập phân hoặc 0x...; host 0 và 1 luôn bị bỏ qua");
                });
            }

            ui.add_space(6.0);
            ui.label(RichText::new("Tùy chọn áp dụng").color(p.weak));
            ui.horizontal_wrapped(|ui| {
                if platform::supports_persistent() {
                    let before = self.settings.persistent;
                    ui.checkbox(&mut self.settings.persistent, "Lưu bền vững (persistent, giữ sau khi khởi động lại)");
                    if before != self.settings.persistent {
                        self.settings_dirty = true;
                    }
                }
                if platform::supports_nodad() {
                    let before = self.settings.nodad;
                    ui.checkbox(&mut self.settings.nodad, "nodad (bỏ qua kiểm tra trùng địa chỉ)");
                    if before != self.settings.nodad {
                        self.settings_dirty = true;
                    }
                }
                if !platform::supports_persistent() && !platform::supports_nodad() {
                    hint(ui, &p, "Địa chỉ được thêm tạm thời (mất khi khởi động lại).");
                }
            });

            ui.add_space(6.0);
            for msg in &elig.blocked {
                ui.label(RichText::new(msg.as_str()).font(theme::semibold(14.0)).color(p.error));
            }
            for msg in &elig.warnings {
                ui.label(RichText::new(msg.as_str()).color(p.warn));
            }
            if elig.allowed() {
                ui.label(RichText::new("Đủ điều kiện tạo địa chỉ IPv6 trên card này").color(p.success));
            }

            if let Some(err) = &self.form_error {
                if elig.allowed() || elig.blocked.join("
") != *err {
                    ui.add_space(4.0);
                    ui.label(RichText::new(err.as_str()).font(theme::semibold(14.0)).color(p.error));
                }
            }

            ui.add_space(8.0);
            let allowed = elig.allowed();
            let can_apply = allowed && !busy && self.can_apply();
            let gen_reason = if busy {
                "Đang có tác vụ chạy, vui lòng chờ"
            } else {
                "Chưa đủ điều kiện, xem thông báo phía trên"
            };
            let apply_reason = if busy {
                "Đang có tác vụ chạy, vui lòng chờ".to_string()
            } else if !allowed {
                "Chưa đủ điều kiện, xem thông báo phía trên".to_string()
            } else if !self.elevated {
                platform::elevation_hint()
            } else {
                self.tools_error.clone().unwrap_or_default()
            };
            let mut do_generate: Option<bool> = None;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(allowed && !busy, plain_button("Tạo danh sách"))
                    .on_disabled_hover_text(gen_reason)
                    .clicked()
                {
                    do_generate = Some(false);
                }
                if primary(ui, &p, can_apply, "Tạo và áp dụng vào card")
                    .on_disabled_hover_text(apply_reason)
                    .clicked()
                {
                    do_generate = Some(true);
                }
            });
            if do_generate.is_none() && enter_generate && allowed && !busy {
                do_generate = Some(false);
            }
            if let Some(then_apply) = do_generate {
                self.generate(then_apply);
            }
        });
    }
}

use eframe::egui::{self, Align, CornerRadius, CursorIcon, Frame, Layout, Margin, RichText, Sense, Stroke};

use super::{badge, dot_label, outline};
use crate::app::App;
use crate::net::interfaces::{Adapter, AdapterKind};

impl App {
    pub fn sidebar(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Card mạng").size(17.0).strong().color(p.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Làm mới").clicked() {
                    self.refresh_adapters();
                    self.toast("Đã làm mới danh sách card mạng");
                }
            });
        });
        if ui.checkbox(&mut self.only_global, "Chỉ hiện card có IPv6 global").changed() {
            self.settings_dirty = true;
        }
        ui.add_space(4.0);

        let only_global = self.only_global;
        let selected = self.selected_adapter.clone();
        let mut clicked: Option<String> = None;
        let list: Vec<Adapter> = self
            .adapters
            .iter()
            .filter(|a| !only_global || a.has_global())
            .cloned()
            .collect();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            if list.is_empty() {
                ui.label(RichText::new("Không tìm thấy card mạng phù hợp").color(p.weak));
            }
            for a in &list {
                let is_sel = selected.as_deref() == Some(a.name.as_str());
                let has_global = a.has_global();
                let fill = if is_sel { p.accent_soft } else { p.card };
                let resp = Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(a.display_name()).strong().size(14.5).color(p.text));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let color = match a.kind {
                                    AdapterKind::Wifi => p.info,
                                    AdapterKind::Wired => p.success,
                                    AdapterKind::Virtual => p.weak,
                                    AdapterKind::Other => p.weak,
                                };
                                badge(ui, a.kind.label(), color);
                            });
                        });
                        if !a.description.is_empty() {
                            ui.add(egui::Label::new(RichText::new(&a.description).small().color(p.weak)).truncate());
                        }
                        ui.horizontal(|ui| {
                            if a.up {
                                dot_label(ui, p.success, RichText::new("Đang kết nối").small());
                            } else {
                                dot_label(ui, p.weak, RichText::new("Ngắt").small());
                            }
                            if let Some(mac) = &a.mac {
                                ui.label(RichText::new(mac).small().monospace().color(p.weak));
                            }
                        });
                        if let Some((ip, len)) = a.ipv4.first() {
                            ui.label(RichText::new(format!("IPv4: {ip}/{len}")).small().color(p.text));
                        }
                        match a.detected_prefix() {
                            Some(d) if has_global => {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("IPv6 global: {}/{}", d.network, d.prefix_len))
                                            .small()
                                            .strong()
                                            .color(p.accent),
                                    )
                                    .truncate(),
                                );
                            }
                            _ => {
                                ui.label(RichText::new("Không có IPv6 global").small().color(p.weak));
                            }
                        }
                    });
                let rect = resp.response.rect;
                let click = ui.interact(rect, ui.scope_id().with(("adapter-card", &a.name)), Sense::click());
                if click.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    if !is_sel {
                        outline(ui, rect, p.accent.gamma_multiply(0.6), 1.0);
                    }
                }
                if is_sel {
                    outline(ui, rect, p.accent, 2.0);
                } else if has_global {
                    outline(ui, rect, p.success.gamma_multiply(0.55), 1.0);
                }
                if click.clicked() {
                    clicked = Some(a.name.clone());
                }
            }
        });

        if let Some(name) = clicked {
            if selected.as_deref() != Some(name.as_str()) {
                self.user_chose_adapter = true;
                self.select_adapter(Some(name));
            }
        }
    }
}

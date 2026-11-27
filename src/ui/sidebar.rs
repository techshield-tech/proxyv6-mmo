use eframe::egui::{self, Align, CornerRadius, CursorIcon, Frame, Layout, Margin, RichText, Sense, Stroke};

use super::{badge, dot_label, outline};
use crate::app::App;
use crate::net::interfaces::{Adapter, AdapterKind};
use crate::theme;

impl App {
    pub fn sidebar(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Card mạng").font(theme::semibold(16.0)).color(p.text));
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
                let fill = if is_sel { p.card_hover } else { p.card };
                let resp = Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin {
                        left: 14,
                        right: 12,
                        top: 12,
                        bottom: 12,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(a.display_name()).font(theme::semibold(14.0)).color(p.text));
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
                                            .monospace()
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
                let strip_color = if is_sel {
                    p.accent_fill
                } else if has_global {
                    p.success
                } else {
                    p.border
                };
                let strip = egui::Rect::from_min_max(
                    rect.left_top() + egui::vec2(1.0, 1.0),
                    egui::pos2(rect.left() + 4.0, rect.bottom() - 1.0),
                );
                ui.painter().rect_filled(
                    strip,
                    CornerRadius {
                        nw: 5,
                        sw: 5,
                        ne: 0,
                        se: 0,
                    },
                    strip_color,
                );
                if click.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    if !is_sel {
                        ui.painter().rect_filled(rect, CornerRadius::same(6), p.accent_soft);
                        outline(ui, rect, p.accent.gamma_multiply(0.5), 1.0);
                    }
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

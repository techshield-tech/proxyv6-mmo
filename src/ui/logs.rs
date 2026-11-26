use eframe::egui::{self, Align, Label, Layout, RichText};

use super::{card, plain_button};
use crate::app::{App, LogLevel};

impl App {
    pub fn logs_tab(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        ui.spacing_mut().item_spacing.y = 12.0;
        card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} dòng nhật ký", self.logs.len())).color(p.weak));
                ui.checkbox(&mut self.logs_errors_only, "Chỉ hiện lỗi");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(plain_button("Xóa log")).clicked() {
                        self.logs.clear();
                    }
                    if ui.add(plain_button("Sao chép log")).clicked() {
                        let text = self
                            .logs
                            .iter()
                            .map(|l| format!("[{}] {}", l.time, l.text))
                            .collect::<Vec<_>>()
                            .join("\n");
                        let ok = self.copy_text(text);
                        if ok {
                            self.toast_ok("Đã sao chép log");
                        } else {
                            self.toast_err("Không truy cập được clipboard");
                        }
                    }
                });
            });
        });

        egui::Frame::new()
            .fill(p.card)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 6.0;
                let logs = &self.logs;
                let errors: Option<Vec<usize>> = self.logs_errors_only.then(|| {
                    logs.iter()
                        .enumerate()
                        .filter(|(_, l)| l.level == LogLevel::Error)
                        .map(|(i, _)| i)
                        .collect()
                });
                let count = errors.as_ref().map_or(logs.len(), Vec::len);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show_rows(ui, row_h, count, |ui, range| {
                        for k in range {
                            let l = match &errors {
                                Some(v) => &logs[v[k]],
                                None => &logs[k],
                            };
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                ui.label(RichText::new(&l.time).monospace().small().color(p.weak));
                                let (tag, color) = match l.level {
                                    LogLevel::Info => ("INFO", p.info),
                                    LogLevel::Ok => ("OK  ", p.success),
                                    LogLevel::Warn => ("WARN", p.warn),
                                    LogLevel::Error => ("ERR ", p.error),
                                };
                                ui.label(RichText::new(tag).monospace().small().strong().color(color));
                                ui.add(Label::new(RichText::new(&l.text).color(p.text)).truncate())
                                    .on_hover_text(l.text.as_str());
                            });
                        }
                    });
            });
    }
}

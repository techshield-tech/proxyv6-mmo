use eframe::egui::{self, RichText, Slider, TextEdit};

use super::{card, hint, title};
use crate::app::App;
use crate::net::platform;
use crate::storage;
use crate::tester;

impl App {
    pub fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;

            card(ui, &p, |ui| {
                title(ui, &p, "Kiểm tra proxy");
                hint(
                    ui,
                    &p,
                    "Mỗi proxy được test bằng cách gửi yêu cầu HTTP tới URL kiểm tra IP bên dưới qua chính proxy đó; phản hồi phải là IP ra Internet.",
                );
                ui.label(RichText::new("URL kiểm tra IP (chỉ http, nên dùng dịch vụ chỉ có IPv6)").color(p.weak));
                if ui
                    .add(
                        TextEdit::singleline(&mut self.settings.ip_check_url)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(360.0),
                    )
                    .changed()
                {
                    self.settings_dirty = true;
                }
                if let Err(e) = tester::parse_check_url(&self.settings.ip_check_url) {
                    ui.label(RichText::new(e).small().color(p.error));
                }
                ui.horizontal(|ui| {
                    for (label, url) in [
                        ("ipify", "http://api6.ipify.org/"),
                        ("icanhazip", "http://ipv6.icanhazip.com/"),
                    ] {
                        if ui.button(label).clicked() {
                            self.settings.ip_check_url = url.to_string();
                            self.settings_dirty = true;
                        }
                    }
                });
                ui.label(RichText::new("Số luồng song song").color(p.weak));
                if ui.add(Slider::new(&mut self.settings.test_threads, 1..=64)).changed() {
                    self.settings_dirty = true;
                }
                ui.label(RichText::new("Timeout (giây)").color(p.weak));
                if ui.add(Slider::new(&mut self.settings.test_timeout_secs, 1..=30)).changed() {
                    self.settings_dirty = true;
                }
            });

            card(ui, &p, |ui| {
                title(ui, &p, "Hệ thống");
                egui::Grid::new("sys-grid").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new("Hệ điều hành").color(p.weak));
                    ui.label(std::env::consts::OS);
                    ui.end_row();
                    ui.label(RichText::new("Quyền").color(p.weak));
                    ui.label(if self.elevated { "Admin/Root" } else { "Thường (thiếu quyền)" });
                    ui.end_row();
                    ui.label(RichText::new("Công cụ").color(p.weak));
                    ui.label(match &self.tools_error {
                        None => "Sẵn sàng".to_string(),
                        Some(e) => e.clone(),
                    });
                    ui.end_row();
                    ui.label(RichText::new("Font").color(p.weak));
                    ui.label(self.font_path.unwrap_or("mặc định của egui"));
                    ui.end_row();
                    ui.label(RichText::new("File trạng thái").color(p.weak));
                    ui.label(RichText::new(storage::path_display()).monospace());
                    ui.end_row();
                });
                hint(ui, &p, "Tự động lưu: cài đặt được ghi vào file trạng thái ngay sau khi bạn thay đổi.");
                if !self.elevated {
                    ui.label(RichText::new(platform::elevation_hint()).color(p.warn));
                }
            });
        });
    }
}

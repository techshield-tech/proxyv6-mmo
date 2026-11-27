use eframe::egui::{self, RichText, TextEdit};

use super::{card, hint, title};
use crate::app::App;
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
            });
        });
    }
}

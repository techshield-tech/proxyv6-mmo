use std::net::IpAddr;
use std::sync::atomic::Ordering;

use eframe::egui::{
    self, Align, Color32, CursorIcon, DragValue, Key, Label, Layout, Modifiers, RichText, Sense, TextEdit,
};
use egui_extras::{Column, TableBuilder};

use super::{card_pad, danger, dot_label, plain_button, primary};
use crate::app::{App, Confirm, ExportScope, LogLevel, OpKind, ResultAction, SortKey, Status, Tab, TestState};
use crate::proxy;

fn ellipsize_left(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1);
    let tail: String = text.chars().skip(n - keep).collect();
    format!("…{tail}")
}

fn progress_row(ui: &mut egui::Ui, frac: f32, color: Color32, text: &str, cancelling: bool) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::ProgressBar::new(frac)
                .desired_width((ui.available_width() - 110.0).max(100.0))
                .fill(color)
                .text(text),
        );
        let label = if cancelling { "Đang dừng…" } else { "Dừng" };
        if ui.add_enabled(!cancelling, plain_button(label)).clicked() {
            clicked = true;
        }
    });
    clicked
}

impl App {
    pub fn proxy_tab(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        ui.spacing_mut().item_spacing.y = 10.0;

        card_pad(ui, &p, 12, |ui| {
            self.address_card(ui);
        });

        card_pad(ui, &p, 12, |ui| {
            self.proxy_card(ui);
        });

        card_pad(ui, &p, 12, |ui| {
            self.test_card(ui);
        });

        card_pad(ui, &p, 12, |ui| {
            self.export_card(ui);
        });

        self.result_line(ui);
        self.progress_rows(ui);
        self.table(ui);
    }

    fn address_card(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let c = self.counts;

        ui.horizontal_wrapped(|ui| {
            let resp = ui.add(
                TextEdit::singleline(&mut self.filter)
                    .hint_text("Tìm địa chỉ, trạng thái, sống/chết...")
                    .desired_width(260.0),
            );
            if resp.changed() {
                self.visible_dirty = true;
            }
            if !self.filter.is_empty() && ui.add(plain_button("Xóa lọc")).clicked() {
                self.filter.clear();
                self.visible_dirty = true;
            }
            ui.label(
                RichText::new(format!(
                    "Hiển thị {}/{} · Chọn {} · Đã thêm {}",
                    self.visible.len(),
                    self.rows.len(),
                    c.selected,
                    c.added
                ))
                .color(p.weak),
            );
        });

        let block = self.apply_block_reason();
        let busy = self.busy();
        let apply_n = c.apply_n();
        ui.horizontal_wrapped(|ui| {
            let apply_label = if c.selected_eligible > 0 {
                format!("Áp dụng đã chọn ({apply_n})")
            } else {
                format!("Áp dụng tất cả chờ ({apply_n})")
            };
            let apply_off = block
                .clone()
                .or_else(|| (apply_n == 0).then(|| "Không có địa chỉ nào để áp dụng".to_string()));
            if primary(ui, &p, apply_off.is_none(), &apply_label)
                .on_disabled_hover_text(apply_off.unwrap_or_default())
                .clicked()
            {
                let t = self.apply_targets();
                self.start_apply(t);
            }

            let sel_off = block
                .clone()
                .or_else(|| (c.selected_added == 0).then(|| "Chưa chọn địa chỉ đã thêm nào".to_string()));
            if ui
                .add_enabled(
                    sel_off.is_none(),
                    plain_button(&format!("Gỡ đã chọn ({})", c.selected_added)),
                )
                .on_disabled_hover_text(sel_off.unwrap_or_default())
                .clicked()
            {
                self.pending_confirm = Some(Confirm::RemoveSelected);
            }

            let all_off = block
                .clone()
                .or_else(|| (c.added == 0).then(|| "Chưa có địa chỉ nào ở trạng thái Đã thêm".to_string()));
            if danger(ui, &p, all_off.is_none(), &format!("Gỡ tất cả đã thêm ({})", c.added))
                .on_disabled_hover_text(all_off.unwrap_or_default())
                .clicked()
            {
                self.pending_confirm = Some(Confirm::RemoveAll);
            }

            ui.separator();
            if ui
                .add_enabled(!busy && !self.rows.is_empty(), plain_button("Xóa khỏi danh sách"))
                .on_hover_text("Chỉ xóa dòng khỏi danh sách, không ảnh hưởng card mạng")
                .clicked()
            {
                self.clear_non_added();
            }
        });
    }

    fn proxy_card(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let running = self.proxy.is_some();
        let stopping = self.stopping_proxy;
        let count = self.counts.added;

        let stale = self.proxy_stale();
        let mut start = false;
        let mut confirm_stop = false;
        ui.horizontal_wrapped(|ui| {
            if running {
                if danger(ui, &p, true, "Dừng proxy").clicked() {
                    confirm_stop = true;
                }
            } else if stopping {
                ui.add_enabled(false, plain_button("Chạy proxy"));
                ui.spinner();
                ui.label(RichText::new("Đang dừng proxy…").strong().color(p.warn));
                return;
            } else if primary(ui, &p, count > 0, "Chạy proxy")
                .on_disabled_hover_text("Cần ít nhất một địa chỉ ở trạng thái Đã thêm")
                .clicked()
            {
                start = true;
            }
            let Some(h) = &self.proxy else {
                dot_label(ui, p.weak, RichText::new("Proxy đang tắt"));
                return;
            };
            let bound = h.stats.bound.load(Ordering::Relaxed);
            let failed = h.stats.failed.load(Ordering::Relaxed);
            let active = h.stats.active.load(Ordering::Relaxed);
            let total = h.stats.total.load(Ordering::Relaxed);
            let planned = h.addrs.len();
            let last = h.start_port as usize + planned.saturating_sub(1);
            if h.is_ready() {
                let color = if failed == 0 { p.success } else { p.warn };
                dot_label(ui, color, RichText::new("Đang chạy").strong());
            } else {
                ui.spinner();
                ui.label(RichText::new("Đang mở cổng...").strong());
            }
            ui.label(RichText::new(format!("{}:{}-{}", h.bind, h.start_port, last)).monospace());
            ui.label(RichText::new(format!("Mở {bound}/{planned}")).color(p.weak));
            if failed > 0 {
                ui.label(RichText::new(format!("lỗi {failed}")).color(p.warn));
            }
            ui.label(RichText::new(format!("Đang kết nối {active}")).color(p.weak));
            ui.label(RichText::new(format!("Tổng {total}")).color(p.weak));
        });
        if stale {
            ui.label(
                RichText::new("Danh sách địa chỉ đã đổi so với lúc chạy proxy, hãy dừng và chạy lại.")
                    .small()
                    .color(p.warn),
            );
        }
        if let Some(err) = &self.proxy_error {
            ui.label(RichText::new(err.as_str()).color(p.error).strong());
        }

        let lan: Vec<String> = self
            .adapter()
            .map(|a| a.ipv4.iter().map(|(ip, _)| ip.to_string()).collect())
            .unwrap_or_default();
        egui::CollapsingHeader::new(RichText::new("Cấu hình proxy").strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.add_enabled_ui(!running && !stopping, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("IPv4 lắng nghe").color(p.weak));
                        let mut options = vec!["127.0.0.1".to_string(), "0.0.0.0".to_string()];
                        for ip in lan {
                            if !options.contains(&ip) {
                                options.push(ip);
                            }
                        }
                        egui::ComboBox::from_id_salt("proxy-bind")
                            .selected_text(self.settings.proxy_bind.clone())
                            .show_ui(ui, |ui| {
                                for o in &options {
                                    if ui
                                        .selectable_value(&mut self.settings.proxy_bind, o.clone(), o.as_str())
                                        .clicked()
                                    {
                                        self.settings_dirty = true;
                                    }
                                }
                            });
                        ui.label(RichText::new("Cổng đầu").color(p.weak));
                        if ui
                            .add(DragValue::new(&mut self.settings.proxy_start_port).range(1..=65535).speed(1.0))
                            .changed()
                        {
                            self.settings_dirty = true;
                        }
                        if let Err(e) = proxy::port_range(self.settings.proxy_start_port, count.max(1)) {
                            ui.label(RichText::new(e).small().color(p.warn));
                        }
                        if ui.checkbox(&mut self.settings.proxy_auth, "Xác thực").changed() {
                            self.settings_dirty = true;
                        }
                        if self.settings.proxy_auth {
                            ui.label(RichText::new("Tên đăng nhập").color(p.weak));
                            if ui
                                .add(TextEdit::singleline(&mut self.settings.proxy_user).desired_width(110.0))
                                .changed()
                            {
                                self.settings_dirty = true;
                            }
                            ui.label(RichText::new("Mật khẩu").color(p.weak));
                            if ui
                                .add(
                                    TextEdit::singleline(&mut self.settings.proxy_pass)
                                        .password(true)
                                        .desired_width(110.0),
                                )
                                .changed()
                            {
                                self.settings_dirty = true;
                            }
                        }
                    });
                });
                if self.settings.proxy_auth {
                    ui.label(
                        RichText::new("Mật khẩu được lưu dạng văn bản thường (không mã hóa) trong file state.json.")
                            .small()
                            .color(p.weak),
                    );
                }
                if !self.settings.proxy_auth && self.settings.proxy_bind.trim() != "127.0.0.1" {
                    ui.label(
                        RichText::new(
                            "Địa chỉ lắng nghe khác 127.0.0.1 mà không bật xác thực: bất kỳ máy nào trong mạng cũng dùng được proxy này.",
                        )
                        .small()
                        .color(p.warn),
                    );
                }
            });

        if start {
            self.start_proxy();
        }
        if confirm_stop {
            self.pending_confirm = Some(Confirm::StopProxy);
        }
    }

    fn test_card(&mut self, ui: &mut egui::Ui) {
        let c = self.counts;
        let testing = self.test.is_some();
        let n = c.test_n();
        ui.horizontal_wrapped(|ui| {
            let off = if testing {
                Some("Đang kiểm tra, vui lòng chờ")
            } else if self.stopping_proxy {
                Some("Proxy đang dừng, vui lòng chờ")
            } else if c.added == 0 {
                Some("Cần ít nhất một địa chỉ ở trạng thái Đã thêm")
            } else {
                None
            };
            if ui
                .add_enabled(off.is_none(), plain_button(&format!("Kiểm tra {n} proxy")))
                .on_hover_text("Gửi yêu cầu qua từng proxy tới URL kiểm tra IP (xem Cài đặt)")
                .on_disabled_hover_text(off.unwrap_or_default())
                .clicked()
            {
                let t = self.test_targets();
                self.start_test(t);
            }
            let retest_off = if testing {
                Some("Đang kiểm tra, vui lòng chờ")
            } else if self.stopping_proxy {
                Some("Proxy đang dừng, vui lòng chờ")
            } else if c.test_failed == 0 {
                Some("Không có proxy nào đang ở trạng thái Chết")
            } else {
                None
            };
            if ui
                .add_enabled(
                    retest_off.is_none(),
                    plain_button(&format!("Kiểm tra lại proxy lỗi ({})", c.test_failed)),
                )
                .on_disabled_hover_text(retest_off.unwrap_or_default())
                .clicked()
            {
                let t = self.retest_targets();
                self.start_test(t);
            }
        });
    }

    fn export_card(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        egui::CollapsingHeader::new(RichText::new("Xuất danh sách").strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Định dạng").color(p.weak));
                    let formats = ["Chỉ IP", "[IP]:cổng", "IP/prefix", "ip:cổng (proxy)", "ip:cổng:user:pass (proxy)"];
                    egui::ComboBox::from_id_salt("export-format")
                        .selected_text(formats[self.settings.export_format.min(4) as usize])
                        .show_ui(ui, |ui| {
                            for (i, f) in formats.iter().enumerate() {
                                if ui.selectable_value(&mut self.settings.export_format, i as u8, *f).clicked() {
                                    self.settings_dirty = true;
                                }
                            }
                        });
                    if self.settings.export_format == 1 {
                        ui.label(RichText::new("Cổng").color(p.weak));
                        if ui
                            .add(TextEdit::singleline(&mut self.settings.export_port).desired_width(60.0))
                            .changed()
                        {
                            self.settings_dirty = true;
                        }
                    }
                    ui.label(RichText::new("Phạm vi").color(p.weak));
                    let before = self.export_scope;
                    egui::ComboBox::from_id_salt("export-scope")
                        .selected_text(scope_label(self.export_scope))
                        .show_ui(ui, |ui| {
                            for s in [ExportScope::All, ExportScope::Selected, ExportScope::Added] {
                                ui.selectable_value(&mut self.export_scope, s, scope_label(s));
                            }
                        });
                    if before != self.export_scope {
                        self.settings_dirty = true;
                    }
                });
                let n = self.export_count();
                let err = self.export_valid().err();
                let off = err.clone().or_else(|| (n == 0).then(|| "Không có địa chỉ để xuất".to_string()));
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(off.is_none(), plain_button(&format!("Sao chép {n} địa chỉ")))
                        .on_disabled_hover_text(off.clone().unwrap_or_default())
                        .clicked()
                    {
                        self.copy_export();
                    }
                    let save_off = if self.exporting {
                        Some("Đang mở hộp thoại lưu file".to_string())
                    } else {
                        off.clone()
                    };
                    if ui
                        .add_enabled(save_off.is_none(), plain_button(&format!("Lưu {n} địa chỉ ra .txt")))
                        .on_disabled_hover_text(save_off.unwrap_or_default())
                        .clicked()
                    {
                        self.save_export();
                    }
                    if self.exporting {
                        ui.spinner();
                    }
                });
                if let Some(e) = err {
                    ui.label(RichText::new(e).small().color(p.error));
                }
            });
    }

    fn result_line(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let Some(r) = &self.last_result else { return };
        let color = match r.level {
            LogLevel::Info => p.info,
            LogLevel::Ok => p.success,
            LogLevel::Warn => p.warn,
            LogLevel::Error => p.error,
        };
        let text = r.text.clone();
        let action = r.action;
        let mut act = false;
        let mut dismiss = false;
        ui.horizontal_wrapped(|ui| {
            dot_label(ui, color, RichText::new(text).strong());
            if action.is_some() && ui.add(plain_button("Xem lỗi")).clicked() {
                act = true;
            }
            if ui.add(plain_button("Ẩn")).clicked() {
                dismiss = true;
            }
        });
        if act {
            match action {
                Some(ResultAction::FilterRows(label)) => {
                    self.filter = label.to_string();
                    self.visible_dirty = true;
                }
                Some(ResultAction::OpenLogs) => {
                    self.logs_errors_only = true;
                    self.tab = Tab::Logs;
                }
                None => {}
            }
        }
        if dismiss {
            self.last_result = None;
        }
    }

    fn progress_rows(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let mut cancel_op = false;
        let mut cancel_test = false;
        if let Some(op) = &self.op {
            let frac = if op.total == 0 { 0.0 } else { op.done as f32 / op.total as f32 };
            let text = format!(
                "{} {}/{} (thành công {}, thất bại {})",
                if op.kind == OpKind::Apply { "Đang áp dụng" } else { "Đang gỡ" },
                op.done,
                op.total,
                op.ok,
                op.fail
            );
            cancel_op = progress_row(ui, frac, p.accent_fill, &text, op.cancelling);
        }
        if let Some(t) = &self.test {
            let frac = if t.total == 0 { 0.0 } else { t.done as f32 / t.total as f32 };
            let text = format!("Kiểm tra proxy {}/{} (sống {})", t.done, t.total, t.ok);
            cancel_test = progress_row(ui, frac, p.info, &text, t.cancelling);
        }
        if cancel_op {
            self.cancel_op();
        }
        if cancel_test {
            self.cancel_test();
        }
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        if self.rows.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Chưa có địa chỉ nào").size(17.0).strong().color(p.weak));
                ui.label(RichText::new("Vào tab \"Tạo địa chỉ\" để sinh danh sách IPv6 mới.").color(p.weak));
            });
            return;
        }

        if !ui.ctx().egui_wants_keyboard_input() && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
            for &i in &self.visible {
                self.rows[i].selected = true;
            }
        }

        let all_selected = !self.visible.is_empty() && self.visible.iter().all(|&i| self.rows[i].selected);
        let mut select_all = all_selected;
        let mut copied: Option<String> = None;
        let mut row_click: Option<usize> = None;
        let mut sort_click: Option<SortKey> = None;
        let shift = ui.input(|i| i.modifiers.shift);
        let host = self.proxy_export_host();
        let sort = self.sort;
        let auth_suffix: Option<String> = if self.settings.proxy_auth {
            Some(format!(":{}:{}", self.settings.proxy_user, self.settings.proxy_pass))
        } else {
            None
        };
        let char_w = {
            let font = egui::TextStyle::Monospace.resolve(ui.style());
            ui.painter().layout_no_wrap("0".to_string(), font, Color32::WHITE).size().x.max(1.0)
        };

        egui::Frame::new()
            .fill(p.card)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let visible = &self.visible;
                let rows = &mut self.rows;
                let ports = &self.port_map;
                let columns: [(&str, Option<SortKey>); 8] = [
                    ("#", None),
                    ("", None),
                    ("Địa chỉ IPv6", Some(SortKey::Addr)),
                    ("Proxy", None),
                    ("Trạng thái", Some(SortKey::Status)),
                    ("Kiểm tra", Some(SortKey::Test)),
                    ("IP ra", None),
                    ("Độ trễ", Some(SortKey::Latency)),
                ];
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(false)
                    .sense(Sense::click())
                    .min_scrolled_height(100.0)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(34.0))
                    .column(Column::exact(26.0))
                    .column(Column::remainder().at_least(140.0).clip(true))
                    .column(Column::exact(124.0))
                    .column(Column::exact(84.0))
                    .column(Column::exact(72.0))
                    .column(Column::exact(90.0).clip(true))
                    .column(Column::exact(64.0))
                    .header(28.0, |mut h| {
                        for (i, (name, key)) in columns.into_iter().enumerate() {
                            h.col(|ui| {
                                if i == 1 {
                                    ui.checkbox(&mut select_all, "");
                                    return;
                                }
                                match key {
                                    Some(k) => {
                                        let arrow = match sort {
                                            Some((sk, asc)) if sk == k => {
                                                if asc {
                                                    " ↑"
                                                } else {
                                                    " ↓"
                                                }
                                            }
                                            _ => "",
                                        };
                                        let resp = ui
                                            .add(
                                                Label::new(
                                                    RichText::new(format!("{name}{arrow}")).strong().color(p.weak),
                                                )
                                                .sense(Sense::click()),
                                            )
                                            .on_hover_text("Nhấn để sắp xếp");
                                        if resp.hovered() {
                                            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                        }
                                        if resp.clicked() {
                                            sort_click = Some(k);
                                        }
                                    }
                                    None => {
                                        ui.label(RichText::new(name).strong().color(p.weak));
                                    }
                                }
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(26.0, visible.len(), |mut row| {
                            let pos = row.index();
                            let i = visible[pos];
                            let r = &mut rows[i];
                            row.set_selected(r.selected);
                            row.col(|ui| {
                                ui.label(RichText::new((i + 1).to_string()).small().color(p.weak));
                            });
                            row.col(|ui| {
                                ui.checkbox(&mut r.selected, "");
                            });
                            row.col(|ui| {
                                let max = ((ui.available_width() / char_w).floor() as usize).max(8);
                                let shown = ellipsize_left(&r.text, max);
                                let resp = ui.add(
                                    Label::new(RichText::new(shown).monospace().color(p.text))
                                        .sense(Sense::click())
                                        .truncate(),
                                );
                                if resp.hovered() {
                                    ui.ctx().set_cursor_icon(CursorIcon::Copy);
                                }
                                if resp.clicked() {
                                    copied = Some(r.text.clone());
                                }
                                resp.on_hover_text(format!("{}\nNhấn để sao chép", r.text));
                            });
                            row.col(|ui| match ports.get(&r.addr) {
                                Some(port) => {
                                    let endpoint = format!("{host}:{port}");
                                    let resp = ui.add(
                                        Label::new(RichText::new(&endpoint).monospace().color(p.text))
                                            .sense(Sense::click())
                                            .truncate(),
                                    );
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(CursorIcon::Copy);
                                    }
                                    if resp.clicked() {
                                        copied = Some(match &auth_suffix {
                                            Some(a) => format!("{endpoint}{a}"),
                                            None => endpoint,
                                        });
                                    }
                                    resp.on_hover_text("Nhấn để sao chép proxy");
                                }
                                None => {
                                    ui.label(RichText::new("—").color(p.weak));
                                }
                            });
                            row.col(|ui| {
                                let resp = dot_label(ui, r.status.color(&p), RichText::new(r.status.label()));
                                if let (Status::Failed, Some(e)) = (r.status, &r.error) {
                                    resp.on_hover_text(e.as_str());
                                }
                            });
                            row.col(|ui| match &r.test {
                                TestState::None => {
                                    ui.label(RichText::new("—").color(p.weak));
                                }
                                TestState::Running => {
                                    ui.label(RichText::new("Đang thử...").color(p.info));
                                }
                                TestState::Ok => {
                                    ui.label(RichText::new("Sống").strong().color(p.success));
                                }
                                TestState::Fail(e) => {
                                    ui.label(RichText::new("Chết").strong().color(p.error)).on_hover_text(e.as_str());
                                }
                            });
                            row.col(|ui| match r.egress {
                                Some(ip) => {
                                    let same = ip == IpAddr::V6(r.addr);
                                    let text = ip.to_string();
                                    let label = if same {
                                        Label::new(RichText::new("✓ trùng").strong().color(p.success))
                                    } else {
                                        Label::new(RichText::new(&text).monospace().color(p.warn)).truncate()
                                    };
                                    let resp = ui.add(label.sense(Sense::click()));
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(CursorIcon::Copy);
                                    }
                                    if resp.clicked() {
                                        copied = Some(text.clone());
                                    }
                                    let note = if same {
                                        "Trùng với địa chỉ IPv6 của proxy".to_string()
                                    } else {
                                        format!("Khác địa chỉ IPv6 của proxy ({}), có thể NAT66", r.addr)
                                    };
                                    resp.on_hover_text(format!("{text}\n{note}\nNhấn để sao chép"));
                                }
                                None => {
                                    ui.label(RichText::new("—").color(p.weak));
                                }
                            });
                            row.col(|ui| match r.latency_ms {
                                Some(ms) => {
                                    ui.label(RichText::new(format!("{ms} ms")).monospace().color(p.text));
                                }
                                None => {
                                    ui.label(RichText::new("—").color(p.weak));
                                }
                            });
                            if row.response().clicked() {
                                row_click = Some(pos);
                            }
                        });
                    });
            });

        if select_all != all_selected {
            for &i in &self.visible {
                self.rows[i].selected = select_all;
            }
        }
        if let Some(k) = sort_click {
            self.sort = match self.sort {
                Some((sk, true)) if sk == k => Some((k, false)),
                Some((sk, false)) if sk == k => None,
                _ => Some((k, true)),
            };
            self.visible_dirty = true;
            self.last_clicked = None;
        }
        if let Some(pos) = row_click {
            if let Some(&idx) = self.visible.get(pos) {
                let state = !self.rows[idx].selected;
                let range_from = self.last_clicked.filter(|&l| shift && l < self.visible.len());
                match range_from {
                    Some(last) => {
                        let (lo, hi) = (last.min(pos), last.max(pos));
                        for &i in &self.visible[lo..=hi] {
                            self.rows[i].selected = state;
                        }
                    }
                    None => self.rows[idx].selected = state,
                }
                self.last_clicked = Some(pos);
            }
        }
        if let Some(text) = copied {
            let ok = self.copy_text(text);
            if ok {
                self.toast_ok("Đã sao chép");
            } else {
                self.toast_err("Không truy cập được clipboard");
            }
        }
    }
}

fn scope_label(s: ExportScope) -> &'static str {
    match s {
        ExportScope::All => "Tất cả",
        ExportScope::Selected => "Đã chọn",
        ExportScope::Added => "Đã thêm",
    }
}

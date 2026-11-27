use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::Ordering;

use eframe::egui::{
    self, Align, Color32, CursorIcon, DragValue, Key, Label, Layout, Modifiers, RichText, Sense, TextEdit,
};
use egui_extras::{Column, TableBuilder};

use super::{addr_job, card_pad, danger, danger_small, dot_label, plain_button, prefix_split, primary};
use crate::app::{is_online, status_hint, App, Confirm, LogLevel, OpKind, ResultAction, SortKey, Status, TestState};
use crate::proxy;
use crate::theme;

fn ellipsize_left(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1);
    let tail: String = text.chars().skip(n - keep).collect();
    format!("…{tail}")
}

fn copy_button(ui: &mut egui::Ui) -> egui::Response {
    let resp = ui.add(egui::Button::new("").small().min_size(egui::vec2(20.0, 18.0)));
    let visuals = ui.style().interact(&resp);
    let stroke = egui::Stroke::new(1.2, visuals.fg_stroke.color);
    let c = resp.rect.center();
    let size = egui::vec2(7.0, 8.0);
    let back = egui::Rect::from_min_size(c + egui::vec2(-4.5, -5.0), size);
    let front = egui::Rect::from_min_size(c + egui::vec2(-2.5, -3.0), size);
    let painter = ui.painter();
    painter.rect_stroke(back, 1.5, stroke, egui::StrokeKind::Middle);
    painter.rect_filled(front, 1.5, visuals.weak_bg_fill);
    painter.rect_stroke(front, 1.5, stroke, egui::StrokeKind::Middle);
    resp
}

fn progress_row(ui: &mut egui::Ui, frac: f32, color: Color32, text: &str) {
    ui.add(
        egui::ProgressBar::new(frac)
            .desired_width(ui.available_width())
            .fill(color)
            .corner_radius(4)
            .text(text),
    );
}

impl App {
    pub fn proxy_tab(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        ui.spacing_mut().item_spacing.y = 8.0;

        card_pad(ui, &p, 10, |ui| {
            self.toolbar(ui);
        });

        self.result_line(ui);
        self.progress_rows(ui);
        self.table(ui);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 8.0;
        self.search_row(ui);
        ui.horizontal_wrapped(|ui| {
            self.address_actions(ui);
            self.test_actions(ui);
        });
        self.proxy_card(ui);
    }

    fn search_row(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let c = self.counts;
        let counts = format!(
            "Hiển thị {}/{} · Chọn {} · Đã thêm {}",
            self.visible.len(),
            self.rows.len(),
            c.selected,
            c.added
        );
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(counts).color(p.weak));
                if !self.filter.is_empty() && ui.add(plain_button("Xóa lọc")).clicked() {
                    self.filter.clear();
                    self.visible_dirty = true;
                }
                let width = ui.available_width().max(120.0);
                let resp = ui.add(
                    TextEdit::singleline(&mut self.filter)
                        .hint_text("Tìm địa chỉ, trạng thái, online/offline...")
                        .margin(egui::Margin::symmetric(8, 7))
                        .desired_width(width),
                );
                if resp.changed() {
                    self.visible_dirty = true;
                }
            });
        });
    }

    fn address_actions(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let c = self.counts;
        let block = self.apply_block_reason();
        let busy = self.busy();
        let apply_n = c.apply_n();

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

        if ui
            .add_enabled(!busy && !self.rows.is_empty(), plain_button("Xóa khỏi danh sách"))
            .on_hover_text("Chỉ xóa dòng khỏi danh sách, không ảnh hưởng card mạng")
            .clicked()
        {
            self.clear_non_added();
        }
        ui.separator();
    }

    fn test_actions(&mut self, ui: &mut egui::Ui) {
        let c = self.counts;
        let testing = self.test.is_some();
        let n = c.test_n();
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
            Some("Không có proxy nào kiểm tra lỗi")
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
    }

    fn pending_proxy_config(&self) -> Option<Result<(), String>> {
        let h = self.proxy.as_ref()?;
        let s = &self.settings;
        let bind = s.proxy_bind.trim().parse::<Ipv4Addr>();
        let want_auth = s
            .proxy_auth
            .then(|| (s.proxy_user.trim().to_string(), s.proxy_pass.clone()));
        let cur_auth = h.auth.as_ref().map(|c| (c.user.clone(), c.pass.clone()));
        let changed = bind.as_ref().map_or(true, |b| *b != h.bind)
            || s.proxy_start_port != h.start_port
            || want_auth != cur_auth;
        if !changed {
            return None;
        }
        if bind.is_err() {
            return Some(Err("Địa chỉ bind IPv4 của proxy không hợp lệ".to_string()));
        }
        if let Err(e) = proxy::port_range(s.proxy_start_port, h.addrs.len()) {
            return Some(Err(e));
        }
        Some(self.settings_credentials().map(|_| ()))
    }

    fn proxy_card(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let running = self.proxy.is_some();
        let stopping = self.stopping_proxy;
        let count = self.counts.added;
        let stale = self.proxy_stale();
        let pending = self.pending_proxy_config();

        let mut start = false;
        let mut restart = false;

        ui.horizontal_wrapped(|ui| {
            if stopping {
                ui.spinner();
                ui.label(RichText::new("Đang dừng proxy…").font(theme::semibold(14.0)).color(p.warn));
                return;
            }
            let Some(h) = &self.proxy else {
                dot_label(ui, p.weak, RichText::new("Proxy đang tắt"));
                if count > 0 && ui.small_button("Chạy proxy").clicked() {
                    start = true;
                }
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
                dot_label(ui, color, RichText::new("Đang chạy").font(theme::semibold(14.0)));
            } else {
                ui.spinner();
                ui.label(RichText::new("Đang mở cổng...").font(theme::semibold(14.0)));
            }
            ui.label(RichText::new(format!("{}:{}-{}", h.bind, h.start_port, last)).monospace());
            ui.label(RichText::new(format!("Mở {bound}/{planned}")).color(p.weak));
            if failed > 0 {
                ui.label(RichText::new(format!("lỗi {failed}")).color(p.warn));
            }
            ui.label(RichText::new(format!("Đang kết nối {active}")).color(p.weak));
            ui.label(RichText::new(format!("Tổng {total}")).color(p.weak));
        });

        if running && !stopping {
            let note = if stale {
                Some(("Danh sách địa chỉ đã đổi so với lúc chạy proxy, proxy sẽ được cập nhật.", true))
            } else if matches!(pending, Some(Ok(()))) {
                Some(("Cấu hình proxy đã thay đổi so với lúc chạy.", true))
            } else {
                None
            };
            if let Some((text, offer)) = note {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(text).small().color(p.warn));
                    if offer && ui.small_button("Khởi động lại proxy để áp dụng").clicked() {
                        restart = true;
                    }
                });
            } else if let Some(Err(e)) = &pending {
                ui.label(RichText::new(format!("Cấu hình proxy chưa hợp lệ: {e}")).small().color(p.warn));
            }
        }
        if let Some(err) = &self.proxy_error {
            ui.label(RichText::new(err.as_str()).font(theme::semibold(14.0)).color(p.error));
        }

        let lan: Vec<String> = self
            .adapter()
            .map(|a| a.ipv4.iter().map(|(ip, _)| ip.to_string()).collect())
            .unwrap_or_default();
        egui::CollapsingHeader::new(RichText::new("Cấu hình proxy").font(theme::semibold(14.0)))
            .default_open(false)
            .show(ui, |ui| {
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
        if restart {
            self.start_after_stop = true;
            self.stop_proxy();
        }
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
            dot_label(ui, color, RichText::new(text).font(theme::semibold(14.0)));
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
                None => {}
            }
        }
        if dismiss {
            self.last_result = None;
        }
    }

    fn progress_rows(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
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
            progress_row(ui, frac, p.accent_fill, &text);
        }
        if let Some(t) = &self.test {
            let frac = if t.total == 0 { 0.0 } else { t.done as f32 / t.total as f32 };
            let text = format!("Kiểm tra proxy {}/{} (sống {})", t.done, t.total, t.ok);
            progress_row(ui, frac, p.info, &text);
        }
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        let p = self.pal;
        let avail = ui.available_size();
        let inner_size = egui::vec2((avail.x - 14.0).max(0.0), (avail.y - 16.0).max(0.0));
        if self.rows.is_empty() {
            egui::Frame::new()
                .fill(p.card)
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_min_size(inner_size);
                    ui.add_space((inner_size.y * 0.3).max(20.0));
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Chưa có địa chỉ nào").font(theme::semibold(17.0)).color(p.weak));
                        ui.label(RichText::new("Vào tab \"Tạo địa chỉ\" để sinh danh sách IPv6 mới.").color(p.weak));
                    });
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
        let mut row_action: Option<(usize, bool)> = None;
        let block = self.apply_block_reason();
        let busy = self.busy();
        let proxy_on = self.proxy.is_some();
        let shift = ui.input(|i| i.modifiers.shift);
        let host = self.proxy_export_host();
        let sort = self.sort;
        let auth_suffix: Option<String> = self
            .proxy_credentials()
            .ok()
            .flatten()
            .map(|c| format!(":{}:{}", c.user, c.pass));
        let char_w = {
            let font = egui::TextStyle::Monospace.resolve(ui.style());
            ui.painter().layout_no_wrap("0".to_string(), font, Color32::WHITE).size().x.max(1.0)
        };

        egui::Frame::new()
            .fill(p.card)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.set_min_size(inner_size);
                ui.spacing_mut().item_spacing.x = 10.0;
                let body_height = ui.available_height();
                let visible = &self.visible;
                let rows = &mut self.rows;
                let ports = &self.port_map;
                let columns: [(&str, Option<SortKey>); 8] = [
                    ("#", None),
                    ("", None),
                    ("Proxy", None),
                    ("Trạng thái", Some(SortKey::Status)),
                    ("IP ra", None),
                    ("Độ trễ", Some(SortKey::Latency)),
                    ("Hành động", None),
                    ("Địa chỉ IPv6", Some(SortKey::Addr)),
                ];
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .auto_shrink([false, false])
                    .sense(Sense::click())
                    .min_scrolled_height(body_height)
                    .max_scroll_height(body_height)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(44.0))
                    .column(Column::exact(30.0))
                    .column(Column::initial(200.0).at_least(170.0).clip(true))
                    .column(Column::initial(100.0).at_least(80.0))
                    .column(Column::initial(150.0).at_least(90.0).clip(true))
                    .column(Column::initial(80.0).at_least(60.0))
                    .column(Column::initial(180.0).at_least(170.0).clip(true))
                    .column(Column::remainder().at_least(260.0).clip(true))
                    .header(30.0, |mut h| {
                        for (i, (name, key)) in columns.into_iter().enumerate() {
                            h.col(|ui| {
                                let cell = ui.max_rect().expand2(egui::vec2(5.0, 0.0));
                                ui.painter().rect_filled(cell, 0.0, p.panel);
                                ui.painter().hline(
                                    cell.x_range(),
                                    cell.bottom() - 0.5,
                                    egui::Stroke::new(1.0, p.border),
                                );
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
                                                    RichText::new(format!("{name}{arrow}")).font(theme::semibold(13.0)).color(p.weak),
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
                                        ui.label(RichText::new(name).font(theme::semibold(13.0)).color(p.weak));
                                    }
                                }
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(28.0, visible.len(), |mut row| {
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
                            row.col(|ui| match ports.get(&r.addr) {
                                Some(port) => {
                                    let endpoint = format!("{host}:{port}");
                                    let value = match &auth_suffix {
                                        Some(a) => format!("{endpoint}{a}"),
                                        None => endpoint.clone(),
                                    };
                                    let resp = ui.add(
                                        Label::new(RichText::new(&endpoint).monospace().color(p.text))
                                            .sense(Sense::click())
                                            .truncate(),
                                    );
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(CursorIcon::Copy);
                                    }
                                    if resp.clicked() {
                                        copied = Some(value.clone());
                                    }
                                    resp.on_hover_text("Nhấn để sao chép proxy");
                                    if copy_button(ui).on_hover_text("Sao chép proxy").clicked() {
                                        copied = Some(value);
                                    }
                                }
                                None => {
                                    ui.label(RichText::new("—").color(p.weak));
                                }
                            });
                            row.col(|ui| {
                                let online = is_online(proxy_on, ports, r);
                                let (color, text) = if online {
                                    (p.success, "Online")
                                } else if r.status == Status::Failed || matches!(r.test, TestState::Fail(_)) {
                                    (p.error, "Offline")
                                } else {
                                    (p.weak, "Offline")
                                };
                                let resp = dot_label(ui, color, RichText::new(text));
                                resp.on_hover_ui(|ui| {
                                    ui.label(status_hint(proxy_on, ports, r));
                                });
                            });
                            row.col(|ui| match r.egress {
                                Some(ip) => {
                                    let same = ip == IpAddr::V6(r.addr);
                                    let text = ip.to_string();
                                    let label = if same {
                                        Label::new(RichText::new("✓ trùng").font(theme::semibold(14.0)).color(p.success))
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
                            row.col(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                let added = r.status == Status::Added;
                                let off: Option<&str> = if added {
                                    block.as_deref()
                                } else if busy {
                                    Some("Đang có tác vụ chạy, vui lòng chờ")
                                } else {
                                    None
                                };
                                let del_hint = if added {
                                    "Gỡ khỏi card mạng và xóa khỏi danh sách"
                                } else {
                                    "Xóa khỏi danh sách"
                                };
                                if danger_small(ui, &p, off.is_none(), "Xóa")
                                    .on_hover_text(del_hint)
                                    .on_disabled_hover_text(off.unwrap_or_default())
                                    .clicked()
                                {
                                    row_action = Some((i, false));
                                }
                                let new_hint = if added {
                                    "Gỡ địa chỉ cũ khỏi card và thêm địa chỉ ngẫu nhiên mới cùng prefix"
                                } else {
                                    "Thay bằng địa chỉ ngẫu nhiên mới cùng prefix"
                                };
                                if ui
                                    .add_enabled(
                                        off.is_none(),
                                        egui::Button::new(RichText::new("Tạo IPv6 mới").font(theme::semibold(13.0)))
                                            .min_size(egui::vec2(0.0, 22.0)),
                                    )
                                    .on_hover_text(new_hint)
                                    .on_disabled_hover_text(off.unwrap_or_default())
                                    .clicked()
                                {
                                    row_action = Some((i, true));
                                }
                            });
                            row.col(|ui| {
                                let max = ((ui.available_width() / char_w).floor() as usize).max(8);
                                let shown = ellipsize_left(&r.text, max);
                                let text: egui::WidgetText = if shown == r.text {
                                    let font = egui::TextStyle::Monospace.resolve(ui.style());
                                    let split = prefix_split(&r.text, r.prefix_len);
                                    addr_job(&r.text, split, p.weak, p.text, font).into()
                                } else {
                                    RichText::new(shown).monospace().color(p.text).into()
                                };
                                let resp = ui.add(Label::new(text).sense(Sense::click()).truncate());
                                if resp.hovered() {
                                    ui.ctx().set_cursor_icon(CursorIcon::Copy);
                                }
                                if resp.clicked() {
                                    copied = Some(r.text.clone());
                                }
                                resp.on_hover_text(format!("{}\nNhấn để sao chép", r.text));
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
        if let Some((i, regen)) = row_action {
            if regen {
                self.regenerate_row(i);
            } else {
                self.delete_row(i);
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

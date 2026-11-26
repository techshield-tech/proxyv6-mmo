#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod net;
mod proxy;
mod storage;
mod tester;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ProxyV6 MMO")
        .with_app_id("proxyv6-mmo")
        .with_inner_size([1200.0, 760.0])
        .with_min_inner_size([1040.0, 640.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "ProxyV6 MMO",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

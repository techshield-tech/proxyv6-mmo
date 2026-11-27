use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, Style,
    TextStyle, Theme, ThemePreference, Visuals,
};

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_fill: Color32,
    pub on_accent: Color32,
    pub danger_fill: Color32,
    pub success: Color32,
    pub warn: Color32,
    pub error: Color32,
    pub info: Color32,
}

impl Palette {
    pub fn get(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                bg: Color32::from_rgb(0x11, 0x15, 0x18),
                panel: Color32::from_rgb(0x16, 0x1B, 0x20),
                card: Color32::from_rgb(0x1C, 0x22, 0x28),
                card_hover: Color32::from_rgb(0x23, 0x2B, 0x33),
                border: Color32::from_rgb(0x2C, 0x35, 0x3F),
                text: Color32::from_rgb(0xDD, 0xE4, 0xEA),
                weak: Color32::from_rgb(0x8A, 0x97, 0xA4),
                accent: Color32::from_rgb(0xF2, 0xB1, 0x34),
                accent_soft: Color32::from_rgba_unmultiplied(242, 177, 52, 26),
                accent_fill: Color32::from_rgb(0xF2, 0xB1, 0x34),
                on_accent: Color32::from_rgb(0x1B, 0x14, 0x05),
                danger_fill: Color32::from_rgb(0xC9, 0x40, 0x40),
                success: Color32::from_rgb(0x3C, 0xCB, 0x7F),
                warn: Color32::from_rgb(0xE5, 0x7A, 0x35),
                error: Color32::from_rgb(0xEF, 0x5B, 0x5B),
                info: Color32::from_rgb(0x5A, 0xA9, 0xE6),
            }
        } else {
            Self {
                dark,
                bg: Color32::from_rgb(0xEC, 0xEE, 0xF0),
                panel: Color32::from_rgb(0xF6, 0xF7, 0xF8),
                card: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                card_hover: Color32::from_rgb(0xF4, 0xF1, 0xE8),
                border: Color32::from_rgb(0xD5, 0xDA, 0xDF),
                text: Color32::from_rgb(0x1C, 0x23, 0x29),
                weak: Color32::from_rgb(0x5F, 0x6B, 0x76),
                accent: Color32::from_rgb(0xA8, 0x6C, 0x00),
                accent_soft: Color32::from_rgba_unmultiplied(240, 166, 46, 40),
                accent_fill: Color32::from_rgb(0xF0, 0xA6, 0x2E),
                on_accent: Color32::from_rgb(0x1E, 0x16, 0x00),
                danger_fill: Color32::from_rgb(0xC0, 0x34, 0x34),
                success: Color32::from_rgb(0x1E, 0x8E, 0x52),
                warn: Color32::from_rgb(0xC2, 0x56, 0x0F),
                error: Color32::from_rgb(0xC9, 0x34, 0x34),
                info: Color32::from_rgb(0x2F, 0x74, 0xB5),
            }
        }
    }
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

fn build_style(p: Palette) -> Style {
    let mut style = Style::default();
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    let radius = CornerRadius::same(4);

    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = if p.dark { Color32::from_rgb(0x0C, 0x0F, 0x12) } else { Color32::WHITE };
    v.faint_bg_color = if p.dark { Color32::from_rgb(0x19, 0x1E, 0x24) } else { Color32::from_rgb(0xF3, 0xF5, 0xF7) };
    v.code_bg_color = p.card;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.window_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 110 } else { 28 }),
    };
    v.popup_shadow = v.window_shadow;
    v.hyperlink_color = p.info;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.override_text_color = Some(p.text);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = p.card;
    w.inactive.weak_bg_fill = p.card;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = p.card_hover;
    w.hovered.weak_bg_fill = p.card_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.7));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.accent_fill;
    w.active.weak_bg_fill = p.accent_fill;
    w.active.bg_stroke = Stroke::new(1.0, p.accent_fill);
    w.active.fg_stroke = Stroke::new(1.5, p.on_accent);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.card_hover;
    w.open.weak_bg_fill = p.card_hover;
    w.open.bg_stroke = Stroke::new(1.0, p.accent);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = radius;

    style.visuals = v;

    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size = egui::vec2(40.0, 28.0);
    style.spacing.window_margin = Margin::same(14);
    style.spacing.menu_margin = Margin::same(8);
    style.spacing.indent = 18.0;
    style.spacing.combo_width = 140.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;

    style.text_styles = [
        (TextStyle::Heading, semibold(20.0)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, semibold(14.0)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();

    style
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    ctx.set_style_of(Theme::Dark, build_style(Palette::get(true)));
    ctx.set_style_of(Theme::Light, build_style(Palette::get(false)));
    ctx.set_theme(if dark { ThemePreference::Dark } else { ThemePreference::Light });
}

fn font_candidates() -> Vec<&'static str> {
    if cfg!(target_os = "windows") {
        vec![
            "C:\\Windows\\Fonts\\segoeui.ttf",
            "C:\\Windows\\Fonts\\tahoma.ttf",
            "C:\\Windows\\Fonts\\arial.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/Library/Fonts/Arial Unicode.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
        ]
    } else {
        vec![
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/opentype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
        ]
    }
}

pub fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).ok()?;
    let image = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
    let options = egui::TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..egui::TextureOptions::LINEAR
    };
    Some(ctx.load_texture("app-logo", image, options))
}

pub fn install_fonts(ctx: &egui::Context) -> Option<&'static str> {
    let mut fonts = FontDefinitions::default();
    let mut regular_path = None;
    for path in font_candidates() {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("system-ui".to_owned(), Arc::new(FontData::from_owned(bytes)));
            if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
                list.insert(0, "system-ui".to_owned());
            }
            regular_path = Some(path);
            break;
        }
    }

    let mut semibold_list = Vec::new();
    if cfg!(target_os = "windows") {
        if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\seguisb.ttf") {
            fonts
                .font_data
                .insert("system-semibold".to_owned(), Arc::new(FontData::from_owned(bytes)));
            semibold_list.push("system-semibold".to_owned());
        }
        let mono = [r"C:\Windows\Fonts\CascadiaMono.ttf", r"C:\Windows\Fonts\consola.ttf"]
            .into_iter()
            .find_map(|path| std::fs::read(path).ok());
        if let Some(bytes) = mono {
            fonts
                .font_data
                .insert("system-mono".to_owned(), Arc::new(FontData::from_owned(bytes)));
            if let Some(list) = fonts.families.get_mut(&FontFamily::Monospace) {
                list.insert(0, "system-mono".to_owned());
            }
        }
    }
    if let Some(list) = fonts.families.get(&FontFamily::Proportional) {
        semibold_list.extend(list.iter().cloned());
    }
    fonts.families.insert(FontFamily::Name("semibold".into()), semibold_list);

    ctx.set_fonts(fonts);
    regular_path
}

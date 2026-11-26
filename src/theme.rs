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
                bg: Color32::from_rgb(14, 17, 26),
                panel: Color32::from_rgb(19, 23, 34),
                card: Color32::from_rgb(27, 32, 47),
                card_hover: Color32::from_rgb(34, 40, 59),
                border: Color32::from_rgb(44, 51, 72),
                text: Color32::from_rgb(226, 231, 243),
                weak: Color32::from_rgb(137, 148, 173),
                accent: Color32::from_rgb(108, 114, 255),
                accent_soft: Color32::from_rgba_premultiplied(30, 32, 70, 90),
                accent_fill: Color32::from_rgb(79, 70, 229),
                danger_fill: Color32::from_rgb(200, 50, 50),
                success: Color32::from_rgb(52, 211, 153),
                warn: Color32::from_rgb(251, 191, 36),
                error: Color32::from_rgb(248, 113, 113),
                info: Color32::from_rgb(96, 165, 250),
            }
        } else {
            Self {
                dark,
                bg: Color32::from_rgb(241, 243, 249),
                panel: Color32::from_rgb(249, 250, 253),
                card: Color32::from_rgb(255, 255, 255),
                card_hover: Color32::from_rgb(243, 245, 255),
                border: Color32::from_rgb(218, 223, 236),
                text: Color32::from_rgb(28, 34, 52),
                weak: Color32::from_rgb(105, 115, 140),
                accent: Color32::from_rgb(79, 70, 229),
                accent_soft: Color32::from_rgba_premultiplied(79, 70, 229, 24),
                accent_fill: Color32::from_rgb(79, 70, 229),
                danger_fill: Color32::from_rgb(200, 35, 35),
                success: Color32::from_rgb(5, 150, 105),
                warn: Color32::from_rgb(180, 110, 0),
                error: Color32::from_rgb(220, 38, 38),
                info: Color32::from_rgb(37, 99, 235),
            }
        }
    }
}

fn build_style(p: Palette) -> Style {
    let mut style = Style::default();
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    let radius = CornerRadius::same(8);

    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = if p.dark { Color32::from_rgb(10, 12, 19) } else { Color32::WHITE };
    v.faint_bg_color = if p.dark { Color32::from_rgb(22, 27, 40) } else { Color32::from_rgb(246, 248, 253) };
    v.code_bg_color = p.card;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 120 } else { 40 }),
    };
    v.popup_shadow = v.window_shadow;
    v.hyperlink_color = p.info;
    v.selection.bg_fill = p.accent.gamma_multiply(0.45);
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
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.accent;
    w.active.weak_bg_fill = p.accent;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.card_hover;
    w.open.weak_bg_fill = p.card_hover;
    w.open.bg_stroke = Stroke::new(1.0, p.accent);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = radius;

    style.visuals = v;

    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.interact_size = egui::vec2(40.0, 28.0);
    style.spacing.window_margin = Margin::same(14);
    style.spacing.menu_margin = Margin::same(8);
    style.spacing.indent = 18.0;
    style.spacing.combo_width = 140.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;

    style.text_styles = [
        (TextStyle::Heading, FontId::new(21.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.5, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.5, FontFamily::Monospace)),
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
    for path in font_candidates() {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = FontDefinitions::default();
            fonts
                .font_data
                .insert("system-ui".to_owned(), Arc::new(FontData::from_owned(bytes)));
            if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
                list.insert(0, "system-ui".to_owned());
            }
            ctx.set_fonts(fonts);
            return Some(path);
        }
    }
    None
}

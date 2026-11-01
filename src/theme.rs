use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Visuals,
};

use crate::sheet::{classify_cell, CellKind, ColKind};

pub const BG: Color32 = Color32::from_rgb(13, 16, 21);
pub const SURFACE: Color32 = Color32::from_rgb(20, 24, 31);
pub const SURFACE_2: Color32 = Color32::from_rgb(28, 34, 44);
pub const LIFT: Color32 = Color32::from_rgb(36, 44, 58);
pub const ACCENT: Color32 = Color32::from_rgb(79, 169, 241);
pub const TEXT: Color32 = Color32::from_rgb(232, 237, 243);
pub const MUTED: Color32 = Color32::from_rgb(138, 149, 164);
pub const GREEN: Color32 = Color32::from_rgb(62, 207, 142);
pub const RED: Color32 = Color32::from_rgb(248, 113, 113);
pub const GOLD: Color32 = Color32::from_rgb(240, 186, 69);
pub const PURPLE: Color32 = Color32::from_rgb(196, 146, 246);
pub const NUMBER: Color32 = Color32::from_rgb(125, 196, 255);
pub const GRID_LINE: Color32 = Color32::from_rgb(38, 46, 58);

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);

    let mut visuals = Visuals::dark();
    visuals.dark_mode = true;
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(10, 12, 16);
    visuals.faint_bg_color = SURFACE;
    visuals.code_bg_color = Color32::from_rgb(10, 12, 16);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(79, 169, 241, 70);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.bg_fill = SURFACE_2;
    visuals.widgets.inactive.weak_bg_fill = SURFACE_2;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.bg_fill = LIFT;
    visuals.widgets.hovered.weak_bg_fill = LIFT;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::from_rgb(8, 12, 18));
    visuals.widgets.open.bg_fill = LIFT;
    visuals.widgets.inactive.corner_radius = CornerRadius::same(7);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(7);
    visuals.widgets.active.corner_radius = CornerRadius::same(7);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.striped = true;
    visuals.window_shadow.blur = 16;
    visuals.popup_shadow.blur = 12;
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(11.0, 6.0);
        style.spacing.indent = 16.0;
        style.interaction.selectable_labels = false;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(14.5, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(20.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(13.5, FontFamily::Monospace),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        );
    });
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    load_font(
        &mut fonts,
        "adwaita",
        "/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf",
        FontFamily::Proportional,
    );
    load_font(
        &mut fonts,
        "adwaita-mono",
        "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf",
        FontFamily::Monospace,
    );
    ctx.set_fonts(fonts);
}

fn load_font(fonts: &mut FontDefinitions, name: &str, path: &str, family: FontFamily) {
    let Ok(data) = std::fs::read(path) else {
        return;
    };
    fonts.font_data.insert(
        name.to_owned(),
        std::sync::Arc::new(FontData::from_owned(data)),
    );
    fonts
        .families
        .entry(family)
        .or_default()
        .insert(0, name.to_owned());
}

pub fn kind_color(kind: ColKind) -> Color32 {
    match kind {
        ColKind::Integer | ColKind::Float => NUMBER,
        ColKind::Bool => GREEN,
        ColKind::Date => PURPLE,
        ColKind::Json => GOLD,
        ColKind::Empty => MUTED,
        ColKind::Mixed => Color32::from_rgb(250, 166, 90),
        ColKind::Text => TEXT,
    }
}

pub fn cell_color(text: &str) -> Color32 {
    if text.trim().is_empty() {
        return MUTED;
    }
    match classify_cell(text) {
        CellKind::Integer | CellKind::Float => NUMBER,
        CellKind::Bool => {
            if crate::sheet::is_true(text) {
                GREEN
            } else {
                RED
            }
        }
        CellKind::Date => PURPLE,
        CellKind::Json => GOLD,
        CellKind::Empty => MUTED,
        CellKind::Text => TEXT,
    }
}

pub fn pill(kind: &str) -> (Color32, Color32) {
    match kind {
        "CSV" => (Color32::from_rgb(20, 48, 38), GREEN),
        "NDJSON" => (Color32::from_rgb(48, 36, 14), GOLD),
        _ => (Color32::from_rgb(42, 34, 16), GOLD),
    }
}

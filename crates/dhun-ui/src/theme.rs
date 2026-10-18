//! Neon-Tokyo visual theme: deep midnight panels, glowing neon accents, rounded
//! widgets, and clean typography (Orbitron display face for titles).

use egui::{Color32, FontFamily, FontId, Rounding, Stroke, TextStyle};

/// Name of the Orbitron display family used for headings/titles.
pub fn display_family() -> FontFamily {
    FontFamily::Name("display".into())
}

/// Registers the bundled Orbitron display font (OFL-1.1, see
/// `assets/fonts/OFL.txt`) as a named family alongside egui's built-in
/// proportional/monospace faces.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "orbitron".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Orbitron-Variable.ttf")),
    );
    fonts
        .families
        .entry(display_family())
        .or_default()
        .insert(0, "orbitron".to_owned());
    ctx.set_fonts(fonts);
}

// --- Palette -------------------------------------------------------------

/// Near-black midnight background.
pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0C, 0x18);
/// Panel fill, a touch lighter than [`BG`].
pub const PANEL: Color32 = Color32::from_rgb(0x11, 0x14, 0x28);
/// Raised surface (cards, inactive widgets).
pub const SURFACE: Color32 = Color32::from_rgb(0x1A, 0x1F, 0x3A);
/// Hairline/border color on dark surfaces.
pub const LINE: Color32 = Color32::from_rgb(0x2A, 0x31, 0x55);
/// Primary text.
pub const TEXT: Color32 = Color32::from_rgb(0xE7, 0xEC, 0xFF);
/// Dimmed text.
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8A, 0x93, 0xC2);

/// Neon magenta.
pub const MAGENTA: Color32 = Color32::from_rgb(0xFF, 0x2E, 0x97);
/// Neon cyan.
pub const CYAN: Color32 = Color32::from_rgb(0x22, 0xE6, 0xFF);
/// Neon purple.
pub const PURPLE: Color32 = Color32::from_rgb(0xA8, 0x55, 0xF7);
/// Neon lime.
pub const LIME: Color32 = Color32::from_rgb(0x8C, 0xFF, 0x4A);
/// Neon amber.
pub const AMBER: Color32 = Color32::from_rgb(0xFF, 0xC4, 0x3D);

/// Per-track accent colors, cycled by channel index.
const ACCENTS: [Color32; 5] = [MAGENTA, CYAN, PURPLE, LIME, AMBER];

/// Returns the neon accent for a channel by its position in the list.
#[must_use]
pub fn channel_accent(index: usize) -> Color32 {
    ACCENTS[index % ACCENTS.len()]
}

/// A translucent version of `color` for glow halos.
#[must_use]
pub fn glow(color: Color32) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 70)
}

// --- Style installation --------------------------------------------------

/// Installs the neon-Tokyo theme on an egui context. Call once at startup.
pub fn apply_theme(ctx: &egui::Context) {
    install_fonts(ctx);

    let mut style = (*ctx.style()).clone();

    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = SURFACE;
    v.window_rounding = Rounding::same(10.0);
    v.window_stroke = Stroke::new(1.0_f32, LINE);
    v.selection.bg_fill = MAGENTA.linear_multiply(0.45_f32);
    v.selection.stroke = Stroke::new(1.0_f32, MAGENTA);
    v.hyperlink_color = CYAN;

    let rounding = Rounding::same(7.0);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.weak_bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.noninteractive.rounding = rounding;

    v.widgets.inactive.bg_fill = SURFACE;
    v.widgets.inactive.weak_bg_fill = SURFACE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);
    v.widgets.inactive.rounding = rounding;

    v.widgets.hovered.bg_fill = SURFACE;
    v.widgets.hovered.weak_bg_fill = SURFACE;
    v.widgets.hovered.bg_stroke = Stroke::new(1.5_f32, CYAN);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, TEXT);
    v.widgets.hovered.rounding = rounding;

    v.widgets.active.bg_fill = MAGENTA.linear_multiply(0.35_f32);
    v.widgets.active.weak_bg_fill = MAGENTA.linear_multiply(0.35_f32);
    v.widgets.active.bg_stroke = Stroke::new(1.5_f32, MAGENTA);
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, TEXT);
    v.widgets.active.rounding = rounding;

    style.visuals = v;

    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.spacing.slider_width = 120.0;

    style.text_styles = [
        // Orbitron display face for section titles / brand.
        (TextStyle::Heading, FontId::new(19.0, display_family())),
        (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(14.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
        (
            TextStyle::Small,
            FontId::new(11.5, FontFamily::Proportional),
        ),
    ]
    .into();

    ctx.set_style(style);
}

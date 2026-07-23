//! Design tokens and egui style setup for the whole app.
//!
//! Single source of truth for colors, font sizes and shape metrics, mirroring
//! the previous Fluent-style look. Designed to be reused by future windows
//! (e.g. the planned code-editor window).

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

// -- Color tokens ------------------------------------------------------------

/// Dark background of the simulated hardware panel.
pub const PANEL_BACKGROUND: Color32 = Color32::from_rgb(0x22, 0x22, 0x24);
/// Background behind the plot panes.
pub const PLOT_FRAME_BACKGROUND: Color32 = Color32::from_rgb(0x11, 0x11, 0x18);
/// Border of the hardware panel.
pub const PANEL_BORDER: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
/// Border of the plot panes.
pub const PLOT_FRAME_BORDER: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
/// Error / warning accent (dialog border, status text).
pub const ERROR_COLOR: Color32 = Color32::from_rgb(0xc0, 0x39, 0x2b);
/// Primary label color on dark panels.
pub const LABEL_ON_DARK: Color32 = Color32::from_rgb(0xcc, 0xcc, 0xcc);
/// Muted section-header color on dark panels.
pub const MUTED_ON_DARK: Color32 = Color32::from_rgb(0x88, 0x88, 0x88);
/// Dialog surface background.
pub const DIALOG_BACKGROUND: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
/// Modal scrim (translucent black overlay).
pub const MODAL_SCRIM: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 0xaa);
/// Fluent-style accent used for primary/selected widgets.
pub const ACCENT: Color32 = Color32::from_rgb(0x00, 0x78, 0xd4);
/// Time-domain (waveform) series color.
pub const WAVEFORM_COLOR: Color32 = Color32::from_rgb(0x21, 0x96, 0xf3);
/// Frequency-domain (spectrum) series color.
pub const SPECTRUM_COLOR: Color32 = Color32::from_rgb(0xe6, 0x7e, 0x22);
/// Plot drawing-area background (plots were rendered on white).
pub const PLOT_BACKGROUND: Color32 = Color32::from_rgb(0x22, 0x22, 0x24);
/// Light-blue app-mark stroke (logo, recent-badge glyph).
pub const LOGO_STROKE: Color32 = Color32::from_rgb(0x90, 0xca, 0xf9);
/// Inset surface for the logo tile / recent-item badge.
pub const INSET_SURFACE: Color32 = Color32::from_rgb(0x26, 0x26, 0x2b);
/// Border of inset surfaces (logo tile, badge).
pub const INSET_BORDER: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x41);
/// Hover highlight for list rows on dark surfaces.
pub const ROW_HOVER: Color32 = Color32::from_rgb(0x26, 0x26, 0x2b);
/// Fill of the active editor tab (matches the code-area background so the
/// selected buffer reads as continuous with its content).
pub const TAB_ACTIVE_BACKGROUND: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1f);

// -- Typography tokens (px, matching the previous UI markup) ------------------------

pub const FONT_SMALL: f32 = 11.0;
pub const FONT_BODY: f32 = 12.0;
pub const FONT_SUBTITLE: f32 = 14.0;
pub const FONT_TITLE: f32 = 16.0;

// -- Shape tokens --------------------------------------------------------------

pub const CORNER_RADIUS: u8 = 4;
pub const DIALOG_CORNER_RADIUS: u8 = 6;

pub fn small_font() -> FontId {
    FontId::new(FONT_SMALL, FontFamily::Proportional)
}

pub fn body_font() -> FontId {
    FontId::new(FONT_BODY, FontFamily::Proportional)
}

pub fn subtitle_font() -> FontId {
    FontId::new(FONT_SUBTITLE, FontFamily::Proportional)
}

pub fn title_font() -> FontId {
    FontId::new(FONT_TITLE, FontFamily::Proportional)
}

/// Installs the app-wide style: a light, Fluent-like theme matching the
/// previous UI appearance.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();

    style.text_styles = [
        (TextStyle::Small, small_font()),
        (TextStyle::Body, body_font()),
        (TextStyle::Button, body_font()),
        (TextStyle::Heading, title_font()),
        (
            TextStyle::Monospace,
            FontId::new(FONT_BODY, FontFamily::Monospace),
        ),
    ]
    .into();

    let mut visuals = Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(0x1c, 0x1c, 0x1f);
    visuals.window_fill = Color32::from_rgb(0x1c, 0x1c, 0x1f);
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.hyperlink_color = ACCENT;

    let radius = CornerRadius::same(CORNER_RADIUS);
    visuals.widgets.noninteractive.corner_radius = radius;
    visuals.widgets.inactive.corner_radius = radius;
    visuals.widgets.hovered.corner_radius = radius;
    visuals.widgets.active.corner_radius = radius;
    visuals.widgets.open.corner_radius = radius;

    // Fluent-like buttons: light gray fill, subtle border, accent on press.
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(0xf0, 0xf0, 0xf0);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xf0, 0xf0, 0xf0);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xd0, 0xd0, 0xd0));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0xe6, 0xe6, 0xe6);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xe6, 0xe6, 0xe6);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xc0, 0xc0, 0xc0));
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    style.visuals = visuals;
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.slider_width = 160.0;

    ctx.set_style_of(egui::Theme::Light, style);
}

/// Returns a button styled as "primary" (accent-filled) when `selected`,
/// mirroring the "primary" button styling used by switches.
pub fn selectable_button(text: &str, selected: bool) -> egui::Button<'static> {
    let button = egui::Button::new(egui::RichText::new(text.to_owned()).color(if selected {
        Color32::WHITE
    } else {
        Color32::from_rgb(0x20, 0x20, 0x20)
    }));
    if selected {
        button.fill(ACCENT)
    } else {
        button.fill(Color32::from_rgb(0xf0, 0xf0, 0xf0))
    }
}

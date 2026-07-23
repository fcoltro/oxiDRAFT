//! The app's dark "glass panel" visual theme: shared colors, spacing/radius
//! tokens, and frame/style builders applied once at startup via [`apply`].

use egui::{Color32, Context, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

/// Spacing, corner-radius, and font-size tokens shared across the UI so
/// panels and widgets stay visually consistent.
pub mod tok {
    pub const SP_2: f32 = 6.0;
    pub const SP_3: f32 = 8.0;
    pub const R_SM: u8 = 8;
    pub const R_MD: u8 = 11;
    pub const R_LG: u8 = 16;
    pub const T_XS: f32 = 11.0;
    // Whole pixel values only: egui/ab_glyph rasterizes each font size into
    // its own texture-atlas entry, and a fractional size (e.g. the old 12.5)
    // forces every glyph edge through extra sub-pixel antialiasing at 100%
    // display scaling — the single biggest source of "slightly blurry" text.
    pub const T_SM: f32 = 13.0;
    pub const T_LG: f32 = 15.0;
}
/// The canvas's background color, behind all drawn geometry.
pub const CANVAS_BG: Color32 = Color32::from_rgb(10, 12, 16);
/// Opaque panel background (menus, dialogs).
pub const PANEL_BG: Color32 = Color32::from_rgb(20, 25, 36);
/// Translucent panel background used by [`glass`] frames (toolbars, HUDs).
pub const PANEL_GLASS: Color32 = Color32::from_rgba_premultiplied(15, 19, 29, 222);
/// Fill for an inactive widget.
pub const WIDGET_BG: Color32 = Color32::from_rgba_premultiplied(12, 12, 12, 12);
/// Fill for a hovered widget.
pub const WIDGET_HOVER: Color32 = Color32::from_rgba_premultiplied(22, 22, 22, 22);
/// The app's accent color, used for selection and active-state highlights.
pub const ACCENT: Color32 = Color32::from_rgb(48, 149, 255);
/// A brighter variant of [`ACCENT`] for hyperlinks and high-emphasis text.
pub const ACCENT_BRIGHT: Color32 = Color32::from_rgb(120, 185, 255);
/// A dim, translucent variant of [`ACCENT`] for subtle fills (selection background).
pub const ACCENT_DIM: Color32 = Color32::from_rgba_premultiplied(10, 30, 52, 52);
/// Color for snap indicators on the canvas.
pub const SNAP: Color32 = Color32::from_rgb(43, 233, 127);
/// Color for live drawing previews; matches [`SNAP`] so in-progress geometry
/// reads as one visual category.
pub const PREVIEW: Color32 = SNAP;
/// Color for inference/alignment guide lines.
pub const GUIDE: Color32 = Color32::from_rgb(255, 90, 160);
/// Status-indicator color for a healthy/positive state (e.g. fully constrained).
pub const STATUS_GREEN: Color32 = Color32::from_rgb(55, 211, 153);
/// Status-indicator color for a warning state (e.g. under-constrained).
pub const STATUS_AMBER: Color32 = Color32::from_rgb(245, 185, 74);
/// Status-indicator color for an error state (e.g. over-constrained/conflicting).
pub const STATUS_RED: Color32 = Color32::from_rgb(240, 96, 96);
/// Muted color for small HUD labels.
pub const HUD_LABEL: Color32 = Color32::from_gray(170);
/// Color for construction/control lines (e.g. grip handles).
pub const CONTROL_LINE: Color32 = Color32::from_rgb(120, 140, 170);
/// Primary text color.
pub const TEXT: Color32 = Color32::from_rgb(233, 239, 248);
/// Secondary/muted text color.
pub const TEXT_DIM: Color32 = Color32::from_rgb(140, 152, 172);
/// Hairline stroke color for panel and widget outlines.
pub const OUTLINE: Color32 = Color32::from_rgba_premultiplied(16, 16, 16, 16);

/// A translucent "glass" panel frame with a soft drop shadow, at the given
/// corner radius — the base look for toolbars, HUDs, and floating panels.
pub fn glass(radius: u8) -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL_GLASS)
        .stroke(Stroke::new(1.0, OUTLINE))
        .corner_radius(CornerRadius::same(radius))
        .inner_margin(egui::Margin::same(8))
        .shadow(egui::epaint::Shadow {
            offset: [0, 10],
            blur: 38,
            spread: 0,
            color: Color32::from_black_alpha(110),
        })
}

/// A faint, semi-transparent red glass — for passive alert-style toasts
/// (command feedback) that should read as "notice" without the heavier
/// opaque panel look of [`glass`].
pub fn toast_alert(radius: u8) -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(120, 40, 40, 60))
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(240, 96, 96, 70),
        ))
        .corner_radius(CornerRadius::same(radius))
        .inner_margin(egui::Margin::same(8))
        .shadow(egui::epaint::Shadow {
            offset: [0, 10],
            blur: 38,
            spread: 0,
            color: Color32::from_black_alpha(110),
        })
}

/// Applies the app's dark glass theme (visuals, spacing, text styles) to the
/// egui context. Called once at startup.
pub fn apply(ctx: &Context) {
    let mut v = Visuals::dark();
    v.panel_fill = CANVAS_BG;
    v.window_fill = PANEL_GLASS;
    v.extreme_bg_color = Color32::from_rgba_unmultiplied(255, 255, 255, 12);
    v.faint_bg_color = WIDGET_BG;
    v.window_stroke = Stroke::new(1.0, OUTLINE);
    v.window_corner_radius = CornerRadius::same(tok::R_LG);
    v.menu_corner_radius = CornerRadius::same(tok::R_MD);
    let panel_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 34,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    v.window_shadow = panel_shadow;
    v.popup_shadow = panel_shadow;
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT_BRIGHT;
    v.override_text_color = Some(TEXT);
    let r = CornerRadius::same(tok::R_SM);
    v.widgets.noninteractive.bg_fill = PANEL_GLASS;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, OUTLINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.bg_fill = WIDGET_BG;
    v.widgets.inactive.weak_bg_fill = WIDGET_BG;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.bg_fill = WIDGET_HOVER;
    v.widgets.hovered.weak_bg_fill = WIDGET_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_DIM);
    v.widgets.hovered.fg_stroke = Stroke::new(1.2, Color32::WHITE);
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.2, Color32::WHITE);
    v.widgets.active.corner_radius = r;
    v.widgets.open.bg_fill = WIDGET_HOVER;
    v.widgets.open.weak_bg_fill = WIDGET_HOVER;
    v.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT_DIM);
    v.widgets.open.corner_radius = r;
    ctx.set_visuals(v);
    let heading_font = crate::fonts::strong_font_id(ctx, tok::T_LG);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(tok::SP_2, 5.0);
        s.spacing.button_padding = egui::vec2(7.0, 4.0);
        s.spacing.menu_margin = egui::Margin::same(tok::SP_3 as i8);
        s.interaction.tooltip_delay = 0.45;
        s.interaction.tooltip_grace_time = 0.25;
        s.text_styles = [
            (
                TextStyle::Small,
                FontId::new(tok::T_XS, FontFamily::Proportional),
            ),
            (
                TextStyle::Body,
                FontId::new(tok::T_SM, FontFamily::Proportional),
            ),
            (
                TextStyle::Button,
                FontId::new(tok::T_SM, FontFamily::Proportional),
            ),
            (TextStyle::Heading, heading_font),
            (
                TextStyle::Monospace,
                FontId::new(tok::T_SM, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}

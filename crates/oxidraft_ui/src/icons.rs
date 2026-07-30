//! Toolbar and UI icons, drawn as glyphs from the bundled icon font (see
//! [`crate::fonts::ICON_FAMILY`]), plus the app window/taskbar icon.
//!
//! Each [`Icon`] names a codepoint in the font's private-use block rather than
//! a bitmap. That means one font in the atlas instead of ~70 tinted GPU
//! textures, crisp edges at any DPI or zoom instead of a bilinear stretch of a
//! fixed-size PNG, and colour that comes from the paint call — the old
//! pipeline had to bake white art and tint it.

use egui::{Align2, Color32, Rect, Response, Sense, Stroke, Ui, Vec2, pos2};
/// One toolbar or UI icon, backed by a glyph in the bundled icon font.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Select,
    Point,
    Line,
    Circle,
    Circle2P,
    Circle3P,
    CircleTtr,
    CircleTtt,
    Ellipse,
    Arc,
    ArcStartCenterEnd,
    ArcCenterStartEnd,
    Rectangle,
    Polygon,
    Spline,
    Polyline,
    Text,
    Move,
    Copy,
    Rotate,
    Scale,
    Mirror,
    Offset,
    Trim,
    Extend,
    Fillet,
    Chamfer,
    Blend,
    Stretch,
    Explode,
    Join,
    Hatch,
    Undo,
    Redo,
    Eye,
    EyeOff,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    Pan,
    AddLayer,
    Delete,
    Plus,
    Minus,
    Dimension,
    DimAngle,
    DimRadius,
    DimDiameter,
    ConHorizontal,
    ConVertical,
    ConParallel,
    ConPerpendicular,
    ConEqual,
    ConTangent,
    ConRadiusLock,
    ConLengthLock,
    ConAngle,
    ConCoincident,
    ConConcentric,
    ConFix,
    ConRemove,
    ConstAuto,
    ConstAutoOff,
    ConstShowHide,
    ConstShowHideOff,
    CurvComb,
}

impl Icon {
    /// Every icon, so tests can walk the whole set.
    pub const ALL: &'static [Icon] = &[
        Icon::Select,
        Icon::Point,
        Icon::Line,
        Icon::Polyline,
        Icon::Circle,
        Icon::Circle2P,
        Icon::Circle3P,
        Icon::CircleTtr,
        Icon::CircleTtt,
        Icon::Ellipse,
        Icon::Arc,
        Icon::ArcStartCenterEnd,
        Icon::ArcCenterStartEnd,
        Icon::Rectangle,
        Icon::Polygon,
        Icon::Spline,
        Icon::Text,
        Icon::Hatch,
        Icon::Blend,
        Icon::Move,
        Icon::Rotate,
        Icon::Scale,
        Icon::Mirror,
        Icon::Copy,
        Icon::Offset,
        Icon::Trim,
        Icon::Extend,
        Icon::Fillet,
        Icon::Chamfer,
        Icon::Stretch,
        Icon::Join,
        Icon::Explode,
        Icon::Dimension,
        Icon::DimAngle,
        Icon::DimRadius,
        Icon::DimDiameter,
        Icon::ConHorizontal,
        Icon::ConVertical,
        Icon::ConParallel,
        Icon::ConPerpendicular,
        Icon::ConEqual,
        Icon::ConTangent,
        Icon::ConCoincident,
        Icon::ConConcentric,
        Icon::ConFix,
        Icon::ConRadiusLock,
        Icon::ConLengthLock,
        Icon::ConAngle,
        Icon::ConRemove,
        Icon::ConstAuto,
        Icon::ConstAutoOff,
        Icon::Eye,
        Icon::ConstShowHide,
        Icon::EyeOff,
        Icon::ConstShowHideOff,
        Icon::CurvComb,
        Icon::Undo,
        Icon::Redo,
        Icon::ZoomIn,
        Icon::ZoomOut,
        Icon::ZoomFit,
        Icon::Pan,
        Icon::AddLayer,
        Icon::Delete,
        Icon::Plus,
        Icon::Minus,
    ];

    /// The icon's codepoint in the font's private-use block.
    ///
    /// Several variants deliberately share a glyph: the font draws one mark per
    /// *concept*, while `Icon` distinguishes every command that needs its own
    /// button. Where a command has no mark of its own, the arm says so.
    fn glyph(self) -> char {
        match self {
            // Tools.
            Icon::Select => '\u{f009}', // ui_selection
            Icon::Point => '\u{f027}',  // tool_point
            Icon::Line => '\u{f028}',   // tool_line
            // No polyline mark; a polyline is a chain of lines.
            Icon::Polyline => '\u{f028}', // tool_line
            // Every circle construction shows the circle it builds.
            Icon::Circle | Icon::Circle2P | Icon::Circle3P | Icon::CircleTtr | Icon::CircleTtt => {
                '\u{f02c}'
            } // tool_circle
            Icon::Ellipse => '\u{f02a}', // tool_elipse
            // ...and likewise for the arc constructions.
            Icon::Arc | Icon::ArcStartCenterEnd | Icon::ArcCenterStartEnd => '\u{f02e}', // tool_arc
            Icon::Rectangle => '\u{f025}', // tool_rectangle
            Icon::Polygon => '\u{f026}',   // tool_polygon
            Icon::Spline => '\u{f024}',    // tool_spline
            Icon::Text => '\u{f023}',      // tool_text
            Icon::Hatch => '\u{f029}',     // tool_hatch
            Icon::Blend => '\u{f02d}',     // tool_blend_curve

            // Transforms.
            Icon::Move => '\u{f021}',   // transform_move
            Icon::Rotate => '\u{f020}', // transform_rotate
            Icon::Scale => '\u{f01f}',  // transform_scale
            Icon::Mirror => '\u{f022}', // transform_mirror

            // Edits.
            Icon::Copy => '\u{f036}',   // edit_copy
            Icon::Offset => '\u{f032}', // edit_offset
            Icon::Trim => '\u{f02f}',   // edit_trim
            Icon::Extend => '\u{f035}', // edit_extend
            Icon::Fillet => '\u{f034}', // edit_fillet
            // No chamfer mark; it is the same corner operation as a fillet.
            Icon::Chamfer => '\u{f034}', // edit_fillet
            Icon::Stretch => '\u{f030}', // edit_stretch
            Icon::Join => '\u{f033}',    // edit_join
            // Exploding a compound splits it into its parts.
            Icon::Explode => '\u{f031}', // edit_split

            // Dimensions: the font carries one dimension mark, not one per kind.
            Icon::Dimension | Icon::DimAngle | Icon::DimRadius | Icon::DimDiameter => '\u{f02b}', // tool_dimension

            // Constraints.
            Icon::ConHorizontal => '\u{f03d}', // const_horizontal
            Icon::ConVertical => '\u{f037}',   // const_vertical
            Icon::ConParallel => '\u{f03c}',   // const_parallel
            Icon::ConPerpendicular => '\u{f03b}', // const_perpendicular
            Icon::ConEqual => '\u{f03f}',      // const_equal
            Icon::ConTangent => '\u{f038}',    // const_tangent
            Icon::ConCoincident => '\u{f042}', // const_coincident
            Icon::ConConcentric => '\u{f040}', // const_concentric
            Icon::ConFix => '\u{f03e}',        // const_fix
            // A locked radius and a locked length are both a padlock.
            Icon::ConRadiusLock | Icon::ConLengthLock => '\u{f00e}', // ui_locked
            Icon::ConAngle => '\u{f03a}',                            // const_smart_dimension
            // Dropping a constraint is a delete.
            Icon::ConRemove => '\u{f01a}', // ui_delete

            // Toggles and chrome.
            Icon::ConstAuto => '\u{f01d}',    // ui_auto_contrain_on
            Icon::ConstAutoOff => '\u{f01e}', // ui_auto_contrain_off
            Icon::Eye | Icon::ConstShowHide => '\u{f007}', // ui_show
            Icon::EyeOff | Icon::ConstShowHideOff => '\u{f008}', // ui_hide
            Icon::CurvComb => '\u{f01b}',     // ui_curv_comb_on
            Icon::Undo => '\u{f005}',         // ui_undo
            Icon::Redo => '\u{f00a}',         // ui_redo
            Icon::ZoomIn => '\u{f001}',       // ui_zoom_in
            Icon::ZoomOut => '\u{f000}',      // ui_zoom_out
            Icon::ZoomFit => '\u{f002}',      // ui_zoom_extents
            Icon::Pan => '\u{f00c}',          // ui_pan
            Icon::AddLayer => '\u{f011}',     // ui_layer_add
            Icon::Delete => '\u{f01a}',       // ui_delete
            Icon::Plus => '\u{f00b}',         // ui_plus
            Icon::Minus => '\u{f00d}',        // ui_minus
        }
    }
}

/// Draws `icon` in `tint`, centred in `rect`.
///
/// `rect`'s height is used as the font size, which is the same thing as the
/// icon's design size: the glyphs were built on a 24-unit grid at 1024 units
/// per em, so an em box of N pixels reproduces the source artwork at N pixels.
/// Each glyph's ink is centred on its line box, so `CENTER_CENTER` seats the
/// mark without a per-glyph nudge.
pub fn paint_icon(
    painter: &egui::Painter,
    ctx: &egui::Context,
    icon: Icon,
    rect: Rect,
    tint: Color32,
) {
    let Some(font) = crate::fonts::icon_font_id(ctx, rect.height()) else {
        return;
    };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon.glyph(),
        font,
        tint,
    );
}

fn decode_png(bytes: &[u8]) -> Option<resvg::tiny_skia::Pixmap> {
    resvg::tiny_skia::Pixmap::decode_png(bytes).ok()
}

fn scaled_pixmap_from_png(bytes: &[u8], w: u32, h: u32) -> Option<resvg::tiny_skia::Pixmap> {
    let src = decode_png(bytes)?;
    if src.width() == w && src.height() == h {
        return Some(src);
    }
    let mut out = resvg::tiny_skia::Pixmap::new(w, h)?;
    let scale = (w as f32 / src.width() as f32).min(h as f32 / src.height() as f32);
    let tx = (w as f32 - src.width() as f32 * scale) * 0.5;
    let ty = (h as f32 - src.height() as f32 * scale) * 0.5;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);
    let paint = resvg::tiny_skia::PixmapPaint {
        quality: resvg::tiny_skia::FilterQuality::Bilinear,
        ..Default::default()
    };
    out.draw_pixmap(0, 0, src.as_ref(), &paint, transform, None);
    Some(out)
}

/// The application window icon at 256×256, straight-alpha RGBA (egui's
/// `IconData` expects unpremultiplied color, unlike the rest of this module's
/// premultiplied textures — hence the unpremultiply pass below).
pub fn app_icon() -> egui::IconData {
    const SIZE: u32 = 256;
    let png = include_bytes!("../assets/logotype/oxidraft_symbol.png");
    match scaled_pixmap_from_png(png, SIZE, SIZE) {
        Some(pixmap) => {
            let mut rgba = pixmap.data().to_vec();
            for px in rgba.chunks_exact_mut(4) {
                let a = px[3] as u32;
                if a > 0 && a < 255 {
                    px[0] = (px[0] as u32 * 255 / a).min(255) as u8;
                    px[1] = (px[1] as u32 * 255 / a).min(255) as u8;
                    px[2] = (px[2] as u32 * 255 / a).min(255) as u8;
                }
            }
            egui::IconData {
                rgba,
                width: SIZE,
                height: SIZE,
            }
        }
        None => egui::IconData {
            rgba: vec![0; 4],
            width: 1,
            height: 1,
        },
    }
}

/// The app logo mark, scaled to `size`×`size` and re-encoded as PNG bytes.
pub fn app_icon_png(size: u32) -> Option<Vec<u8>> {
    let png = include_bytes!("../assets/logotype/oxidraft_symbol.png");
    scaled_pixmap_from_png(png, size, size)?.encode_png().ok()
}

/// The executable's Windows icon: the authored `.ico`, passed through as-is.
///
/// This used to downscale one 256px PNG into seven sizes and hand-assemble the
/// container. Bilinear downscaling is the wrong tool for a 16px icon — strokes
/// drawn for 256px turn to mush — so the mark is now authored per size in
/// Illustrator and exported as a real `.ico` (8 images, 16 to 256, 32bpp PNG).
/// Returning `Option` keeps the signature its callers already expect.
pub fn app_icon_ico() -> Option<Vec<u8>> {
    const ICO: &[u8] = include_bytes!("../assets/logotype/oxidraft_symbol.ico");
    Some(ICO.to_vec())
}

/// The app's logotype as a GPU texture, loaded and cached on first use.
pub fn logo_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let id = egui::Id::new("oxidraft_logo_tex");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    let png = include_bytes!("../assets/logotype/oxidraft_logotype.png");
    let pixmap = decode_png(png)?;
    let image = egui::ColorImage::from_rgba_premultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        pixmap.data(),
    );
    let tex = ctx.load_texture("oxidraft_logo", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

const ICON_SIZE: f32 = 30.0;
const GLYPH_PX: f32 = 24.0;

/// A clickable icon button at the default toolbar size, with hover/active
/// highlight animation and a rich tooltip.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str, active: bool) -> Response {
    icon_button_sized(ui, icon, tooltip, active, ICON_SIZE)
}

/// Like [`icon_button`], at an explicit pixel size.
pub fn icon_button_sized(
    ui: &mut Ui,
    icon: Icon,
    tooltip: &str,
    active: bool,
    size: f32,
) -> Response {
    let (raw_rect, mut response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered() && ui.is_enabled();
    let ppp = ui.ctx().pixels_per_point();
    let rect = snap_rect(raw_rect, ppp);
    let anim = ui.ctx().animate_bool(response.id, hovered);
    let act = ui
        .ctx()
        .animate_bool(response.id.with("active"), active && ui.is_enabled());

    let radius = (size * 0.27).round().clamp(8.0, 13.0);
    let painter = ui.painter_at(rect);
    if anim > 0.001 && act < 0.5 {
        painter.rect_filled(
            rect,
            radius,
            crate::theme::WIDGET_HOVER.gamma_multiply(anim * 0.9),
        );
    }
    if act > 0.001 {
        painter.rect(
            rect,
            radius,
            crate::theme::ACCENT.gamma_multiply(0.18 * act),
            Stroke::new(1.0, crate::theme::ACCENT.gamma_multiply(0.55 * act)),
            egui::StrokeKind::Inside,
        );
    }

    let tint = if ui.is_enabled() {
        Color32::WHITE
    } else {
        Color32::WHITE.gamma_multiply(0.4)
    };
    let glyph = GLYPH_PX.min(size - 4.0).max(8.0);
    let area = snap_rect(
        Rect::from_center_size(rect.center(), Vec2::splat(glyph)),
        ppp,
    );
    paint_icon(&painter, ui.ctx(), icon, area, tint);

    if hovered {
        response = response.on_hover_ui(|ui| rich_tooltip(ui, tooltip));
    }
    response
}

pub(crate) fn rich_tooltip(ui: &mut Ui, text: &str) {
    let (head, keys) = match (text.find('('), text.find(')')) {
        (Some(o), Some(c)) if c > o => (
            text[..o].trim().to_string(),
            Some(text[o + 1..c].trim().to_string()),
        ),
        _ => (text.to_string(), None),
    };
    let desc = text
        .find(')')
        .map(|c| text[c + 1..].trim())
        .unwrap_or("")
        .trim_start_matches('—')
        .trim()
        .to_string();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 7.0;
        ui.label(
            egui::RichText::new(head)
                .font(crate::fonts::strong_font_id(
                    ui.ctx(),
                    crate::theme::tok::T_SM,
                ))
                .color(crate::theme::TEXT),
        );
        if let Some(k) = keys.filter(|k| !k.is_empty()) {
            let galley = ui.painter().layout_no_wrap(
                k.clone(),
                egui::FontId::monospace(11.0),
                crate::theme::TEXT,
            );
            let pad = Vec2::new(5.0, 2.0);
            let size = galley.size() + pad * 2.0;
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter().rect(
                rect,
                4.0,
                Color32::from_rgba_unmultiplied(40, 48, 64, 235),
                Stroke::new(1.0, crate::theme::OUTLINE),
                egui::StrokeKind::Inside,
            );
            ui.painter()
                .galley(rect.min + pad, galley, crate::theme::TEXT);
        }
    });
    if !desc.is_empty() {
        ui.label(
            egui::RichText::new(desc)
                .size(12.0)
                .color(crate::theme::TEXT_DIM),
        );
    }
}

fn snap_rect(r: Rect, pixels_per_point: f32) -> Rect {
    let snap = |v: f32| (v * pixels_per_point).round() / pixels_per_point;
    Rect::from_min_max(
        pos2(snap(r.min.x), snap(r.min.y)),
        pos2(snap(r.max.x), snap(r.max.y)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON_FONT: &[u8] = include_bytes!("../assets/icons/icon.ttf");

    /// The glyph the font must hold at the codepoint each icon maps to.
    ///
    /// An exhaustive match on purpose: it is what makes it impossible to add an
    /// `Icon` and leave it out of the check below.
    fn expected_name(icon: Icon) -> &'static str {
        match icon {
            Icon::Select => "ui_selection",
            Icon::Point => "tool_point",
            Icon::Line => "tool_line",
            Icon::Polyline => "tool_line",
            Icon::Circle => "tool_circle",
            Icon::Circle2P => "tool_circle",
            Icon::Circle3P => "tool_circle",
            Icon::CircleTtr => "tool_circle",
            Icon::CircleTtt => "tool_circle",
            Icon::Ellipse => "tool_elipse",
            Icon::Arc => "tool_arc",
            Icon::ArcStartCenterEnd => "tool_arc",
            Icon::ArcCenterStartEnd => "tool_arc",
            Icon::Rectangle => "tool_rectangle",
            Icon::Polygon => "tool_polygon",
            Icon::Spline => "tool_spline",
            Icon::Text => "tool_text",
            Icon::Hatch => "tool_hatch",
            Icon::Blend => "tool_blend_curve",
            Icon::Move => "transform_move",
            Icon::Rotate => "transform_rotate",
            Icon::Scale => "transform_scale",
            Icon::Mirror => "transform_mirror",
            Icon::Copy => "edit_copy",
            Icon::Offset => "edit_offset",
            Icon::Trim => "edit_trim",
            Icon::Extend => "edit_extend",
            Icon::Fillet => "edit_fillet",
            Icon::Chamfer => "edit_fillet",
            Icon::Stretch => "edit_stretch",
            Icon::Join => "edit_join",
            Icon::Explode => "edit_split",
            Icon::Dimension => "tool_dimension",
            Icon::DimAngle => "tool_dimension",
            Icon::DimRadius => "tool_dimension",
            Icon::DimDiameter => "tool_dimension",
            Icon::ConHorizontal => "const_horizontal",
            Icon::ConVertical => "const_vertical",
            Icon::ConParallel => "const_parallel",
            Icon::ConPerpendicular => "const_perpendicular",
            Icon::ConEqual => "const_equal",
            Icon::ConTangent => "const_tangent",
            Icon::ConCoincident => "const_coincident",
            Icon::ConConcentric => "const_concentric",
            Icon::ConFix => "const_fix",
            Icon::ConRadiusLock => "ui_locked",
            Icon::ConLengthLock => "ui_locked",
            Icon::ConAngle => "const_smart_dimension",
            Icon::ConRemove => "ui_delete",
            Icon::ConstAuto => "ui_auto_contrain_on",
            Icon::ConstAutoOff => "ui_auto_contrain_off",
            Icon::Eye => "ui_show",
            Icon::ConstShowHide => "ui_show",
            Icon::EyeOff => "ui_hide",
            Icon::ConstShowHideOff => "ui_hide",
            Icon::CurvComb => "ui_curv_comb_on",
            Icon::Undo => "ui_undo",
            Icon::Redo => "ui_redo",
            Icon::ZoomIn => "ui_zoom_in",
            Icon::ZoomOut => "ui_zoom_out",
            Icon::ZoomFit => "ui_zoom_extents",
            Icon::Pan => "ui_pan",
            Icon::AddLayer => "ui_layer_add",
            Icon::Delete => "ui_delete",
            Icon::Plus => "ui_plus",
            Icon::Minus => "ui_minus",
        }
    }

    struct Ink(usize);
    impl ttf_parser::OutlineBuilder for Ink {
        fn move_to(&mut self, _: f32, _: f32) {
            self.0 += 1;
        }
        fn line_to(&mut self, _: f32, _: f32) {
            self.0 += 1;
        }
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
            self.0 += 1;
        }
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
            self.0 += 1;
        }
        fn close(&mut self) {}
    }

    #[test]
    fn every_icon_resolves_to_the_glyph_it_names() {
        // Codepoints in this font are handed out by the export tool, not chosen
        // by hand, so regenerating it can renumber the whole set — renaming one
        // source file is enough to shift everything after it. Nothing about
        // that failure is visible from Rust: the icons still draw, they are
        // just the wrong ones. Checking the glyph NAME behind each icon is what
        // turns a silently rearranged toolbar into a failing test.
        let face = ttf_parser::Face::parse(ICON_FONT, 0).expect("icon font should parse");
        for &icon in Icon::ALL {
            let want = expected_name(icon);
            let cp = icon.glyph() as u32;
            let gid = face
                .glyph_index(icon.glyph())
                .unwrap_or_else(|| panic!("no glyph at U+{cp:04X}, wanted {want}"));
            assert_eq!(
                face.glyph_name(gid),
                Some(want),
                "U+{cp:04X} now holds a different glyph; the font was \
                 regenerated and the mapping has to be regenerated with it"
            );
            let mut ink = Ink(0);
            face.outline_glyph(gid, &mut ink);
            assert!(
                ink.0 > 0,
                "{want} (U+{cp:04X}) has no outline — an export carrying \
                 neither a glyf nor a CFF table draws every icon blank"
            );
        }
    }

    #[test]
    fn all_lists_each_icon_exactly_once() {
        let mut seen: Vec<u8> = Icon::ALL.iter().map(|i| *i as u8).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), before, "an Icon appears twice in Icon::ALL");
    }
}

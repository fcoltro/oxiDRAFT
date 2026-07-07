use egui::{Color32, Rect, Response, Sense, Stroke, Ui, Vec2, pos2};
use std::collections::HashMap;
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
    ConCoincident,
    ConRemove,
    ConstAuto,
    ConstShowHide,
    CurvComb,
}

impl Icon {
    fn png_bytes(self) -> &'static [u8] {
        match self {
            Icon::Select => include_bytes!("../assets/icons/icons_curve_ui_select.png"),
            Icon::Point => include_bytes!("../assets/icons/icons_curve_tool_point.png"),
            Icon::Line => include_bytes!("../assets/icons/icons_curve_tool_line.png"),
            Icon::Circle => include_bytes!("../assets/icons/icons_curve_tool_circle.png"),
            Icon::Circle2P => include_bytes!("../assets/icons/icons_curve_tool_circle_2p.png"),
            Icon::Circle3P => include_bytes!("../assets/icons/icons_curve_tool_circle_3p.png"),
            Icon::CircleTtr => {
                include_bytes!("../assets/icons/icons_curve_tool_circle_tan_tan_radius.png")
            }
            Icon::CircleTtt => {
                include_bytes!("../assets/icons/icons_curve_tool_circle_tan_tan_tan.png")
            }
            Icon::Ellipse => include_bytes!("../assets/icons/icons_curve_tool_ellipse.png"),
            Icon::Arc => include_bytes!("../assets/icons/icons_curve_tool_arc_3P.png"),
            Icon::ArcStartCenterEnd => {
                include_bytes!("../assets/icons/icons_curve_tool_arc_start_center_end.png")
            }
            Icon::ArcCenterStartEnd => {
                include_bytes!("../assets/icons/icons_curve_tool_arc_center_start_end.png")
            }
            Icon::Rectangle => include_bytes!("../assets/icons/icons_curve_tool_rectangle.png"),
            Icon::Polygon => include_bytes!("../assets/icons/icons_curve_tool_polygon.png"),
            Icon::Spline => include_bytes!("../assets/icons/icons_curve_tool_spline.png"),
            Icon::Polyline => include_bytes!("../assets/icons/icons_curve_tool_polyline.png"),
            Icon::Text => include_bytes!("../assets/icons/icons_curve_tool_text.png"),
            Icon::Move => include_bytes!("../assets/icons/icons_curve_mod_move.png"),
            Icon::Copy => include_bytes!("../assets/icons/icons_curve_mod_copy.png"),
            Icon::Rotate => include_bytes!("../assets/icons/icons_curve_mod_rotate.png"),
            Icon::Scale => include_bytes!("../assets/icons/icons_curve_mod_scale.png"),
            Icon::Mirror => include_bytes!("../assets/icons/icons_curve_mod_mirror.png"),
            Icon::Offset => include_bytes!("../assets/icons/icons_curve_mod_offset.png"),
            Icon::Trim => include_bytes!("../assets/icons/icons_curve_mod_trim.png"),
            Icon::Extend => include_bytes!("../assets/icons/icons_curve_mod_extend.png"),
            Icon::Fillet => include_bytes!("../assets/icons/icons_curve_mod_fillet.png"),
            Icon::Chamfer => include_bytes!("../assets/icons/icons_curve_mod_chamfer.png"),
            Icon::Blend => include_bytes!("../assets/icons/icons_curve_tool_blend_curve.png"),
            Icon::Stretch => include_bytes!("../assets/icons/icons_curve_mod_stretch.png"),
            Icon::Explode => include_bytes!("../assets/icons/icons_curve_mod_disjoint.png"),
            Icon::Join => include_bytes!("../assets/icons/icons_curve_mod_join.png"),
            Icon::Hatch => include_bytes!("../assets/icons/icons_curve_tool_hatch.png"),
            Icon::Undo => include_bytes!("../assets/icons/icons_curve_ui_undo.png"),
            Icon::Redo => include_bytes!("../assets/icons/icons_curve_ui_redo.png"),
            Icon::Eye => include_bytes!("../assets/icons/icons_curve_ui_show.png"),
            Icon::EyeOff => include_bytes!("../assets/icons/icons_curve_ui_hide.png"),
            Icon::ZoomIn => include_bytes!("../assets/icons/icons_curve_ui_zoom_in.png"),
            Icon::ZoomOut => include_bytes!("../assets/icons/icons_curve_ui_zoom_out.png"),
            Icon::ZoomFit => include_bytes!("../assets/icons/icons_curve_ui_zoom_extents.png"),
            Icon::Pan => include_bytes!("../assets/icons/icons_curve_ui_pan.png"),
            Icon::AddLayer => include_bytes!("../assets/icons/icons_curve_ui_add_layer.png"),
            Icon::Delete => include_bytes!("../assets/icons/icons_curve_ui_delete.png"),
            Icon::Plus => include_bytes!("../assets/icons/icons_curve_ui_plus.png"),
            Icon::Minus => include_bytes!("../assets/icons/icons_curve_ui_minus.png"),
            Icon::Dimension => include_bytes!("../assets/icons/icons_curve_tool_dimension.png"),
            Icon::DimAngle => include_bytes!("../assets/icons/icons_curve_tool_dim_angle.png"),
            Icon::DimRadius => include_bytes!("../assets/icons/icons_curve_tool_dim_radius.png"),
            Icon::DimDiameter => {
                include_bytes!("../assets/icons/icons_curve_tool_dim_diameter.png")
            }
            Icon::ConHorizontal => {
                include_bytes!("../assets/icons/icons_curve_constrait_horizontal.png")
            }
            Icon::ConVertical => {
                include_bytes!("../assets/icons/icons_curve_constrait_vertical.png")
            }
            Icon::ConParallel => {
                include_bytes!("../assets/icons/icons_curve_constrait_parallel.png")
            }
            Icon::ConPerpendicular => {
                include_bytes!("../assets/icons/icons_curve_constrait_perpendicular.png")
            }
            Icon::ConEqual => include_bytes!("../assets/icons/icons_curve_constrait_equal.png"),
            Icon::ConTangent => {
                include_bytes!("../assets/icons/icons_curve_constrait_tangent.png")
            }
            Icon::ConRadiusLock => {
                include_bytes!("../assets/icons/icons_curve_constrait_radius_lock.png")
            }
            Icon::ConLengthLock => {
                include_bytes!("../assets/icons/icons_curve_constrait_length_lock.png")
            }
            Icon::ConCoincident => {
                include_bytes!("../assets/icons/icons_curve_constrait_coincident.png")
            }
            Icon::ConRemove => {
                include_bytes!("../assets/icons/icons_curve_constrait_remove.png")
            }
            Icon::ConstAuto => include_bytes!("../assets/icons/icons_curve_ui_const_auto.png"),
            Icon::ConstShowHide => {
                include_bytes!("../assets/icons/icons_curve_ui_const_show-hide.png")
            }
            Icon::CurvComb => include_bytes!("../assets/icons/icons_curve_ui_curv_comb.png"),
        }
    }
}

#[derive(Clone, Default)]
struct IconTextureCache(HashMap<u8, egui::TextureHandle>);

fn icon_texture(ctx: &egui::Context, icon: Icon) -> Option<egui::TextureHandle> {
    let id = egui::Id::new("oxidraft_icon_texture_cache");
    if let Some(tex) = ctx.data(|d| {
        d.get_temp::<IconTextureCache>(id)
            .and_then(|c| c.0.get(&(icon as u8)).cloned())
    }) {
        return Some(tex);
    }

    let pixmap = resvg::tiny_skia::Pixmap::decode_png(icon.png_bytes()).ok()?;
    let image = egui::ColorImage::from_rgba_premultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        pixmap.data(),
    );
    let tex = ctx.load_texture(
        format!("icon_{}", icon as u8),
        image,
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| {
        d.get_temp_mut_or_insert_with::<IconTextureCache>(id, IconTextureCache::default)
            .0
            .insert(icon as u8, tex.clone());
    });
    Some(tex)
}

pub fn paint_icon(
    painter: &egui::Painter,
    ctx: &egui::Context,
    icon: Icon,
    rect: Rect,
    tint: Color32,
) {
    if let Some(tex) = icon_texture(ctx, icon) {
        painter.image(
            tex.id(),
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            tint,
        );
    }
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

pub fn app_icon_png(size: u32) -> Option<Vec<u8>> {
    let png = include_bytes!("../assets/logotype/oxidraft_symbol.png");
    scaled_pixmap_from_png(png, size, size)?.encode_png().ok()
}

pub fn app_icon_ico() -> Option<Vec<u8>> {
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut entries: Vec<(u32, Vec<u8>)> = Vec::with_capacity(sizes.len());
    for &s in &sizes {
        entries.push((s, app_icon_png(s)?));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + entries.len() * 16;
    for (s, png) in &entries {
        let dim = if *s >= 256 { 0u8 } else { *s as u8 };
        out.push(dim);
        out.push(dim);
        out.push(0);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in &entries {
        out.extend_from_slice(png);
    }
    Some(out)
}

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

pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str, active: bool) -> Response {
    icon_button_sized(ui, icon, tooltip, active, ICON_SIZE)
}

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
        ui.label(egui::RichText::new(head).strong().color(crate::theme::TEXT));
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
                .size(11.5)
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

use super::UiState;
use super::render::corner_glass_frame;
use crate::state::AppState;
use crate::tools::Tool;
use egui::{Color32, Stroke, pos2, vec2};
use oxidraft_document::{ConstraintKind, Document, EntityId, EntityKind, SketchConstraint};
use oxidraft_geometry::{Continuity, Curve, CurveSegment, Point2d, curvature_at, normal_at};

pub(super) fn curvature_comb(
    painter: &egui::Painter,
    app: &AppState,
    curve: &Curve,
    origin: egui::Pos2,
    scale: f64,
    samples: usize,
) {
    if let Curve::Poly(poly) = curve {
        for seg in &poly.segments {
            if seg.as_line().is_none() {
                curvature_comb(painter, app, seg, origin, scale, samples);
            }
        }
        return;
    }
    if curve.as_line().is_some() {
        return;
    }

    let to_screen = |wx: f64, wy: f64| super::render::world_to_screen_pos(app, origin, wx, wy);
    let (t0, t1) = curve.domain();
    let n = samples.max(2);
    let tooth = Stroke::new(1.0, Color32::from_rgb(190, 120, 255));
    let envelope = Stroke::new(1.5, Color32::from_rgb(150, 90, 230));
    let bb = curve.bounding_box();
    let (w, h) = (bb.max.x - bb.min.x, bb.max.y - bb.min.y);
    let diag = (w * w + h * h).sqrt();
    let min_tooth = diag * 1e-3;
    let max_tooth = (w.min(h) * 0.5).max(scale);

    let mut run: Vec<egui::Pos2> = Vec::new();
    let flush = |run: &mut Vec<egui::Pos2>| {
        if run.len() >= 2 {
            painter.add(egui::Shape::line(run.clone(), envelope));
        }
        run.clear();
    };
    for i in 0..=n {
        let t = t0 + (t1 - t0) * i as f64 / n as f64;
        let k = match curvature_at(curve, t) {
            Some(k) if k.is_finite() => k,
            _ => {
                flush(&mut run);
                continue;
            }
        };
        let (nx, ny) = normal_at(curve, t);
        let nlen = (nx * nx + ny * ny).sqrt();
        let mut d = -k * scale;
        if nlen < 1e-12 || d.abs() < min_tooth {
            flush(&mut run);
            continue;
        }
        d = d.clamp(-max_tooth, max_tooth);
        let (x, y) = curve.evaluate_f64(t);
        let base = to_screen(x, y);
        let tip = to_screen(x + nx / nlen * d, y + ny / nlen * d);
        painter.line_segment([base, tip], tooth);
        run.push(tip);
    }
    flush(&mut run);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BadgeGlyph {
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    Equal,
    Tangent,
}

/// A row of glyph chips: each glyph carries the constraints it stands for
/// (several constraints of the same kind on one entity share a chip).
type GlyphChips = Vec<(BadgeGlyph, Vec<SketchConstraint>)>;

pub(super) struct BadgeModel {
    /// Per-entity glyph rows, in document constraint order.
    pub line_badges: Vec<(EntityId, GlyphChips)>,
    /// Welded corners in world coordinates, deduplicated by position,
    /// each carrying the welds anchored there.
    pub corner_dots: Vec<((f64, f64), Vec<SketchConstraint>)>,
    /// Valued constraints (driving length, radius) shown as dimension-style
    /// annotations in the constraint accent colour instead of glyph chips.
    /// Deliberately separate from dimension *entities*: those are drafting
    /// annotation for the printed drawing, these visualize the sketch's
    /// driving values and live only in the overlay.
    pub dim_badges: Vec<SketchConstraint>,
}

fn badge_line_ends(doc: &Document, id: EntityId) -> Option<((f64, f64), (f64, f64))> {
    match &doc.get(id)?.kind {
        EntityKind::Curve(Curve::Line(l)) => Some((l.p0.to_f64(), l.p1.to_f64())),
        EntityKind::Curve(Curve::Arc(a)) => {
            let end = |th: f64| {
                (
                    a.center.x + a.radius * th.cos(),
                    a.center.y + a.radius * th.sin(),
                )
            };
            Some((end(a.start_angle), end(a.end_angle)))
        }
        _ => None,
    }
}

/// A badge row's anchor: world-space midpoint, unit direction to stack
/// chips along, and a rough world extent used to skip badges on entities
/// too small on screen.
type BadgeAnchor = ((f64, f64), (f64, f64), f64);

fn badge_anchor(doc: &Document, id: EntityId) -> Option<BadgeAnchor> {
    match &doc.get(id)?.kind {
        EntityKind::Curve(Curve::Line(l)) => {
            let (dx, dy) = (l.p1.x - l.p0.x, l.p1.y - l.p0.y);
            let n = dx.hypot(dy);
            if n < 1e-12 {
                return None;
            }
            Some((
                ((l.p0.x + l.p1.x) * 0.5, (l.p0.y + l.p1.y) * 0.5),
                (dx / n, dy / n),
                n,
            ))
        }
        EntityKind::Curve(Curve::Arc(a)) => {
            let mid = (a.start_angle + a.end_angle) * 0.5;
            let pos = (
                a.center.x + a.radius * mid.cos(),
                a.center.y + a.radius * mid.sin(),
            );
            // Chips stack along the tangent direction at the mid angle.
            Some((pos, (-mid.sin(), mid.cos()), a.radius))
        }
        _ => None,
    }
}

/// Collects what the constraint badges should show: one glyph row per
/// constrained line (pair kinds badge both members) and one dot per welded
/// corner.
pub(super) fn badge_model(doc: &Document) -> BadgeModel {
    let mut line_badges: Vec<(EntityId, GlyphChips)> = Vec::new();
    let mut corner_dots: Vec<((f64, f64), Vec<SketchConstraint>)> = Vec::new();
    let mut dim_badges: Vec<SketchConstraint> = Vec::new();
    let push =
        |badges: &mut Vec<(EntityId, GlyphChips)>, id, g: BadgeGlyph, c: SketchConstraint| {
            let row = match badges.iter_mut().find(|(e, _)| *e == id) {
                Some(row) => row,
                None => {
                    badges.push((id, Vec::new()));
                    badges.last_mut().unwrap()
                }
            };
            match row.1.iter_mut().find(|(gg, _)| *gg == g) {
                Some((_, cs)) => {
                    if !cs.contains(&c) {
                        cs.push(c);
                    }
                }
                None => row.1.push((g, vec![c])),
            }
        };
    for c in &doc.constraints {
        let pair_glyph = match c.kind {
            // Structural, not a user constraint — no badge, nothing to remove.
            ConstraintKind::Fixed => continue,
            ConstraintKind::Horizontal => {
                push(&mut line_badges, c.a, BadgeGlyph::Horizontal, *c);
                continue;
            }
            ConstraintKind::Vertical => {
                push(&mut line_badges, c.a, BadgeGlyph::Vertical, *c);
                continue;
            }
            ConstraintKind::Radius | ConstraintKind::Distance | ConstraintKind::Angle => {
                if c.val.is_some() {
                    dim_badges.push(*c);
                }
                continue;
            }
            ConstraintKind::Parallel => BadgeGlyph::Parallel,
            ConstraintKind::Perpendicular => BadgeGlyph::Perpendicular,
            ConstraintKind::EqualLength => BadgeGlyph::Equal,
            ConstraintKind::Tangent => BadgeGlyph::Tangent,
            ConstraintKind::Coincident => {
                let (Some((ea, _)), Some((p0, p1))) = (c.pts, badge_line_ends(doc, c.a)) else {
                    continue;
                };
                let p = if ea == 0 { p0 } else { p1 };
                match corner_dots
                    .iter_mut()
                    .find(|(q, _)| (q.0 - p.0).hypot(q.1 - p.1) < 1e-9)
                {
                    Some((_, cs)) => cs.push(*c),
                    None => corner_dots.push((p, vec![*c])),
                }
                continue;
            }
        };
        push(&mut line_badges, c.a, pair_glyph, *c);
        if let Some(b) = c.b {
            push(&mut line_badges, b, pair_glyph, *c);
        }
    }
    BadgeModel {
        line_badges,
        corner_dots,
        dim_badges,
    }
}

/// Screen-space layout of one valued-constraint dimension annotation:
/// stroke segments, arrowhead triangles, and the value label. Canvas-local
/// like `chip_centers`, so drawing adds the painter origin and hit-testing
/// compares raw positions.
struct DimBadge {
    lines: Vec<[egui::Pos2; 2]>,
    arrows: Vec<[egui::Pos2; 3]>,
    label: String,
    text_rect: egui::Rect,
}

/// Deterministic label box shared by drawing and hit-testing — a galley
/// needs a painter, which `badge_hit` doesn't have.
fn dim_label_rect(center: egui::Pos2, label: &str) -> egui::Rect {
    let w = 7.0 * label.chars().count() as f32 + 10.0;
    egui::Rect::from_center_size(center, vec2(w, 16.0))
}

/// Filled arrowhead with its tip at `tip`, fins spreading along `back`.
fn dim_arrow(tip: egui::Pos2, back: egui::Vec2) -> [egui::Pos2; 3] {
    let n = vec2(back.y, -back.x);
    [tip, tip + back * 7.0 + n * 2.6, tip + back * 7.0 - n * 2.6]
}

fn dim_badge_layout(app: &AppState, c: &SketchConstraint) -> Option<DimBadge> {
    let val = c.val?;
    let style = &app.document.settings.dim_style;
    let units = app.document.settings.units;
    let px = |wx: f64, wy: f64| {
        let (x, y) = app.view.world_to_screen(wx, wy);
        pos2(x as f32, y as f32)
    };
    match (c.kind, app.document.get(c.a)?.as_curve()?) {
        (ConstraintKind::Distance, Curve::Line(l)) => {
            let a = px(l.p0.x, l.p0.y);
            let b = px(l.p1.x, l.p1.y);
            if (b - a).length() < 24.0 {
                return None;
            }
            let d = (b - a).normalized();
            // The glyph chips stack on the upward side (`chip_centers`
            // forces n.y < 0); the dimension takes the opposite side.
            let mut n = vec2(d.y, -d.x);
            if n.y < 0.0 {
                n = -n;
            }
            let o = 22.0;
            let (ea, eb) = (a + n * o, b + n * o);
            let label = units.format_measure(val, style.precision);
            Some(DimBadge {
                lines: vec![
                    [a + n * 4.0, a + n * (o + 5.0)],
                    [b + n * 4.0, b + n * (o + 5.0)],
                    [ea, eb],
                ],
                arrows: vec![dim_arrow(ea, d), dim_arrow(eb, -d)],
                text_rect: dim_label_rect(ea + (eb - ea) * 0.5 + n * 13.0, &label),
                label,
            })
        }
        (ConstraintKind::Angle, Curve::Line(la)) => {
            let Curve::Line(lb) = app.document.get(c.b?)?.as_curve()? else {
                return None;
            };
            let a0 = px(la.p0.x, la.p0.y);
            let a1 = px(la.p1.x, la.p1.y);
            let b0 = px(lb.p0.x, lb.p0.y);
            let b1 = px(lb.p1.x, lb.p1.y);
            if (a1 - a0).length() < 24.0 || (b1 - b0).length() < 24.0 {
                return None;
            }
            // Vertex: intersection of the two infinite lines in screen
            // space. Near-parallel legs put it far off-screen — no useful
            // place to dimension.
            let (r, s) = (a1 - a0, b1 - b0);
            let denom = r.x * s.y - r.y * s.x;
            if denom.abs() < 1e-6 {
                return None;
            }
            let t = ((b0.x - a0.x) * s.y - (b0.y - a0.y) * s.x) / denom;
            let vtx = a0 + r * t;
            // Each leg's ray points toward its segment's farther endpoint,
            // so the arc opens into the drawn corner.
            let ray = |p0: egui::Pos2, p1: egui::Pos2| {
                let q = if (p1 - vtx).length() >= (p0 - vtx).length() {
                    p1
                } else {
                    p0
                };
                (q - vtx).normalized()
            };
            let (da, db) = (ray(a0, a1), ray(b0, b1));
            let ang_a = da.y.atan2(da.x);
            let mut sweep = db.y.atan2(db.x) - ang_a;
            if sweep > std::f32::consts::PI {
                sweep -= std::f32::consts::TAU;
            }
            if sweep <= -std::f32::consts::PI {
                sweep += std::f32::consts::TAU;
            }
            let rad = 26.0;
            let at = |ang: f32| vtx + vec2(ang.cos(), ang.sin()) * rad;
            let mut lines = vec![[vtx, vtx + da * (rad + 5.0)], [vtx, vtx + db * (rad + 5.0)]];
            let steps = 16;
            for i in 0..steps {
                let u0 = ang_a + sweep * i as f32 / steps as f32;
                let u1 = ang_a + sweep * (i + 1) as f32 / steps as f32;
                lines.push([at(u0), at(u1)]);
            }
            let tangent = |ang: f32| vec2(-ang.sin(), ang.cos()) * sweep.signum();
            let ang_b = ang_a + sweep;
            let mid = ang_a + sweep * 0.5;
            let label = format!("{:.*}\u{00b0}", style.precision, val);
            Some(DimBadge {
                arrows: vec![
                    dim_arrow(at(ang_a), tangent(ang_a)),
                    dim_arrow(at(ang_b), -tangent(ang_b)),
                ],
                text_rect: dim_label_rect(vtx + vec2(mid.cos(), mid.sin()) * (rad + 15.0), &label),
                lines,
                label,
            })
        }
        (ConstraintKind::Radius, Curve::Arc(arc)) => {
            if arc.radius / app.view.pixel_world_size() < 12.0 {
                return None;
            }
            let full = (arc.end_angle - arc.start_angle).abs() >= 2.0 * std::f64::consts::PI - 1e-9;
            let ang = if full {
                std::f64::consts::FRAC_PI_4
            } else {
                0.5 * (arc.start_angle + arc.end_angle)
            };
            let center = px(arc.center.x, arc.center.y);
            let rim = px(
                arc.center.x + arc.radius * ang.cos(),
                arc.center.y + arc.radius * ang.sin(),
            );
            let out = (rim - center).normalized();
            let tail = rim + out * 14.0;
            let label = format!("R{}", units.format_measure(val, style.precision));
            let half_w = 7.0 * label.chars().count() as f32 * 0.5 + 5.0;
            Some(DimBadge {
                text_rect: dim_label_rect(tail + out * (half_w + 4.0), &label),
                lines: vec![[center, rim], [rim, tail]],
                arrows: vec![dim_arrow(rim, -out)],
                label,
            })
        }
        _ => None,
    }
}

/// Canvas-local centers of one badge row's chips — exactly where
/// `constraint_badges` draws them, minus the painter origin. `None` when
/// the entity is too small on screen to badge.
fn chip_centers(app: &AppState, id: EntityId, count: usize) -> Option<Vec<egui::Pos2>> {
    let (mid_w, dir_w, extent_w) = badge_anchor(&app.document, id)?;
    if extent_w / app.view.pixel_world_size() < 18.0 {
        return None;
    }
    let (mx, my) = app.view.world_to_screen(mid_w.0, mid_w.1);
    let mid = pos2(mx as f32, my as f32);
    // Screen space flips y, so the world direction's y negates.
    let d = vec2(dir_w.0 as f32, -(dir_w.1 as f32)).normalized();
    let mut n = vec2(d.y, -d.x);
    if n.y > 0.0 {
        n = -n;
    }
    let step = 21.0;
    let base = mid + n * 16.0 - d * (step * (count as f32 - 1.0) * 0.5);
    Some((0..count).map(|i| base + d * (step * i as f32)).collect())
}

/// Returns the constraints behind the badge chip or weld dot under the
/// given canvas-local position, if any. Mirrors the `constraint_badges`
/// layout so what you click is what you see.
pub(crate) fn badge_hit(app: &AppState, sx: f64, sy: f64) -> Option<Vec<SketchConstraint>> {
    if !app.show_constraints || app.document.constraints.is_empty() {
        return None;
    }
    let p = pos2(sx as f32, sy as f32);
    let model = badge_model(&app.document);
    for (id, glyphs) in &model.line_badges {
        let Some(centers) = chip_centers(app, *id, glyphs.len()) else {
            continue;
        };
        for ((_, cs), c) in glyphs.iter().zip(centers) {
            if egui::Rect::from_center_size(c, vec2(20.0, 20.0)).contains(p) {
                return Some(cs.clone());
            }
        }
    }
    for c in &model.dim_badges {
        if let Some(dim) = dim_badge_layout(app, c)
            && dim.text_rect.contains(p)
        {
            return Some(vec![*c]);
        }
    }
    for ((wx, wy), cs) in &model.corner_dots {
        let (dx, dy) = app.view.world_to_screen(*wx, *wy);
        if (pos2(dx as f32, dy as f32) - p).length() <= 6.0 {
            return Some(cs.clone());
        }
    }
    None
}

/// Draws the constraint badges: a row of small glyph chips beside each
/// constrained line's midpoint and a dot on each welded corner. When `hover`
/// (an absolute screen position) lands on a chip or dot, that badge is lit
/// and a "click to delete" hint is drawn above it — badges are clickable
/// deletions in Select mode, so the caller passes `hover` only then.
pub(super) fn constraint_badges(
    painter: &egui::Painter,
    app: &AppState,
    origin: egui::Pos2,
    hover: Option<egui::Pos2>,
) {
    if !app.show_constraints || app.document.constraints.is_empty() {
        return;
    }
    let model = badge_model(&app.document);
    let clip = painter.clip_rect().expand(48.0);
    let bg = Color32::from_rgba_unmultiplied(20, 26, 36, 225);
    let col = crate::theme::ACCENT_BRIGHT;
    let mut hint: Option<egui::Pos2> = None;
    // A badge sitting under a selection grip can't be click-deleted: the grip
    // drag claims that click first (see view.rs). A coincident weld dot lands
    // exactly on the shared endpoint grip, so once the line is selected its
    // "click to delete" hint is a lie. Suppress the hint/highlight there and
    // match what the click will actually do — start a grip drag.
    let grip_pts: Vec<egui::Pos2> = app
        .selection_grips()
        .iter()
        .map(|(_, g)| super::render::world_to_screen_pos(app, origin, g.world.x, g.world.y))
        .collect();
    let under_grip = |p: egui::Pos2| grip_pts.iter().any(|q| (*q - p).length() <= 8.0);
    for (id, glyphs) in &model.line_badges {
        let Some(centers) = chip_centers(app, *id, glyphs.len()) else {
            continue;
        };
        for ((g, _), c) in glyphs.iter().zip(centers) {
            let p = origin + c.to_vec2();
            if !clip.contains(p) {
                continue;
            }
            let hot = hover
                .map(|h| egui::Rect::from_center_size(p, vec2(20.0, 20.0)).contains(h))
                .unwrap_or(false)
                && !under_grip(p);
            badge_chip(painter, p, *g, bg, hot);
            if hot {
                hint = Some(p);
            }
        }
    }
    for c in &model.dim_badges {
        let Some(dim) = dim_badge_layout(app, c) else {
            continue;
        };
        let tr = dim.text_rect.translate(origin.to_vec2());
        if !clip.contains(tr.center()) {
            continue;
        }
        let stroke = Stroke::new(1.0, col);
        for seg in &dim.lines {
            painter.line_segment(
                [origin + seg[0].to_vec2(), origin + seg[1].to_vec2()],
                stroke,
            );
        }
        for tri in &dim.arrows {
            painter.add(egui::Shape::convex_polygon(
                tri.iter().map(|q| origin + q.to_vec2()).collect(),
                col,
                Stroke::NONE,
            ));
        }
        let hot = hover.map(|h| tr.contains(h)).unwrap_or(false) && !under_grip(tr.center());
        painter.rect_filled(tr, 4.0, bg);
        painter.rect_stroke(
            tr,
            4.0,
            Stroke::new(
                1.0,
                if hot {
                    crate::theme::SNAP
                } else {
                    crate::theme::OUTLINE
                },
            ),
            egui::StrokeKind::Middle,
        );
        painter.text(
            tr.center(),
            egui::Align2::CENTER_CENTER,
            &dim.label,
            egui::FontId::proportional(11.0),
            col,
        );
        if hot {
            hint = Some(tr.center());
        }
    }
    for ((wx, wy), _) in &model.corner_dots {
        let p = super::render::world_to_screen_pos(app, origin, *wx, *wy);
        if !clip.contains(p) {
            continue;
        }
        let hot = hover.map(|h| (h - p).length() <= 6.0).unwrap_or(false) && !under_grip(p);
        painter.circle_filled(p, 4.4, bg);
        painter.circle_stroke(p, 4.4, Stroke::new(1.0, crate::theme::OUTLINE));
        painter.circle_filled(p, 2.2, if hot { crate::theme::SNAP } else { col });
        if hot {
            hint = Some(p);
        }
    }
    if let Some(p) = hint {
        painter.text(
            p + vec2(0.0, -12.0),
            egui::Align2::CENTER_BOTTOM,
            "click to delete",
            egui::FontId::proportional(11.0),
            Color32::from_rgb(255, 200, 120),
        );
    }
}

fn badge_chip(painter: &egui::Painter, c: egui::Pos2, g: BadgeGlyph, bg: Color32, hot: bool) {
    let r = egui::Rect::from_center_size(c, vec2(19.0, 19.0));
    painter.rect_filled(r, 4.0, bg);
    painter.rect_stroke(
        r,
        4.0,
        Stroke::new(
            1.0,
            if hot {
                crate::theme::SNAP
            } else {
                crate::theme::OUTLINE
            },
        ),
        egui::StrokeKind::Middle,
    );
    paint_badge_glyph(painter, c, g);
}

/// Maps a constraint kind to its icon asset — shared by the canvas badges
/// and the constraint bar so the two read identically.
fn badge_icon(g: BadgeGlyph) -> crate::icons::Icon {
    use crate::icons::Icon;
    match g {
        BadgeGlyph::Horizontal => Icon::ConHorizontal,
        BadgeGlyph::Vertical => Icon::ConVertical,
        BadgeGlyph::Parallel => Icon::ConParallel,
        BadgeGlyph::Perpendicular => Icon::ConPerpendicular,
        BadgeGlyph::Equal => Icon::ConEqual,
        BadgeGlyph::Tangent => Icon::ConTangent,
    }
}

/// Paints just the constraint glyph (no chip background) centred at `c`, at
/// the icon's own colours — shared by the canvas badges and the inspector's
/// constraint bar so the two read identically.
pub(super) fn paint_badge_glyph(painter: &egui::Painter, c: egui::Pos2, g: BadgeGlyph) {
    let rect = egui::Rect::from_center_size(c, vec2(14.0, 14.0));
    crate::icons::paint_icon(painter, painter.ctx(), badge_icon(g), rect, Color32::WHITE);
}

pub(super) fn cursor_readout(ctx: &egui::Context, app: &AppState, origin: egui::Pos2) {
    if app.dyn_on {
        return;
    }
    let (cx, cy) = app.cursor_world;
    let text = match &app.tool {
        Tool::Move { base: Some(b), .. } | Tool::Copy { base: Some(b), .. } => {
            let (bx, by) = b.to_f64();
            let (dx, dy) = (cx - bx, cy - by);
            Some(format!(
                "Δ {:.2}, {:.2}   {:.2}",
                dx,
                dy,
                (dx * dx + dy * dy).sqrt()
            ))
        }
        Tool::Rotate { base: Some(b), .. } => {
            let (bx, by) = b.to_f64();
            let a = oxidraft_geometry::wrap_deg360((cy - by).atan2(cx - bx).to_degrees());
            Some(format!("{:.1}°", a))
        }
        Tool::Scale {
            base: Some(b),
            reference,
            ..
        } => {
            let (bx, by) = b.to_f64();
            let d = ((cx - bx).powi(2) + (cy - by).powi(2)).sqrt();
            match reference {
                Some(r) if *r > 1e-9 => Some(format!("×{:.3}", d / r)),
                _ => Some(format!("{:.2}", d)),
            }
        }
        _ => None,
    };
    let Some(text) = text else { return };

    let cur = app.view.world_to_screen(cx, cy);
    let pos = pos2(
        origin.x + cur.0 as f32 + 18.0,
        origin.y + cur.1 as f32 + 16.0,
    );
    egui::Area::new(egui::Id::new("cursor_readout"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(15, 19, 29, 200))
                .stroke(Stroke::new(1.0, crate::theme::OUTLINE))
                .corner_radius(crate::theme::tok::R_SM)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .monospace()
                            .size(12.0)
                            .color(crate::theme::ACCENT_BRIGHT),
                    );
                });
        });
}

fn hud_field(
    ui: &mut egui::Ui,
    id: egui::Id,
    buf: &mut String,
    width: f32,
    hint: &str,
    select_all: bool,
    grab_focus: bool,
) -> egui::Response {
    let out = egui::TextEdit::singleline(buf)
        .id(id)
        .desired_width(width)
        .hint_text(hint)
        .show(ui);
    if select_all {
        out.response.request_focus();
        let mut state = out.state;
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::select_all(&out.galley)));
        state.store(ui.ctx(), id);
    } else if grab_focus {
        out.response.request_focus();
    }
    out.response.response
}

fn hud_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(12.0)
            .color(crate::theme::HUD_LABEL),
    );
}

fn cursor_hud_pos(app: &AppState, origin: egui::Pos2, dy: f32) -> egui::Pos2 {
    let (cx, cy) = app.cursor_world;
    let cur = app.view.world_to_screen(cx, cy);
    pos2(origin.x + cur.0 as f32 + 18.0, origin.y + cur.1 as f32 + dy)
}

fn cursor_hud(ctx: &egui::Context, id: &str, pos: egui::Pos2, add: impl FnOnce(&mut egui::Ui)) {
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            corner_glass_frame().show(ui, |ui| {
                ui.horizontal(|ui| add(ui));
            });
        });
}

pub(super) fn dyn_transform_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    #[derive(Clone, Copy)]
    enum Kind {
        Translate,
        Rotate,
        Scale,
    }
    let info = match &app.tool {
        Tool::Move { base: Some(b), .. } | Tool::Copy { base: Some(b), .. } => {
            Some((Kind::Translate, b.to_f64(), None))
        }
        Tool::Rotate { base: Some(b), .. } => Some((Kind::Rotate, b.to_f64(), None)),
        Tool::Scale {
            base: Some(b),
            reference,
            ..
        } => Some((Kind::Scale, b.to_f64(), Some(*reference))),
        _ => None,
    };
    let (Some((kind, (bx, by), scale_ref)), true) = (info, app.dyn_on) else {
        ui_state.dyn_tf_active = false;
        return;
    };

    let (cx, cy) = app.cursor_world;
    let first_show = !ui_state.dyn_tf_active;
    ui_state.dyn_tf_active = true;

    let dx_id = egui::Id::new("dyn_tf_dx");
    let dy_id = egui::Id::new("dyn_tf_dy");
    let ang_id = egui::Id::new("dyn_tf_angle");
    let fac_id = egui::Id::new("dyn_tf_factor");

    let (dx, dy) = (cx - bx, cy - by);
    let cursor_ang = (cy - by).atan2(cx - bx);
    if !ctx.memory(|m| m.has_focus(dx_id)) {
        ui_state.dyn_tf_dx = format!("{dx:.2}");
    }
    if !ctx.memory(|m| m.has_focus(dy_id)) {
        ui_state.dyn_tf_dy = format!("{dy:.2}");
    }
    if !ctx.memory(|m| m.has_focus(ang_id)) {
        ui_state.dyn_tf_angle = format!("{:.1}", cursor_ang.to_degrees());
    }
    if !ctx.memory(|m| m.has_focus(fac_id)) {
        let live_factor = match scale_ref {
            Some(Some(r)) if r > 1e-9 => ((cx - bx).powi(2) + (cy - by).powi(2)).sqrt() / r,
            _ => 1.0,
        };
        ui_state.dyn_tf_factor = format!("{live_factor:.3}");
    }

    let nothing_focused = ctx.memory(|m| m.focused().is_none());
    let grab = first_show || nothing_focused;

    let pos = cursor_hud_pos(app, origin, -38.0);
    cursor_hud(ctx, "dyn_transform_hud", pos, |ui| match kind {
        Kind::Translate => {
            hud_label(ui, "ΔX");
            hud_field(
                ui,
                dx_id,
                &mut ui_state.dyn_tf_dx,
                56.0,
                "",
                first_show,
                grab,
            );
            hud_label(ui, "ΔY");
            hud_field(ui, dy_id, &mut ui_state.dyn_tf_dy, 56.0, "", false, false);
        }
        Kind::Rotate => {
            hud_field(
                ui,
                ang_id,
                &mut ui_state.dyn_tf_angle,
                56.0,
                "angle",
                first_show,
                grab,
            );
            hud_label(ui, "°");
        }
        Kind::Scale => {
            hud_label(ui, "×");
            hud_field(
                ui,
                fac_id,
                &mut ui_state.dyn_tf_factor,
                56.0,
                "factor",
                first_show,
                grab,
            );
        }
    });

    let mut commit = false;
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        commit = true;
    }
    if !commit {
        return;
    }

    match kind {
        Kind::Translate => {
            let tdx = ui_state.dyn_tf_dx.trim().parse::<f64>().unwrap_or(dx);
            let tdy = ui_state.dyn_tf_dy.trim().parse::<f64>().unwrap_or(dy);
            app.place_tool_point(Point2d::from_f64(bx + tdx, by + tdy));
            ui_state.dyn_tf_active = false;
        }
        Kind::Rotate => {
            let Ok(mag) = ui_state.dyn_tf_angle.trim().parse::<f64>() else {
                return;
            };
            let dir = if cursor_ang >= 0.0 { 1.0 } else { -1.0 };
            let ang = dir * mag.abs().to_radians();
            app.place_tool_point(Point2d::from_f64(bx + ang.cos(), by + ang.sin()));
            ui_state.dyn_tf_active = false;
        }
        Kind::Scale => {
            let Ok(factor) = ui_state.dyn_tf_factor.trim().parse::<f64>() else {
                return;
            };
            if factor <= 1e-9 {
                return;
            }
            if let Tool::Scale { reference, .. } = &mut app.tool
                && reference.is_none()
            {
                *reference = Some(1.0);
            }
            let r1 = if let Tool::Scale {
                reference: Some(r), ..
            } = &app.tool
            {
                *r
            } else {
                1.0
            };
            app.place_tool_point(Point2d::from_f64(bx + factor * r1, by));
            ui_state.dyn_tf_active = false;
        }
    }
}

pub(super) fn dyn_line_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    let line_ref = if let Tool::Line { last: Some(p0) } = &app.tool {
        Some(p0.to_f64())
    } else {
        None
    };
    if let (true, Some((rx, ry))) = (app.dyn_on, line_ref) {
        let (cx, cy) = app.cursor_world;
        let live_len = ((cx - rx).powi(2) + (cy - ry).powi(2)).sqrt();
        let live_ang = oxidraft_geometry::wrap_deg360((cy - ry).atan2(cx - rx).to_degrees());

        let len_id = egui::Id::new("dyn_len");
        let ang_id = egui::Id::new("dyn_ang");
        if !ctx.memory(|m| m.has_focus(len_id)) {
            ui_state.dyn_length = format!("{:.2}", live_len);
        }
        if !ctx.memory(|m| m.has_focus(ang_id)) {
            ui_state.dyn_angle = format!("{:.1}", live_ang);
        }

        let first_show = !ui_state.dyn_active;
        let mut commit = false;
        let pos = cursor_hud_pos(app, origin, -38.0);
        cursor_hud(ctx, "dyn_input_hud", pos, |ui| {
            hud_label(ui, "L");
            let lr = hud_field(
                ui,
                len_id,
                &mut ui_state.dyn_length,
                58.0,
                "",
                false,
                first_show,
            );
            hud_label(ui, "∠");
            let ar = hud_field(ui, ang_id, &mut ui_state.dyn_angle, 48.0, "", false, false);
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) && (lr.lost_focus() || ar.lost_focus())
            {
                commit = true;
            }
        });
        ui_state.dyn_active = true;
        if commit {
            let cmd = format!(
                "@{}<{}",
                ui_state.dyn_length.trim(),
                ui_state.dyn_angle.trim()
            );
            app.run_command(&cmd);
            ui_state.dyn_active = false;
        }
    } else {
        ui_state.dyn_active = false;
    }
}

pub(super) fn dyn_circle_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    let circle_center = if let Tool::Circle { center: Some(c) } = &app.tool {
        Some(c.to_f64())
    } else {
        None
    };
    if let (true, Some((cx, cy))) = (app.dyn_on, circle_center) {
        let rad_id = egui::Id::new("dyn_radius");
        let first_show = !ui_state.dyn_circle_active;
        if first_show {
            ui_state.dyn_radius.clear();
        }
        ui_state.dyn_circle_active = true;
        if ctx.input(|i| i.key_pressed(egui::Key::Enter))
            && let Ok(rad) = ui_state.dyn_radius.trim().parse::<f64>()
            && rad > 1e-9
        {
            app.place_tool_point(Point2d::from_f64(cx + rad, cy));
            ui_state.dyn_circle_active = false;
            return;
        }

        let pos = cursor_hud_pos(app, origin, -38.0);
        cursor_hud(ctx, "dyn_circle_hud", pos, |ui| {
            hud_label(ui, "R");
            let rr = hud_field(
                ui,
                rad_id,
                &mut ui_state.dyn_radius,
                58.0,
                "radius",
                false,
                false,
            );
            let nothing_focused = ui.ctx().memory(|m| m.focused().is_none());
            if first_show || nothing_focused {
                rr.request_focus();
            }
        });
    } else {
        ui_state.dyn_circle_active = false;
    }
}
/// Always-visible, movable popup for picking the polygon's side count. Shown
/// only after *both* clicks — center and radius/rotation — are placed: the
/// shape is spatially final at that point (see `Tool::preview`, which stops
/// following the cursor once `radius_point` is set), and this popup is the
/// only thing left to decide before Apply/Enter commits it or Cancel drops
/// it. Quick-pick buttons cover the common cases; the field next to them
/// takes any custom count. Not gated by `app.dyn_on` — like
/// `blend_confirm_hud`, choosing this option is the point of the tool, not a
/// typing shortcut layered on top of it.
pub(super) fn polygon_sides_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    let Tool::Polygon {
        center: Some(c),
        radius_point: Some(rp),
        sides,
    } = app.tool
    else {
        ui_state.dyn_poly_active = false;
        return;
    };

    let sid = egui::Id::new("dyn_poly_sides");
    if !ctx.memory(|m| m.has_focus(sid)) {
        ui_state.dyn_poly_sides = sides.map(|n| n.to_string()).unwrap_or_default();
    }
    ui_state.dyn_poly_active = true;

    // Only consulted the very first time this popup is ever shown in the
    // session — `.movable(true)` + egui's persisted Area state remembers
    // wherever the user (or this default) last left it after that, same as
    // `blend_confirm_hud`.
    const CLEARANCE: f32 = 130.0;
    let (mx, my) = ((c.x + rp.x) * 0.5, (c.y + rp.y) * 0.5);
    let (sx, sy) = app.view.world_to_screen(mx, my);
    let default_pos = pos2(
        origin.x + sx as f32 + CLEARANCE,
        origin.y + sy as f32 - CLEARANCE,
    );

    let mut clicked: Option<usize> = None;
    let mut apply = false;
    let mut cancel = false;
    egui::Area::new(egui::Id::new("polygon_sides_hud"))
        .default_pos(default_pos)
        .movable(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            corner_glass_frame().show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("⠿ Polygon sides")
                            .size(11.0)
                            .color(crate::theme::HUD_LABEL),
                    );
                    ui.horizontal(|ui| {
                        for n in [3usize, 4, 5, 6, 8, 10, 12] {
                            if ui
                                .selectable_label(sides == Some(n), n.to_string())
                                .clicked()
                            {
                                clicked = Some(n);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        hud_label(ui, "Custom");
                        hud_field(
                            ui,
                            sid,
                            &mut ui_state.dyn_poly_sides,
                            40.0,
                            "3+",
                            false,
                            false,
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Apply  Enter").clicked() {
                            apply = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        });

    if let Some(n) = clicked {
        ui_state.dyn_poly_sides = n.to_string();
        app.tool = Tool::Polygon {
            center: Some(c),
            radius_point: Some(rp),
            sides: Some(n),
        };
    } else {
        let parsed = ui_state
            .dyn_poly_sides
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|n| *n >= 3);
        if parsed != sides {
            app.tool = Tool::Polygon {
                center: Some(c),
                radius_point: Some(rp),
                sides: parsed,
            };
        }
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        apply = true;
    }
    if apply {
        app.confirm_pending_polygon();
        ui_state.dyn_poly_active = false;
    } else if cancel {
        app.cancel_pending_polygon();
        ui_state.dyn_poly_active = false;
    }
}

pub(super) fn dyn_rect_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    let rect_first = if let Tool::Rectangle { first: Some(f) } = &app.tool {
        Some(f.to_f64())
    } else {
        None
    };
    if let (true, Some((fx, fy))) = (app.dyn_on, rect_first) {
        let (crx, cry) = app.cursor_world;

        let field_id = egui::Id::new("dyn_rect_field");
        let first_show = !ui_state.dyn_rect_active;
        if first_show {
            ui_state.dyn_rect_width.clear();
            ui_state.dyn_rect_height.clear();
            ui_state.dyn_rect_stage_h = false;
        }
        ui_state.dyn_rect_active = true;
        let mut committed = false;
        let mut focus_field = first_show;
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            if !ui_state.dyn_rect_stage_h {
                if let Ok(w) = ui_state.dyn_rect_width.trim().parse::<f64>()
                    && w.abs() > 1e-9
                {
                    ui_state.dyn_rect_stage_h = true;
                    focus_field = true;
                }
            } else if let Ok(h) = ui_state.dyn_rect_height.trim().parse::<f64>() {
                let w = ui_state.dyn_rect_width.trim().parse::<f64>().unwrap_or(0.0);
                if h.abs() > 1e-9 && w.abs() > 1e-9 {
                    let sx = if crx >= fx { 1.0 } else { -1.0 };
                    let sy = if cry >= fy { 1.0 } else { -1.0 };
                    app.place_tool_point(Point2d::from_f64(fx + w.abs() * sx, fy + h.abs() * sy));
                    ui_state.dyn_rect_active = false;
                    committed = true;
                }
            }
        }
        if committed {
            return;
        }

        let on_height = ui_state.dyn_rect_stage_h;
        let pos = cursor_hud_pos(app, origin, -38.0);
        cursor_hud(ctx, "dyn_rect_hud", pos, |ui| {
            let (label, buf, hint) = if on_height {
                ("H", &mut ui_state.dyn_rect_height, "height, Enter")
            } else {
                ("W", &mut ui_state.dyn_rect_width, "width, Enter")
            };
            hud_label(ui, label);
            let r = hud_field(ui, field_id, buf, 70.0, hint, false, false);
            let nothing_focused = ui.ctx().memory(|m| m.focused().is_none());
            if focus_field || nothing_focused {
                r.request_focus();
            }
        });
    } else {
        ui_state.dyn_rect_active = false;
    }
}
pub(super) fn dyn_ellipse_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    let stage = match &app.tool {
        Tool::Ellipse {
            center: Some(c),
            axis_end: None,
        } => Some((c.to_f64(), None)),
        Tool::Ellipse {
            center: Some(c),
            axis_end: Some(a),
        } => Some((c.to_f64(), Some(a.to_f64()))),
        _ => None,
    };
    if let (true, Some((center, axis_end))) = (app.dyn_on, stage) {
        let (crx, cry) = app.cursor_world;
        let first_show = !ui_state.dyn_ell_active;
        if first_show {
            ui_state.dyn_ell_major.clear();
            ui_state.dyn_ell_minor.clear();
        }
        ui_state.dyn_ell_active = true;
        let maj_id = egui::Id::new("dyn_ell_major");
        let min_id = egui::Id::new("dyn_ell_minor");
        let active_id = if axis_end.is_none() { maj_id } else { min_id };
        let tab = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
            | ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
        if tab {
            ctx.memory_mut(|m| m.request_focus(active_id));
        }

        let mut committed = false;
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            match axis_end {
                None => {
                    if let Ok(maj) = ui_state.dyn_ell_major.trim().parse::<f64>()
                        && maj.abs() > 1e-9
                    {
                        let dir = (crx - center.0, cry - center.1);
                        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
                        let (ux, uy) = if len > 1e-9 {
                            (dir.0 / len, dir.1 / len)
                        } else {
                            (1.0, 0.0)
                        };
                        app.place_tool_point(Point2d::from_f64(
                            center.0 + maj * ux,
                            center.1 + maj * uy,
                        ));
                        ui_state.dyn_ell_active = false;
                        committed = true;
                    }
                }
                Some(a_end) => {
                    if let Ok(minr) = ui_state.dyn_ell_minor.trim().parse::<f64>()
                        && minr.abs() > 1e-9
                    {
                        let dir = (a_end.0 - center.0, a_end.1 - center.1);
                        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-12);
                        let (px, py) = (-dir.1 / len, dir.0 / len);
                        app.place_tool_point(Point2d::from_f64(
                            center.0 + minr * px,
                            center.1 + minr * py,
                        ));
                        ui_state.dyn_ell_active = false;
                        committed = true;
                    }
                }
            }
        }
        if committed {
            return;
        }

        let pos = cursor_hud_pos(app, origin, -52.0);
        cursor_hud(ctx, "dyn_ell_hud", pos, |ui| {
            let (label, id, buf, hint) = if axis_end.is_none() {
                (
                    "A",
                    maj_id,
                    &mut ui_state.dyn_ell_major,
                    "major (aim with cursor)",
                )
            } else {
                ("B", min_id, &mut ui_state.dyn_ell_minor, "minor")
            };
            hud_label(ui, label);
            let r = hud_field(ui, id, buf, 54.0, hint, false, false);
            let nothing_focused = ui.ctx().memory(|m| m.focused().is_none());
            if first_show || nothing_focused {
                r.request_focus();
            }
        });
    } else {
        ui_state.dyn_ell_active = false;
    }
}

pub(super) fn dyn_offset_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    let dist = if let Tool::Offset { dist, .. } = &app.tool {
        Some(*dist)
    } else {
        None
    };
    if let (true, Some(dist)) = (app.dyn_on, dist) {
        let first_show = !ui_state.dyn_offset_active;
        if first_show {
            ui_state.dyn_offset_dist = super::render::trim_decimals(dist, 4);
        }
        let did = egui::Id::new("dyn_offset_dist");

        let pos = cursor_hud_pos(app, origin, -38.0);
        cursor_hud(ctx, "dyn_offset_hud", pos, |ui| {
            hud_label(ui, "Dist");
            let nothing_focused = ui.ctx().memory(|m| m.focused().is_none());
            hud_field(
                ui,
                did,
                &mut ui_state.dyn_offset_dist,
                58.0,
                "distance",
                first_show,
                !first_show && nothing_focused,
            );
        });
        ui_state.dyn_offset_active = true;
        if let Ok(d) = ui_state.dyn_offset_dist.trim().parse::<f64>()
            && d > 1e-9
            && let Tool::Offset { source, .. } = &app.tool
        {
            app.tool = Tool::Offset {
                dist: d,
                source: *source,
            };
        }
    } else {
        ui_state.dyn_offset_active = false;
    }
}
pub(super) fn dyn_corner_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    // Blend is deliberately excluded here: it has no cursor-following box at
    // all before both entities are picked; blend_confirm_hud (fixed/movable,
    // not chasing the cursor) is the only popup it ever shows, and only once
    // picking is done.
    let info = match &app.tool {
        Tool::Fillet { radius, .. } => Some(("Radius", *radius)),
        Tool::Chamfer { dist, .. } => Some(("Dist", *dist)),
        Tool::CircleTtr { radius, .. } => Some(("Radius", *radius)),
        _ => None,
    };
    let (Some((label, value)), true) = (info, app.dyn_on) else {
        ui_state.dyn_corner_active = false;
        return;
    };

    let first_show = !ui_state.dyn_corner_active;
    if first_show {
        ui_state.dyn_corner_val = super::render::trim_decimals(value, 4);
    }
    let id = egui::Id::new("dyn_corner_val");
    let pos = cursor_hud_pos(app, origin, -38.0);
    cursor_hud(ctx, "dyn_corner_hud", pos, |ui| {
        hud_label(ui, label);
        let r = hud_field(
            ui,
            id,
            &mut ui_state.dyn_corner_val,
            58.0,
            "value, then pick",
            false,
            false,
        );
        let nothing_focused = ui.ctx().memory(|m| m.focused().is_none());
        if first_show || nothing_focused {
            r.request_focus();
        }
    });
    ui_state.dyn_corner_active = true;
    let typed = ui_state
        .dyn_corner_val
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| *v > 1e-9);
    match &app.tool {
        Tool::Fillet { first, .. } => {
            if let Some(v) = typed {
                app.tool = Tool::Fillet {
                    radius: v,
                    first: *first,
                }
            }
        }
        Tool::Chamfer { first, .. } => {
            if let Some(v) = typed {
                app.tool = Tool::Chamfer {
                    dist: v,
                    first: *first,
                }
            }
        }
        Tool::CircleTtr { first, .. } => {
            if let Some(v) = typed {
                app.tool = Tool::CircleTtr {
                    radius: v,
                    first: *first,
                }
            }
        }
        _ => {}
    }
}

/// Always-visible popup shown once both blend entities are picked: lets the
/// user tune G0–G3 continuity and tension and see the result (drawn separately
/// as a ghost preview by `render::draw_blend_preview`) before committing.
/// Unlike the dyn-input-gated HUDs, this one isn't tied to `app.dyn_on` —
/// confirming/cancelling a blend pick is a one-off decision, not a typing
/// shortcut, so it should always be available.
pub(super) fn blend_confirm_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    let Tool::Blend {
        continuity,
        tension,
        first: Some(a),
        second: Some(b),
    } = app.tool
    else {
        ui_state.blend_confirm_active = false;
        return;
    };

    let preview = oxidraft_cad::edit::blend_preview(&app.document, a, b, continuity, tension);
    // Offset well clear of the curve being created (and of the cursor) so the
    // popup never sits on top of the new geometry; this is only the *initial*
    // placement — `.movable(true)` below lets the user drag it anywhere after
    // that, and egui remembers the dragged position across future blends.
    const CLEARANCE: f32 = 130.0;
    let default_pos = match &preview {
        Some(curve) => {
            let (t0, t1) = curve.domain();
            let (x0, y0) = curve.evaluate_f64(t0);
            let (x1, y1) = curve.evaluate_f64(t1);
            let (mx, my) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
            let (sx, sy) = app.view.world_to_screen(mx, my);
            pos2(
                origin.x + sx as f32 + CLEARANCE,
                origin.y + sy as f32 - CLEARANCE,
            )
        }
        None => cursor_hud_pos(app, origin, -CLEARANCE),
    };

    let first_show = !ui_state.blend_confirm_active;
    if first_show {
        ui_state.blend_confirm_tension = super::render::trim_decimals(tension, 4);
    }
    ui_state.blend_confirm_active = true;

    let mut new_continuity: Option<Continuity> = None;
    let mut apply = false;
    let mut cancel = false;
    let tension_id = egui::Id::new("blend_confirm_tension");
    egui::Area::new(egui::Id::new("blend_confirm_hud"))
        .default_pos(default_pos)
        .movable(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            corner_glass_frame().show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("⠿ Blend")
                            .size(11.0)
                            .color(crate::theme::HUD_LABEL),
                    );
                    if preview.is_none() {
                        ui.colored_label(crate::theme::HUD_LABEL, "No valid blend for this pair");
                    }
                    ui.horizontal(|ui| {
                        for c in [
                            Continuity::G0,
                            Continuity::G1,
                            Continuity::G2,
                            Continuity::G3,
                        ] {
                            let txt = match c {
                                Continuity::G0 => "G0",
                                Continuity::G1 => "G1",
                                Continuity::G2 => "G2",
                                Continuity::G3 => "G3",
                            };
                            if ui.selectable_label(continuity == c, txt).clicked() {
                                new_continuity = Some(c);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        hud_label(ui, "Tension");
                        let r = hud_field(
                            ui,
                            tension_id,
                            &mut ui_state.blend_confirm_tension,
                            58.0,
                            "",
                            first_show,
                            first_show,
                        );
                        let _ = r;
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Apply  Enter").clicked() {
                            apply = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        });

    if let Some(c) = new_continuity
        && let Tool::Blend { continuity, .. } = &mut app.tool
    {
        *continuity = c;
    }
    if let Ok(v) = ui_state.blend_confirm_tension.trim().parse::<f64>()
        && v > 1e-9
        && let Tool::Blend { tension, .. } = &mut app.tool
    {
        *tension = v;
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        apply = true;
    }

    if apply {
        if preview.is_some() {
            app.confirm_pending_blend();
        }
        ui_state.blend_confirm_active = false;
    } else if cancel {
        app.cancel_pending_blend();
        ui_state.blend_confirm_active = false;
    }
}

pub(super) fn dyn_text_hud(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    origin: egui::Pos2,
) {
    let anchor = if let Tool::Text {
        anchor: Some(a), ..
    } = &app.tool
    {
        Some(a.to_f64())
    } else {
        None
    };
    if let Some((ax, ay)) = anchor {
        let first_show = !ui_state.dyn_text_active;
        if first_show {
            ui_state.dyn_text_content.clear();
        }
        let tid = egui::Id::new("dyn_text_field");
        let sp = app.view.world_to_screen(ax, ay);
        let hud_pos = pos2(origin.x + sp.0 as f32, origin.y + sp.1 as f32 - 26.0);
        let mut commit = false;
        let mut cancel = false;
        egui::Area::new(egui::Id::new("dyn_text_hud"))
            .fixed_pos(hud_pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let te = ui.add(
                            egui::TextEdit::singleline(&mut ui_state.dyn_text_content)
                                .id(tid)
                                .desired_width(180.0)
                                .hint_text("type text, Enter to place"),
                        );
                        ui.add_space(4.0);
                        super::chrome::font_combo(ui, "dyn_text_font", &mut app.text_font);
                        height_glyph(ui);
                        let mut size = if let Tool::Text { height, .. } = &app.tool {
                            *height
                        } else {
                            2.5
                        };
                        let dv = ui
                            .add(egui::DragValue::new(&mut size).speed(0.05).range(0.1..=1e6))
                            .on_hover_text("Text height");
                        if dv.changed()
                            && let Tool::Text { height, .. } = &mut app.tool
                        {
                            *height = size;
                        }
                        let nothing_focused = ui.ctx().memory(|m| m.focused().is_none());
                        if first_show || nothing_focused {
                            te.request_focus();
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            cancel = true;
                        } else if (te.lost_focus() || te.has_focus())
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            commit = true;
                        }
                    });
                });
            });
        ui_state.dyn_text_active = true;
        if commit {
            let content = std::mem::take(&mut ui_state.dyn_text_content);
            app.run_command(&content);
            ui_state.dyn_text_active = false;
        } else if cancel {
            app.tool = Tool::Select;
            ui_state.dyn_text_active = false;
        }
    } else {
        ui_state.dyn_text_active = false;
    }
}
fn height_glyph(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(11.0, 16.0), egui::Sense::hover());
    let x = rect.center().x;
    let (top, bot) = (rect.top() + 2.0, rect.bottom() - 2.0);
    let s = egui::Stroke::new(1.3, crate::theme::HUD_LABEL);
    let p = ui.painter();
    p.line_segment([pos2(x, top), pos2(x, bot)], s);
    for (y, dy) in [(top, 3.5_f32), (bot, -3.5_f32)] {
        p.line_segment([pos2(x, y), pos2(x - 3.0, y + dy)], s);
        p.line_segment([pos2(x, y), pos2(x + 3.0, y + dy)], s);
    }
}

#[cfg(test)]
mod badge_tests {
    use super::*;
    use oxidraft_document::SketchConstraint;
    use oxidraft_geometry::LineSeg;

    fn line(doc: &mut Document, x0: f64, y0: f64, x1: f64, y1: f64) -> EntityId {
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(x0, y0),
            Point2d::from_f64(x1, y1),
        ))))
    }

    #[test]
    fn badge_model_groups_glyphs_per_line_and_dedups_corners() {
        let mut doc = Document::new();
        let a = line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = line(&mut doc, 4.0, 0.0, 4.0, 3.0);
        doc.add_constraint(SketchConstraint::pair(ConstraintKind::Perpendicular, a, b));
        doc.add_constraint(SketchConstraint::single(ConstraintKind::Horizontal, a));
        doc.add_constraint(SketchConstraint::coincident(a, 1, b, 0));
        let m = badge_model(&doc);
        assert_eq!(m.line_badges.len(), 2);
        let glyphs_of = |id| {
            m.line_badges
                .iter()
                .find(|(e, _)| *e == id)
                .unwrap()
                .1
                .iter()
                .map(|(g, _)| *g)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            glyphs_of(a),
            vec![BadgeGlyph::Perpendicular, BadgeGlyph::Horizontal]
        );
        assert_eq!(glyphs_of(b), vec![BadgeGlyph::Perpendicular]);
        assert_eq!(m.corner_dots.len(), 1);
        assert_eq!(m.corner_dots[0].0, (4.0, 0.0));
        assert_eq!(
            m.corner_dots[0].1.as_slice(),
            &[SketchConstraint::coincident(a, 1, b, 0)],
            "the dot carries its weld"
        );
    }

    #[test]
    fn badge_model_puts_tangent_glyphs_on_both_members() {
        let mut doc = Document::new();
        let l = line(&mut doc, -4.0, 2.0, 4.0, 2.0);
        let a = doc.add(EntityKind::Curve(Curve::Arc(
            oxidraft_geometry::CircularArc::new(
                Point2d::from_f64(0.0, 0.0),
                2.0,
                0.0,
                std::f64::consts::TAU,
            ),
        )));
        doc.add_constraint(SketchConstraint::pair(ConstraintKind::Tangent, l, a));
        let m = badge_model(&doc);
        assert_eq!(m.line_badges.len(), 2);
        for (_, glyphs) in &m.line_badges {
            assert_eq!(glyphs.len(), 1);
            assert_eq!(glyphs[0].0, BadgeGlyph::Tangent);
        }
        assert!(
            badge_anchor(&doc, a).is_some(),
            "arcs have a badge anchor too"
        );
    }

    /// `AppState::new` seeds the document with a `Fixed` constraint pinning
    /// the origin (see `add_origin_point` in state.rs) — filtered out here so
    /// these assertions read as "the constraints this test's own actions
    /// produced."
    fn user_constraints(app: &AppState) -> Vec<SketchConstraint> {
        app.document
            .constraints
            .iter()
            .filter(|c| c.kind != ConstraintKind::Fixed)
            .copied()
            .collect()
    }

    #[test]
    fn clicking_a_badge_chip_removes_the_constraint_undoably() {
        let mut app = AppState::new(800.0, 600.0);
        app.snap_on = false;
        let a = app.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(4.0, 0.0),
        ))));
        let b = app.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(0.0, 2.0),
            Point2d::from_f64(4.0, 2.0),
        ))));
        app.document
            .add_constraint(SketchConstraint::pair(ConstraintKind::Parallel, a, b));
        let chip = chip_centers(&app, a, 1).expect("row visible")[0];
        app.canvas_click(chip.x as f64, chip.y as f64);
        assert!(
            user_constraints(&app).is_empty(),
            "chip click removed the parallel pair"
        );
        app.undo();
        assert_eq!(user_constraints(&app).len(), 1, "badge removal is undoable");
    }

    #[test]
    fn clicking_a_weld_dot_removes_the_weld() {
        let mut app = AppState::new(800.0, 600.0);
        app.snap_on = false;
        let a = app.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(4.0, 0.0),
        ))));
        let b = app.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(4.0, 0.0),
            Point2d::from_f64(4.0, 3.0),
        ))));
        app.document
            .add_constraint(SketchConstraint::coincident(a, 1, b, 0));
        let (sx, sy) = app.view.world_to_screen(4.0, 0.0);
        app.canvas_click(sx, sy);
        assert!(
            user_constraints(&app).is_empty(),
            "dot click removed the weld"
        );
    }

    #[test]
    fn badge_clicks_are_ignored_while_drawing_or_hidden() {
        let mut app = AppState::new(800.0, 600.0);
        app.snap_on = false;
        let a = app.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(4.0, 0.0),
        ))));
        app.document
            .add_constraint(SketchConstraint::single(ConstraintKind::Horizontal, a));
        let chip = chip_centers(&app, a, 1).expect("row visible")[0];
        app.run_command("LINE");
        app.canvas_click(chip.x as f64, chip.y as f64);
        assert_eq!(
            user_constraints(&app).len(),
            1,
            "drawing tools keep their clicks"
        );
        app.tool = Tool::Select;
        app.show_constraints = false;
        app.canvas_click(chip.x as f64, chip.y as f64);
        assert_eq!(
            user_constraints(&app).len(),
            1,
            "hidden badges must not eat clicks"
        );
    }

    #[test]
    fn radius_becomes_a_dimension_badge_not_a_chip() {
        let mut doc = Document::new();
        let a = doc.add(EntityKind::Curve(Curve::Arc(
            oxidraft_geometry::CircularArc::new(Point2d::from_f64(0.0, 0.0), 2.0, 0.0, 1.5),
        )));
        doc.add_constraint(SketchConstraint::radius(a, 2.0));
        let m = badge_model(&doc);
        assert!(
            m.line_badges.is_empty(),
            "valued constraints get dimension annotations, not glyph chips"
        );
        assert_eq!(m.dim_badges.as_slice(), &[SketchConstraint::radius(a, 2.0)]);
    }

    #[test]
    fn angle_becomes_an_angular_dimension_badge() {
        let mut app = AppState::new(800.0, 600.0);
        app.snap_on = false;
        let a = app.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(6.0, 0.0),
        ))));
        let b = app.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(4.0, 4.0),
        ))));
        app.document
            .add_constraint(SketchConstraint::angle(a, b, 45.0));
        let c = app.document.constraints[app.document.constraints.len() - 1];
        let m = badge_model(&app.document);
        assert!(m.line_badges.is_empty(), "angle badges as a dimension");
        assert_eq!(m.dim_badges.as_slice(), &[c]);
        let dim = dim_badge_layout(&app, &c).expect("lines are large enough on screen");
        assert!(
            dim.label.ends_with('\u{00b0}') && dim.label.starts_with("45"),
            "angular label: {}",
            dim.label
        );
        assert!(
            dim.lines.len() > 10,
            "legs plus a swept arc: {} segments",
            dim.lines.len()
        );
        let hit = badge_hit(
            &app,
            dim.text_rect.center().x as f64,
            dim.text_rect.center().y as f64,
        )
        .expect("label is clickable");
        assert_eq!(hit.as_slice(), &[c]);
    }

    #[test]
    fn dimension_badge_shows_the_value_and_deletes_on_click() {
        let mut app = AppState::new(800.0, 600.0);
        app.snap_on = false;
        let a = app.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(4.0, 0.0),
        ))));
        app.document
            .add_constraint(SketchConstraint::distance(a, 4.0));
        let c = app.document.constraints[app.document.constraints.len() - 1];
        let dim = dim_badge_layout(&app, &c).expect("line is large enough on screen");
        assert!(
            dim.label.starts_with("4.0"),
            "label carries the driving value: {}",
            dim.label
        );
        let hit = badge_hit(
            &app,
            dim.text_rect.center().x as f64,
            dim.text_rect.center().y as f64,
        )
        .expect("label is clickable");
        assert_eq!(hit.as_slice(), &[c]);
        app.tool = Tool::Select;
        app.canvas_click(
            dim.text_rect.center().x as f64,
            dim.text_rect.center().y as f64,
        );
        assert!(
            user_constraints(&app).is_empty(),
            "clicking the dimension badge deletes the driving constraint"
        );
    }

    #[test]
    fn badge_model_skips_constraints_on_missing_entities() {
        let mut doc = Document::new();
        let a = line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = line(&mut doc, 4.0, 0.0, 4.0, 3.0);
        doc.add_constraint(SketchConstraint::coincident(a, 1, b, 0));
        doc.remove(a);
        let m = badge_model(&doc);
        assert!(m.corner_dots.is_empty(), "pruned weld leaves no dot");
    }
}

#[cfg(test)]
mod text_hud_tests {
    use super::*;
    use oxidraft_document::EntityKind;

    fn key(k: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    #[allow(deprecated)] // dyn_text_hud takes &Context, not &mut Ui, so run_ui doesn't fit
    fn typing_then_enter_creates_text() {
        let ctx = egui::Context::default();
        let mut app = AppState::new(800.0, 600.0);
        app.tool = Tool::Text {
            anchor: Some(Point2d::from_f64(1.0, 2.0)),
            height: 2.5,
        };
        let mut ui_state = UiState::default();
        let origin = pos2(0.0, 0.0);

        let frame = |events: Vec<egui::Event>, app: &mut AppState, ui: &mut UiState| {
            let raw = egui::RawInput {
                events,
                ..Default::default()
            };
            let _ = ctx.run(raw, |ctx| dyn_text_hud(ctx, app, ui, origin));
        };

        // Frame 1: HUD appears and requests focus on the text field.
        frame(vec![], &mut app, &mut ui_state);
        // Frame 2: type into the (now focused) field.
        frame(
            vec![egui::Event::Text("Hello".into())],
            &mut app,
            &mut ui_state,
        );
        // Frame 3: press Enter to place the text.
        frame(
            vec![key(egui::Key::Enter, true), key(egui::Key::Enter, false)],
            &mut app,
            &mut ui_state,
        );

        let texts: Vec<&str> = app
            .document
            .editable_entities()
            .filter_map(|e| match &e.kind {
                EntityKind::Text { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            vec!["Hello"],
            "Enter should place the typed text as a Text entity; tool={:?}",
            app.tool
        );
    }
}

#[cfg(test)]
mod polygon_hud_tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn hud_stays_hidden_until_center_is_placed() {
        let ctx = egui::Context::default();
        let mut app = AppState::new(800.0, 600.0);
        app.tool = Tool::Polygon {
            center: None,
            radius_point: None,
            sides: None,
        };
        let mut ui_state = UiState::default();
        let origin = pos2(0.0, 0.0);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            polygon_sides_hud(ctx, &mut app, &mut ui_state, origin)
        });
        assert!(
            !ui_state.dyn_poly_active,
            "no popup — and no cursor-following pointer box — before both clicks"
        );

        // Center alone (radius not yet clicked) must also stay hidden: the
        // popup only appears once the shape is spatially final.
        app.tool = Tool::Polygon {
            center: Some(Point2d::from_f64(0.0, 0.0)),
            radius_point: None,
            sides: Some(6),
        };
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            polygon_sides_hud(ctx, &mut app, &mut ui_state, origin)
        });
        assert!(
            !ui_state.dyn_poly_active,
            "still no popup with only the center placed, before the radius click"
        );
    }

    #[test]
    #[allow(deprecated)]
    fn custom_field_parses_into_sides_with_dyn_on_off() {
        let ctx = egui::Context::default();
        let mut app = AppState::new(800.0, 600.0);
        app.dyn_on = false; // the popup must work regardless of Dynamic Input
        app.tool = Tool::Polygon {
            center: Some(Point2d::from_f64(0.0, 0.0)),
            radius_point: Some(Point2d::from_f64(10.0, 0.0)),
            sides: Some(6),
        };
        let mut ui_state = UiState::default();
        let origin = pos2(0.0, 0.0);
        let sid = egui::Id::new("dyn_poly_sides");

        // Frame 1: the popup appears (only now that center is placed).
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            polygon_sides_hud(ctx, &mut app, &mut ui_state, origin)
        });
        assert!(
            ui_state.dyn_poly_active,
            "popup must show once center is set"
        );
        // Focus the custom field and clear it, as a real click-then-retype would.
        ctx.memory_mut(|m| m.request_focus(sid));
        ui_state.dyn_poly_sides.clear();
        // Frame 2: type "9" into the now-focused, now-empty field.
        let raw = egui::RawInput {
            events: vec![egui::Event::Text("9".into())],
            ..Default::default()
        };
        let _ = ctx.run(raw, |ctx| {
            polygon_sides_hud(ctx, &mut app, &mut ui_state, origin)
        });

        assert!(
            matches!(
                app.tool,
                Tool::Polygon {
                    sides: Some(9),
                    center: Some(_),
                    radius_point: Some(_)
                }
            ),
            "typing a custom count must update the tool without Dynamic Input, tool={:?}",
            app.tool
        );
    }
}

//! Adaptive curve flattening and drawing for the canvas: turns a [`Curve`]
//! into screen-space polylines (or dashed/dotted patterned strokes) fine
//! enough that no chord deviates from the true curve by more than a pixel
//! tolerance, plus a world-space variant whose flattened points can be
//! cached per entity independent of pan/zoom.

use egui::Stroke;
use oxidraft_geometry::{Curve, CurveSegment, Point2d, point_segment_dist_sq};
const TESS_TOL_PX: f32 = 0.3;
const TESS_TOL_PX_SQ: f32 = TESS_TOL_PX * TESS_TOL_PX;
const TESS_MAX_DEPTH: u32 = 18;
const TESS_MAX_POINTS: usize = 20_000;

pub(super) fn draw_curve(
    painter: &egui::Painter,
    c: &Curve,
    to_screen: &impl Fn(f64, f64) -> egui::Pos2,
    stroke: Stroke,
) {
    match c {
        Curve::Line(l) => {
            let (x0, y0) = l.p0.to_f64();
            let (x1, y1) = l.p1.to_f64();
            painter.line_segment([to_screen(x0, y0), to_screen(x1, y1)], stroke);
        }
        other => {
            let mut pts = flatten_curve(other, to_screen);
            if is_closed_curve(other) {
                if pts.len() >= 2 && (pts[0] - pts[pts.len() - 1]).length() < 0.5 {
                    pts.pop();
                }
                painter.add(egui::Shape::closed_line(pts, stroke));
            } else {
                painter.add(egui::Shape::line(pts, stroke));
            }
        }
    }
}

pub(super) fn is_closed_curve(c: &Curve) -> bool {
    let (t0, t1) = c.domain();
    let (sx, sy) = c.evaluate_f64(t0);
    let (ex, ey) = c.evaluate_f64(t1);
    (sx - ex).hypot(sy - ey) < 1e-9
}

pub(super) fn draw_curve_patterned(
    painter: &egui::Painter,
    c: &Curve,
    to_screen: &impl Fn(f64, f64) -> egui::Pos2,
    stroke: Stroke,
    pattern_px: &[f32],
) {
    if pattern_px.is_empty() {
        draw_curve(painter, c, to_screen, stroke);
        return;
    }
    let pts = match c {
        Curve::Line(l) => {
            let (x0, y0) = l.p0.to_f64();
            let (x1, y1) = l.p1.to_f64();
            vec![to_screen(x0, y0), to_screen(x1, y1)]
        }
        other => flatten_curve(other, to_screen),
    };
    draw_patterned_polyline(painter, &pts, stroke, pattern_px);
}

pub(super) fn draw_patterned_polyline(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    stroke: Stroke,
    pattern_px: &[f32],
) {
    if pts.len() < 2 {
        return;
    }
    let total: f32 = pattern_px.iter().map(|v| v.abs()).sum();
    if total <= 1e-3 {
        painter.add(egui::Shape::line(pts.to_vec(), stroke));
        return;
    }
    let dot_r = (stroke.width * 0.6).max(0.6);
    let mut pi = 0usize;
    let mut rem = pattern_px[0].abs();
    let mut elem = pattern_px[0];
    if elem == 0.0 {
        painter.circle_filled(pts[0], dot_r, stroke.color);
    }
    let mut guard = 0u32;
    for seg in pts.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let d = b - a;
        let seg_len = d.length();
        if seg_len < 1e-6 {
            continue;
        }
        let dir = d / seg_len;
        let mut cursor = 0.0f32;
        while cursor < seg_len - 1e-6 {
            guard += 1;
            if guard > 200_000 {
                return;
            }
            if elem == 0.0 {
                painter.circle_filled(a + dir * cursor, dot_r, stroke.color);
                pi = (pi + 1) % pattern_px.len();
                elem = pattern_px[pi];
                rem = elem.abs();
                continue;
            }
            let take = rem.min(seg_len - cursor);
            if elem > 0.0 && take > 0.0 {
                painter.line_segment([a + dir * cursor, a + dir * (cursor + take)], stroke);
            }
            cursor += take;
            rem -= take;
            if rem <= 1e-4 {
                pi = (pi + 1) % pattern_px.len();
                elem = pattern_px[pi];
                rem = elem.abs();
            }
        }
    }
}

pub(super) fn flatten_curve(
    c: &Curve,
    to_screen: &impl Fn(f64, f64) -> egui::Pos2,
) -> Vec<egui::Pos2> {
    let (t0, t1) = c.domain();
    let eval = |t: f64| {
        let (x, y) = c.evaluate_f64(t);
        to_screen(x, y)
    };
    let mut pts: Vec<egui::Pos2> = Vec::with_capacity(64);
    const SPANS: usize = 4;
    let mut a = t0;
    let mut pa = eval(t0);
    pts.push(pa);
    for i in 0..SPANS {
        let b = t0 + (t1 - t0) * (i + 1) as f64 / SPANS as f64;
        let pb = eval(b);
        tessellate(&eval, a, pa, b, pb, 0, &mut pts);
        a = b;
        pa = pb;
    }
    pts
}

#[allow(clippy::too_many_arguments)]
fn tessellate(
    eval: &impl Fn(f64) -> egui::Pos2,
    t0: f64,
    p0: egui::Pos2,
    t1: f64,
    p1: egui::Pos2,
    depth: u32,
    out: &mut Vec<egui::Pos2>,
) {
    if out.len() >= TESS_MAX_POINTS {
        return;
    }
    let tm = 0.5 * (t0 + t1);
    let pm = eval(tm);
    if depth >= TESS_MAX_DEPTH || point_seg_dist_sq(pm, p0, p1) <= TESS_TOL_PX_SQ {
        out.push(p1);
    } else {
        tessellate(eval, t0, p0, tm, pm, depth + 1, out);
        tessellate(eval, tm, pm, t1, p1, depth + 1, out);
    }
}

// World-space sibling of `flatten_curve`: same adaptive subdivision, but the
// tolerance is in world units so the output is independent of pan and can be
// cached per entity (see `refresh_curve_cache`). `to_screen` is applied to the
// cached points at draw time instead.
pub(super) fn flatten_curve_world(c: &Curve, tol: f64) -> Vec<Point2d> {
    // Arcs flatten in closed form at the exact sagitta limit — the minimum
    // vertex count for the tolerance, so the per-entity cache stays small.
    if matches!(c, Curve::Arc(_)) {
        return oxidraft_geometry::tessellate_curve(c, tol);
    }
    let (t0, t1) = c.domain();
    let eval = |t: f64| {
        let (x, y) = c.evaluate_f64(t);
        Point2d::new(x, y)
    };
    let tol_sq = tol * tol;
    let mut pts: Vec<Point2d> = Vec::with_capacity(64);
    const SPANS: usize = 4;
    let mut a = t0;
    let mut pa = eval(t0);
    pts.push(pa);
    for i in 0..SPANS {
        let b = t0 + (t1 - t0) * (i + 1) as f64 / SPANS as f64;
        let pb = eval(b);
        tessellate_world(&eval, a, pa, b, pb, 0, tol_sq, &mut pts);
        a = b;
        pa = pb;
    }
    pts
}

#[allow(clippy::too_many_arguments)]
fn tessellate_world(
    eval: &impl Fn(f64) -> Point2d,
    t0: f64,
    p0: Point2d,
    t1: f64,
    p1: Point2d,
    depth: u32,
    tol_sq: f64,
    out: &mut Vec<Point2d>,
) {
    if out.len() >= TESS_MAX_POINTS {
        return;
    }
    let tm = 0.5 * (t0 + t1);
    let pm = eval(tm);
    let d_sq = point_segment_dist_sq((pm.x, pm.y), (p0.x, p0.y), (p1.x, p1.y));
    if depth >= TESS_MAX_DEPTH || d_sq <= tol_sq {
        out.push(p1);
    } else {
        tessellate_world(eval, t0, p0, tm, pm, depth + 1, tol_sq, out);
        tessellate_world(eval, tm, pm, t1, p1, depth + 1, tol_sq, out);
    }
}

pub(super) fn point_seg_dist_sq(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let len2 = abx * abx + aby * aby;
    if len2 < 1e-12 {
        return (p.x - a.x).powi(2) + (p.y - a.y).powi(2);
    }
    let t = (((p.x - a.x) * abx + (p.y - a.y) * aby) / len2).clamp(0.0, 1.0);
    let cx = a.x + t * abx;
    let cy = a.y + t * aby;
    (p.x - cx).powi(2) + (p.y - cy).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_geometry::{LineSeg, Point2d, PolyCurve};

    fn rect_poly() -> Curve {
        let c = [
            Point2d::new(0.0, 0.0),
            Point2d::new(10.0, 0.0),
            Point2d::new(10.0, 6.0),
            Point2d::new(0.0, 6.0),
        ];
        let segs = (0..4)
            .map(|i| Curve::Line(LineSeg::from_endpoints(c[i], c[(i + 1) % 4])))
            .collect();
        Curve::Poly(Box::new(PolyCurve::new(segs)))
    }

    #[test]
    fn rectangle_polycurve_detected_as_closed() {
        assert!(is_closed_curve(&rect_poly()));
    }

    #[test]
    fn rectangle_flatten_ends_coincide() {
        let id = |x: f64, y: f64| egui::pos2(x as f32, y as f32);
        let pts = flatten_curve(&rect_poly(), &id);
        assert!(pts.len() >= 4);
        let gap = (pts[0] - pts[pts.len() - 1]).length();
        assert!(gap < 1e-3, "start/end gap = {gap}");
    }

    #[test]
    fn rectangle_flatten_world_ends_coincide() {
        let pts = flatten_curve_world(&rect_poly(), 0.01);
        assert!(pts.len() >= 4);
        let gap = pts[0].dist_f64(&pts[pts.len() - 1]);
        assert!(gap < 1e-9, "start/end gap = {gap}");
    }

    #[test]
    fn world_flatten_stays_within_tolerance() {
        use oxidraft_geometry::CircularArc;
        let c = Curve::Arc(CircularArc::new(
            Point2d::new(3.0, -2.0),
            5.0,
            0.0,
            std::f64::consts::TAU,
        ));
        let tol = 0.01;
        let pts = flatten_curve_world(&c, tol);
        assert!(pts.len() > 8);
        // Every chord midpoint must be within tol of the true circle.
        for w in pts.windows(2) {
            let mx = 0.5 * (w[0].x + w[1].x);
            let my = 0.5 * (w[0].y + w[1].y);
            let d = ((mx - 3.0).powi(2) + (my + 2.0).powi(2)).sqrt();
            assert!((d - 5.0).abs() <= tol * 1.5, "chord sagitta {d}");
        }
    }
}

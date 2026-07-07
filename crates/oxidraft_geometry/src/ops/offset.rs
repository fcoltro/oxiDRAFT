use crate::curve::{Curve, CurveSegment};
use crate::nurbs::NurbsCurve;
use crate::point::Point2d;
use crate::primitives::{CircularArc, CubicBezier, LineSeg};

pub fn offset_curve(curve: &Curve, dist: f64) -> Curve {
    match curve {
        Curve::Line(l) => Curve::Line(l.offset_exact(dist)),
        Curve::Arc(a) => {
            // An inward offset past the center is degenerate: the true offset
            // collapses through the center. Clamp to a point-like arc instead
            // of reflecting the radius, which silently produced an arc on the
            // wrong side of the center.
            let r = (a.radius + dist).max(1e-12);
            Curve::Arc(CircularArc::new(a.center, r, a.start_angle, a.end_angle))
        }
        Curve::Bezier(bz) => offset_bezier(bz, dist),
        Curve::Nurbs(nc) => offset_nurbs(nc, dist),
        Curve::Poly(pc) => offset_polycurve(pc, dist),
        Curve::Ellipse(_) | Curve::Rational(_) => offset_by_sampling(curve, dist),
    }
}

fn offset_polycurve(pc: &crate::primitives::PolyCurve, dist: f64) -> Curve {
    use crate::primitives::PolyCurve;
    let n = pc.segments.len();
    if n == 0 {
        return Curve::Poly(Box::new(pc.clone()));
    }

    let mut offs: Vec<Curve> = pc.segments.iter().map(|s| offset_curve(s, dist)).collect();

    let (first_start, _) = seg_ends(&pc.segments[0]);
    let (_, last_end) = seg_ends(&pc.segments[n - 1]);
    let closed = (first_start.0 - last_end.0).hypot(first_start.1 - last_end.1) < 1e-9;

    let joints = if closed { n } else { n.saturating_sub(1) };
    // AutoCAD-style miter limit: a spike longer than this many offset widths
    // (near-parallel reflex joints shoot toward infinity) is beveled instead.
    let miter_limit = 4.0 * dist.abs().max(1e-12);
    let mut bevels: Vec<Option<Curve>> = vec![None; n];
    for j in 0..joints {
        let k = (j + 1) % n;
        if try_miter(&mut offs, j, k, miter_limit) {
            continue;
        }
        // Non-line joints (arcs, subdivided Béziers) and over-limit miters get
        // a bevel segment so the offset chain stays G0-connected; the old code
        // silently left a gap here.
        let (_, pe) = seg_ends(&offs[j]);
        let (ps, _) = seg_ends(&offs[k]);
        if (pe.0 - ps.0).hypot(pe.1 - ps.1) > 1e-9 {
            bevels[j] = Some(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(pe.0, pe.1),
                Point2d::from_f64(ps.0, ps.1),
            )));
        }
    }
    let mut segs: Vec<Curve> = Vec::with_capacity(n);
    for (i, c) in offs.into_iter().enumerate() {
        segs.push(c);
        if let Some(b) = bevels[i].take() {
            segs.push(b);
        }
    }
    Curve::Poly(Box::new(PolyCurve::new(segs)))
}

/// Extends two adjacent offset lines to their intersection. Returns false when
/// either segment is not a line, the lines are parallel with a real gap, or
/// the miter point spikes beyond `limit` — the caller bevels those.
fn try_miter(offs: &mut [Curve], i: usize, k: usize, limit: f64) -> bool {
    let (a, b) = match (as_line_f64(&offs[i]), as_line_f64(&offs[k])) {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    let Some(x) = infinite_line_intersection(a, b) else {
        // Parallel: a straight continuation already meets end-to-start and
        // needs no join at all; an offset parallel gap needs a bevel.
        return (a.1.0 - b.0.0).hypot(a.1.1 - b.0.1) <= 1e-9;
    };
    let spike_i = (x.0 - a.1.0).hypot(x.1 - a.1.1);
    let spike_k = (x.0 - b.0.0).hypot(x.1 - b.0.1);
    if spike_i > limit || spike_k > limit {
        return false;
    }
    set_line_p1(&mut offs[i], x);
    set_line_p0(&mut offs[k], x);
    true
}

fn seg_ends(c: &Curve) -> ((f64, f64), (f64, f64)) {
    let (t0, t1) = c.domain();
    (c.evaluate_f64(t0), c.evaluate_f64(t1))
}

fn as_line_f64(c: &Curve) -> Option<((f64, f64), (f64, f64))> {
    match c {
        Curve::Line(l) => Some((l.p0.to_f64(), l.p1.to_f64())),
        _ => None,
    }
}

fn set_line_p0(c: &mut Curve, x: (f64, f64)) {
    if let Curve::Line(l) = c {
        l.p0 = Point2d::from_f64(x.0, x.1);
    }
}
fn set_line_p1(c: &mut Curve, x: (f64, f64)) {
    if let Curve::Line(l) = c {
        l.p1 = Point2d::from_f64(x.0, x.1);
    }
}

fn infinite_line_intersection(
    a: ((f64, f64), (f64, f64)),
    b: ((f64, f64), (f64, f64)),
) -> Option<(f64, f64)> {
    let imp =
        |((x0, y0), (x1, y1)): ((f64, f64), (f64, f64))| (y0 - y1, x1 - x0, x0 * y1 - x1 * y0);
    let (a1, b1, c1) = imp(a);
    let (a2, b2, c2) = imp(b);
    let det = a1 * b2 - a2 * b1;
    if det.abs() < 1e-12 {
        return None;
    }
    Some(((b1 * c2 - b2 * c1) / det, (a2 * c1 - a1 * c2) / det))
}

fn offset_nurbs(nc: &NurbsCurve, dist: f64) -> Curve {
    let m = nc.control.len();
    if m < 2 {
        return offset_by_sampling(&Curve::Nurbs(nc.clone()), dist);
    }
    let params: Vec<f64> = (0..m).map(|k| k as f64 / (m - 1) as f64).collect();
    let data: Vec<Point2d> = params
        .iter()
        .map(|&t| {
            let (px, py) = nc.evaluate_f64(t);
            let (tx, ty) = nc.tangent_f64(t);
            let len = (tx * tx + ty * ty).sqrt().max(1e-12);
            Point2d::from_f64(px + dist * (-ty / len), py + dist * (tx / len))
        })
        .collect();
    match interpolate_nurbs(&data, &nc.weights) {
        Some(fit) => Curve::Nurbs(fit),
        None => offset_by_sampling(&Curve::Nurbs(nc.clone()), dist),
    }
}

pub fn interpolate_nurbs(data: &[Point2d], weights: &[f64]) -> Option<NurbsCurve> {
    let m = data.len();
    if m < 2 || weights.len() != m {
        return None;
    }
    let params: Vec<f64> = (0..m).map(|k| k as f64 / (m - 1) as f64).collect();
    let mut qx: Vec<f64> = data.iter().map(|p| p.x).collect();
    let mut qy: Vec<f64> = data.iter().map(|p| p.y).collect();

    let mat: Vec<Vec<f64>> = params
        .iter()
        .map(|&t| crate::nurbs::rational_basis_all(m, weights, t))
        .collect();
    let mut mat = mat;
    solve2(&mut mat, &mut qx, &mut qy).map(|()| {
        let control = qx
            .iter()
            .zip(&qy)
            .map(|(&x, &y)| Point2d::from_f64(x, y))
            .collect();
        NurbsCurve::new(control, weights.to_vec())
    })
}

pub fn refit_nurbs_subcurve(nc: &NurbsCurve, a: f64, b: f64) -> NurbsCurve {
    let m = nc.control.len().max(2);
    let data: Vec<Point2d> = (0..m)
        .map(|k| {
            let t = a + (b - a) * (k as f64 / (m - 1) as f64);
            let (x, y) = nc.evaluate_f64(t);
            Point2d::from_f64(x, y)
        })
        .collect();
    let weights = vec![1.0; m];
    interpolate_nurbs(&data, &weights).unwrap_or_else(|| nc.clone())
}

#[allow(clippy::needless_range_loop)]
fn solve2(a: &mut [Vec<f64>], b1: &mut [f64], b2: &mut [f64]) -> Option<()> {
    let n = a.len();
    for col in 0..n {
        let mut piv = col;
        let mut best = a[col][col].abs();
        for r in (col + 1)..n {
            if a[r][col].abs() > best {
                best = a[r][col].abs();
                piv = r;
            }
        }
        if best < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        b1.swap(col, piv);
        b2.swap(col, piv);
        for r in (col + 1)..n {
            let f = a[r][col] / a[col][col];
            if f == 0.0 {
                continue;
            }
            for c in col..n {
                a[r][c] -= f * a[col][c];
            }
            b1[r] -= f * b1[col];
            b2[r] -= f * b2[col];
        }
    }
    for col in (0..n).rev() {
        let mut s1 = b1[col];
        let mut s2 = b2[col];
        for c in (col + 1)..n {
            s1 -= a[col][c] * b1[c];
            s2 -= a[col][c] * b2[c];
        }
        b1[col] = s1 / a[col][col];
        b2[col] = s2 / a[col][col];
    }
    Some(())
}

/// Offset a cubic with error control: fit a single-cubic candidate, measure
/// how far its points sit from the true offset distance, and subdivide the
/// base curve until every piece is within tolerance. The old single-fit
/// version was visibly wrong on high-curvature segments.
fn offset_bezier(bz: &CubicBezier, dist: f64) -> Curve {
    let tol = (dist.abs() * 1e-3).max(1e-9);
    let mut pieces: Vec<Curve> = Vec::new();
    offset_bezier_adaptive(bz, dist, tol, 0, &mut pieces);
    if pieces.len() == 1 {
        pieces.pop().expect("len checked")
    } else {
        Curve::Poly(Box::new(crate::primitives::PolyCurve::new(pieces)))
    }
}

fn offset_bezier_adaptive(bz: &CubicBezier, dist: f64, tol: f64, depth: u32, out: &mut Vec<Curve>) {
    let candidate = offset_bezier_single(bz, dist);
    // Error metric: sample the candidate and measure how far each sample is
    // from the base curve; a perfect offset sits at |dist| everywhere. A cusp
    // (|dist| beyond the curvature radius) can never converge, so the depth
    // cap keeps degenerate inputs from recursing forever.
    if depth >= 8 || offset_error(&candidate, bz, dist) <= tol {
        out.push(Curve::Bezier(candidate));
        return;
    }
    let (left, right) = split_cubic(bz, 0.5);
    offset_bezier_adaptive(&left, dist, tol, depth + 1, out);
    offset_bezier_adaptive(&right, dist, tol, depth + 1, out);
}

fn offset_error(candidate: &CubicBezier, base: &CubicBezier, dist: f64) -> f64 {
    let base_curve = Curve::Bezier(base.clone());
    let mut worst = 0.0f64;
    for k in 1..6 {
        let t = k as f64 / 6.0;
        let (x, y) = candidate.evaluate_f64(t);
        let d = crate::ops::point_to_curve_distance(&base_curve, x, y);
        worst = worst.max((d - dist.abs()).abs());
    }
    worst
}

fn split_cubic(bz: &CubicBezier, t: f64) -> (CubicBezier, CubicBezier) {
    let l = |a: Point2d, b: Point2d| a.lerp(&b, t);
    let p01 = l(bz.p0, bz.p1);
    let p12 = l(bz.p1, bz.p2);
    let p23 = l(bz.p2, bz.p3);
    let p012 = l(p01, p12);
    let p123 = l(p12, p23);
    let mid = l(p012, p123);
    (
        CubicBezier::new(bz.p0, p01, p012, mid),
        CubicBezier::new(mid, p123, p23, bz.p3),
    )
}

fn offset_bezier_single(bz: &CubicBezier, dist: f64) -> CubicBezier {
    let ts = [0.0f64, 1.0 / 3.0, 2.0 / 3.0, 1.0];
    let mut offset_pts = [(0.0f64, 0.0f64); 4];

    for (i, &t) in ts.iter().enumerate() {
        let (px, py) = bz.evaluate_f64(t);
        let (tx, ty) = bz.tangent_f64(t);
        let len = (tx * tx + ty * ty).sqrt().max(1e-20);
        let (nx, ny) = (-ty / len, tx / len);
        offset_pts[i] = (px + dist * nx, py + dist * ny);
    }

    let p0 = Point2d::from_f64(offset_pts[0].0, offset_pts[0].1);
    let p3 = Point2d::from_f64(offset_pts[3].0, offset_pts[3].1);

    let (t0x, t0y) = bz.tangent_f64(0.0);
    let (t1x, t1y) = bz.tangent_f64(1.0);
    let chord = ((offset_pts[3].0 - offset_pts[0].0).powi(2)
        + (offset_pts[3].1 - offset_pts[0].1).powi(2))
    .sqrt();
    let scale = chord / 3.0;

    let p1 = Point2d::from_f64(
        offset_pts[0].0 + t0x * scale / (t0x * t0x + t0y * t0y).sqrt().max(1e-20),
        offset_pts[0].1 + t0y * scale / (t0x * t0x + t0y * t0y).sqrt().max(1e-20),
    );
    let p2 = Point2d::from_f64(
        offset_pts[3].0 - t1x * scale / (t1x * t1x + t1y * t1y).sqrt().max(1e-20),
        offset_pts[3].1 - t1y * scale / (t1x * t1x + t1y * t1y).sqrt().max(1e-20),
    );

    CubicBezier::new(p0, p1, p2, p3)
}

fn offset_by_sampling(curve: &Curve, dist: f64) -> Curve {
    use crate::primitives::PolyCurve;

    let (t0, t1) = curve.domain();
    let steps = 16usize;
    let mut segs = Vec::new();
    let mut prev_pt: Option<(f64, f64)> = None;

    for i in 0..=steps {
        let t = t0 + (t1 - t0) * i as f64 / steps as f64;
        let (px, py) = curve.evaluate_f64(t);
        let (tx, ty) = curve.tangent_f64(t);
        let len = (tx * tx + ty * ty).sqrt().max(1e-20);
        let (nx, ny) = (-ty / len, tx / len);
        let op = (px + dist * nx, py + dist * ny);
        if let Some(prev) = prev_pt {
            segs.push(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(prev.0, prev.1),
                Point2d::from_f64(op.0, op.1),
            )));
        }
        prev_pt = Some(op);
    }
    Curve::Poly(Box::new(PolyCurve::new(segs)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::Point2d;
    use crate::primitives::LineSeg;

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn offset_horizontal_line() {
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 0)));
        let off = offset_curve(&line, 1.0);
        if let Curve::Line(l) = off {
            let y0 = l.p0.y;
            let y1 = l.p1.y;
            assert!((y0 - 1.0).abs() < 1e-5, "y0={}", y0);
            assert!((y1 - 1.0).abs() < 1e-5, "y1={}", y1);
        } else {
            panic!("Expected Line");
        }
    }

    #[test]
    fn offset_circle_increases_radius() {
        let arc = Curve::Arc(CircularArc::new(
            pt(0, 0),
            3.0,
            0.0,
            2.0 * std::f64::consts::PI,
        ));
        let off = offset_curve(&arc, 2.0);
        if let Curve::Arc(a) = off {
            assert!((a.radius - 5.0).abs() < 1e-9);
        } else {
            panic!("Expected Arc");
        }
    }

    #[test]
    fn offset_spline_stays_a_spline() {
        use crate::nurbs::NurbsCurve;
        let cvs = vec![pt(0, 0), pt(2, 4), pt(6, 4), pt(8, 0), pt(10, 4), pt(12, 0)];
        let spline = Curve::Nurbs(NurbsCurve::uniform(cvs.clone()));
        let dist = 1.0;
        let off = offset_curve(&spline, dist);

        let nc = match &off {
            Curve::Nurbs(nc) => nc,
            other => panic!("expected a spline offset, got {:?}", other),
        };
        assert_eq!(
            nc.control.len(),
            cvs.len(),
            "control-vertex count preserved"
        );

        let m = cvs.len();
        for k in 0..m {
            let t = k as f64 / (m - 1) as f64;
            let (px, py) = spline.evaluate_f64(t);
            let (ox, oy) = off.evaluate_f64(t);
            let d = ((ox - px).powi(2) + (oy - py).powi(2)).sqrt();
            assert!(
                (d - dist).abs() < 1e-6,
                "node {k}: offset distance {d}, want {dist}"
            );
        }
    }

    #[test]
    fn offset_square_polycurve_miters_corners() {
        use crate::primitives::PolyCurve;
        let segs = vec![
            Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 0))),
            Curve::Line(LineSeg::from_endpoints(pt(4, 0), pt(4, 4))),
            Curve::Line(LineSeg::from_endpoints(pt(4, 4), pt(0, 4))),
            Curve::Line(LineSeg::from_endpoints(pt(0, 4), pt(0, 0))),
        ];
        let sq = Curve::Poly(Box::new(PolyCurve::new(segs)));
        let off = offset_curve(&sq, 1.0);
        let pc = match &off {
            Curve::Poly(pc) => pc,
            o => panic!("expected Poly, got {:?}", o),
        };
        assert_eq!(pc.segments.len(), 4, "still 4 sides — no jitter facets");

        let near = |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).hypot(p.1 - q.1) < 1e-9;
        let (s0, e0) = seg_ends(&pc.segments[0]);
        assert!(
            near(s0, (1.0, 1.0)) && near(e0, (3.0, 1.0)),
            "bottom edge {s0:?}->{e0:?}"
        );
        for j in 0..4 {
            let (_, end) = seg_ends(&pc.segments[j]);
            let (start_next, _) = seg_ends(&pc.segments[(j + 1) % 4]);
            assert!(
                near(end, start_next),
                "corner {j} discontinuous: {end:?} vs {start_next:?}"
            );
        }
    }

    #[test]
    fn offset_arc_past_center_collapses_instead_of_reflecting() {
        let arc = Curve::Arc(CircularArc::new(
            pt(0, 0),
            5.0,
            0.0,
            2.0 * std::f64::consts::PI,
        ));
        let off = offset_curve(&arc, -8.0);
        if let Curve::Arc(a) = off {
            assert!(
                a.radius <= 1e-9,
                "inward offset past the center must collapse, got radius {}",
                a.radius
            );
        } else {
            panic!("Expected Arc");
        }
    }

    #[test]
    fn offset_high_curvature_bezier_stays_within_tolerance() {
        use crate::ops::point_to_curve_distance;
        // A tight open U-turn: the single-cubic fit is visibly wrong here and
        // the adaptive subdivision must bring the error down. (A closed or
        // self-near curve would break the error metric: offset points near
        // one end measure their distance against the other branch.)
        let bz = CubicBezier::new(
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(0.0, 6.0),
            Point2d::from_f64(4.0, 6.0),
            Point2d::from_f64(4.0, 0.0),
        );
        let dist = 0.5;
        let base = Curve::Bezier(bz.clone());
        let off = offset_curve(&base, dist);
        let (t0, t1) = off.domain();
        let mut worst = 0.0f64;
        for k in 0..=200 {
            let t = t0 + (t1 - t0) * k as f64 / 200.0;
            let (x, y) = off.evaluate_f64(t);
            let d = point_to_curve_distance(&base, x, y);
            worst = worst.max((d - dist).abs());
        }
        assert!(
            worst < 0.01,
            "offset must stay within 1% of the requested distance, worst error {worst}"
        );
    }

    #[test]
    fn offset_sharp_spike_is_beveled_not_mitred_to_infinity() {
        use crate::primitives::PolyCurve;
        // Nearly-parallel V: the miter point would be ~40 units away for a
        // 1-unit offset. It must be beveled within the miter limit instead.
        let segs = vec![
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(0.0, 0.0),
                Point2d::from_f64(10.0, 0.0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(10.0, 0.0),
                Point2d::from_f64(0.0, 0.5),
            )),
        ];
        let v = Curve::Poly(Box::new(PolyCurve::new(segs)));
        let off = offset_curve(&v, 1.0);
        let pc = match &off {
            Curve::Poly(pc) => pc,
            o => panic!("expected Poly, got {:?}", o),
        };
        assert_eq!(pc.segments.len(), 3, "two sides + one bevel");
        assert!(
            pc.check_g0(1e-9),
            "beveled offset chain must stay connected"
        );
        for seg in &pc.segments {
            let (s, e) = seg_ends(seg);
            for (x, y) in [s, e] {
                assert!(
                    (-2.0..=16.0).contains(&x) && y.abs() <= 6.0,
                    "vertex ({x}, {y}) spiked past the miter limit"
                );
            }
        }
    }

    #[test]
    fn offset_line_arc_joint_is_closed_with_bevel() {
        use crate::primitives::PolyCurve;
        // Perpendicular line→arc joint: the offset pieces separate and the
        // old code left the gap open.
        let segs = vec![
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(0.0, 0.0),
                Point2d::from_f64(4.0, 0.0),
            )),
            Curve::Arc(CircularArc::new(
                Point2d::from_f64(2.0, 0.0),
                2.0,
                0.0,
                std::f64::consts::FRAC_PI_2,
            )),
        ];
        let chain = Curve::Poly(Box::new(PolyCurve::new(segs)));
        let off = offset_curve(&chain, 0.5);
        let pc = match &off {
            Curve::Poly(pc) => pc,
            o => panic!("expected Poly, got {:?}", o),
        };
        assert!(
            pc.check_g0(1e-9),
            "offset chain across a line/arc joint must be G0-connected"
        );
    }

    #[test]
    fn offset_circle_decreases_radius() {
        let arc = Curve::Arc(CircularArc::new(
            pt(0, 0),
            5.0,
            0.0,
            2.0 * std::f64::consts::PI,
        ));
        let off = offset_curve(&arc, -2.0);
        if let Curve::Arc(a) = off {
            assert!((a.radius - 3.0).abs() < 1e-9);
        } else {
            panic!("Expected Arc");
        }
    }
}

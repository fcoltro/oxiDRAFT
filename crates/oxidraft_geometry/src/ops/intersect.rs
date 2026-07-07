use crate::curve::{Curve, CurveSegment};
use crate::point::Point2d;
use crate::primitives::{CircularArc, LineSeg};

#[derive(Clone, Debug)]
pub struct CurveIntersection {
    pub point: (f64, f64),
    pub t1: f64,
    pub t2: f64,
}

pub fn intersect_line_line(l1: &LineSeg, l2: &LineSeg) -> Option<CurveIntersection> {
    intersect_segments_f64(
        l1.p0.to_f64(),
        l1.p1.to_f64(),
        l2.p0.to_f64(),
        l2.p1.to_f64(),
    )
    .map(|(point, t1, t2)| CurveIntersection { point, t1, t2 })
}

pub fn intersect_lines_unbounded(l1: &LineSeg, l2: &LineSeg) -> Option<Point2d> {
    let (x1, y1) = l1.p0.to_f64();
    let (x2, y2) = l1.p1.to_f64();
    let (x3, y3) = l2.p0.to_f64();
    let (x4, y4) = l2.p1.to_f64();
    let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    let scale = (x1 - x2).hypot(y1 - y2) * (x3 - x4).hypot(y3 - y4);
    if denom.abs() <= 1e-12 * scale.max(1.0) {
        return None;
    }
    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
    Some(Point2d::from_f64(x1 + t * (x2 - x1), y1 + t * (y2 - y1)))
}

pub fn intersect_line_circle(line: &LineSeg, arc: &CircularArc) -> Vec<CurveIntersection> {
    let (ax, ay) = line.p0.to_f64();
    let (bx, by) = line.p1.to_f64();
    let (cx, cy) = arc.center.to_f64();
    let r = arc.radius;

    let (dx, dy) = (bx - ax, by - ay);
    let (fx, fy) = (ax - cx, ay - cy);
    let qa = dx * dx + dy * dy;
    if qa < 1e-20 {
        return vec![];
    }
    let qb = 2.0 * (fx * dx + fy * dy);
    let qc = fx * fx + fy * fy - r * r;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return vec![];
    }
    let sq = disc.sqrt();
    let mut ts = vec![(-qb - sq) / (2.0 * qa)];
    if sq > 1e-12 {
        ts.push((-qb + sq) / (2.0 * qa));
    }

    let mut results = Vec::new();
    for t in ts {
        if !(-1e-9..=1.0 + 1e-9).contains(&t) {
            continue;
        }
        let t1 = t.clamp(0.0, 1.0);
        let (px, py) = (ax + t1 * dx, ay + t1 * dy);
        let angle = (py - cy).atan2(px - cx);
        if angle_in_arc(angle, arc.start_angle, arc.end_angle) {
            results.push(CurveIntersection {
                point: (px, py),
                t1,
                t2: angle_on_domain(angle, arc.start_angle, arc.end_angle),
            });
        }
    }
    results
}

// A reversed arc (`end < start`, produced by `reverse_curve` or clockwise
// drawing) covers the *same* angle set as its CCW twin — treating its span as
// the 2π-complement admitted phantom hits on angles the arc never visits.
// Order the bounds first, then reason CCW from the lower one.
fn angle_in_arc(angle: f64, start: f64, end: f64) -> bool {
    let (lo, hi) = if end >= start {
        (start, end)
    } else {
        (end, start)
    };
    let pi2 = 2.0 * std::f64::consts::PI;
    let a = crate::util::wrap_tau(angle - lo);
    let mut span = hi - lo;
    if span <= 0.0 {
        span += pi2;
    }
    a <= span + 1e-9
}

/// Maps a hit angle into the arc's own domain interval; the returned value is
/// a valid `evaluate_f64` parameter whether the arc runs CCW or CW.
fn angle_on_domain(angle: f64, start: f64, end: f64) -> f64 {
    let (lo, hi) = if end >= start {
        (start, end)
    } else {
        (end, start)
    };
    let pi2 = 2.0 * std::f64::consts::PI;
    let a = crate::util::wrap_tau(angle - lo);
    let mut span = hi - lo;
    if span <= 0.0 {
        span += pi2;
    }
    lo + a.min(span)
}

pub fn intersect_circle_circle(c1: &CircularArc, c2: &CircularArc) -> Vec<CurveIntersection> {
    let (cx1, cy1) = c1.center.to_f64();
    let (cx2, cy2) = c2.center.to_f64();
    let r1 = c1.radius;
    let r2 = c2.radius;

    let dx = cx2 - cx1;
    let dy = cy2 - cy1;
    let d = (dx * dx + dy * dy).sqrt();

    if d < 1e-12 || d > r1 + r2 + 1e-10 || d < (r1 - r2).abs() - 1e-10 {
        return vec![];
    }

    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h_sq = r1 * r1 - a * a;
    let h = h_sq.max(0.0).sqrt();

    let ux = dx / d;
    let uy = dy / d;

    let mx = cx1 + a * ux;
    let my = cy1 + a * uy;

    let mut results = Vec::new();
    let signs: &[f64] = if h < 1e-9 { &[0.0] } else { &[-1.0, 1.0] };

    for &sign in signs {
        let px = mx + sign * h * (-uy);
        let py = my + sign * h * ux;

        let angle1 = (py - cy1).atan2(px - cx1);
        let angle2 = (py - cy2).atan2(px - cx2);

        if angle_in_arc(angle1, c1.start_angle, c1.end_angle)
            && angle_in_arc(angle2, c2.start_angle, c2.end_angle)
        {
            results.push(CurveIntersection {
                point: (px, py),
                t1: angle_on_domain(angle1, c1.start_angle, c1.end_angle),
                t2: angle_on_domain(angle2, c2.start_angle, c2.end_angle),
            });
        }
    }
    results
}

fn intersect_segments_f64(
    pa: (f64, f64),
    pb: (f64, f64),
    qa: (f64, f64),
    qb: (f64, f64),
) -> Option<((f64, f64), f64, f64)> {
    let ux = pb.0 - pa.0;
    let uy = pb.1 - pa.1;
    let vx = qb.0 - qa.0;
    let vy = qb.1 - qa.1;

    // Relative parallelism test: the cross product scales with both segment lengths,
    // so a fixed absolute floor mislabels long near-parallel segments. Scale by them.
    let denom = ux * vy - uy * vx;
    let scale = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
    if denom.abs() <= 1e-12 * scale.max(1.0) {
        return None;
    }

    let dx = qa.0 - pa.0;
    let dy = qa.1 - pa.1;

    let t = (dx * vy - dy * vx) / denom;
    let s = (dx * uy - dy * ux) / denom;

    let eps = 1e-9;
    if (-eps..=1.0 + eps).contains(&t) && (-eps..=1.0 + eps).contains(&s) {
        let t_clamped = t.clamp(0.0, 1.0);
        let s_clamped = s.clamp(0.0, 1.0);
        let x = pa.0 + t_clamped * ux;
        let y = pa.1 + t_clamped * uy;
        Some(((x, y), t_clamped, s_clamped))
    } else {
        None
    }
}

fn refine_intersection(c1: &Curve, c2: &Curve, t1_init: f64, t2_init: f64) -> CurveIntersection {
    // Reversed arcs/ellipses report a descending domain; order the bounds so
    // the clamps below never see min > max.
    let (d0_1, d1_1) = c1.domain();
    let (d0_2, d1_2) = c2.domain();
    let (t0_1, t1_1) = (d0_1.min(d1_1), d0_1.max(d1_1));
    let (t0_2, t1_2) = (d0_2.min(d1_2), d0_2.max(d1_2));

    // Central-difference derivatives w.r.t. the *global* parameter. Composite
    // curves (Poly, Nurbs) report segment-local tangents whose magnitude
    // misses the segment→global chain-rule factor; a Newton step scaled by
    // that factor overshoots and trips the divergence guard, leaving the
    // coarse seed unrefined. Differencing evaluate_f64 sidesteps the
    // per-variant tangent scaling entirely.
    let deriv = |c: &Curve, t: f64, lo: f64, hi: f64| {
        let h = (hi - lo).abs().max(1e-12) * 1e-7;
        let a = (t - h).max(lo);
        let b = (t + h).min(hi);
        let (ax, ay) = c.evaluate_f64(a);
        let (bx, by) = c.evaluate_f64(b);
        let dt = (b - a).max(1e-300);
        ((bx - ax) / dt, (by - ay) / dt)
    };

    let mut t1 = t1_init;
    let mut t2 = t2_init;
    let step_cap_1 = (t1_1 - t0_1).abs() * 0.1;
    let step_cap_2 = (t1_2 - t0_2).abs() * 0.1;

    for _ in 0..20 {
        let (x1, y1) = c1.evaluate_f64(t1);
        let (x2, y2) = c2.evaluate_f64(t2);

        let rx = x1 - x2;
        let ry = y1 - y2;

        if (rx * rx + ry * ry).sqrt() < 1e-12 {
            break;
        }

        let (dx1, dy1) = deriv(c1, t1, t0_1, t1_1);
        let (dx2, dy2) = deriv(c2, t2, t0_2, t1_2);

        let det = -dx1 * dy2 + dy1 * dx2;
        if det.abs() < 1e-12 {
            break;
        }

        let dt1 = (rx * dy2 - ry * dx2) / det;
        let dt2 = (-dx1 * ry + dy1 * rx) / det;

        // Diverging away from the seeded chord means the seed was bogus —
        // stop rather than walk to an unrelated root.
        if dt1.abs() > step_cap_1 || dt2.abs() > step_cap_2 {
            break;
        }

        t1 = (t1 + dt1).clamp(t0_1, t1_1);
        t2 = (t2 + dt2).clamp(t0_2, t1_2);
    }

    let point = c1.evaluate_f64(t1);
    CurveIntersection { point, t1, t2 }
}

pub fn intersect_general(c1: &Curve, c2: &Curve) -> Vec<CurveIntersection> {
    if let Curve::Poly(p) = c1 {
        let n = p.segments.len().max(1) as f64;
        let mut out: Vec<CurveIntersection> = Vec::new();
        for (i, seg) in p.segments.iter().enumerate() {
            let (s0, s1) = seg.domain();
            for h in intersect(seg, c2) {
                let local = if (s1 - s0).abs() < 1e-12 {
                    0.0
                } else {
                    (h.t1 - s0) / (s1 - s0)
                };
                let global = (i as f64 + local.clamp(0.0, 1.0)) / n;
                if out
                    .iter()
                    .all(|o| (o.point.0 - h.point.0).hypot(o.point.1 - h.point.1) >= 1e-5)
                {
                    out.push(CurveIntersection {
                        point: h.point,
                        t1: global,
                        t2: h.t2,
                    });
                }
            }
        }
        return out;
    }
    if let Curve::Poly(p) = c2 {
        let n = p.segments.len().max(1) as f64;
        let mut out: Vec<CurveIntersection> = Vec::new();
        for (i, seg) in p.segments.iter().enumerate() {
            let (s0, s1) = seg.domain();
            for h in intersect(c1, seg) {
                let local = if (s1 - s0).abs() < 1e-12 {
                    0.0
                } else {
                    (h.t2 - s0) / (s1 - s0)
                };
                let global = (i as f64 + local.clamp(0.0, 1.0)) / n;
                if out
                    .iter()
                    .all(|o| (o.point.0 - h.point.0).hypot(o.point.1 - h.point.1) >= 1e-5)
                {
                    out.push(CurveIntersection {
                        point: h.point,
                        t1: h.t1,
                        t2: global,
                    });
                }
            }
        }
        return out;
    }

    // Seed with tolerance-driven adaptive flattenings instead of a fixed
    // sample count: a fixed grid aliases past pairs of crossings that fall
    // inside one sample span (high-curvature splines against lines were the
    // worst case). Every chord crossing is then Newton-refined on the true
    // curves.
    let bb = c1.bounding_box().union(&c2.bounding_box());
    let diag = bb.min.dist_f64(&bb.max).max(1e-12);
    let tol = diag * 1e-4;
    let pts1 = flatten_params(c1, tol);
    let pts2 = flatten_params(c2, tol);

    let dedup_pt = diag * 1e-7;
    let mut intersections: Vec<CurveIntersection> = Vec::new();
    for w1 in pts1.windows(2) {
        let (u0, pa) = w1[0];
        let (u1, pb) = w1[1];
        let (axmin, axmax) = (pa.0.min(pb.0), pa.0.max(pb.0));
        let (aymin, aymax) = (pa.1.min(pb.1), pa.1.max(pb.1));
        for w2 in pts2.windows(2) {
            let (v0, qa) = w2[0];
            let (v1, qb) = w2[1];
            if qa.0.min(qb.0) > axmax + tol
                || qa.0.max(qb.0) < axmin - tol
                || qa.1.min(qb.1) > aymax + tol
                || qa.1.max(qb.1) < aymin - tol
            {
                continue;
            }
            if let Some((_, t_seg, s_seg)) = intersect_segments_f64(pa, pb, qa, qb) {
                let t1_approx = u0 + t_seg * (u1 - u0);
                let t2_approx = v0 + s_seg * (v1 - v0);

                let hit = refine_intersection(c1, c2, t1_approx, t2_approx);

                if !intersections.iter().any(|other: &CurveIntersection| {
                    let dx = other.point.0 - hit.point.0;
                    let dy = other.point.1 - hit.point.1;
                    (dx * dx + dy * dy).sqrt() < dedup_pt
                }) {
                    intersections.push(hit);
                }
            }
        }
    }

    intersections
}

/// Adaptive flattening that carries the curve parameter with every vertex, so
/// a chord hit can be mapped back to a Newton seed on the true curve.
fn flatten_params(c: &Curve, tol: f64) -> Vec<(f64, (f64, f64))> {
    let (t0, t1) = c.domain();
    if matches!(c, Curve::Line(_)) {
        return vec![(t0, c.evaluate_f64(t0)), (t1, c.evaluate_f64(t1))];
    }
    // Several initial spans so symmetric shapes (a full circle's chord touches
    // its own midpoint) can't fool the flatness test at the first level.
    const SPANS: usize = 8;
    let mut out = Vec::with_capacity(64);
    let mut a = t0;
    let mut pa = c.evaluate_f64(t0);
    out.push((a, pa));
    for i in 0..SPANS {
        let b = t0 + (t1 - t0) * (i + 1) as f64 / SPANS as f64;
        let pb = c.evaluate_f64(b);
        flatten_params_rec(c, a, pa, b, pb, tol * tol, 0, &mut out);
        a = b;
        pa = pb;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn flatten_params_rec(
    c: &Curve,
    t0: f64,
    p0: (f64, f64),
    t1: f64,
    p1: (f64, f64),
    tol_sq: f64,
    depth: u32,
    out: &mut Vec<(f64, (f64, f64))>,
) {
    const MAX_PTS: usize = 4096;
    let tm = 0.5 * (t0 + t1);
    let pm = c.evaluate_f64(tm);
    let sag = crate::util::point_segment_dist_sq(pm, p0, p1);
    if depth >= 14 || sag <= tol_sq || out.len() >= MAX_PTS {
        out.push((t1, p1));
    } else {
        flatten_params_rec(c, t0, p0, tm, pm, tol_sq, depth + 1, out);
        flatten_params_rec(c, tm, pm, t1, p1, tol_sq, depth + 1, out);
    }
}

pub fn intersect(c1: &Curve, c2: &Curve) -> Vec<CurveIntersection> {
    match (c1, c2) {
        (Curve::Line(l1), Curve::Line(l2)) => intersect_line_line(l1, l2).into_iter().collect(),
        (Curve::Line(l), Curve::Arc(a)) => intersect_line_circle(l, a),
        (Curve::Arc(a), Curve::Line(l)) => intersect_line_circle(l, a)
            .into_iter()
            .map(|h| CurveIntersection {
                point: h.point,
                t1: h.t2,
                t2: h.t1,
            })
            .collect(),
        (Curve::Arc(a1), Curve::Arc(a2)) => intersect_circle_circle(a1, a2),
        _ => intersect_general(c1, c2),
    }
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
    fn arc_first_dispatch_returns_arc_param_in_t1() {
        use crate::primitives::CircularArc;
        let arc = Curve::Arc(CircularArc::new(
            pt(0, 0),
            5.0,
            0.0,
            1.5 * std::f64::consts::PI,
        ));
        let x = -5.0 / 2f64.sqrt();
        let line = Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(x, -6.0),
            Point2d::from_f64(x, 0.0),
        ));
        {
            let hits = intersect(&arc, &line);
            assert_eq!(hits.len(), 1);
            let h = &hits[0];
            let expected = 1.25 * std::f64::consts::PI;
            assert!(
                (h.t1 - expected).abs() < 1e-6,
                "t1 must be the arc angle 5π/4, got {}",
                h.t1
            );
            let (ex, ey) = arc.evaluate_f64(h.t1);
            assert!(
                (ex - h.point.0).abs() < 1e-6 && (ey - h.point.1).abs() < 1e-6,
                "evaluating the arc at t1 must reproduce the hit point"
            );
        }
    }

    #[test]
    fn polyline_zigzag_crossings_all_found() {
        use crate::primitives::PolyCurve;
        let mut segs = Vec::new();
        for i in 0..40 {
            let x0 = 0.25 * i as f64;
            let x1 = 0.25 * (i + 1) as f64;
            let y0 = if i % 2 == 0 { -2.0 } else { 2.0 };
            segs.push(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(x0, y0),
                Point2d::from_f64(x1, -y0),
            )));
        }
        let poly = Curve::Poly(Box::new(PolyCurve::new(segs)));
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(10, 0)));
        let hits = intersect(&line, &poly);
        assert_eq!(hits.len(), 40, "every zigzag crossing must be found");
        for h in &hits {
            let (x, y) = poly.evaluate_f64(h.t2);
            assert!(
                (x - h.point.0).abs() < 1e-6 && (y - h.point.1).abs() < 1e-6,
                "poly param t2 must reproduce the hit point"
            );
        }
    }

    #[test]
    fn oscillating_spline_crossings_all_found() {
        use crate::nurbs::NurbsCurve;
        // A 40-CV zigzag spline crosses y=0 roughly once per control span —
        // far more often than the old fixed 32-sample seeding could see.
        let cvs: Vec<Point2d> = (0..40)
            .map(|i| Point2d::from_f64(i as f64, if i % 2 == 0 { -1.0 } else { 1.0 }))
            .collect();
        let spline = Curve::Nurbs(NurbsCurve::uniform(cvs));
        let line = Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(-1.0, 0.0),
            Point2d::from_f64(40.0, 0.0),
        ));

        // Ground truth: count sign changes of y(t) on a dense sweep.
        let mut expected = 0;
        let mut prev = spline.evaluate_f64(0.0).1;
        for k in 1..=20000 {
            let y = spline.evaluate_f64(k as f64 / 20000.0).1;
            if (prev > 0.0) != (y > 0.0) {
                expected += 1;
            }
            prev = y;
        }
        assert!(expected > 32, "test needs more crossings than old samples");

        let hits = intersect(&spline, &line);
        assert_eq!(
            hits.len(),
            expected,
            "adaptive seeding must find every crossing"
        );
        for h in &hits {
            let (x, y) = spline.evaluate_f64(h.t1);
            assert!(
                (x - h.point.0).abs() < 1e-6 && (y - h.point.1).abs() < 1e-6,
                "t1 must reproduce the hit point"
            );
            assert!(
                y.abs() < 1e-6,
                "hit must lie on the line: t1={} y={y} point=({}, {})",
                h.t1,
                h.point.0,
                h.point.1
            );
        }
    }

    #[test]
    fn clockwise_arc_has_no_phantom_hits() {
        // A quarter arc from π/2 down to 0 (as reverse_curve produces) covers
        // only the first quadrant. The old span logic read it as the CCW
        // 3π/2 complement and reported hits at angles the arc never visits.
        let arc = CircularArc::new(pt(0, 0), 5.0, std::f64::consts::FRAC_PI_2, 0.0);
        let vertical =
            LineSeg::from_endpoints(Point2d::from_f64(0.0, -10.0), Point2d::from_f64(0.0, 10.0));
        let hits = intersect_line_circle(&vertical, &arc);
        assert_eq!(hits.len(), 1, "only the top crossing is on the arc");
        assert!((hits[0].point.1 - 5.0).abs() < 1e-9, "hit at (0, 5)");
        let (ex, ey) = Curve::Arc(arc).evaluate_f64(hits[0].t2);
        assert!(
            (ex - hits[0].point.0).abs() < 1e-9 && (ey - hits[0].point.1).abs() < 1e-9,
            "t2 must be a valid domain parameter"
        );

        let horizontal = LineSeg::from_endpoints(
            Point2d::from_f64(-10.0, -3.0),
            Point2d::from_f64(10.0, -3.0),
        );
        let arc2 = CircularArc::new(pt(0, 0), 5.0, std::f64::consts::FRAC_PI_2, 0.0);
        assert!(
            intersect_line_circle(&horizontal, &arc2).is_empty(),
            "y=-3 crosses the circle only outside the quarter arc"
        );
    }

    #[test]
    fn line_line_crossing() {
        let l1 = LineSeg::from_endpoints(pt(0, 0), pt(4, 4));
        let l2 = LineSeg::from_endpoints(pt(0, 4), pt(4, 0));
        let hit = intersect_line_line(&l1, &l2).unwrap();
        assert!((hit.point.0 - 2.0).abs() < 1e-9);
        assert!((hit.point.1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn line_line_parallel() {
        let l1 = LineSeg::from_endpoints(pt(0, 0), pt(2, 0));
        let l2 = LineSeg::from_endpoints(pt(0, 1), pt(2, 1));
        assert!(intersect_line_line(&l1, &l2).is_none());
    }

    #[test]
    fn line_circle_two_points() {
        let line =
            LineSeg::from_endpoints(Point2d::from_f64(-10.0, 0.0), Point2d::from_f64(10.0, 0.0));
        let arc = CircularArc::new(pt(0, 0), 5.0, -std::f64::consts::PI, std::f64::consts::PI);
        let hits = intersect_line_circle(&line, &arc);
        assert_eq!(
            hits.len(),
            2,
            "Expected 2 intersections, got {}",
            hits.len()
        );
        let mut xs: Vec<f64> = hits.iter().map(|h| h.point.0).collect();
        xs.sort_by(f64::total_cmp);
        assert!((xs[0] + 5.0).abs() < 1e-4);
        assert!((xs[1] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn circle_circle_two_circles() {
        let c1 = CircularArc::new(pt(0, 0), 2.0, -std::f64::consts::PI, std::f64::consts::PI);
        let c2 = CircularArc::new(pt(2, 0), 2.0, -std::f64::consts::PI, std::f64::consts::PI);
        let hits = intersect_circle_circle(&c1, &c2);
        assert_eq!(hits.len(), 2, "Expected 2 intersections, got {:?}", hits);
        for h in &hits {
            assert!((h.point.0 - 1.0).abs() < 1e-4, "x={}", h.point.0);
            assert!((h.point.1.abs() - 3f64.sqrt()).abs() < 1e-3);
        }
    }

    #[test]
    fn line_circle_intersect_shifted_center() {
        let line =
            LineSeg::from_endpoints(Point2d::from_f64(-10.0, 4.0), Point2d::from_f64(10.0, 4.0));
        let arc = CircularArc::new(pt(3, 4), 5.0, -std::f64::consts::PI, std::f64::consts::PI);
        let hits = intersect_line_circle(&line, &arc);
        assert_eq!(
            hits.len(),
            2,
            "Expected 2 intersections, got {}",
            hits.len()
        );
        let mut pts: Vec<(f64, f64)> = hits.iter().map(|h| h.point).collect();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!((pts[0].0 - -2.0).abs() < 1e-4);
        assert!((pts[0].1 - 4.0).abs() < 1e-4);
        assert!((pts[1].0 - 8.0).abs() < 1e-4);
        assert!((pts[1].1 - 4.0).abs() < 1e-4);
    }

    #[test]
    fn line_circle_exact_tangent_is_single_point() {
        let line = LineSeg::from_endpoints(pt(-8, 5), pt(8, 5));
        let arc = CircularArc::new(pt(0, 0), 5.0, -std::f64::consts::PI, std::f64::consts::PI);
        let hits = intersect_line_circle(&line, &arc);
        assert_eq!(
            hits.len(),
            1,
            "exact tangent should be a single touch point"
        );
        assert!(
            (hits[0].point.0).abs() < 1e-9,
            "x≈0, got {}",
            hits[0].point.0
        );
        assert!(
            (hits[0].point.1 - 5.0).abs() < 1e-9,
            "y≈5, got {}",
            hits[0].point.1
        );
    }

    #[test]
    fn line_circle_exact_vertical_tangent_is_exact_point() {
        let line = LineSeg::from_endpoints(pt(5, -8), pt(5, 8));
        let arc = CircularArc::new(pt(0, 0), 5.0, -std::f64::consts::PI, std::f64::consts::PI);
        let hits = intersect_line_circle(&line, &arc);
        assert_eq!(
            hits.len(),
            1,
            "vertical tangent should be a single touch point"
        );
        assert!(
            (hits[0].point.0 - 5.0).abs() < 1e-12,
            "x≈5, got {}",
            hits[0].point.0
        );
        assert!(
            (hits[0].point.1).abs() < 1e-12,
            "y≈0, got {}",
            hits[0].point.1
        );
    }

    #[test]
    fn ellipse_ellipse_four_points() {
        use crate::primitives::EllipticalArc;
        let tau = std::f64::consts::TAU;
        let e1 = Curve::Ellipse(EllipticalArc::axis_aligned(pt(0, 0), 2.0, 1.0, 0.0, tau));
        let e2 = Curve::Ellipse(EllipticalArc::axis_aligned(pt(0, 0), 1.0, 2.0, 0.0, tau));

        let hits = intersect(&e1, &e2);
        assert_eq!(
            hits.len(),
            4,
            "two crossing ellipses meet in 4 points, got {}",
            hits.len()
        );

        let expect = 2.0 / 5f64.sqrt();
        for h in &hits {
            let (x, y) = h.point;
            assert!((x.abs() - expect).abs() < 1e-6, "x={}", x);
            assert!((y.abs() - expect).abs() < 1e-6, "y={}", y);
            assert!((0.0..=tau).contains(&h.t1) && (0.0..=tau).contains(&h.t2));
        }
    }
}

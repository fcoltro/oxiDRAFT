//! Closest-point queries: projecting a point onto a curve, the point-to-curve
//! distance, and the minimum distance between two curves.

use crate::curve::{Curve, CurveSegment};

/// The nearest point found on a curve when projecting a query point.
#[derive(Clone, Debug)]
pub struct ProjectionResult {
    /// The closest point on the curve, in world coordinates.
    pub point: (f64, f64),
    /// The curve parameter at that point.
    pub t: f64,
    /// Distance from the query point to `point`.
    pub distance: f64,
}

/// Shortest distance from the point `(px, py)` to `curve`.
pub fn point_to_curve_distance(curve: &Curve, px: f64, py: f64) -> f64 {
    project_point_onto_curve(curve, px, py).distance
}

/// Projects `(px, py)` onto `curve`, returning the nearest point, its parameter,
/// and the distance. Closed-form for lines/arcs, a bounded numeric search for
/// free-form curves.
pub fn project_point_onto_curve(curve: &Curve, px: f64, py: f64) -> ProjectionResult {
    use Curve::*;

    match curve {
        Line(l) => {
            let (ax, ay) = l.p0.to_f64();
            let (bx, by) = l.p1.to_f64();
            let dx = bx - ax;
            let dy = by - ay;
            let len_sq = dx * dx + dy * dy;
            let t = if len_sq < 1e-20 {
                0.0
            } else {
                ((px - ax) * dx + (py - ay) * dy) / len_sq
            }
            .clamp(0.0, 1.0);
            let qx = ax + t * dx;
            let qy = ay + t * dy;
            let d = ((px - qx).powi(2) + (py - qy).powi(2)).sqrt();
            return ProjectionResult {
                point: (qx, qy),
                t,
                distance: d,
            };
        }
        Arc(a) => {
            let (cx, cy) = a.center.to_f64();
            let r = a.radius;
            let angle = (py - cy).atan2(px - cx);
            let angle_clamped = clamp_angle(angle, a.start_angle, a.end_angle);
            let qx = cx + r * angle_clamped.cos();
            let qy = cy + r * angle_clamped.sin();
            let d = ((px - qx).powi(2) + (py - qy).powi(2)).sqrt();
            return ProjectionResult {
                point: (qx, qy),
                t: angle_clamped,
                distance: d,
            };
        }
        Bezier(b) => {
            let (x0, y0) = b.p0.to_f64();
            let (x1, y1) = b.p1.to_f64();
            let (x2, y2) = b.p2.to_f64();
            let (x3, y3) = b.p3.to_f64();
            let ev = move |t: f64| {
                let u = 1.0 - t;
                (
                    u * u * u * x0 + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x3,
                    u * u * u * y0 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y3,
                )
            };
            return golden_section_projection_fn(&ev, (0.0, 1.0), px, py, 32);
        }
        Nurbs(nc) => {
            // Decompose the spline into Bézier segments ONCE, then sample the prepared
            // segments. Calling `nc.evaluate_f64` directly would re-run the full
            // knot-insertion decomposition on every one of the ~80 sample evaluations.
            let segs = nc.segments();
            let n = segs.len();
            if n == 0 {
                return ProjectionResult {
                    point: (px, py),
                    t: 0.0,
                    distance: 0.0,
                };
            }
            let ev = move |t: f64| {
                let (i, lt) = crate::nurbs::seg_param(n, t);
                segs[i].evaluate_f64(lt)
            };
            return golden_section_projection_fn(&ev, (0.0, 1.0), px, py, 32);
        }
        Poly(pc) => {
            // Project onto every child segment and keep the best. Sampling the
            // whole polycurve with one fixed-count pass under-samples long
            // polylines (a 100-segment chain would get ~1 probe per 3 segments)
            // and snapping would lock onto the wrong segment.
            let n = pc.segments.len();
            if n == 0 {
                return ProjectionResult {
                    point: (px, py),
                    t: 0.0,
                    distance: 0.0,
                };
            }
            let mut best: Option<ProjectionResult> = None;
            for (i, seg) in pc.segments.iter().enumerate() {
                let hit = project_point_onto_curve(seg, px, py);
                if best.as_ref().is_none_or(|b| hit.distance < b.distance) {
                    // Map the segment-local parameter to the polycurve's
                    // uniform [0,1] domain (segment i covers [i/n, (i+1)/n]).
                    let (s0, s1) = seg.domain();
                    let local = if (s1 - s0).abs() < 1e-12 {
                        0.0
                    } else {
                        (hit.t - s0) / (s1 - s0)
                    };
                    best = Some(ProjectionResult {
                        point: hit.point,
                        t: (i as f64 + local.clamp(0.0, 1.0)) / n as f64,
                        distance: hit.distance,
                    });
                }
            }
            return best.expect("n > 0 guarantees at least one projection");
        }
        _ => {}
    }

    let ev = |t: f64| curve.evaluate_f64(t);
    golden_section_projection_fn(&ev, curve.domain(), px, py, 32)
}

fn clamp_angle(angle: f64, start: f64, end: f64) -> f64 {
    let pi2 = 2.0 * std::f64::consts::PI;
    // Wrap from the lower domain end so reversed arcs (end < start, as
    // reverse_curve produces) clamp onto the span they actually cover
    // rather than its complement.
    let lo = start.min(end);
    let a = crate::util::wrap_tau(angle - lo);
    let span = (end - start).abs();
    if a <= span {
        lo + a
    } else {
        let d_lo = a.min(pi2 - a);
        let d_hi = a - span;
        if d_lo < d_hi { lo } else { lo + span }
    }
}

fn golden_section_projection_fn(
    ev: &dyn Fn(f64) -> (f64, f64),
    domain: (f64, f64),
    px: f64,
    py: f64,
    samples: usize,
) -> ProjectionResult {
    let (t0, t1) = domain;
    let dt = (t1 - t0) / samples as f64;
    let dist_sq = |t: f64| {
        let (qx, qy) = ev(t);
        (qx - px).powi(2) + (qy - py).powi(2)
    };

    let mut best_t = t0;
    let mut best_d = f64::INFINITY;
    for i in 0..=samples {
        let t = t0 + i as f64 * dt;
        let d = dist_sq(t);
        if d < best_d {
            best_d = d;
            best_t = t;
        }
    }

    let mut a = (best_t - dt).max(t0);
    let mut b = (best_t + dt).min(t1);
    let phi = (5f64.sqrt() - 1.0) / 2.0;
    let mut c = b - phi * (b - a);
    let mut d = a + phi * (b - a);
    let mut fc = dist_sq(c);
    let mut fd = dist_sq(d);
    for _ in 0..50 {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - phi * (b - a);
            fc = dist_sq(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + phi * (b - a);
            fd = dist_sq(d);
        }
        if (b - a).abs() < 1e-12 {
            break;
        }
    }
    let t_opt = (a + b) / 2.0;
    let (qx, qy) = ev(t_opt);
    let d = ((px - qx).powi(2) + (py - qy).powi(2)).sqrt();
    ProjectionResult {
        point: (qx, qy),
        t: t_opt,
        distance: d,
    }
}

/// Minimum distance between two curves — `0` when they intersect.
pub fn curve_to_curve_distance(c1: &Curve, c2: &Curve) -> f64 {
    let (t0_1, t1_1) = c1.domain();
    let (t0_2, t1_2) = c2.domain();
    let n = 16;
    let mut best = f64::INFINITY;
    for i in 0..=n {
        let t = t0_1 + (t1_1 - t0_1) * i as f64 / n as f64;
        let (px, py) = c1.evaluate_f64(t);
        let d = point_to_curve_distance(c2, px, py);
        if d < best {
            best = d;
        }
    }
    for i in 0..=n {
        let t = t0_2 + (t1_2 - t0_2) * i as f64 / n as f64;
        let (px, py) = c2.evaluate_f64(t);
        let d = point_to_curve_distance(c1, px, py);
        if d < best {
            best = d;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::Point2d;
    use crate::primitives::{CircularArc, LineSeg};

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn point_to_line_distance() {
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 0)));
        let d = point_to_curve_distance(&line, 2.0, 3.0);
        assert!((d - 3.0).abs() < 1e-9, "d={}", d);
    }

    #[test]
    fn point_to_circle_distance() {
        let arc = Curve::Arc(CircularArc::new(
            pt(0, 0),
            5.0,
            -std::f64::consts::PI,
            std::f64::consts::PI,
        ));
        let d = point_to_curve_distance(&arc, 8.0, 0.0);
        assert!((d - 3.0).abs() < 1e-6, "d={}", d);
    }

    #[test]
    fn projection_onto_line() {
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 0)));
        let proj = project_point_onto_curve(&line, 3.0, 5.0);
        assert!((proj.point.0 - 3.0).abs() < 1e-9);
        assert!((proj.point.1).abs() < 1e-9);
        assert!((proj.distance - 5.0).abs() < 1e-9);
    }

    #[test]
    fn projection_onto_long_polyline_finds_right_segment() {
        use crate::primitives::PolyCurve;
        // 120-segment zigzag: one fixed-count sampling pass over the whole
        // curve probes fewer points than there are segments and used to lock
        // onto a wrong segment far from the query.
        let n = 120;
        let segs: Vec<Curve> = (0..n)
            .map(|i| {
                let x0 = i as f64;
                let y0 = if i % 2 == 0 { 0.0 } else { 1.0 };
                let y1 = if i % 2 == 0 { 1.0 } else { 0.0 };
                Curve::Line(LineSeg::from_endpoints(
                    Point2d::from_f64(x0, y0),
                    Point2d::from_f64(x0 + 1.0, y1),
                ))
            })
            .collect();
        let poly = Curve::Poly(Box::new(PolyCurve::new(segs)));
        // Query point sits exactly on the vertex at x=100.
        let proj = project_point_onto_curve(&poly, 100.0, 0.0);
        assert!(
            proj.distance < 1e-9,
            "point on the polyline must project at distance 0, got {}",
            proj.distance
        );
        let (x, y) = poly.evaluate_f64(proj.t);
        assert!(
            (x - 100.0).abs() < 1e-9 && y.abs() < 1e-9,
            "global t must reproduce the hit point, got ({x}, {y})"
        );
    }

    #[test]
    fn projection_onto_arc_slightly_negative() {
        let arc = Curve::Arc(CircularArc::new(pt(0, 0), 5.0, 0.0, std::f64::consts::PI));
        let proj = project_point_onto_curve(&arc, 5.0, -0.1);
        assert!((proj.point.0 - 5.0).abs() < 1e-4);
        assert!((proj.point.1 - 0.0).abs() < 1e-4);
        assert!((proj.t - 0.0).abs() < 1e-4);
    }
}

//! Welding a region's boundary loops: snapping near-coincident endpoints
//! together so the loops are watertight before a boolean operation runs.

use crate::region::Region;
use oxidraft_geometry::{
    CubicBezier, Curve, CurveSegment, LineSeg, NurbsCurve, Point2d, PolyCurve, RationalBezier,
};

/// Default endpoint-snapping tolerance used when welding region boundaries.
pub const WELD_TOL: f64 = 1e-6;

/// Returns a copy of `r` with boundary vertices closer than `tol` snapped
/// together, closing tiny gaps so the loops are watertight.
pub fn weld_region(r: &Region, tol: f64) -> Region {
    Region::with_holes(
        weld_loop(&r.outer, tol),
        r.holes.iter().map(|h| weld_loop(h, tol)).collect(),
    )
}

#[allow(clippy::needless_range_loop)]
fn weld_loop(curves: &[Curve], tol: f64) -> Vec<Curve> {
    if curves.is_empty() {
        return Vec::new();
    }

    let mut eps: Vec<(f64, f64)> = Vec::with_capacity(curves.len() * 2);
    for c in curves {
        let (t0, t1) = c.domain();
        eps.push(c.evaluate_f64(t0));
        eps.push(c.evaluate_f64(t1));
    }

    let n = eps.len();
    let tol_sq = tol * tol;
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in 0..i {
            let dx = eps[i].0 - eps[j].0;
            let dy = eps[i].1 - eps[j].1;
            if dx * dx + dy * dy <= tol_sq {
                let ri = find(&mut parent, i);
                let rj = find(&mut parent, j);
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    let mut sum: Vec<(f64, f64, usize)> = vec![(0.0, 0.0, 0); n];
    for i in 0..n {
        let r = find(&mut parent, i);
        sum[r].0 += eps[i].0;
        sum[r].1 += eps[i].1;
        sum[r].2 += 1;
    }
    let target = |i: usize, parent: &mut [usize]| -> (f64, f64) {
        let r = find(parent, i);
        let (sx, sy, cnt) = sum[r];
        (sx / cnt as f64, sy / cnt as f64)
    };

    let mut out: Vec<Curve> = Vec::with_capacity(curves.len());
    for (k, c) in curves.iter().enumerate() {
        let start_root = find(&mut parent, 2 * k);
        let end_root = find(&mut parent, 2 * k + 1);
        if start_root == end_root && curve_extent(c) <= tol {
            continue;
        }
        let s = target(2 * k, &mut parent);
        let e = target(2 * k + 1, &mut parent);
        out.push(snap_endpoints(c, s, e));
    }
    out
}

fn curve_extent(c: &Curve) -> f64 {
    let bb = c.bounding_box();
    ((bb.max.x - bb.min.x).powi(2) + (bb.max.y - bb.min.y).powi(2)).sqrt()
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn snap_endpoints(c: &Curve, s: (f64, f64), e: (f64, f64)) -> Curve {
    match c {
        Curve::Line(_) => Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(s.0, s.1),
            Point2d::from_f64(e.0, e.1),
        )),
        Curve::Bezier(b) => {
            let (p0x, p0y) = b.p0.to_f64();
            let (p3x, p3y) = b.p3.to_f64();
            let (p1x, p1y) = b.p1.to_f64();
            let (p2x, p2y) = b.p2.to_f64();
            let p1 = (p1x + (s.0 - p0x), p1y + (s.1 - p0y));
            let p2 = (p2x + (e.0 - p3x), p2y + (e.1 - p3y));
            Curve::Bezier(CubicBezier::new(
                Point2d::from_f64(s.0, s.1),
                Point2d::from_f64(p1.0, p1.1),
                Point2d::from_f64(p2.0, p2.1),
                Point2d::from_f64(e.0, e.1),
            ))
        }
        Curve::Arc(a) => {
            let mut a = *a;
            let (cx, cy) = a.center.to_f64();
            let new_start = (s.1 - cy).atan2(s.0 - cx);
            let new_end = (e.1 - cy).atan2(e.0 - cx);
            set_arc_angles(&mut a.start_angle, &mut a.end_angle, new_start, new_end);
            Curve::Arc(a)
        }
        Curve::Ellipse(el) => {
            // Returning the arc untouched here meant an ellipse never took
            // part in a weld: its neighbours snapped to the cluster average
            // (which its own endpoints had been counted into), so the seam
            // narrowed but never closed, and `weld_region` handed back a loop
            // it presents as watertight while it still had a hole in it.
            let mut el = *el;
            let new_start = eccentric_angle(&el, s);
            let new_end = eccentric_angle(&el, e);
            set_arc_angles(&mut el.start_angle, &mut el.end_angle, new_start, new_end);
            Curve::Ellipse(el)
        }
        Curve::Poly(pc) => {
            let mut segs = pc.segments.clone();
            if let Some(first) = segs.first().cloned() {
                let (_, end_pt) = endpoints_f64(&first);
                segs[0] = snap_endpoints(&first, s, end_pt);
            }
            if let Some(last) = segs.last().cloned() {
                let last_idx = segs.len() - 1;
                let (start_pt, _) = endpoints_f64(&last);
                segs[last_idx] = snap_endpoints(&last, start_pt, e);
            }
            Curve::Poly(Box::new(PolyCurve::new(segs)))
        }
        Curve::Rational(rb) => {
            let mut pts = rb.points.clone();
            let n = pts.len();
            let d_start = (s.0 - pts[0].x, s.1 - pts[0].y);
            let d_end = (e.0 - pts[n - 1].x, e.1 - pts[n - 1].y);
            if n >= 4 {
                pts[1] = Point2d::from_f64(pts[1].x + d_start.0, pts[1].y + d_start.1);
                pts[n - 2] = Point2d::from_f64(pts[n - 2].x + d_end.0, pts[n - 2].y + d_end.1);
            }
            pts[0] = Point2d::from_f64(s.0, s.1);
            pts[n - 1] = Point2d::from_f64(e.0, e.1);
            Curve::Rational(RationalBezier::new(pts, rb.weights.to_vec()))
        }
        Curve::Nurbs(nc) => {
            let mut cv = nc.control().to_vec();
            let n = cv.len();
            let d_start = (s.0 - cv[0].x, s.1 - cv[0].y);
            let d_end = (e.0 - cv[n - 1].x, e.1 - cv[n - 1].y);
            if n >= 4 {
                cv[1] = Point2d::from_f64(cv[1].x + d_start.0, cv[1].y + d_start.1);
                cv[n - 2] = Point2d::from_f64(cv[n - 2].x + d_end.0, cv[n - 2].y + d_end.1);
            }
            cv[0] = Point2d::from_f64(s.0, s.1);
            cv[n - 1] = Point2d::from_f64(e.0, e.1);
            Curve::Nurbs(NurbsCurve::new(cv, nc.weights().to_vec()))
        }
    }
}

fn endpoints_f64(c: &Curve) -> ((f64, f64), (f64, f64)) {
    let (t0, t1) = c.domain();
    (c.evaluate_f64(t0), c.evaluate_f64(t1))
}

/// Rewrites an arc's angles for endpoints that have moved by at most the weld
/// tolerance.
///
/// `atan2` answers in `(-pi, pi]`, so the recovered angles have to be lifted
/// back onto the branch the arc was already using. Choosing that branch by
/// forcing `end > start` — which is what this did — silently rewrites every
/// arc to a positive sweep: a 90-degree clockwise arc came back as the
/// 270-degree counter-clockwise one with the same endpoints, and a multi-turn
/// arc was collapsed to a single turn. `reverse_curve` and `mirror_x` both
/// produce clockwise arcs, and welding runs at the top of every boolean, so
/// mirroring a shape and then unioning it flipped the arc back across its own
/// chord.
///
/// Since the endpoints only moved by a hair, the branch is not a choice to be
/// made at all: take each angle's representative nearest the one it is
/// replacing. Direction and turn count then survive because they are never
/// consulted.
fn set_arc_angles(start: &mut f64, end: &mut f64, new_start: f64, new_end: f64) {
    *start = nearest_branch(new_start, *start);
    *end = nearest_branch(new_end, *end);
}

/// The eccentric angle of the point on `el` nearest `p`.
///
/// The inverse of `evaluate_f64`: undo the centre offset and the axis
/// rotation, then divide out the semi-axes so the ellipse becomes the unit
/// circle and the angle is a plain `atan2`. A weld target need not sit exactly
/// on the ellipse, which is fine — the same radial slack the circular-arc arm
/// already accepts.
fn eccentric_angle(el: &oxidraft_geometry::EllipticalArc, p: (f64, f64)) -> f64 {
    let (cx, cy) = el.center.to_f64();
    let (dx, dy) = (p.0 - cx, p.1 - cy);
    let (cos_r, sin_r) = (el.rotation.cos(), el.rotation.sin());
    let u = dx * cos_r + dy * sin_r;
    let v = -dx * sin_r + dy * cos_r;
    (v / el.semi_minor).atan2(u / el.semi_major)
}

/// `raw` shifted by whole turns to land as close as possible to `reference`.
fn nearest_branch(raw: f64, reference: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let turns = ((reference - raw) / tau).round();
    if turns.is_finite() {
        raw + tau * turns
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_geometry::{CircularArc, EllipticalArc};

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Curve {
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(x0, y0),
            Point2d::from_f64(x1, y1),
        ))
    }

    fn seam_gap(loop_curves: &[Curve], k: usize) -> f64 {
        let (_, end) = endpoints_f64(&loop_curves[k]);
        let (start_next, _) = endpoints_f64(&loop_curves[(k + 1) % loop_curves.len()]);
        ((end.0 - start_next.0).powi(2) + (end.1 - start_next.1).powi(2)).sqrt()
    }

    #[test]
    fn weld_closes_open_loop() {
        let g = 1e-9;
        let curves = vec![
            line(0.0, 0.0, 4.0, 0.0),
            line(4.0, 0.0, 4.0, 4.0),
            line(4.0, 4.0, 0.0, 4.0),
            line(g, 4.0, g, g),
        ];
        assert!(seam_gap(&curves, 3) > 0.0);

        let welded = weld_loop(&curves, WELD_TOL);
        assert_eq!(welded.len(), 4);
        for k in 0..welded.len() {
            assert!(seam_gap(&welded, k) < 1e-12, "seam {} not closed", k);
        }
    }

    #[test]
    fn weld_keeps_distinct_vertices() {
        let curves = vec![line(0.0, 0.0, 4.0, 0.0), line(4.0, 0.0, 4.0, 4.0)];
        let welded = weld_loop(&curves, WELD_TOL);
        assert_eq!(welded.len(), 2);
        let (s0, _) = endpoints_f64(&welded[0]);
        assert!(
            s0.0.abs() < 1e-9 && s0.1.abs() < 1e-9,
            "far vertex moved: {:?}",
            s0
        );
    }

    fn apex(c: &Curve) -> (f64, f64) {
        let (t0, t1) = c.domain();
        c.evaluate_f64((t0 + t1) * 0.5)
    }

    #[test]
    fn weld_keeps_a_clockwise_arc_on_its_own_side() {
        // `atan2` lands in (-pi, pi], and picking the branch by forcing
        // `end > start` turned every clockwise arc into the long way round
        // with the same endpoints — so a lower half-disc came back as the
        // upper one. `mirror_x` and `reverse_curve` both produce clockwise
        // arcs and welding runs at the top of every boolean, so this was
        // reachable by mirroring a shape and then unioning it.
        let arc = CircularArc::new(Point2d::from_f64(0.0, 0.0), 1.0, 0.0, -std::f64::consts::PI);
        let c = Curve::Arc(arc);
        let (_, t1) = c.domain();
        let e = c.evaluate_f64(t1);
        let before = apex(&c);
        assert!(before.1 < 0.0, "fixture should be the LOWER half-disc");

        let welded = weld_loop(&[c, line(e.0, e.1, 1.0, 0.0)], WELD_TOL);
        let after = apex(&welded[0]);
        assert!(
            after.1 < 0.0,
            "the arc must stay on the side it was drawn on: apex {before:?} -> {after:?}"
        );
    }

    #[test]
    fn weld_keeps_the_turn_count_of_a_multi_turn_arc() {
        // The old branch clamp (`while e > s + tau`) collapsed any arc of
        // more than one turn down to a single turn.
        let two_turns = 4.0 * std::f64::consts::PI;
        let arc = CircularArc::new(Point2d::from_f64(0.0, 0.0), 1.0, 0.0, two_turns);
        let welded = weld_loop(&[Curve::Arc(arc)], WELD_TOL);
        let Curve::Arc(a) = &welded[0] else {
            panic!("expected an arc");
        };
        let sweep = a.end_angle - a.start_angle;
        assert!(
            (sweep - two_turns).abs() < 1e-9,
            "a two-turn arc must still be two turns: {sweep} vs {two_turns}"
        );
    }

    #[test]
    fn weld_actually_moves_an_ellipse() {
        // The ellipse arm used to return the arc untouched, so an ellipse
        // never took part in a weld: its neighbours snapped to a cluster
        // average its own endpoints had been counted into, which narrowed the
        // seam without ever closing it, and the loop was still reported
        // watertight.
        let el = EllipticalArc::new(
            Point2d::from_f64(0.0, 0.0),
            2.0,
            1.0,
            0.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        );
        let ec = Curve::Ellipse(el);
        let (a0, a1) = ec.domain();
        let (es, ee) = (ec.evaluate_f64(a0), ec.evaluate_f64(a1));
        let welded = weld_loop(&[ec, line(ee.0 + 1e-9, ee.1, es.0, es.1)], WELD_TOL);
        assert_eq!(welded.len(), 2);
        assert!(
            seam_gap(&welded, 0) < 1e-12,
            "the ellipse seam must close, not just narrow: {:.3e}",
            seam_gap(&welded, 0)
        );
    }

    #[test]
    fn weld_drops_degenerate_segment() {
        let curves = vec![
            line(0.0, 0.0, 4.0, 0.0),
            line(4.0, 0.0, 4.0 + 1e-10, 1e-10),
            line(4.0, 0.0, 0.0, 0.0),
        ];
        let welded = weld_loop(&curves, WELD_TOL);
        assert_eq!(welded.len(), 2, "degenerate segment should be dropped");
    }
}

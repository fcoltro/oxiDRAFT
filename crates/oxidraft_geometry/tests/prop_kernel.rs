//! Property-based checks of kernel primitives: universal statements about
//! flattening tolerance, offset distance, projection and intersection that
//! must hold for any randomly generated input.

use oxidraft_geometry::{
    CircularArc, Curve, CurveSegment, LineSeg, Point2d, intersect, offset_curve,
    point_to_curve_distance, project_point_onto_curve, tessellate_curve,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn arc_flattening_respects_sagitta_tolerance(
        cx in -20.0..20.0f64,
        cy in -20.0..20.0f64,
        r in 0.1..50.0f64,
        start in -6.0..6.0f64,
        sweep in 0.05..std::f64::consts::TAU,
        tol_frac in 1e-4..1e-1f64,
    ) {
        let tol = r * tol_frac;
        let arc = CircularArc::new(Point2d::from_f64(cx, cy), r, start, start + sweep);
        let pts = tessellate_curve(&Curve::Arc(arc), tol);
        prop_assert!(pts.len() >= 2);
        for w in pts.windows(2) {
            let mx = 0.5 * (w[0].x + w[1].x) - cx;
            let my = 0.5 * (w[0].y + w[1].y) - cy;
            let sag = r - mx.hypot(my);
            prop_assert!(
                sag <= tol * 1.0001,
                "sagitta {sag} exceeds tol {tol} (r={r}, sweep={sweep})"
            );
        }
    }

    #[test]
    fn arc_offset_is_exactly_parallel(
        r in 0.5..20.0f64,
        d in -0.4..10.0f64,
        start in -3.0..3.0f64,
        sweep in 0.2..std::f64::consts::TAU,
    ) {
        // Keep the offset outside the degenerate collapse regime.
        prop_assume!(r + d > 0.05);
        let base = Curve::Arc(CircularArc::new(
            Point2d::from_f64(1.0, -2.0),
            r,
            start,
            start + sweep,
        ));
        let off = offset_curve(&base, d);
        let (t0, t1) = off.domain();
        for k in 0..=16 {
            let t = t0 + (t1 - t0) * k as f64 / 16.0;
            let (x, y) = off.evaluate_f64(t);
            let err = (point_to_curve_distance(&base, x, y) - d.abs()).abs();
            prop_assert!(err < 1e-6, "offset sample {err} off the parallel");
        }
    }

    #[test]
    fn projection_returns_the_true_nearest_point(
        px in -15.0..15.0f64,
        py in -15.0..15.0f64,
        r in 0.5..8.0f64,
    ) {
        // For a full circle the nearest point has a closed form; the kernel
        // projection must match it.
        let c = Curve::Arc(CircularArc::new(
            Point2d::from_f64(2.0, 1.0),
            r,
            0.0,
            std::f64::consts::TAU,
        ));
        let dist_center = (px - 2.0).hypot(py - 1.0);
        prop_assume!(dist_center > 1e-3);
        let proj = project_point_onto_curve(&c, px, py);
        let want = (dist_center - r).abs();
        prop_assert!(
            (proj.distance - want).abs() < 1e-6,
            "projection distance {} but geometry says {want}",
            proj.distance
        );
    }

    #[test]
    fn line_circle_hits_lie_on_both_curves(
        x0 in -12.0..12.0f64,
        y0 in -12.0..12.0f64,
        x1 in -12.0..12.0f64,
        y1 in -12.0..12.0f64,
        r in 0.5..8.0f64,
    ) {
        prop_assume!((x0 - x1).hypot(y0 - y1) > 1e-3);
        let line = Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(x0, y0),
            Point2d::from_f64(x1, y1),
        ));
        let circle = Curve::Arc(CircularArc::new(
            Point2d::from_f64(0.0, 0.0),
            r,
            0.0,
            std::f64::consts::TAU,
        ));
        for h in intersect(&line, &circle) {
            let (lx, ly) = line.evaluate_f64(h.t1);
            let (cx, cy) = circle.evaluate_f64(h.t2);
            prop_assert!((lx - h.point.0).abs() < 1e-6 && (ly - h.point.1).abs() < 1e-6);
            prop_assert!((cx - h.point.0).abs() < 1e-6 && (cy - h.point.1).abs() < 1e-6);
            let on_circle = (h.point.0.hypot(h.point.1) - r).abs();
            prop_assert!(on_circle < 1e-6, "hit {on_circle} off the circle");
        }
    }
}

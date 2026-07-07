//! Solver properties over randomized inputs: whatever the starting guess,
//! a satisfiable constraint set must solve to residual ~0, and solving an
//! already-solved sketch must terminate immediately (idempotence).

use oxidraft_constraint::{Constraint, Sketch};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn sloppy_rectangles_always_square_up(
        w in 1.0..12.0f64,
        h in 1.0..12.0f64,
        noise in proptest::collection::vec(-0.4..0.4f64, 8),
    ) {
        let mut s = Sketch::new();
        let a = s.add_point(noise[0], noise[1]);
        let b = s.add_point(w + noise[2], noise[3]);
        let c = s.add_point(w + noise[4], h + noise[5]);
        let d = s.add_point(noise[6], h + noise[7]);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Vertical(b, c));
        s.constrain(Constraint::Horizontal(c, d));
        s.constrain(Constraint::Vertical(d, a));
        s.constrain(Constraint::Distance(a, b, w));
        s.constrain(Constraint::Distance(b, c, h));
        let res = s.solve();
        prop_assert!(res.converged, "residual {} after {} iters", res.residual, res.iterations);
        let (bx, by) = s.point(b);
        let (cx, cy) = s.point(c);
        prop_assert!((bx - w).abs() < 1e-5 && by.abs() < 1e-5, "b=({bx},{by})");
        prop_assert!((cx - w).abs() < 1e-5 && (cy - h).abs() < 1e-5, "c=({cx},{cy})");

        // Idempotence: an already-satisfied sketch needs no iterations.
        let again = s.solve();
        prop_assert!(again.converged);
        prop_assert_eq!(again.iterations, 0, "re-solve must be a no-op");
    }

    #[test]
    fn triangles_from_valid_side_lengths(
        la in 1.0..10.0f64,
        lb in 1.0..10.0f64,
        t in 0.15..0.85f64,
        gx in -1.0..1.0f64,
        gy in 0.5..3.0f64,
    ) {
        // Third side chosen strictly inside the triangle inequality band.
        let lc = (la - lb).abs() + t * (la + lb - (la - lb).abs());
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(lc * 0.9, 0.1);
        let c = s.add_point(gx, gy);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Distance(a, b, lc));
        s.constrain(Constraint::Distance(a, c, la));
        s.constrain(Constraint::Distance(b, c, lb));
        let res = s.solve();
        // Degenerate (near-collinear) triangles may stall in a fold; only
        // assert on comfortably non-degenerate ones.
        if lc > (la - lb).abs() + 0.2 && lc < la + lb - 0.2 {
            prop_assert!(res.converged, "residual {}", res.residual);
            let d = |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).hypot(p.1 - q.1);
            prop_assert!((d(s.point(a), s.point(c)) - la).abs() < 1e-5);
            prop_assert!((d(s.point(b), s.point(c)) - lb).abs() < 1e-5);
        }
    }
}

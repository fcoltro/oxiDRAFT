//! Hostile-input battery for the curve operations the modify tools call:
//! offset, blend, tangent constructions, splitting, projection, curvature,
//! and tessellation. Companion to the loaders' `corrupt_input.rs` and the
//! boolean crate's `hostile_input.rs`: every case a user can reach through
//! a degenerate pick or a corrupt document must return *something* —
//! `None`, an empty vec, endpoints — never panic, hang, or balloon memory.

use oxidraft_geometry::{
    CircularArc, Continuity, CubicBezier, Curve, LineSeg, NurbsCurve, Point2d, PolyCurve,
    blend_curves, circle_through_three_points, common_tangent_segments, curvature_at,
    curve_to_curve_distance, intersect, normal_at, offset_curve, point_to_curve_distance,
    project_point_onto_curve, refit_nurbs_subcurve, reverse_curve, split_curve, tangent_at,
    tangent_circle_ttr, tangent_circle_ttt, tangent_points_from_point, tessellate_curve,
};

fn p(x: f64, y: f64) -> Point2d {
    Point2d::from_f64(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Curve {
    Curve::Line(LineSeg::from_endpoints(p(x0, y0), p(x1, y1)))
}

fn zero_line() -> Curve {
    line(1.0, 1.0, 1.0, 1.0)
}

/// A cubic whose four control points coincide: evaluates to one point
/// everywhere, so every derivative-based quantity degenerates.
fn point_bezier() -> Curve {
    Curve::Bezier(CubicBezier::new(
        p(2.0, 2.0),
        p(2.0, 2.0),
        p(2.0, 2.0),
        p(2.0, 2.0),
    ))
}

fn arc(cx: f64, cy: f64, r: f64, a0: f64, a1: f64) -> Curve {
    Curve::Arc(CircularArc::new(p(cx, cy), r, a0, a1))
}

fn menagerie() -> Vec<Curve> {
    vec![
        line(0.0, 0.0, 4.0, 3.0),
        zero_line(),
        point_bezier(),
        arc(0.0, 0.0, 2.0, 0.0, std::f64::consts::PI),
        arc(0.0, 0.0, 1e-30, 0.0, 1.0),
        arc(0.0, 0.0, 2.0, 1.0, 1.0), // zero sweep
        Curve::Bezier(CubicBezier::new(
            p(0.0, 0.0),
            p(1.0, 5.0),
            p(3.0, -5.0),
            p(4.0, 0.0),
        )),
        Curve::Nurbs(NurbsCurve::uniform(vec![
            p(0.0, 0.0),
            p(1.0, 2.0),
            p(2.0, -2.0),
            p(3.0, 0.0),
        ])),
        // Extreme but positive weights are accepted by the validator and
        // stress the homogeneous arithmetic.
        Curve::Nurbs(NurbsCurve::new(
            vec![p(0.0, 0.0), p(1.0, 1.0), p(2.0, 0.0)],
            vec![1e-30, 1e30, 1e-30],
        )),
        Curve::Poly(Box::new(PolyCurve::new(vec![
            line(0.0, 0.0, 1.0, 0.0),
            zero_line(),
            line(1.0, 0.0, 1.0, 1.0),
        ]))),
        Curve::Poly(Box::new(PolyCurve::new(Vec::new()))),
    ]
}

fn assert_finite_curve(c: &Curve, label: &str) {
    assert!(c.is_finite(), "{label}: non-finite curve escaped: {c:?}");
}

#[test]
fn offset_survives_every_degenerate_curve() {
    for (i, c) in menagerie().iter().enumerate() {
        for dist in [0.0, 1.0, -1.0, 1e-30, 1e15, -1e15] {
            let out = offset_curve(c, dist);
            if c.is_finite() {
                assert_finite_curve(&out, &format!("offset #{i} dist {dist}"));
            }
        }
        // Non-finite distances must at least not panic or hang.
        let _ = offset_curve(c, f64::NAN);
        let _ = offset_curve(c, f64::INFINITY);
    }
}

#[test]
fn split_and_reverse_survive_hostile_parameters() {
    for (i, c) in menagerie().iter().enumerate() {
        let _ = reverse_curve(c);
        for t in [0.0, 1.0, 0.5, -1.0, 2.0, f64::NAN, f64::INFINITY] {
            let _ = split_curve(c, t);
        }
        let _ = i;
    }
}

#[test]
fn blend_survives_degenerate_ends_and_tensions() {
    let a = line(0.0, 0.0, 4.0, 0.0);
    let b = line(6.0, 1.0, 10.0, 1.0);
    for cont in [Continuity::G0, Continuity::G1, Continuity::G2] {
        for tension in [0.0, 1.0, -1.0, 1e15, f64::NAN] {
            let out = blend_curves(&a, true, &b, false, cont, tension, 1.0);
            if tension.is_finite()
                && let Some(cv) = out
            {
                assert_finite_curve(&cv, "blend finite tension");
            }
        }
        // Coincident join points: no chord to span.
        assert!(
            blend_curves(&a, true, &line(4.0, 0.0, 8.0, 8.0), false, cont, 1.0, 1.0).is_none(),
            "coincident joins must decline"
        );
        // Degenerate operands.
        let _ = blend_curves(&zero_line(), true, &b, false, cont, 1.0, 1.0);
        let _ = blend_curves(
            &point_bezier(),
            true,
            &point_bezier(),
            false,
            cont,
            1.0,
            1.0,
        );
    }
}

#[test]
fn tangent_constructions_survive_degenerate_picks() {
    // Collinear and coincident triples.
    assert!(circle_through_three_points(p(0.0, 0.0), p(1.0, 0.0), p(2.0, 0.0)).is_none());
    assert!(circle_through_three_points(p(1.0, 1.0), p(1.0, 1.0), p(1.0, 1.0)).is_none());

    let l1 = line(0.0, 0.0, 4.0, 0.0);
    let l2 = line(0.0, 2.0, 4.0, 2.0);
    let zl = zero_line();
    for r in [0.0, -1.0, 1e-30, 1e15, f64::NAN] {
        let _ = tangent_circle_ttr(&l1, &l2, r, p(2.0, 1.0));
        let _ = tangent_circle_ttr(&l1, &l1, r, p(2.0, 0.0));
        let _ = tangent_circle_ttr(&zl, &l2, r, p(1.0, 1.5));
    }
    let _ = tangent_circle_ttt(&l1, &l2, &l1, p(2.0, 1.0));
    let _ = tangent_circle_ttt(&zl, &zl, &zl, p(1.0, 1.0));

    // Query point on the center, inside, and on the rim.
    for q in [p(0.0, 0.0), p(0.5, 0.0), p(2.0, 0.0)] {
        let _ = tangent_points_from_point(p(0.0, 0.0), 2.0, q);
    }
    // Concentric and coincident circle pairs.
    let _ = common_tangent_segments(p(0.0, 0.0), 2.0, p(0.0, 0.0), 1.0);
    let _ = common_tangent_segments(p(0.0, 0.0), 2.0, p(0.0, 0.0), 2.0);
    let _ = common_tangent_segments(p(0.0, 0.0), 2.0, p(1.0, 0.0), f64::NAN);
}

#[test]
fn differential_queries_survive_hostile_parameters() {
    for c in menagerie() {
        for t in [0.0, 0.5, 1.0, -1.0, 2.0, f64::NAN] {
            let _ = tangent_at(&c, t);
            let _ = normal_at(&c, t);
            let _ = curvature_at(&c, t);
        }
    }
}

#[test]
fn projection_and_distance_survive_degenerate_curves() {
    for c in menagerie() {
        for (qx, qy) in [(1.0, 1.0), (f64::NAN, 0.0), (1e15, -1e15)] {
            let _ = project_point_onto_curve(&c, qx, qy);
            let _ = point_to_curve_distance(&c, qx, qy);
        }
    }
    let m = menagerie();
    for a in &m {
        let _ = curve_to_curve_distance(a, &m[0]);
        let _ = intersect(a, &m[0]);
        let _ = intersect(a, a);
    }
}

#[test]
fn tessellation_tolerance_cannot_balloon_memory() {
    for (i, c) in menagerie().iter().enumerate() {
        for tol in [1e-3, 0.0, -1.0, 1e-300, f64::NAN, f64::INFINITY] {
            let pts = tessellate_curve(c, tol);
            assert!(
                pts.len() <= 70_000,
                "curve #{i} tol {tol}: {} points is a memory balloon",
                pts.len()
            );
        }
    }
}

#[test]
fn nurbs_refit_survives_hostile_ranges() {
    let nc = NurbsCurve::uniform(vec![p(0.0, 0.0), p(1.0, 2.0), p(2.0, -2.0), p(3.0, 0.0)]);
    for (a, b) in [
        (0.2, 0.8),
        (0.8, 0.2),
        (0.5, 0.5),
        (-1.0, 2.0),
        (f64::NAN, 0.5),
    ] {
        let out = refit_nurbs_subcurve(&nc, a, b);
        if a.is_finite() && b.is_finite() {
            assert!(
                out.control.iter().all(|q| q.is_finite()),
                "refit ({a},{b}) produced non-finite control points"
            );
        }
    }
}

#[test]
fn interpolate_nurbs_declines_bad_weights_instead_of_panicking() {
    use oxidraft_geometry::ops::offset::interpolate_nurbs;
    let data = [p(0.0, 0.0), p(1.0, 1.0), p(2.0, 0.0)];
    assert!(interpolate_nurbs(&data, &[1.0, -1.0, 1.0]).is_none());
    assert!(interpolate_nurbs(&data, &[1.0, 0.0, 1.0]).is_none());
    assert!(interpolate_nurbs(&data, &[1.0, f64::NAN, 1.0]).is_none());
    assert!(interpolate_nurbs(&data, &[1.0, 1.0]).is_none());
    assert!(interpolate_nurbs(&[], &[]).is_none());
    assert!(interpolate_nurbs(&[p(f64::NAN, 0.0), p(1.0, 1.0), p(2.0, 0.0)], &[1.0; 3]).is_none());
    // Sane input still interpolates.
    assert!(interpolate_nurbs(&data, &[1.0, 1.0, 1.0]).is_some());
}

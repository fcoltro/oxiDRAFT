use oxidraft_geometry::*;

fn pt(x: i64, y: i64) -> Point2d {
    Point2d::from_i64(x, y)
}

#[test]
fn bezier_bezier_intersection() {
    let up = Curve::Bezier(CubicBezier::new(pt(0, 0), pt(3, 3), pt(6, 3), pt(9, 0)));
    let down = Curve::Bezier(CubicBezier::new(pt(0, 3), pt(3, 0), pt(6, 0), pt(9, 3)));

    let hits = intersect(&up, &down);
    assert!(
        hits.len() >= 2,
        "Bézier×Bézier expected ≥2 intersections, got {}",
        hits.len()
    );

    for h in &hits {
        let (x, y) = h.point;
        let du = project_point_onto_curve(&up, x, y);
        let dd = project_point_onto_curve(&down, x, y);
        assert!(
            du.distance < 1e-4,
            "pt not on up-curve: dist={}",
            du.distance
        );
        assert!(
            dd.distance < 1e-4,
            "pt not on down-curve: dist={}",
            dd.distance
        );
    }
}

#[test]
fn line_tangent_to_circle_gives_one_point() {
    let circle = CircularArc::new(pt(0, 0), 5.0, 0.0, 2.0 * std::f64::consts::PI);
    let line = LineSeg::from_endpoints(pt(-8, 5), pt(8, 5));
    let hits = intersect_line_circle(&line, &circle);
    assert!(!hits.is_empty(), "tangent line should touch the circle");
    for h in &hits {
        assert!((h.point.0).abs() < 1e-3, "tangent x≈0, got {}", h.point.0);
        assert!(
            (h.point.1 - 5.0).abs() < 1e-3,
            "tangent y≈5, got {}",
            h.point.1
        );
    }
}

#[test]
fn curvature_of_parabola_at_vertex() {
    let para = Curve::Bezier(CubicBezier::new(
        Point2d::new(-1.0, 1.0),
        Point2d::new(-1.0 / 3.0, -1.0 / 3.0),
        Point2d::new(1.0 / 3.0, -1.0 / 3.0),
        Point2d::new(1.0, 1.0),
    ));
    let k = curvature_at(&para, 0.5);
    assert!(k.is_some(), "curvature should be defined at vertex");
    let kv = k.unwrap();
    assert!(
        kv.abs() > 0.1,
        "vertex curvature should be substantial, got {}",
        kv
    );
}

#[test]
fn offset_circle_is_concentric_and_correct_radius() {
    let circle = Curve::Arc(CircularArc::new(
        pt(10, 20),
        7.0,
        0.0,
        2.0 * std::f64::consts::PI,
    ));
    let outer = offset_curve(&circle, 3.0);
    if let Curve::Arc(a) = outer {
        let (cx, cy) = a.center.to_f64();
        assert!(
            (cx - 10.0).abs() < 1e-6 && (cy - 20.0).abs() < 1e-6,
            "center moved"
        );
        assert!((a.radius - 10.0).abs() < 1e-6, "radius should be 7+3=10");
    } else {
        panic!("offset of arc should be an arc");
    }
}

#[test]
fn polycurve_evaluation_traverses_all_segments() {
    let pc = PolyCurve::new(vec![
        Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(2, 0))),
        Curve::Line(LineSeg::from_endpoints(pt(2, 0), pt(2, 2))),
        Curve::Line(LineSeg::from_endpoints(pt(2, 2), pt(0, 2))),
        Curve::Line(LineSeg::from_endpoints(pt(0, 2), pt(0, 0))),
    ]);
    let (x0, y0) = pc.evaluate_f64(0.0);
    assert!(
        (x0).abs() < 1e-9 && (y0).abs() < 1e-9,
        "t=0 should be (0,0), got ({},{})",
        x0,
        y0
    );

    let (x1, y1) = pc.evaluate_f64(0.249);
    assert!(
        x1 > 1.5 && y1.abs() < 0.5,
        "t≈0.25 should be near (2,0), got ({},{})",
        x1,
        y1
    );

    assert!(
        (pc.arc_length() - 8.0).abs() < 1e-6,
        "perimeter should be 8, got {}",
        pc.arc_length()
    );
}

#[test]
fn three_point_circle_exact_center() {
    let p1 = Point2d::from_f64(1.0, 0.0);
    let p2 = Point2d::from_f64(0.0, 1.0);
    let p3 = Point2d::from_f64(-1.0, 0.0);
    let arc = CircularArc::from_three_points(&p1, &p2, &p3).expect("non-collinear");
    let (cx, cy) = arc.center.to_f64();
    assert!(
        cx.abs() < 1e-9 && cy.abs() < 1e-9,
        "center should be origin, got ({},{})",
        cx,
        cy
    );
    assert!((arc.radius - 1.0).abs() < 1e-6, "radius should be 1");
}

#[test]
fn three_collinear_points_no_circle() {
    let p1 = pt(0, 0);
    let p2 = pt(1, 1);
    let p3 = pt(2, 2);
    assert!(
        CircularArc::from_three_points(&p1, &p2, &p3).is_none(),
        "collinear points must not yield a circle"
    );
}

#[test]
fn param_at_length_is_linear_on_uniform_speed_curves() {
    let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(10, 0)));
    let t = line.param_at_length(2.5);
    assert!((t - 0.25).abs() < 1e-12, "line t={t}");

    let arc = Curve::Arc(CircularArc::new(
        Point2d::from_f64(0.0, 0.0),
        2.0,
        0.0,
        std::f64::consts::FRAC_PI_2,
    ));
    // Quarter circle of length π; halfway along is the 45° angle.
    let t = arc.param_at_length(std::f64::consts::PI / 2.0 / 2.0 * 2.0);
    assert!((t - std::f64::consts::FRAC_PI_4).abs() < 1e-12, "arc t={t}");
}

#[test]
fn param_at_length_undoes_uneven_bezier_parameterization() {
    // Straight-line geometry with wildly uneven parameter speed: uniform
    // parameter steps bunch points near the slow end, but uniform arc-length
    // steps must land evenly spaced along the line.
    let b = Curve::Bezier(CubicBezier::new(
        Point2d::from_f64(0.0, 0.0),
        Point2d::from_f64(0.1, 0.0),
        Point2d::from_f64(0.2, 0.0),
        Point2d::from_f64(4.0, 0.0),
    ));
    let len = b.arc_length();
    assert!((len - 4.0).abs() < 1e-3, "straight bezier length {len}");
    for i in 1..4 {
        let s = len * i as f64 / 4.0;
        let (x, _) = b.evaluate_f64(b.param_at_length(s));
        assert!(
            (x - i as f64).abs() < 0.05,
            "quarter {i}: landed at x={x}, wanted {}",
            i
        );
    }
}

#[test]
fn param_at_length_walks_polycurve_segments_by_length() {
    // Two segments of length 1 and 3: the poly parameter gives each half
    // the domain, so arc length 2 is NOT at t=0.5 — it is one third into
    // the second segment.
    let p = Curve::Poly(Box::new(PolyCurve::new(vec![
        Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(1, 0))),
        Curve::Line(LineSeg::from_endpoints(pt(1, 0), pt(4, 0))),
    ])));
    let t = p.param_at_length(2.0);
    let (x, y) = p.evaluate_f64(t);
    assert!(
        (x - 2.0).abs() < 1e-9 && y.abs() < 1e-9,
        "landed at ({x},{y})"
    );

    // Clamping at both ends.
    assert_eq!(p.param_at_length(-1.0), 0.0);
    assert_eq!(p.param_at_length(100.0), 1.0);
}

#[test]
fn polycurve_offset_trims_self_intersection_loops() {
    // A 10×6 outline with a deep notch (x∈[4,6], down to y=2). Offsetting
    // inward by 1.5 makes the raw chain cross itself under the notch (the
    // notch-bottom offset dips below the outer-bottom offset). The trimmed
    // result must hold the offset distance everywhere and never cross
    // itself; the outward offset must pass the same checks untouched.
    let pts = [
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 6.0),
        (6.0, 6.0),
        (6.0, 2.0),
        (4.0, 2.0),
        (4.0, 6.0),
        (0.0, 6.0),
    ];
    let segs: Vec<Curve> = (0..pts.len())
        .map(|i| {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(a.0, a.1),
                Point2d::from_f64(b.0, b.1),
            ))
        })
        .collect();
    let source = Curve::Poly(Box::new(PolyCurve::new(segs)));
    let d = 1.5;

    for sign in [1.0, -1.0] {
        let off = offset_curve(&source, sign * d);
        let Curve::Poly(op) = &off else {
            panic!("poly offset stays a poly");
        };
        assert!(!op.segments.is_empty(), "offset survived trimming");
        // Every piece holds the offset distance (the raw inward chain
        // fails this under the notch before trimming).
        for (i, s) in op.segments.iter().enumerate() {
            let (t0, t1) = s.domain();
            for f in [0.25, 0.5, 0.75] {
                let (x, y) = s.evaluate_f64(t0 + (t1 - t0) * f);
                let dist = point_to_curve_distance(&source, x, y);
                assert!(
                    dist >= d - 0.05,
                    "sign {sign}: segment {i} point ({x:.2},{y:.2}) sits at {dist:.3} < {d}"
                );
            }
        }
        // And the chain no longer crosses itself away from its joints.
        let m = op.segments.len();
        for i in 0..m {
            for j in (i + 2)..m {
                if i == 0 && j == m - 1 {
                    continue; // closing joint
                }
                for hit in intersect(&op.segments[i], &op.segments[j]) {
                    // Endpoint touches from the trim reassembly are fine.
                    let (e0, e1) = {
                        let (t0, t1) = op.segments[i].domain();
                        (
                            op.segments[i].evaluate_f64(t0),
                            op.segments[i].evaluate_f64(t1),
                        )
                    };
                    let at_end = [e0, e1]
                        .iter()
                        .any(|e| (e.0 - hit.point.0).hypot(e.1 - hit.point.1) < 1e-6);
                    assert!(
                        at_end,
                        "sign {sign}: segments {i} and {j} still cross at {:?}",
                        hit.point
                    );
                }
            }
        }
    }
}

#[test]
fn projection_onto_a_reversed_arc_uses_its_true_span() {
    // Quarter arc traversed backwards (pi/2 -> 0), as reverse_curve
    // produces. The nearest point to (1.5, 1.5) is the arc's midpoint
    // (sqrt(2)/2, sqrt(2)/2); clamping against the complementary sweep
    // would instead snap to an endpoint.
    let rev = Curve::Arc(CircularArc::new(
        pt(0, 0),
        1.0,
        std::f64::consts::FRAC_PI_2,
        0.0,
    ));
    let pr = project_point_onto_curve(&rev, 1.5, 1.5);
    let m = std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        (pr.point.0 - m).abs() < 1e-9 && (pr.point.1 - m).abs() < 1e-9,
        "nearest point must be the arc midpoint, got {:?}",
        pr.point
    );
    let expected = (1.5f64 * 1.5 * 2.0).sqrt() - 1.0;
    assert!((pr.distance - expected).abs() < 1e-9);
}

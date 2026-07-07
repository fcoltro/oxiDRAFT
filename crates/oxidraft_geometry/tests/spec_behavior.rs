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

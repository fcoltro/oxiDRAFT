use oxidraft_geometry::*;

fn pt(x: f64, y: f64) -> Point2d {
    Point2d::from_f64(x, y)
}

#[test]
fn three_point_arc_passes_through_middle_clockwise() {
    let p1 = pt(1.0, 0.0);
    let p2 = pt(0.0, -1.0);
    let p3 = pt(-1.0, 0.0);
    let arc = CircularArc::from_three_points(&p1, &p2, &p3).expect("not collinear");

    let (a0, a1) = arc.domain();
    let mut min_dist_to_mid = f64::INFINITY;
    let n = 200;
    for i in 0..=n {
        let t = a0 + (a1 - a0) * i as f64 / n as f64;
        let (x, y) = arc.evaluate_f64(t);
        let d = ((x - 0.0).powi(2) + (y - (-1.0)).powi(2)).sqrt();
        min_dist_to_mid = min_dist_to_mid.min(d);
    }
    assert!(
        min_dist_to_mid < 1e-3,
        "3-point arc does not pass through the middle point (0,-1); closest approach = {}",
        min_dist_to_mid
    );
}

#[test]
fn three_point_arc_passes_through_middle_ccw() {
    let p1 = pt(1.0, 0.0);
    let p2 = pt(0.0, 1.0);
    let p3 = pt(-1.0, 0.0);
    let arc = CircularArc::from_three_points(&p1, &p2, &p3).unwrap();
    let (a0, a1) = arc.domain();
    let mut min_d = f64::INFINITY;
    for i in 0..=200 {
        let t = a0 + (a1 - a0) * i as f64 / 200.0;
        let (x, y) = arc.evaluate_f64(t);
        min_d = min_d.min(((x).powi(2) + (y - 1.0).powi(2)).sqrt());
    }
    assert!(min_d < 1e-3, "CCW 3-point arc missed middle; d={}", min_d);
}

#[test]
fn line_offset_distance_is_exact_at_angle() {
    let line = Curve::Line(LineSeg::from_endpoints(pt(0.0, 0.0), pt(3.0, 4.0)));
    let off = offset_curve(&line, 2.0);
    if let Curve::Line(l) = off {
        let d = point_to_curve_distance(&Curve::Line(l.clone()), 0.0, 0.0);
        assert!(
            (d - 2.0).abs() < 1e-3,
            "offset distance = {}, expected 2",
            d
        );
    } else {
        panic!("offset of line should be a line");
    }
}

#[test]
fn mirror_arc_keeps_radius_flips_center() {
    let arc = Curve::Arc(CircularArc::new(
        pt(3.0, 4.0),
        5.0,
        0.0,
        std::f64::consts::FRAC_PI_2,
    ));
    let t = Transform2d::mirror_x();
    if let Curve::Arc(a) = t.apply_curve(&arc) {
        assert!((a.center.x - 3.0).abs() < 1e-9);
        assert!(
            (a.center.y + 4.0).abs() < 1e-9,
            "center y should flip to -4"
        );
        assert!((a.radius - 5.0).abs() < 1e-6, "radius preserved");
        assert!(
            (a.included_angle() - std::f64::consts::FRAC_PI_2).abs() < 1e-6,
            "included angle changed, expected a quarter arc: {}",
            a.included_angle()
        );
        let (ms_x, ms_y) = a.evaluate_f64(a.domain().0);
        let (me_x, me_y) = a.evaluate_f64(a.domain().1);
        let (os_x, os_y) = arc.evaluate_f64(arc.domain().0);
        let (oe_x, oe_y) = arc.evaluate_f64(arc.domain().1);
        assert!(
            (ms_x - oe_x).abs() < 1e-6 && (ms_y + oe_y).abs() < 1e-6,
            "mirrored start ({},{}) is not the reflection of original end ({},{})",
            ms_x,
            ms_y,
            oe_x,
            oe_y
        );
        assert!(
            (me_x - os_x).abs() < 1e-6 && (me_y + os_y).abs() < 1e-6,
            "mirrored end ({},{}) is not the reflection of original start ({},{})",
            me_x,
            me_y,
            os_x,
            os_y
        );
    } else {
        panic!("expected arc");
    }
}

#[test]
fn rotate_circle_about_point() {
    let circ = Curve::Arc(CircularArc::new(
        pt(2.0, 0.0),
        1.0,
        0.0,
        std::f64::consts::TAU,
    ));
    let t = Transform2d::rotation_about(&pt(0.0, 0.0), std::f64::consts::FRAC_PI_2);
    if let Curve::Arc(a) = t.apply_curve(&circ) {
        assert!((a.center.x).abs() < 1e-6, "cx≈0, got {}", a.center.x);
        assert!((a.center.y - 2.0).abs() < 1e-6, "cy≈2, got {}", a.center.y);
        assert!((a.radius - 1.0).abs() < 1e-6);
    } else {
        panic!();
    }
}

#[test]
fn line_circle_tangent_single_touch() {
    let circle = CircularArc::new(
        pt(0.0, 0.0),
        5.0,
        -std::f64::consts::PI,
        std::f64::consts::PI,
    );
    let line = LineSeg::from_endpoints(pt(-10.0, 5.0), pt(10.0, 5.0));
    let hits = intersect_line_circle(&line, &circle);
    assert!(!hits.is_empty(), "tangent should touch");
    for h in &hits {
        assert!(
            (h.point.0).abs() < 1e-2 && (h.point.1 - 5.0).abs() < 1e-2,
            "tangent touch should be ≈(0,5), got {:?}",
            h.point
        );
    }
}

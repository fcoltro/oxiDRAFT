use oxidraft_boolean::{Region, difference, intersection, union};
use oxidraft_geometry::{Curve, LineSeg, Point2d};

fn square(x0: i64, y0: i64, x1: i64, y1: i64) -> Region {
    Region::new(vec![
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(x0, y0),
            Point2d::from_i64(x1, y0),
        )),
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(x1, y0),
            Point2d::from_i64(x1, y1),
        )),
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(x1, y1),
            Point2d::from_i64(x0, y1),
        )),
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(x0, y1),
            Point2d::from_i64(x0, y0),
        )),
    ])
}

#[test]
fn disjoint_union_keeps_both() {
    let a = square(0, 0, 2, 2);
    let b = square(5, 5, 7, 7);
    let u = union(&a, &b);
    assert_eq!(u.len(), 2, "disjoint union must keep both components");
    assert!(u.iter().any(|r| r.contains_point(1.0, 1.0)));
    assert!(u.iter().any(|r| r.contains_point(6.0, 6.0)));
    assert!(!u.iter().any(|r| r.contains_point(3.5, 3.5)));
}

#[test]
fn intersection_of_disjoint_is_empty() {
    let a = square(0, 0, 2, 2);
    let b = square(5, 5, 7, 7);
    let inter = intersection(&a, &b);
    assert!(
        inter.is_empty(),
        "disjoint intersection must be empty, got {} region(s)",
        inter.len()
    );
}

#[test]
fn difference_self_is_empty_interior() {
    let a = square(0, 0, 4, 4);
    let b = square(0, 0, 4, 4);
    let diff = difference(&a, &b);
    use oxidraft_geometry::CurveSegment;
    for region in &diff {
        for seg in &region.outer {
            let (t0, t1) = seg.domain();
            let (mx, my) = seg.evaluate_f64((t0 + t1) / 2.0);
            assert!(
                !a.contains_point(mx, my) || b.contains_point(mx, my),
                "A−A left a segment at ({},{})",
                mx,
                my
            );
        }
    }
}

#[test]
fn region_winding_number_basic() {
    let a = square(0, 0, 10, 10);
    assert!(a.contains_point(5.0, 5.0), "center inside");
    assert!(!a.contains_point(-1.0, 5.0), "left of square outside");
    assert!(!a.contains_point(11.0, 5.0), "right of square outside");
    assert!(!a.contains_point(5.0, 20.0), "above square outside");
}

#[test]
fn region_with_hole_excludes_hole_interior() {
    let outer = vec![
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(0, 0),
            Point2d::from_i64(10, 0),
        )),
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(10, 0),
            Point2d::from_i64(10, 10),
        )),
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(10, 10),
            Point2d::from_i64(0, 10),
        )),
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(0, 10),
            Point2d::from_i64(0, 0),
        )),
    ];
    let hole = vec![
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(3, 3),
            Point2d::from_i64(3, 7),
        )),
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(3, 7),
            Point2d::from_i64(7, 7),
        )),
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(7, 7),
            Point2d::from_i64(7, 3),
        )),
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(7, 3),
            Point2d::from_i64(3, 3),
        )),
    ];
    let region = Region::with_holes(outer, vec![hole]);
    assert!(
        region.contains_point(1.0, 5.0),
        "ring point should be inside"
    );
    assert!(
        !region.contains_point(5.0, 5.0),
        "hole center should be outside"
    );
}

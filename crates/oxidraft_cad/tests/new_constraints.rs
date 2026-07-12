//! The Phase-2 constraint kinds: concentric, collinear, equal-radius, the
//! pick-based point relations (midpoint, point-on-line, point-on-circle),
//! and the driving point distances (straight, horizontal, vertical). Each
//! kind must move the geometry into the relation on apply, record it, and
//! reject genuine conflicts without touching the document.

use oxidraft_cad::constrain::{
    constrain_lines, constrain_point_distance, constrain_point_pair, resolve_after_transform,
    selection_validity,
};
use oxidraft_document::{
    ANCHOR_DERIVED, ConstraintKind, Document, EntityId, EntityKind, SketchConstraint,
};
use oxidraft_geometry::{CircularArc, Curve, LineSeg, Point2d};

fn add_line(doc: &mut Document, x0: f64, y0: f64, x1: f64, y1: f64) -> EntityId {
    doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
        Point2d::from_f64(x0, y0),
        Point2d::from_f64(x1, y1),
    ))))
}

fn add_circle(doc: &mut Document, cx: f64, cy: f64, r: f64) -> EntityId {
    doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
        Point2d::from_f64(cx, cy),
        r,
        0.0,
        std::f64::consts::TAU,
    ))))
}

fn add_point(doc: &mut Document, x: f64, y: f64) -> EntityId {
    doc.add(EntityKind::Point(Point2d::from_f64(x, y)))
}

fn line(doc: &Document, id: EntityId) -> LineSeg {
    match doc.get(id).and_then(|e| e.as_curve()) {
        Some(Curve::Line(l)) => l.clone(),
        other => panic!("expected a line, got {other:?}"),
    }
}

fn circle(doc: &Document, id: EntityId) -> CircularArc {
    match doc.get(id).and_then(|e| e.as_curve()) {
        Some(Curve::Arc(a)) => *a,
        other => panic!("expected an arc, got {other:?}"),
    }
}

fn point(doc: &Document, id: EntityId) -> Point2d {
    match doc.get(id).map(|e| &e.kind) {
        Some(EntityKind::Point(p)) => *p,
        other => panic!("expected a point, got {other:?}"),
    }
}

#[test]
fn concentric_welds_the_centers_and_survives_a_drag() {
    let mut doc = Document::new();
    let a = add_circle(&mut doc, 0.0, 0.0, 2.0);
    let b = add_circle(&mut doc, 5.0, 1.0, 0.8);
    constrain_lines(&mut doc, &[a, b], ConstraintKind::Concentric).expect("concentric");
    let (ca, cb) = (circle(&doc, a), circle(&doc, b));
    assert!(
        (ca.center.x - cb.center.x).abs() < 1e-6 && (ca.center.y - cb.center.y).abs() < 1e-6,
        "centers coincide: {:?} vs {:?}",
        ca.center,
        cb.center
    );
    assert!((cb.radius - 0.8).abs() < 1e-6, "mover kept its radius");

    // Drag the first circle elsewhere: the second must follow its center.
    if let Some(e) = doc.get_mut(a) {
        e.kind = EntityKind::Curve(Curve::Arc(CircularArc::new(
            Point2d::from_f64(10.0, -3.0),
            2.0,
            0.0,
            std::f64::consts::TAU,
        )));
    }
    assert!(resolve_after_transform(&mut doc, &[a]));
    let cb = circle(&doc, b);
    assert!(
        (cb.center.x - 10.0).abs() < 1e-6 && (cb.center.y + 3.0).abs() < 1e-6,
        "second circle followed: {:?}",
        cb.center
    );
}

#[test]
fn equal_radius_resizes_the_mover_in_place() {
    let mut doc = Document::new();
    let a = add_circle(&mut doc, 0.0, 0.0, 2.5);
    let b = add_circle(&mut doc, 6.0, 0.0, 1.0);
    constrain_lines(&mut doc, &[a, b], ConstraintKind::EqualRadius).expect("equal radius");
    let cb = circle(&doc, b);
    assert!(
        (cb.radius - 2.5).abs() < 1e-6,
        "radius matched: {}",
        cb.radius
    );
    assert!(
        (cb.center.x - 6.0).abs() < 1e-6,
        "mover stayed centered: {:?}",
        cb.center
    );
}

#[test]
fn equal_radius_rejects_two_conflicting_driving_radii() {
    let mut doc = Document::new();
    let a = add_circle(&mut doc, 0.0, 0.0, 2.0);
    let b = add_circle(&mut doc, 6.0, 0.0, 1.0);
    doc.add_constraint(SketchConstraint::radius(a, 2.0));
    doc.add_constraint(SketchConstraint::radius(b, 1.0));
    let before = doc.constraints.len();
    let err = constrain_lines(&mut doc, &[a, b], ConstraintKind::EqualRadius).unwrap_err();
    assert!(
        err.message.contains("radius"),
        "names the conflicting kind: {}",
        err.message
    );
    assert!(!err.culprits.is_empty(), "carries culprit entities");
    assert_eq!(doc.constraints.len(), before, "record rolled back");
    assert!(
        (circle(&doc, b).radius - 1.0).abs() < 1e-9,
        "geometry untouched"
    );
}

#[test]
fn collinear_lays_the_mover_onto_the_carrier() {
    let mut doc = Document::new();
    let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
    let b = add_line(&mut doc, 5.0, 1.2, 8.0, 1.5);
    constrain_lines(&mut doc, &[a, b], ConstraintKind::Collinear).expect("collinear");
    let lb = line(&doc, b);
    assert!(
        lb.p0.y.abs() < 1e-6 && lb.p1.y.abs() < 1e-6,
        "mover landed on the x axis: {lb:?}"
    );
    let kept = (lb.p1.x - lb.p0.x).hypot(lb.p1.y - lb.p0.y);
    assert!((kept - (3.0f64.powi(2) + 0.3f64.powi(2)).sqrt()).abs() < 1e-6);
}

#[test]
fn collinear_rejects_against_perpendicular() {
    let mut doc = Document::new();
    let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
    let b = add_line(&mut doc, 5.0, 1.0, 8.0, 1.3);
    constrain_lines(&mut doc, &[a, b], ConstraintKind::Perpendicular).expect("perp");
    let err = constrain_lines(&mut doc, &[a, b], ConstraintKind::Collinear).unwrap_err();
    assert!(
        err.message.contains("conflicts with its existing"),
        "diagnosed: {}",
        err.message
    );
}

// The pick-based relations move BOTH sides minimally (the same policy as
// the existing weld), so these assert the relation itself against the
// post-solve geometry, not absolute positions.

#[test]
fn midpoint_holds_a_point_at_a_lines_middle() {
    let mut doc = Document::new();
    let l = add_line(&mut doc, 0.0, 0.0, 4.0, 2.0);
    let p = add_point(&mut doc, 3.0, 3.0);
    constrain_point_pair(
        &mut doc,
        ConstraintKind::Midpoint,
        (p, 0),
        (l, ANCHOR_DERIVED),
    )
    .expect("midpoint");
    let pt = point(&doc, p).to_f64();
    let ll = line(&doc, l);
    let mid = ((ll.p0.x + ll.p1.x) * 0.5, (ll.p0.y + ll.p1.y) * 0.5);
    assert!(
        (pt.0 - mid.0).abs() < 1e-6 && (pt.1 - mid.1).abs() < 1e-6,
        "point sits at the line's midpoint: {pt:?} vs {mid:?}"
    );
}

#[test]
fn point_on_line_drops_the_point_onto_the_carrier() {
    let mut doc = Document::new();
    let l = add_line(&mut doc, 0.0, 0.0, 6.0, 0.0);
    let p = add_point(&mut doc, 2.5, 1.7);
    // Pin the line so the relation resolves by moving the point.
    doc.add_constraint(SketchConstraint::fixed(l));
    constrain_point_pair(&mut doc, ConstraintKind::PointOnLine, (p, 0), (l, 0))
        .expect("point on line");
    let pt = point(&doc, p).to_f64();
    assert!(pt.1.abs() < 1e-6, "point dropped onto the line: {pt:?}");
    assert!((pt.0 - 2.5).abs() < 0.5, "roughly where it started");
}

#[test]
fn point_on_circle_pulls_the_point_to_the_rim() {
    let mut doc = Document::new();
    let c = add_circle(&mut doc, 0.0, 0.0, 2.0);
    let p = add_point(&mut doc, 2.7, 0.4);
    constrain_point_pair(&mut doc, ConstraintKind::PointOnCircle, (p, 0), (c, 0))
        .expect("point on circle");
    let pt = point(&doc, p).to_f64();
    let cc = circle(&doc, c);
    let d = (pt.0 - cc.center.x).hypot(pt.1 - cc.center.y);
    assert!(
        (d - cc.radius).abs() < 1e-6,
        "point sits on the rim: {pt:?} vs {cc:?}"
    );
}

#[test]
fn point_distance_drives_two_endpoints_apart() {
    let mut doc = Document::new();
    let a = add_line(&mut doc, 0.0, 0.0, 1.0, 0.0);
    let b = add_line(&mut doc, 2.0, 0.0, 3.0, 0.0);
    doc.add_constraint(SketchConstraint::fixed(a));
    constrain_point_distance(
        &mut doc,
        ConstraintKind::PointDistance,
        (a, 0),
        (b, 0),
        Some(5.0),
        None,
    )
    .expect("point distance");
    let (la, lb) = (line(&doc, a), line(&doc, b));
    let d = (lb.p0.x - la.p0.x).hypot(lb.p0.y - la.p0.y);
    assert!((d - 5.0).abs() < 1e-6, "separation driven to 5: {d}");
}

#[test]
fn h_and_v_distance_drive_axis_separations() {
    let mut doc = Document::new();
    let a = add_point(&mut doc, 0.0, 0.0);
    let b = add_point(&mut doc, 1.0, 1.0);
    doc.add_constraint(SketchConstraint::fixed(a));
    constrain_point_distance(
        &mut doc,
        ConstraintKind::HDistance,
        (a, 0),
        (b, 0),
        Some(4.0),
        None,
    )
    .expect("hdist");
    constrain_point_distance(
        &mut doc,
        ConstraintKind::VDistance,
        (a, 0),
        (b, 0),
        Some(3.0),
        None,
    )
    .expect("vdist");
    let pb = point(&doc, b).to_f64();
    assert!((pb.0.abs() - 4.0).abs() < 1e-6, "dx driven to 4: {pb:?}");
    assert!((pb.1.abs() - 3.0).abs() < 1e-6, "dy driven to 3: {pb:?}");
}

#[test]
fn point_distance_retargets_like_other_driving_dimensions() {
    let mut doc = Document::new();
    let a = add_point(&mut doc, 0.0, 0.0);
    let b = add_point(&mut doc, 2.0, 0.0);
    doc.add_constraint(SketchConstraint::fixed(a));
    constrain_point_distance(
        &mut doc,
        ConstraintKind::PointDistance,
        (a, 0),
        (b, 0),
        Some(2.0),
        None,
    )
    .expect("first");
    constrain_point_distance(
        &mut doc,
        ConstraintKind::PointDistance,
        (a, 0),
        (b, 0),
        Some(6.0),
        None,
    )
    .expect("retarget");
    let records: Vec<_> = doc
        .constraints
        .iter()
        .filter(|c| c.kind == ConstraintKind::PointDistance)
        .collect();
    assert_eq!(records.len(), 1, "one record, retargeted");
    assert_eq!(records[0].val, Some(6.0));
    let pb = point(&doc, b).to_f64();
    assert!((pb.0.hypot(pb.1) - 6.0).abs() < 1e-6);
}

#[test]
fn selection_validity_matches_dispatch_requirements() {
    let mut doc = Document::new();
    let l1 = add_line(&mut doc, 0.0, 0.0, 1.0, 0.0);
    let l2 = add_line(&mut doc, 0.0, 1.0, 1.0, 1.0);
    let c1 = add_circle(&mut doc, 0.0, 0.0, 1.0);
    let c2 = add_circle(&mut doc, 3.0, 0.0, 1.0);

    assert!(selection_validity(&doc, &[l1], ConstraintKind::Horizontal).is_ok());
    assert!(selection_validity(&doc, &[c1], ConstraintKind::Horizontal).is_err());
    assert!(selection_validity(&doc, &[l1, l2], ConstraintKind::Collinear).is_ok());
    assert!(selection_validity(&doc, &[l1], ConstraintKind::Collinear).is_err());
    assert!(selection_validity(&doc, &[c1, c2], ConstraintKind::Concentric).is_ok());
    assert!(selection_validity(&doc, &[l1, c1], ConstraintKind::Concentric).is_err());
    assert!(selection_validity(&doc, &[l1, c1], ConstraintKind::Tangent).is_ok());
    // Pick-based kinds accept any selection — they open a pick tool.
    assert!(selection_validity(&doc, &[], ConstraintKind::Midpoint).is_ok());
    assert!(selection_validity(&doc, &[], ConstraintKind::PointDistance).is_ok());
}

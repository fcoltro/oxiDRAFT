//! The validation gate that accepts or rejects a new constraint must draw
//! the line exactly at *geometric* consistency: a relation the solver can
//! satisfy by honestly moving and stretching things is never a conflict,
//! even when it forces some line's length to change — and a relation whose
//! only numeric "solution" degenerates a line to a point is always one.
//! Each false-positive case here was reachable from the UI (the welded-quad
//! ones from hand-drawn rectangles, the retarget ones from the smart
//! dimension tool).

use oxidraft_cad::constrain::{
    constrain_angle, constrain_coincident_points, constrain_distance, constrain_line_distance,
    constrain_lines,
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

fn line(doc: &Document, id: EntityId) -> LineSeg {
    match doc.get(id).and_then(|e| e.as_curve()) {
        Some(Curve::Line(l)) => l.clone(),
        other => panic!("expected a line, got {other:?}"),
    }
}

/// Four lines welded corner-to-corner like the Rectangle tool emits them:
/// top (0,4)→(6,4), right (6,4)→(6,0), bottom (6,0)→(skew,0), left
/// (skew,0)→(0,4). `skew` slides the bottom-left corner right, so the left
/// edge leans — the shape of any hand-drawn almost-rectangle.
fn welded_quad(skew: f64) -> (Document, [EntityId; 4]) {
    let mut doc = Document::new();
    let top = add_line(&mut doc, 0.0, 4.0, 6.0, 4.0);
    let right = add_line(&mut doc, 6.0, 4.0, 6.0, 0.0);
    let bottom = add_line(&mut doc, 6.0, 0.0, skew, 0.0);
    let left = add_line(&mut doc, skew, 0.0, 0.0, 4.0);
    doc.add_constraint(SketchConstraint::coincident(top, 1, right, 0));
    doc.add_constraint(SketchConstraint::coincident(right, 1, bottom, 0));
    doc.add_constraint(SketchConstraint::coincident(bottom, 1, left, 0));
    doc.add_constraint(SketchConstraint::coincident(left, 1, top, 0));
    (doc, [top, right, bottom, left])
}

/// A perfect rectangle with H/V already recorded on every side — what the
/// Rectangle tool produces with auto-constrain on.
fn hv_rect() -> (Document, [EntityId; 4]) {
    let (mut doc, ids) = welded_quad(0.0);
    let [top, right, bottom, left] = ids;
    for id in [top, bottom] {
        doc.add_constraint(SketchConstraint::single(ConstraintKind::Horizontal, id));
    }
    for id in [left, right] {
        doc.add_constraint(SketchConstraint::single(ConstraintKind::Vertical, id));
    }
    (doc, ids)
}

// ---- previously rejected as "conflicts", must be accepted ----------------

#[test]
fn skewed_quad_left_vertical_after_top_h_right_v() {
    let (mut doc, [top, right, _bottom, left]) = welded_quad(1.0);
    constrain_lines(&mut doc, &[top], ConstraintKind::Horizontal).expect("top H");
    constrain_lines(&mut doc, &[right], ConstraintKind::Vertical).expect("right V");
    constrain_lines(&mut doc, &[left], ConstraintKind::Vertical)
        .expect("left V is satisfiable — the edge just has to shorten");
    let l = line(&doc, left);
    assert!(
        (l.p0.x - l.p1.x).abs() < 1e-5,
        "left actually went vertical: {l:?}"
    );
}

#[test]
fn skewed_quad_left_parallel_to_right() {
    let (mut doc, [top, right, _bottom, left]) = welded_quad(1.0);
    constrain_lines(&mut doc, &[top], ConstraintKind::Horizontal).expect("top H");
    constrain_lines(&mut doc, &[right], ConstraintKind::Vertical).expect("right V");
    constrain_lines(&mut doc, &[right, left], ConstraintKind::Parallel)
        .expect("left ∥ right is satisfiable");
    let l = line(&doc, left);
    assert!(
        (l.p0.x - l.p1.x).abs() < 1e-5,
        "left settled parallel to the vertical right edge: {l:?}"
    );
}

#[test]
fn skewed_quad_left_perpendicular_to_top() {
    let (mut doc, [top, right, _bottom, left]) = welded_quad(1.0);
    constrain_lines(&mut doc, &[top], ConstraintKind::Horizontal).expect("top H");
    constrain_lines(&mut doc, &[right], ConstraintKind::Vertical).expect("right V");
    constrain_lines(&mut doc, &[top, left], ConstraintKind::Perpendicular)
        .expect("left ⊥ top is satisfiable");
}

#[test]
fn skewed_quad_angle_between_top_and_left() {
    let (mut doc, [top, right, _bottom, left]) = welded_quad(1.0);
    constrain_lines(&mut doc, &[top], ConstraintKind::Horizontal).expect("top H");
    constrain_lines(&mut doc, &[right], ConstraintKind::Vertical).expect("right V");
    constrain_angle(&mut doc, &[top, left], Some(90.0))
        .expect("driving the corner to 90° is satisfiable");
}

#[test]
fn hv_rectangle_accepts_redundant_parallel() {
    // Redundant-but-consistent is not a conflict: with all four sides H/V,
    // left ∥ right adds no information but must record cleanly.
    let (mut doc, [_top, right, _bottom, left]) = hv_rect();
    constrain_lines(&mut doc, &[right, left], ConstraintKind::Parallel)
        .expect("redundant parallel on a fully H/V rectangle");
}

#[test]
fn hv_rectangle_width_retarget_resizes() {
    // The smart-dimension flow: lock the current width, then retarget it.
    // Both records must validate, and the retarget must actually resize —
    // the horizontal sides stretch from 6 to 8.
    let (mut doc, [top, right, _bottom, left]) = hv_rect();
    constrain_line_distance(&mut doc, &[left, right], None).expect("lock current width");
    constrain_line_distance(&mut doc, &[left, right], Some(8.0))
        .expect("width retarget is satisfiable — the H sides stretch");
    let width = (line(&doc, right).p0.x - line(&doc, left).p0.x).abs();
    assert!((width - 8.0).abs() < 1e-5, "rectangle resized to {width}");
    let t = line(&doc, top);
    assert!(
        (t.p0.y - t.p1.y).abs() < 1e-5,
        "top stayed horizontal through the resize: {t:?}"
    );
}

#[test]
fn hv_rectangle_side_length_retarget_resizes() {
    let (mut doc, [top, _right, bottom, _left]) = hv_rect();
    constrain_distance(&mut doc, &[top], Some(9.0)).expect("driving the top length");
    let t = line(&doc, top);
    let b = line(&doc, bottom);
    assert!(
        ((t.p1.x - t.p0.x).abs() - 9.0).abs() < 1e-5,
        "top resized: {t:?}"
    );
    assert!(
        ((b.p1.x - b.p0.x).abs() - 9.0).abs() < 1e-5,
        "bottom followed through the welds: {b:?}"
    );
}

#[test]
fn hv_rectangle_equal_length_squares_it() {
    let (mut doc, [top, right, _bottom, _left]) = hv_rect();
    constrain_lines(&mut doc, &[top, right], ConstraintKind::EqualLength)
        .expect("equalizing adjacent sides is satisfiable");
}

// ---- pick-based welds (endpoint / midpoint / center / point) -------------

#[test]
fn origin_welds_to_a_line_midpoint() {
    // The reported flow: coincident between the fixed origin point and the
    // midpoint of a line. The line must slide so its middle lands on (0,0);
    // the origin must not move.
    let mut doc = Document::new();
    let origin = doc.add(EntityKind::Point(Point2d::from_i64(0, 0)));
    doc.add_constraint(SketchConstraint::fixed(origin));
    let l = add_line(&mut doc, 2.0, 3.0, 6.0, 5.0);
    constrain_coincident_points(&mut doc, (origin, 0), (l, ANCHOR_DERIVED))
        .expect("origin→midpoint weld");
    let ls = line(&doc, l);
    let mid = ((ls.p0.x + ls.p1.x) * 0.5, (ls.p0.y + ls.p1.y) * 0.5);
    assert!(
        mid.0.abs() < 1e-6 && mid.1.abs() < 1e-6,
        "line midpoint landed on the origin: {mid:?}"
    );
    let len = (ls.p1.x - ls.p0.x).hypot(ls.p1.y - ls.p0.y);
    assert!(
        (len - 20.0f64.sqrt()).abs() < 1e-6,
        "the line only translated, keeping its length: {len}"
    );
}

#[test]
fn two_line_midpoints_weld_together() {
    let mut doc = Document::new();
    let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
    let b = add_line(&mut doc, 10.0, 5.0, 14.0, 7.0);
    constrain_coincident_points(&mut doc, (a, ANCHOR_DERIVED), (b, ANCHOR_DERIVED))
        .expect("midpoint→midpoint weld");
    let (la, lb) = (line(&doc, a), line(&doc, b));
    let ma = ((la.p0.x + la.p1.x) * 0.5, (la.p0.y + la.p1.y) * 0.5);
    let mb = ((lb.p0.x + lb.p1.x) * 0.5, (lb.p0.y + lb.p1.y) * 0.5);
    assert!(
        (ma.0 - mb.0).hypot(ma.1 - mb.1) < 1e-6,
        "midpoints met: {ma:?} vs {mb:?}"
    );
}

#[test]
fn circle_center_welds_to_a_line_endpoint() {
    let mut doc = Document::new();
    let l = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
    let c = doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
        Point2d::from_f64(7.0, 2.0),
        1.5,
        0.0,
        std::f64::consts::TAU,
    ))));
    constrain_coincident_points(&mut doc, (c, ANCHOR_DERIVED), (l, 1)).expect("center→end weld");
    let center = match doc.get(c).and_then(|e| e.as_curve()) {
        Some(Curve::Arc(a)) => a.center.to_f64(),
        other => panic!("expected the circle, got {other:?}"),
    };
    let end = line(&doc, l).p1.to_f64();
    assert!(
        (center.0 - end.0).hypot(center.1 - end.1) < 1e-6,
        "center met the endpoint: {center:?} vs {end:?}"
    );
}

#[test]
fn weld_rejects_duplicates_and_self_welds() {
    let mut doc = Document::new();
    let origin = doc.add(EntityKind::Point(Point2d::from_i64(0, 0)));
    doc.add_constraint(SketchConstraint::fixed(origin));
    let l = add_line(&mut doc, 2.0, 3.0, 6.0, 5.0);
    constrain_coincident_points(&mut doc, (origin, 0), (l, ANCHOR_DERIVED)).expect("first weld");
    assert!(
        constrain_coincident_points(&mut doc, (origin, 0), (l, ANCHOR_DERIVED)).is_err(),
        "the same weld twice reports 'already welded'"
    );
    assert!(
        constrain_coincident_points(&mut doc, (l, 0), (l, ANCHOR_DERIVED)).is_err(),
        "welding a line to itself is refused"
    );
}

// ---- genuine conflicts, must still be rejected ---------------------------

#[test]
fn horizontal_then_vertical_on_one_line_is_rejected() {
    let mut doc = Document::new();
    let a = add_line(&mut doc, 0.0, 0.0, 5.0, 1.0);
    constrain_lines(&mut doc, &[a], ConstraintKind::Horizontal).expect("H first");
    let r = constrain_lines(&mut doc, &[a], ConstraintKind::Vertical);
    assert!(r.is_err(), "H and V on one line is impossible: {r:?}");
    assert_eq!(
        doc.constraints
            .iter()
            .filter(|c| c.kind == ConstraintKind::Vertical)
            .count(),
        0,
        "the rejected record was unwound"
    );
}

#[test]
fn parallel_then_perpendicular_on_one_pair_is_rejected() {
    // Also the zero-length-escape guard: numerically, ∥ and ⊥ hold at once
    // for a line collapsed to a point, and only there. The gate must not
    // accept that as a solution.
    let mut doc = Document::new();
    let a = add_line(&mut doc, 0.0, 0.0, 5.0, 0.0);
    let b = add_line(&mut doc, 1.0, 2.0, 5.0, 2.5);
    constrain_lines(&mut doc, &[a, b], ConstraintKind::Parallel).expect("∥ first");
    let r = constrain_lines(&mut doc, &[a, b], ConstraintKind::Perpendicular);
    assert!(r.is_err(), "∥ and ⊥ on one pair is impossible: {r:?}");
    let lb = line(&doc, b);
    assert!(
        (lb.p1.x - lb.p0.x).hypot(lb.p1.y - lb.p0.y) > 1.0,
        "the mover kept its extent: {lb:?}"
    );
}

#[test]
fn hv_rectangle_rejects_a_45_degree_corner() {
    let (mut doc, [top, right, _bottom, _left]) = hv_rect();
    let r = constrain_angle(&mut doc, &[top, right], Some(45.0));
    assert!(
        r.is_err(),
        "45° between an H side and a V side is impossible: {r:?}"
    );
    assert_eq!(
        doc.constraints
            .iter()
            .filter(|c| c.kind == ConstraintKind::Angle)
            .count(),
        0,
        "the rejected record was unwound"
    );
}

#[test]
fn hv_rectangle_rejects_an_impossible_width() {
    // A width record between the two horizontal sides of an H/V rectangle
    // whose vertical sides are length-locked cannot hold at a new value.
    let (mut doc, [top, right, bottom, left]) = hv_rect();
    constrain_distance(&mut doc, &[left], Some(4.0)).expect("lock left length");
    constrain_distance(&mut doc, &[right], Some(4.0)).expect("lock right length");
    let r = constrain_line_distance(&mut doc, &[bottom, top], Some(7.0));
    assert!(
        r.is_err(),
        "gap 7 between rails joined by length-4 verticals is impossible: {r:?}"
    );
}

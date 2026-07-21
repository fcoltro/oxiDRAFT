//! Object snapping: finding the significant points on nearby geometry the
//! cursor should latch onto (endpoints, midpoints, centres, intersections, …)
//! and choosing the best one under a tolerance.

use oxidraft_document::{Document, EntityId, EntityKind};
use oxidraft_geometry::{Curve, CurveSegment, Point2d, intersect, project_point_onto_curve};

/// A kind of snap point that can be offered on geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    /// A curve's endpoint.
    Endpoint,
    /// A curve's midpoint.
    Midpoint,
    /// A circle/arc centre.
    Center,
    /// A circle/arc quadrant point (0/90/180/270°).
    Quadrant,
    /// An intersection between two curves.
    Intersection,
    /// The foot of a perpendicular from a reference point.
    Perpendicular,
    /// A tangent point on a circle/arc from a reference point.
    Tangent,
    /// The nearest point on a curve.
    Nearest,
    /// A standalone point entity.
    Node,
    /// A block insertion point.
    Insertion,
}

impl SnapKind {
    /// Tie-break priority when several snaps land under the cursor (lower wins).
    pub fn priority(self) -> u8 {
        match self {
            SnapKind::Endpoint => 0,
            SnapKind::Intersection | SnapKind::Node | SnapKind::Insertion => 1,
            SnapKind::Center | SnapKind::Midpoint | SnapKind::Quadrant => 2,
            SnapKind::Perpendicular | SnapKind::Tangent => 3,
            SnapKind::Nearest => 9,
        }
    }
}

/// All snap kinds paired with their display names, for the snap settings UI.
pub const SNAP_KINDS: [(SnapKind, &str); 10] = [
    (SnapKind::Endpoint, "Endpoint"),
    (SnapKind::Midpoint, "Midpoint"),
    (SnapKind::Center, "Center"),
    (SnapKind::Quadrant, "Quadrant"),
    (SnapKind::Intersection, "Intersection"),
    (SnapKind::Perpendicular, "Perpendicular"),
    (SnapKind::Tangent, "Tangent"),
    (SnapKind::Nearest, "Nearest"),
    (SnapKind::Node, "Node"),
    (SnapKind::Insertion, "Insertion"),
];

/// A candidate snap: its kind, world position, and the entity it belongs to.
#[derive(Clone, Debug)]
pub struct SnapPoint {
    /// Which kind of snap this is.
    pub kind: SnapKind,
    /// The snap position, in world coordinates.
    pub pos: (f64, f64),
    /// The entity the snap sits on.
    pub entity: EntityId,
}

/// Which snap kinds are active and how close the cursor must be to snap.
#[derive(Clone, Debug)]
pub struct SnapSettings {
    /// The snap kinds currently enabled.
    pub enabled: Vec<SnapKind>,
    /// Snap distance in world units.
    pub tolerance: f64,
}

impl Default for SnapSettings {
    fn default() -> Self {
        SnapSettings {
            enabled: vec![
                SnapKind::Endpoint,
                SnapKind::Midpoint,
                SnapKind::Center,
                SnapKind::Quadrant,
                SnapKind::Intersection,
                SnapKind::Perpendicular,
                SnapKind::Tangent,
                SnapKind::Node,
            ],
            tolerance: 0.5,
        }
    }
}

fn dist((ax, ay): (f64, f64), (bx, by): (f64, f64)) -> f64 {
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

/// All snap candidates near `cursor` given the enabled kinds in `settings`.
/// `reference` is the prior point, needed for perpendicular/tangent snaps.
pub fn find_snaps(
    doc: &Document,
    cursor: (f64, f64),
    settings: &SnapSettings,
    reference: Option<(f64, f64)>,
) -> Vec<SnapPoint> {
    find_snaps_excluding(doc, cursor, settings, reference, None)
}

/// Like [`find_snaps`], but skips entity `exclude` — used so a curve being
/// drawn doesn't snap to itself.
pub fn find_snaps_excluding(
    doc: &Document,
    cursor: (f64, f64),
    settings: &SnapSettings,
    reference: Option<(f64, f64)>,
    exclude: Option<EntityId>,
) -> Vec<SnapPoint> {
    let mut out = Vec::new();
    let tol = settings.tolerance;
    let on = |k: SnapKind| settings.enabled.contains(&k);

    let entities: Vec<_> = doc
        .editable_entities()
        .filter(|e| Some(e.id) != exclude)
        .collect();

    for e in &entities {
        match &e.kind {
            EntityKind::Curve(c) => {
                let pad = tol * 4.0;
                let (minx, miny, maxx, maxy) = fast_bbox_f64(c);
                let near_bbox = cursor.0 >= minx - pad
                    && cursor.0 <= maxx + pad
                    && cursor.1 >= miny - pad
                    && cursor.1 <= maxy + pad;
                let near_center = on(SnapKind::Center)
                    && center(c).map(|p| dist(p, cursor) <= tol).unwrap_or(false);
                if !near_bbox && !near_center {
                    continue;
                }

                if on(SnapKind::Endpoint) {
                    for p in endpoints(c) {
                        push_if_near(&mut out, SnapKind::Endpoint, p, e.id, cursor, tol);
                    }
                }
                if on(SnapKind::Midpoint) {
                    for p in midpoints(c) {
                        push_if_near(&mut out, SnapKind::Midpoint, p, e.id, cursor, tol);
                    }
                }
                if on(SnapKind::Center)
                    && let Some(p) = center(c)
                {
                    push_if_near(&mut out, SnapKind::Center, p, e.id, cursor, tol);
                }
                if on(SnapKind::Quadrant) {
                    for p in quadrants(c) {
                        push_if_near(&mut out, SnapKind::Quadrant, p, e.id, cursor, tol);
                    }
                }
                if on(SnapKind::Nearest) {
                    let pr = project_point_onto_curve(c, cursor.0, cursor.1);
                    push_if_near(&mut out, SnapKind::Nearest, pr.point, e.id, cursor, tol);
                }
                if on(SnapKind::Perpendicular)
                    && let Some(r) = reference
                    && let Some(p) = perpendicular_foot(c, r)
                {
                    let pr = project_point_onto_curve(c, cursor.0, cursor.1);
                    if dist(pr.point, cursor) <= tol && dist(p, cursor) <= tol * 4.0 {
                        out.push(SnapPoint {
                            kind: SnapKind::Perpendicular,
                            pos: p,
                            entity: e.id,
                        });
                    }
                }
                if on(SnapKind::Tangent)
                    && let Some(r) = reference
                {
                    let pr = project_point_onto_curve(c, cursor.0, cursor.1);
                    if dist(pr.point, cursor) <= tol {
                        for p in tangent_points(c, r) {
                            if dist(p, cursor) <= tol * 4.0 {
                                out.push(SnapPoint {
                                    kind: SnapKind::Tangent,
                                    pos: p,
                                    entity: e.id,
                                });
                            }
                        }
                    }
                }
            }
            EntityKind::Point(p) if on(SnapKind::Node) => {
                push_if_near(&mut out, SnapKind::Node, p.to_f64(), e.id, cursor, tol);
            }
            EntityKind::Insert { transform, .. } if on(SnapKind::Insertion) => {
                let base = transform.apply_point(&Point2d::new(0.0, 0.0));
                push_if_near(
                    &mut out,
                    SnapKind::Insertion,
                    base.to_f64(),
                    e.id,
                    cursor,
                    tol,
                );
            }
            _ => {}
        }
    }

    if on(SnapKind::Intersection) {
        let pad = tol * 5.0;
        let curves: Vec<_> = entities
            .iter()
            .filter_map(|e| {
                e.as_curve().and_then(|c| {
                    let (minx, miny, maxx, maxy) = fast_bbox_f64(c);
                    let near = cursor.0 >= minx - pad
                        && cursor.0 <= maxx + pad
                        && cursor.1 >= miny - pad
                        && cursor.1 <= maxy + pad;
                    if near { Some((e.id, c)) } else { None }
                })
            })
            .collect();
        for i in 0..curves.len() {
            for j in (i + 1)..curves.len() {
                for hit in intersect(curves[i].1, curves[j].1) {
                    push_if_near(
                        &mut out,
                        SnapKind::Intersection,
                        hit.point,
                        curves[i].0,
                        cursor,
                        tol,
                    );
                }
            }
        }
    }

    out.sort_by(|a, b| {
        a.kind.priority().cmp(&b.kind.priority()).then(
            dist(a.pos, cursor)
                .partial_cmp(&dist(b.pos, cursor))
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    out
}

/// The single best snap under `cursor` (highest priority, then nearest), or
/// `None` if nothing is within tolerance.
pub fn best_snap(
    doc: &Document,
    cursor: (f64, f64),
    settings: &SnapSettings,
    reference: Option<(f64, f64)>,
) -> Option<SnapPoint> {
    find_snaps(doc, cursor, settings, reference)
        .into_iter()
        .next()
}

fn push_if_near(
    out: &mut Vec<SnapPoint>,
    kind: SnapKind,
    pos: (f64, f64),
    entity: EntityId,
    cursor: (f64, f64),
    tol: f64,
) {
    if dist(pos, cursor) <= tol {
        out.push(SnapPoint { kind, pos, entity });
    }
}

fn endpoints(c: &Curve) -> Vec<(f64, f64)> {
    match c {
        Curve::Arc(a) => {
            let span = (a.end_angle - a.start_angle).abs();
            if (span - 2.0 * std::f64::consts::PI).abs() < 1e-9 {
                return vec![];
            }
            vec![c.evaluate_f64(a.start_angle), c.evaluate_f64(a.end_angle)]
        }
        Curve::Poly(p) => p.segments.iter().flat_map(endpoints).collect(),
        _ => {
            let (t0, t1) = c.domain();
            vec![c.evaluate_f64(t0), c.evaluate_f64(t1)]
        }
    }
}

fn fast_bbox_f64(c: &Curve) -> (f64, f64, f64, f64) {
    fn join(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
        (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
    }
    match c {
        Curve::Line(l) => {
            let (x0, y0) = l.p0.to_f64();
            let (x1, y1) = l.p1.to_f64();
            (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
        }
        Curve::Arc(a) => {
            let (cx, cy) = a.center.to_f64();
            let r = a.radius;
            (cx - r, cy - r, cx + r, cy + r)
        }
        Curve::Ellipse(e) => {
            let (cx, cy) = e.center.to_f64();
            let r = e.semi_major.max(e.semi_minor);
            (cx - r, cy - r, cx + r, cy + r)
        }
        Curve::Bezier(b) => {
            let pts = [b.p0.to_f64(), b.p1.to_f64(), b.p2.to_f64(), b.p3.to_f64()];
            pts.iter().fold(
                (
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ),
                |acc, &(x, y)| join(acc, (x, y, x, y)),
            )
        }
        Curve::Poly(p) => p.segments.iter().map(fast_bbox_f64).fold(
            (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            join,
        ),
        Curve::Rational(rb) => rb.points.iter().fold(
            (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            |acc, p| join(acc, (p.x, p.y, p.x, p.y)),
        ),
        Curve::Nurbs(nc) => nc.control.iter().fold(
            (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            |acc, p| join(acc, (p.x, p.y, p.x, p.y)),
        ),
    }
}

fn midpoints(c: &Curve) -> Vec<(f64, f64)> {
    match c {
        Curve::Poly(p) => p.segments.iter().filter_map(midpoint).collect(),
        _ => midpoint(c).into_iter().collect(),
    }
}

fn midpoint(c: &Curve) -> Option<(f64, f64)> {
    match c {
        Curve::Line(l) => {
            let (x0, y0) = l.p0.to_f64();
            let (x1, y1) = l.p1.to_f64();
            Some(((x0 + x1) / 2.0, (y0 + y1) / 2.0))
        }
        Curve::Arc(a) => {
            let span = (a.end_angle - a.start_angle).abs();
            if (span - 2.0 * std::f64::consts::PI).abs() < 1e-9 {
                return None;
            }
            let mid = (a.start_angle + a.end_angle) / 2.0;
            Some(c.evaluate_f64(mid))
        }
        _ => {
            let (t0, t1) = c.domain();
            Some(c.evaluate_f64((t0 + t1) / 2.0))
        }
    }
}

fn center(c: &Curve) -> Option<(f64, f64)> {
    match c {
        Curve::Arc(a) => Some(a.center.to_f64()),
        Curve::Ellipse(e) => Some(e.center.to_f64()),
        _ => None,
    }
}

fn quadrants(c: &Curve) -> Vec<(f64, f64)> {
    use std::f64::consts::{FRAC_PI_2, TAU};
    match c {
        Curve::Arc(a) => {
            let (cx, cy) = a.center.to_f64();
            let span = (a.end_angle - a.start_angle).abs();
            let full = span >= TAU - 1e-9;
            (0..4)
                .filter_map(|k| {
                    let ang = k as f64 * FRAC_PI_2;
                    // contains_angle wraps from the lower domain end, so
                    // reversed arcs (end < start, from reverse_curve/JOIN)
                    // keep their quadrant snaps; wrapping from start_angle
                    // admitted none of them.
                    let in_range = full || a.contains_angle(ang);
                    in_range.then(|| (cx + a.radius * ang.cos(), cy + a.radius * ang.sin()))
                })
                .collect()
        }
        Curve::Ellipse(e) => {
            let (cx, cy) = e.center.to_f64();
            let mut out = Vec::with_capacity(4);
            for (len, ang) in [
                (e.semi_major, e.rotation),
                (e.semi_minor, e.rotation + FRAC_PI_2),
            ] {
                for s in [1.0, -1.0] {
                    out.push((cx + s * len * ang.cos(), cy + s * len * ang.sin()));
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

fn perpendicular_foot(c: &Curve, reference: (f64, f64)) -> Option<(f64, f64)> {
    match c {
        Curve::Line(l) => {
            let (ax, ay) = l.p0.to_f64();
            let (bx, by) = l.p1.to_f64();
            let (dx, dy) = (bx - ax, by - ay);
            let len_sq = dx * dx + dy * dy;
            if len_sq < 1e-20 {
                return None;
            }
            let t = ((reference.0 - ax) * dx + (reference.1 - ay) * dy) / len_sq;
            if (-1e-9..=1.0 + 1e-9).contains(&t) {
                let t_clamped = t.clamp(0.0, 1.0);
                Some((ax + t_clamped * dx, ay + t_clamped * dy))
            } else {
                None
            }
        }
        Curve::Arc(a) => {
            let (cx, cy) = a.center.to_f64();
            let r = a.radius;
            let (dx, dy) = (reference.0 - cx, reference.1 - cy);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-12 {
                return None;
            }
            let angle = dy.atan2(dx);
            if a.contains_angle(angle) {
                Some((cx + r * dx / len, cy + r * dy / len))
            } else {
                None
            }
        }
        _ => {
            let pr = project_point_onto_curve(c, reference.0, reference.1);
            Some(pr.point)
        }
    }
}

fn tangent_points(c: &Curve, reference: (f64, f64)) -> Vec<(f64, f64)> {
    match c {
        Curve::Arc(a) => {
            let (cx, cy) = a.center.to_f64();
            let r = a.radius;
            let (dx, dy) = (reference.0 - cx, reference.1 - cy);
            let d = (dx * dx + dy * dy).sqrt();
            if d <= r + 1e-12 {
                return vec![];
            }
            let base = dy.atan2(dx);
            let off = (r / d).acos();
            let mut result = Vec::new();
            for angle in [base + off, base - off] {
                if a.contains_angle(angle) {
                    result.push((cx + r * angle.cos(), cy + r * angle.sin()));
                }
            }
            result
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_document::EntityKind;
    use oxidraft_geometry::{CircularArc, CubicBezier, LineSeg};

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    fn doc_with_line() -> (Document, EntityId) {
        let mut doc = Document::new();
        let id = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(10, 0),
        ))));
        (doc, id)
    }

    #[test]
    fn reversed_arc_keeps_its_quadrant_snaps() {
        // A half arc traversed backwards (π → 0), as reverse_curve/JOIN
        // store. The top quadrant (0, 5) is interior to the span and must
        // still be offered; wrapping from start_angle admitted none.
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
            pt(0, 0),
            5.0,
            std::f64::consts::PI,
            0.0,
        ))));
        let s = SnapSettings {
            enabled: vec![SnapKind::Quadrant],
            ..SnapSettings::default()
        };
        let hits = find_snaps(&doc, (0.2, 4.8), &s, None);
        assert!(
            hits.iter().any(|h| h.kind == SnapKind::Quadrant
                && (h.pos.0).abs() < 1e-9
                && (h.pos.1 - 5.0).abs() < 1e-9),
            "top quadrant of the reversed half arc must snap: {hits:?}"
        );
    }

    #[test]
    fn snap_scan_stays_fast_after_many_trims() {
        use std::time::Instant;
        let mut doc = Document::new();
        for i in 0..150 {
            let x = 0.123456789012 + i as f64 * 0.37;
            let y = 0.987654321098 + i as f64 * 0.11;
            doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(x, y),
                Point2d::from_f64(x + 1.234567890123, y + 0.55),
            ))));
        }
        for i in 0..20 {
            let x = i as f64 * 2.345678901234;
            doc.add(EntityKind::Curve(Curve::Bezier(CubicBezier::new(
                Point2d::from_f64(x, 0.1),
                Point2d::from_f64(x + 0.7, 2.3),
                Point2d::from_f64(x + 1.3, -1.7),
                Point2d::from_f64(x + 2.1, 0.4),
            ))));
        }
        let s = SnapSettings::default();
        let start = Instant::now();
        for k in 0..50 {
            let cx = 10.0 + (k as f64) * 0.05;
            let _ = find_snaps(&doc, (cx, 5.0), &s, Some((0.0, 0.0)));
        }
        assert!(
            start.elapsed().as_millis() < 300,
            "snap scan too slow for interactive use: {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn intersection_snap_over_two_beziers_is_fast() {
        use std::time::Instant;
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Bezier(CubicBezier::new(
            pt(0, 0),
            pt(3, 10),
            pt(7, 10),
            pt(10, 0),
        ))));
        doc.add(EntityKind::Curve(Curve::Bezier(CubicBezier::new(
            pt(0, 8),
            pt(3, -2),
            pt(7, -2),
            pt(10, 8),
        ))));
        let s = SnapSettings {
            enabled: vec![
                SnapKind::Intersection,
                SnapKind::Nearest,
                SnapKind::Endpoint,
                SnapKind::Midpoint,
            ],
            tolerance: 1.0,
        };
        let start = Instant::now();
        for _ in 0..50 {
            let _ = find_snaps(&doc, (5.0, 5.2), &s, None);
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 500,
            "intersection snapping over two Béziers too slow: {elapsed:?} for 50 frames"
        );
    }

    #[test]
    fn snap_endpoint() {
        let (doc, _) = doc_with_line();
        let s = SnapSettings {
            enabled: vec![SnapKind::Endpoint],
            tolerance: 0.5,
        };
        let snaps = find_snaps(&doc, (0.1, 0.1), &s, None);
        assert!(
            snaps
                .iter()
                .any(|sp| sp.kind == SnapKind::Endpoint && dist(sp.pos, (0.0, 0.0)) < 1e-9)
        );
    }

    #[test]
    fn snap_midpoint_exact() {
        let (doc, _) = doc_with_line();
        let s = SnapSettings {
            enabled: vec![SnapKind::Midpoint],
            tolerance: 0.5,
        };
        let snaps = find_snaps(&doc, (5.1, 0.1), &s, None);
        let mid = snaps
            .iter()
            .find(|sp| sp.kind == SnapKind::Midpoint)
            .unwrap();
        assert!((mid.pos.0 - 5.0).abs() < 1e-9 && mid.pos.1.abs() < 1e-9);
    }

    fn doc_with_square_poly() -> Document {
        use oxidraft_geometry::PolyCurve;
        let mut doc = Document::new();
        let segs = vec![
            Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 0))),
            Curve::Line(LineSeg::from_endpoints(pt(4, 0), pt(4, 4))),
            Curve::Line(LineSeg::from_endpoints(pt(4, 4), pt(0, 4))),
            Curve::Line(LineSeg::from_endpoints(pt(0, 4), pt(0, 0))),
        ];
        doc.add(EntityKind::Curve(Curve::Poly(Box::new(PolyCurve::new(
            segs,
        )))));
        doc
    }

    #[test]
    fn snap_polycurve_interior_vertex_endpoint() {
        let doc = doc_with_square_poly();
        let s = SnapSettings {
            enabled: vec![SnapKind::Endpoint],
            tolerance: 0.5,
        };
        let snaps = find_snaps(&doc, (4.1, 3.9), &s, None);
        assert!(
            snaps
                .iter()
                .any(|sp| sp.kind == SnapKind::Endpoint && dist(sp.pos, (4.0, 4.0)) < 1e-9),
            "must snap to the (4,4) vertex"
        );
    }

    #[test]
    fn snap_polycurve_segment_midpoint() {
        let doc = doc_with_square_poly();
        let s = SnapSettings {
            enabled: vec![SnapKind::Midpoint],
            tolerance: 0.5,
        };
        let snaps = find_snaps(&doc, (2.05, 0.05), &s, None);
        assert!(
            snaps
                .iter()
                .any(|sp| sp.kind == SnapKind::Midpoint && dist(sp.pos, (2.0, 0.0)) < 1e-9),
            "must snap to the bottom-edge midpoint"
        );
    }

    #[test]
    fn snap_center_of_circle() {
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
            pt(3, 4),
            5.0,
            0.0,
            2.0 * std::f64::consts::PI,
        ))));
        let s = SnapSettings {
            enabled: vec![SnapKind::Center],
            tolerance: 0.5,
        };
        let snaps = find_snaps(&doc, (3.2, 4.1), &s, None);
        let c = snaps.iter().find(|sp| sp.kind == SnapKind::Center).unwrap();
        assert!((c.pos.0 - 3.0).abs() < 1e-9 && (c.pos.1 - 4.0).abs() < 1e-9);
    }

    #[test]
    fn snap_intersection_of_two_lines() {
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(10, 10),
        ))));
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 10),
            pt(10, 0),
        ))));
        let s = SnapSettings {
            enabled: vec![SnapKind::Intersection],
            tolerance: 0.5,
        };
        let snaps = find_snaps(&doc, (5.2, 4.9), &s, None);
        let x = snaps
            .iter()
            .find(|sp| sp.kind == SnapKind::Intersection)
            .unwrap();
        assert!((x.pos.0 - 5.0).abs() < 1e-6 && (x.pos.1 - 5.0).abs() < 1e-6);
    }

    #[test]
    fn snap_perpendicular_to_line() {
        let (doc, _) = doc_with_line();
        let s = SnapSettings {
            enabled: vec![SnapKind::Perpendicular],
            tolerance: 1.0,
        };
        let snaps = find_snaps(&doc, (3.1, 0.1), &s, Some((3.0, 5.0)));
        let p = snaps
            .iter()
            .find(|sp| sp.kind == SnapKind::Perpendicular)
            .unwrap();
        assert!((p.pos.0 - 3.0).abs() < 1e-9 && p.pos.1.abs() < 1e-9);
    }

    #[test]
    fn snap_tangent_to_circle() {
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
            pt(0, 0),
            1.0,
            0.0,
            2.0 * std::f64::consts::PI,
        ))));
        let s = SnapSettings {
            enabled: vec![SnapKind::Tangent],
            tolerance: 5.0,
        };
        let snaps = find_snaps(&doc, (0.5, 0.9), &s, Some((2.0, 0.0)));
        assert!(snaps.iter().any(|sp| sp.kind == SnapKind::Tangent
            && (sp.pos.0 - 0.5).abs() < 1e-6
            && (sp.pos.1.abs() - (3f64.sqrt() / 2.0)).abs() < 1e-6));
    }

    #[test]
    fn snap_nearest_on_line() {
        let (doc, _) = doc_with_line();
        let s = SnapSettings {
            enabled: vec![SnapKind::Nearest],
            tolerance: 1.0,
        };
        let snaps = find_snaps(&doc, (7.0, 0.3), &s, None);
        let n = snaps
            .iter()
            .find(|sp| sp.kind == SnapKind::Nearest)
            .unwrap();
        assert!((n.pos.0 - 7.0).abs() < 1e-6 && n.pos.1.abs() < 1e-6);
    }
}

//! Hostile-input battery for the interactive tool layer: drawing commands,
//! transforms, grips, trim/extend/fillet/chamfer, selection, snapping,
//! hatching, inquiry, and constraints. Companion to the geometry crate's
//! `hostile_ops.rs`: every tool must decline degenerate input — never
//! panic, hang, balloon the document, or write non-finite geometry into it.

use oxidraft_cad::{constrain, draw, edit, grips, hatch, infer, inquiry, selection, snap};
use oxidraft_document::{ConstraintKind, Document, EntityId, EntityKind, HatchPattern};
use oxidraft_geometry::{
    BoundingBox, CircularArc, Continuity, CubicBezier, Curve, LineSeg, Point2d,
};

fn p(x: f64, y: f64) -> Point2d {
    Point2d::from_f64(x, y)
}

fn line_curve(x0: f64, y0: f64, x1: f64, y1: f64) -> Curve {
    Curve::Line(LineSeg::from_endpoints(p(x0, y0), p(x1, y1)))
}

fn kind_is_finite(kind: &EntityKind) -> bool {
    match kind {
        EntityKind::Curve(c) => c.is_finite(),
        EntityKind::Point(q) => q.is_finite(),
        _ => true,
    }
}

fn assert_doc_finite(doc: &Document, label: &str) {
    for e in doc.editable_entities() {
        assert!(
            kind_is_finite(&e.kind),
            "{label}: non-finite entity escaped into the document: {:?}",
            e.kind
        );
    }
}

/// Finite but degenerate entities every tool must survive.
fn menagerie_doc() -> (Document, Vec<EntityId>) {
    let mut doc = Document::new();
    let ids = vec![
        draw::line(&mut doc, p(0.0, 0.0), p(4.0, 3.0)),
        draw::line(&mut doc, p(1.0, 1.0), p(1.0, 1.0)), // zero length
        draw::circle(&mut doc, p(10.0, 0.0), 2.0),
        draw::circle(&mut doc, p(20.0, 0.0), 1e-30),
        draw::arc(&mut doc, p(30.0, 0.0), 2.0, 1.0, 1.0), // zero sweep
        draw::bezier(&mut doc, p(2.0, 2.0), p(2.0, 2.0), p(2.0, 2.0), p(2.0, 2.0)),
        draw::bezier(&mut doc, p(0.0, 5.0), p(1.0, 9.0), p(3.0, 1.0), p(4.0, 5.0)),
        draw::point(&mut doc, p(-3.0, -3.0)),
        draw::ellipse(&mut doc, p(40.0, 0.0), 3.0, 1.0, 0.5),
        draw::ellipse(&mut doc, p(50.0, 0.0), 0.0, 0.0, 0.0), // zero axes
        draw::polycurve(
            &mut doc,
            vec![
                line_curve(60.0, 0.0, 61.0, 0.0),
                line_curve(61.0, 0.0, 61.0, 0.0), // zero segment
                line_curve(61.0, 0.0, 61.0, 1.0),
            ],
        ),
    ];
    (doc, ids)
}

/// A corrupt document: non-finite entities alongside one healthy line.
/// (Loaders drop these on read, but a plugin or an older file version can
/// still hand the tool layer a poisoned document.)
fn poisoned_doc() -> (Document, EntityId) {
    let mut doc = Document::new();
    doc.add(EntityKind::Curve(line_curve(f64::NAN, 0.0, 1.0, 1.0)));
    doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
        p(0.0, 0.0),
        f64::INFINITY,
        0.0,
        1.0,
    ))));
    doc.add(EntityKind::Curve(Curve::Bezier(CubicBezier::new(
        p(0.0, 0.0),
        p(f64::NAN, f64::NAN),
        p(1.0, 1.0),
        p(2.0, 0.0),
    ))));
    doc.add(EntityKind::Point(p(f64::NAN, f64::NAN)));
    let good = draw::line(&mut doc, p(100.0, 100.0), p(110.0, 100.0));
    (doc, good)
}

fn bogus_id() -> EntityId {
    EntityId(u64::MAX)
}

#[test]
fn draw_tools_decline_hostile_counts_and_coordinates() {
    let mut doc = Document::new();
    // Side counts below 3 used to assert; huge counts would balloon.
    for n in [0, 1, 2, 5_000, u32::MAX] {
        assert!(
            draw::polygon(&mut doc, &p(0.0, 0.0), n, 5.0, true, 0.0).is_empty(),
            "polygon must decline n={n}"
        );
    }
    for bad in [f64::NAN, f64::INFINITY] {
        assert!(draw::polygon(&mut doc, &p(0.0, 0.0), 6, bad, true, 0.0).is_empty());
        assert!(draw::polygon(&mut doc, &p(bad, 0.0), 6, 5.0, true, 0.0).is_empty());
        assert!(draw::polygon(&mut doc, &p(0.0, 0.0), 6, 5.0, true, bad).is_empty());
    }
    assert_eq!(doc.len(), 0, "declined polygons must not add entities");
    assert_eq!(
        draw::polygon(&mut doc, &p(0.0, 0.0), 6, 5.0, true, 0.0).len(),
        6
    );

    let c = line_curve(0.0, 0.0, 10.0, 0.0);
    assert!(draw::divide(&mut doc, &c, u32::MAX).is_empty());
    assert!(draw::divide(&mut doc, &c, 0).is_empty());
    assert_eq!(draw::divide(&mut doc, &c, 5).len(), 4);
    // Dividing a poisoned curve must not store NaN points.
    let bad = line_curve(f64::NAN, 0.0, 10.0, 0.0);
    draw::divide(&mut doc, &bad, 5);
    assert_doc_finite(&doc, "divide over poisoned curve");

    assert!(
        draw::circle_3p(&mut doc, &p(0.0, 0.0), &p(1.0, 0.0), &p(2.0, 0.0)).is_none(),
        "collinear points have no circle"
    );
}

#[test]
fn transforms_cannot_poison_the_document() {
    let (mut doc, ids) = menagerie_doc();
    let before = format!("{:?}", doc.get(ids[0]).unwrap().kind);
    let count = doc.len();

    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        edit::move_by(&mut doc, &ids, bad, 0.0);
        edit::move_by(&mut doc, &ids, 0.0, bad);
        edit::rotate(&mut doc, &ids, &p(0.0, 0.0), bad);
        edit::scale(&mut doc, &ids, &p(0.0, 0.0), bad);
        edit::stretch(&mut doc, &ids, (0.0, 0.0, 5.0, 5.0), bad, bad);
        assert!(edit::copy_by(&mut doc, &ids, bad, bad).is_empty());
        assert!(edit::offset(&mut doc, &ids, bad).is_empty());
    }
    // A zero-length mirror axis has no reflection.
    edit::mirror(&mut doc, &ids, &p(1.0, 1.0), &p(1.0, 1.0), false);
    assert!(edit::mirror(&mut doc, &ids, &p(1.0, 1.0), &p(1.0, 1.0), true).is_empty());
    // Rotating about a non-finite center is equally undefined.
    edit::rotate(&mut doc, &ids, &p(f64::NAN, 0.0), 1.0);

    assert_eq!(doc.len(), count, "declined edits must not add entities");
    assert_eq!(
        format!("{:?}", doc.get(ids[0]).unwrap().kind),
        before,
        "declined edits must leave geometry untouched"
    );
    assert_doc_finite(&doc, "hostile transforms");
}

#[test]
fn arrays_cannot_balloon_memory() {
    let (mut doc, ids) = menagerie_doc();
    let count = doc.len();
    // Would be ~1.8e19 duplications; must decline instantly.
    assert!(edit::array_rect(&mut doc, &ids, u32::MAX, u32::MAX, 1.0, 1.0).is_empty());
    assert!(edit::array_rect(&mut doc, &ids, 1, u32::MAX, 1.0, 1.0).is_empty());
    assert!(edit::array_polar(&mut doc, &ids, &p(0.0, 0.0), u32::MAX, 6.28).is_empty());
    assert_eq!(doc.len(), count);
    // NaN spacing/angle must not clone poisoned copies.
    edit::array_rect(&mut doc, &ids, 2, 2, f64::NAN, 1.0);
    edit::array_polar(&mut doc, &ids, &p(0.0, 0.0), 4, f64::NAN);
    assert_doc_finite(&doc, "NaN array parameters");
    // Sane arrays still work.
    assert_eq!(
        edit::array_rect(&mut doc, &[ids[0]], 2, 3, 5.0, 5.0).len(),
        5
    );
    assert_eq!(
        edit::array_polar(&mut doc, &[ids[0]], &p(0.0, 0.0), 8, std::f64::consts::TAU).len(),
        7
    );
}

#[test]
fn grips_survive_hostile_cursors_and_values() {
    let (doc, _) = menagerie_doc();
    for e in doc.editable_entities() {
        for grip in grips::grips_for(&e.kind) {
            let before = format!("{:?}", e.kind);
            let unchanged = grips::apply_grip(&e.kind, &grip, p(f64::NAN, f64::NAN));
            assert_eq!(
                format!("{unchanged:?}"),
                before,
                "NaN drag must leave the entity untouched"
            );
            let far = grips::apply_grip(&e.kind, &grip, p(1e300, -1e300));
            assert!(kind_is_finite(&far), "huge drags must stay finite");
            for v in [f64::NAN, f64::INFINITY] {
                let out = grips::apply_grip_value(&e.kind, &grip, v, p(1.0, 1.0));
                assert!(kind_is_finite(&out));
            }
            let out = grips::apply_grip_value(&e.kind, &grip, 2.0, p(f64::NAN, 0.0));
            assert!(kind_is_finite(&out));
        }
    }
}

#[test]
fn trim_extend_and_break_survive_hostile_picks() {
    for (px, py) in [(f64::NAN, f64::NAN), (1e300, -1e300), (2.0, 0.5)] {
        let mut doc = Document::new();
        let target = draw::line(&mut doc, p(0.0, 0.0), p(4.0, 0.0));
        let cutter = draw::line(&mut doc, p(2.0, -1.0), p(2.0, 1.0));
        edit::trim(&mut doc, target, &[cutter], px, py);
        assert_doc_finite(&doc, "trim");

        let mut doc = Document::new();
        let target = draw::line(&mut doc, p(0.0, 0.0), p(4.0, 0.0));
        let boundary = draw::line(&mut doc, p(6.0, -1.0), p(6.0, 1.0));
        let _ = edit::extend(&mut doc, target, &[boundary], px, py);
        let _ = edit::extend_preview(&doc, target, &[boundary], px, py);
        assert_doc_finite(&doc, "extend");
    }

    // Self-referential and dangling operands.
    let (mut doc, ids) = menagerie_doc();
    edit::trim(&mut doc, ids[0], &ids, 2.0, 1.5);
    edit::trim(&mut doc, bogus_id(), &[ids[2]], 0.0, 0.0);
    let _ = edit::extend(&mut doc, bogus_id(), &[bogus_id()], 0.0, 0.0);

    for t in [f64::NAN, f64::INFINITY, -1.0, 2.0, 0.5] {
        let mut doc = Document::new();
        let id = draw::line(&mut doc, p(0.0, 0.0), p(4.0, 0.0));
        let pieces = edit::break_at(&mut doc, id, t);
        assert!(!pieces.is_empty(), "break_at must return the survivors");
        assert_doc_finite(&doc, "break_at");
    }
}

#[test]
fn corner_tools_decline_degenerate_parameters() {
    let mut doc = Document::new();
    let a = draw::line(&mut doc, p(0.0, 0.0), p(4.0, 0.0));
    let b = draw::line(&mut doc, p(4.0, 0.0), p(4.0, 4.0));
    let z = draw::line(&mut doc, p(9.0, 9.0), p(9.0, 9.0));

    for r in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        assert!(edit::fillet(&mut doc, a, b, r, 3.5, 0.5).is_none());
    }
    assert!(edit::fillet(&mut doc, a, a, 1.0, 3.5, 0.5).is_none());
    assert!(edit::fillet(&mut doc, a, bogus_id(), 1.0, 3.5, 0.5).is_none());
    let _ = edit::fillet(&mut doc, a, z, 1.0, 3.5, 0.5);

    for d in [f64::NAN, f64::INFINITY] {
        assert!(edit::chamfer(&mut doc, a, b, d, 1.0).is_none());
        assert!(edit::chamfer(&mut doc, a, b, 1.0, d).is_none());
    }
    assert!(edit::chamfer(&mut doc, a, a, 1.0, 1.0).is_none());

    // The freeform (spline/ellipse) corner path must survive degenerate
    // operands and hostile parameters just like the exact one.
    let (mut doc, ids) = menagerie_doc();
    let normal_line = ids[0];
    let point_bez = ids[5];
    let wavy_bez = ids[6];
    let ellipse = ids[8];
    for r in [f64::NAN, f64::INFINITY, -1.0, 0.0, 1e-30, 0.5] {
        let _ = edit::fillet(&mut doc, normal_line, wavy_bez, r, 2.0, 5.0);
        let _ = edit::fillet(&mut doc, point_bez, wavy_bez, r, 2.0, 2.0);
        let _ = edit::fillet(&mut doc, normal_line, ellipse, r, 40.0, 0.0);
    }
    for d in [f64::NAN, -1.0, 0.0, 0.5, 1e15] {
        let _ = edit::chamfer(&mut doc, normal_line, wavy_bez, d, 0.5);
        let _ = edit::chamfer(&mut doc, wavy_bez, ellipse, 0.5, d);
    }
    assert_doc_finite(&doc, "freeform corner tools");

    for cont in [Continuity::G0, Continuity::G1, Continuity::G2] {
        assert!(edit::blend(&mut doc, a, b, cont, f64::NAN).is_none());
        let _ = edit::blend(&mut doc, z, z, cont, 1.0);
        let _ = edit::blend_preview(&doc, a, z, cont, 1e15);
    }
    assert_doc_finite(&doc, "corner tools");

    // Joining degenerate fragments must not panic.
    let (mut doc, ids) = menagerie_doc();
    let _ = edit::join(&mut doc, &ids);
    assert_doc_finite(&doc, "join");
}

#[test]
fn selection_survives_poisoned_documents() {
    let (doc, good) = poisoned_doc();
    // Hostile queries over a corrupt document: no panic, no hang.
    for (x, y, tol) in [
        (f64::NAN, f64::NAN, f64::NAN),
        (0.5, 0.5, f64::INFINITY),
        (1e300, -1e300, 1.0),
    ] {
        let _ = selection::pick_at(&doc, x, y, tol);
    }
    // Corrupt neighbours must not hide a healthy entity from a good pick.
    assert_eq!(selection::pick_at(&doc, 105.0, 100.0, 0.5), Some(good));

    let nan_rect = BoundingBox::from_corners(f64::NAN, f64::NAN, f64::NAN, f64::NAN);
    let _ = selection::select_window(&doc, &nan_rect);
    let _ = selection::select_crossing(&doc, &nan_rect);
    let inverted = BoundingBox::from_corners(10.0, 10.0, -10.0, -10.0);
    let _ = selection::select_window(&doc, &inverted);
    let _ = selection::select_fence(&doc, &[p(f64::NAN, 0.0), p(1.0, 1.0)]);
    let _ = selection::select_fence(&doc, &[p(0.0, 0.0)]);

    let wide = BoundingBox::from_corners(90.0, 90.0, 120.0, 110.0);
    assert!(selection::select_window(&doc, &wide).contains(&good));
}

#[test]
fn snapping_survives_poisoned_documents() {
    let (doc, good) = poisoned_doc();
    let settings = snap::SnapSettings::default();
    for cursor in [(f64::NAN, f64::NAN), (1e300, 1e300), (0.0, 0.0)] {
        let _ = snap::find_snaps(&doc, cursor, &settings, Some((f64::NAN, 0.0)));
        let _ = snap::best_snap(&doc, cursor, &settings, None);
    }
    let mut hostile = settings.clone();
    hostile.tolerance = f64::NAN;
    let _ = snap::find_snaps(&doc, (0.0, 0.0), &hostile, None);

    // The healthy line's endpoint still snaps despite corrupt neighbours.
    let hit = snap::best_snap(&doc, (100.1, 100.1), &settings, None)
        .expect("endpoint snap on the healthy line");
    assert_eq!(hit.entity, good);
    assert!(hit.pos.0.is_finite() && hit.pos.1.is_finite());

    for (px, py) in [(f64::NAN, 0.0), (0.0, 0.0), (1e300, 0.0)] {
        let _ = infer::infer_axis((0.0, 0.0), (px, py), (1.0, 1.0), 0.5);
        let _ = infer::infer_axis((0.0, 0.0), (1.0, 0.0), (px, py), f64::NAN);
    }
}

#[test]
fn hatch_survives_hostile_regions() {
    let (doc, _) = poisoned_doc();
    for (x, y) in [(f64::NAN, f64::NAN), (0.5, 0.5), (1e300, 0.0)] {
        let _ = hatch::trace_pick_region(&doc, x, y);
    }

    // A region can still be traced next to corrupt entities.
    let (mut doc, _) = poisoned_doc();
    draw::rectangle(&mut doc, &p(0.0, 0.0), &p(4.0, 4.0));
    let (boundary, holes) =
        hatch::trace_pick_region(&doc, 2.0, 2.0).expect("rectangle encloses the click");

    let _ = hatch::region_contains(&boundary, &holes, f64::NAN, f64::NAN);
    let _ = hatch::triangulate_with_tol(&boundary, &holes, f64::NAN);
    let _ = hatch::outline_loops(&boundary, &holes, f64::NAN);
    for pat in [
        HatchPattern::Lines {
            angle_deg: f64::NAN,
            spacing: 1.0,
        },
        HatchPattern::Lines {
            angle_deg: 0.0,
            spacing: f64::NAN,
        },
        HatchPattern::Cross {
            angle_deg: 45.0,
            spacing: -1.0,
        },
        HatchPattern::Dots { spacing: 0.0 },
    ] {
        let _ = hatch::pattern_lines(&boundary, &holes, pat.clone());
        let _ = hatch::pattern_dots(&boundary, &holes, pat);
    }

    // Degenerate boundaries: zero-length loop, empty loop, poisoned loop.
    let zero_loop = vec![line_curve(1.0, 1.0, 1.0, 1.0)];
    let nan_loop = vec![line_curve(f64::NAN, 0.0, 1.0, 1.0)];
    for b in [&zero_loop, &nan_loop, &Vec::new()] {
        assert!(hatch::triangulate(b, &[]).is_empty());
        let _ = hatch::pattern_lines(
            b,
            &[],
            HatchPattern::Lines {
                angle_deg: 45.0,
                spacing: 1.0,
            },
        );
        let _ = hatch::outline_loops(b, &[], 1e-3);
    }
    let _ = hatch::triangulate(&boundary, &[zero_loop, nan_loop, Vec::new()]);

    // A fill whose stroke count would be absurd must decline outright —
    // not spin for minutes emitting a partial pattern.
    let huge = vec![line_curve(-1e15, -1e15, 1e15, -1e15)];
    let lines = hatch::pattern_lines(
        &huge,
        &[],
        HatchPattern::Lines {
            angle_deg: 0.0,
            spacing: 1e-9 * 2.0,
        },
    );
    assert!(lines.is_empty(), "absurd fills must decline");
    let dots = hatch::pattern_dots(
        &huge,
        &[],
        HatchPattern::Dots {
            spacing: 1e-9 * 2.0,
        },
    );
    assert!(dots.is_empty(), "absurd fills must decline");
    // ... while an ordinary fill still produces strokes.
    let sane = hatch::pattern_lines(
        &boundary,
        &holes,
        HatchPattern::Lines {
            angle_deg: 45.0,
            spacing: 0.5,
        },
    );
    assert!(!sane.is_empty(), "sane fills must still hatch");
    assert!(
        !hatch::pattern_dots(&boundary, &holes, HatchPattern::Dots { spacing: 0.5 }).is_empty()
    );
}

#[test]
fn constraints_survive_degenerate_selections() {
    let (mut doc, ids) = menagerie_doc();
    let zero_a = ids[1];
    let normal = ids[0];
    for kind in [
        ConstraintKind::Parallel,
        ConstraintKind::Perpendicular,
        ConstraintKind::EqualLength,
        ConstraintKind::Coincident,
        ConstraintKind::Horizontal,
        ConstraintKind::Vertical,
        ConstraintKind::Tangent,
    ] {
        let _ = constrain::constrain_lines(&mut doc, &[normal, zero_a], kind);
        let _ = constrain::constrain_lines(&mut doc, &[zero_a, zero_a], kind);
        let _ = constrain::constrain_lines(&mut doc, &[bogus_id()], kind);
        let _ = constrain::constrain_lines(&mut doc, &[], kind);
    }
    for v in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        assert!(constrain::constrain_radius(&mut doc, &ids, Some(v)).is_err());
        assert!(constrain::constrain_distance(&mut doc, &ids, Some(v)).is_err());
    }
    // Angle: NaN/inf decline; degenerate legs and bogus ids must not panic.
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(constrain::constrain_angle(&mut doc, &[normal, zero_a], Some(v)).is_err());
    }
    let _ = constrain::constrain_angle(&mut doc, &[normal, zero_a], Some(45.0));
    let _ = constrain::constrain_angle(&mut doc, &[normal, zero_a], None);
    let _ = constrain::constrain_angle(&mut doc, &[zero_a, zero_a], Some(45.0));
    let _ = constrain::constrain_angle(&mut doc, &[bogus_id(), normal], None);
    assert_doc_finite(&doc, "angle constraints over degenerate entities");
    // Tangent between a line and a degenerate circle.
    let tiny = ids[3];
    let _ = constrain::constrain_lines(&mut doc, &[normal, tiny], ConstraintKind::Tangent);
    assert_doc_finite(&doc, "constraints over degenerate entities");

    let _ = constrain::dof_report(&doc, &ids);
    let _ = constrain::diagnose_conflict(&doc, &ids);
    let _ = constrain::resolve_after_transform(&mut doc, &ids);
    assert_doc_finite(&doc, "constraint resolution");
}

#[test]
fn inquiry_survives_poisoned_documents() {
    let (doc, good) = poisoned_doc();
    let all: Vec<EntityId> = doc.editable_entities().map(|e| e.id).collect();
    for &a in &all {
        for &b in &all {
            let _ = inquiry::distance_entities(&doc, a, b);
        }
        let _ = inquiry::list_entity(&doc, a);
    }
    let _ = inquiry::distance_entities(&doc, good, bogus_id());
    let _ = inquiry::area_of_loop(&doc, &all);
    let _ = inquiry::total_length(&doc, &all);
    let (dsq, d) = inquiry::distance_points(&p(f64::NAN, 0.0), &p(1.0, 1.0));
    assert!(dsq.is_nan() && d.is_nan(), "NaN in, NaN out — but no panic");
}

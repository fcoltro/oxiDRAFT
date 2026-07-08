//! Corrupt and hostile input must degrade to dropped records, never to a
//! panic, a hang, or NaN leaking into the document. Each case here was a
//! real failure: arcs panicked the loader through the trusted constructor,
//! infinite angles hung the τ-stepping normalization loops, and dropped
//! records shifted constraint ordinals onto the wrong entities.

use oxidraft_document::{ConstraintKind, Document, EntityKind, SketchConstraint};
use oxidraft_geometry::{Curve, LineSeg, Point2d};
use oxidraft_io::{from_o2d, import_dxf, import_svg, to_o2d};

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> EntityKind {
    EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
        Point2d::from_f64(x0, y0),
        Point2d::from_f64(x1, y1),
    )))
}

#[test]
fn corrupt_arc_radius_drops_record_instead_of_panicking() {
    for bad in ["abc", "-5", "0", "nan", "inf"] {
        let text = format!(
            "O2D 1\n\
             E LINE 0 bylayer 0;0 10;0 ByLayer bylayer\n\
             E ARC 0 bylayer 5;5 {bad} 0 6.28 ByLayer bylayer\n\
             E LINE 0 bylayer 0;5 10;5 ByLayer bylayer\n"
        );
        let doc = from_o2d(&text).expect("salvageable file must load");
        assert_eq!(
            doc.iter().count(),
            2,
            "radius {bad:?}: both lines survive, the arc is dropped"
        );
    }
}

#[test]
fn non_finite_values_do_not_hang_or_poison_the_document() {
    // `inf` as an arc angle formerly hung bounding_box's angle loop; NaN
    // coordinates loaded silently and poisoned zoom-to-fit for the session.
    let text = "O2D 1\n\
                E ARC 0 bylayer 0;0 1 inf 0 ByLayer bylayer\n\
                E LINE 0 bylayer nan;0 10;0 ByLayer bylayer\n\
                E POINT 0 bylayer inf;inf ByLayer bylayer\n\
                E LINE 0 bylayer 0;1 2;3 ByLayer bylayer\n";
    let doc = from_o2d(text).expect("salvageable file must load");
    assert_eq!(doc.iter().count(), 1, "only the finite line survives");
    let b = doc
        .iter()
        .next()
        .and_then(|e| e.bounding_box())
        .expect("line has a bbox");
    assert!(b.is_finite());
}

#[test]
fn constraints_keep_their_entities_when_a_corrupt_record_is_dropped() {
    // The dropped arc sits between the two constrained lines; its file
    // ordinal must still count, or PAR would attach to the wrong pair.
    let text = "O2D 1\n\
                E LINE 0 bylayer 0;0 10;0 ByLayer bylayer\n\
                E ARC 0 bylayer 5;5 -1 0 6.28 ByLayer bylayer\n\
                E LINE 0 bylayer 0;5 10;5 ByLayer bylayer\n\
                C PAR 0 2\n\
                C PERP 0 1\n";
    let doc = from_o2d(text).expect("salvageable file must load");
    let ids: Vec<_> = doc.iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(
        doc.constraints.len(),
        1,
        "the constraint on the dropped arc is discarded"
    );
    let c = &doc.constraints[0];
    assert_eq!(c.kind, ConstraintKind::Parallel);
    assert_eq!((c.a, c.b), (ids[0], Some(ids[1])));
}

#[test]
fn unserialized_kinds_do_not_shift_constraint_ordinals_on_save() {
    // XLine has no record in the format; an entity the writer skips must
    // not offset the ordinals of the entities written after it.
    let mut doc = Document::new();
    let a = doc.add(line(0.0, 0.0, 10.0, 0.0));
    doc.add(EntityKind::XLine {
        through: Point2d::from_f64(0.0, 0.0),
        dir: (1.0, 0.0),
    });
    let b = doc.add(line(0.0, 5.0, 10.0, 5.0));
    doc.add_constraint(SketchConstraint::pair(ConstraintKind::Parallel, a, b));

    let reloaded = from_o2d(&to_o2d(&doc)).expect("round trip");
    let ids: Vec<_> = reloaded.iter().map(|e| e.id).collect();
    assert_eq!(
        ids.len(),
        2,
        "the two lines round-trip; the xline is skipped"
    );
    assert_eq!(reloaded.constraints.len(), 1);
    let c = &reloaded.constraints[0];
    assert_eq!(
        (c.a, c.b),
        (ids[0], Some(ids[1])),
        "constraint must rebind to the same two lines"
    );
}

#[test]
fn dxf_degenerate_entities_are_dropped_not_panicked() {
    // Zero-radius circles and NaN coordinates are common junk in real-world
    // DXF exports; the importer used to feed them to trusted constructors.
    let dxf = "0\nSECTION\n2\nENTITIES\n\
               0\nCIRCLE\n10\n1.0\n20\n2.0\n40\n0.0\n\
               0\nARC\n10\n0.0\n20\n0.0\n40\n-3.0\n50\n0\n51\n90\n\
               0\nLINE\n10\nnan\n20\n0.0\n11\n5.0\n21\n0.0\n\
               0\nLINE\n10\n0.0\n20\n0.0\n11\n5.0\n21\n0.0\n\
               0\nENDSEC\n0\nEOF\n";
    let doc = import_dxf(dxf);
    assert_eq!(doc.iter().count(), 1, "only the finite line survives");
}

#[test]
fn svg_hostile_arc_and_transform_are_dropped_not_panicked() {
    // `rx="nan"` slipped past the `< 1e-12` degenerate check straight into
    // the panicking arc constructor; scale(0) collapses shapes to NaN-free
    // degenerate geometry, but scale with NaN must not enter the document.
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
        <path d="M 0 0 A nan 5 0 0 1 10 10"/>
        <g transform="scale(nan)"><circle cx="1" cy="1" r="4"/></g>
        <line x1="0" y1="0" x2="5" y2="5"/>
    </svg>"#;
    let doc = import_svg(svg);
    for e in doc.iter() {
        assert!(
            e.kind.is_finite(),
            "no non-finite entity may enter the document"
        );
    }
    assert!(
        doc.iter().count() >= 1,
        "the valid line must survive the hostile siblings"
    );
}

#[test]
fn huge_declared_hatch_hole_count_stops_at_eof() {
    // The hole loop iterated the declared count even after input ran out;
    // usize::MAX spun it until out-of-memory. It must stop at EOF instead.
    let text = "O2D 1\n\
                E HATCH 0 bylayer 1,2,3 1 18446744073709551615 solid ByLayer bylayer\n\
                SEG LINE 0;0 4;0\n";
    let doc = from_o2d(text).expect("file must load");
    assert_eq!(doc.iter().count(), 1, "the hatch loads with no holes");
}

#[test]
fn degenerate_ellipse_axes_are_dropped() {
    let text = "O2D 1\n\
                E ELLIPSE 0 bylayer 0;0 0 3 0 0 6.28 ByLayer bylayer\n\
                E ELLIPSE 0 bylayer 0;0 4 -3 0 0 6.28 ByLayer bylayer\n\
                E ELLIPSE 0 bylayer 0;0 4 3 0 0 6.28 ByLayer bylayer\n";
    let doc = from_o2d(text).expect("salvageable file must load");
    assert_eq!(doc.iter().count(), 1, "only the valid ellipse survives");
}

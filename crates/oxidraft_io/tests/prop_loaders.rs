//! Property-based fuzzing of the three loaders. The deterministic tests in
//! `corrupt_input.rs` pin the corruptions we've already met; these generate
//! thousands of new ones — random bytes, and valid files mangled by
//! truncation, token substitution, and line-level edits — and check the
//! safety contract that must hold for *any* input:
//!
//! - the loader returns (no panic, no hang, no unbounded allocation),
//! - whatever loads carries only finite numbers, and
//! - whatever survives salvage re-saves and re-loads cleanly (a document we
//!   accepted must never become a file we reject).

use oxidraft_document::{ConstraintKind, Document, EntityKind, HatchPattern, SketchConstraint};
use oxidraft_geometry::{
    CircularArc, CubicBezier, Curve, EllipticalArc, LineSeg, Point2d, PolyCurve,
};
use oxidraft_io::{export_dxf, export_svg, from_o2d, import_dxf, import_svg, to_o2d};
use proptest::prelude::*;
use std::f64::consts::{FRAC_PI_2, TAU};

fn p(x: f64, y: f64) -> Point2d {
    Point2d::from_f64(x, y)
}

/// One of every kind the native format serializes, plus a constraint, so
/// mutations can hit every record parser.
fn seed_doc() -> Document {
    let mut doc = Document::new();
    let a = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
        p(0.0, 0.0),
        p(10.0, 0.0),
    ))));
    let b = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
        p(0.0, 5.0),
        p(10.0, 5.0),
    ))));
    doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
        p(3.0, 4.0),
        5.0,
        0.0,
        TAU,
    ))));
    doc.add(EntityKind::Curve(Curve::Ellipse(EllipticalArc::new(
        p(1.0, 2.0),
        4.0,
        2.0,
        0.3,
        0.0,
        TAU,
    ))));
    doc.add(EntityKind::Curve(Curve::Bezier(CubicBezier::new(
        p(0.0, 0.0),
        p(1.0, 2.0),
        p(3.0, 2.0),
        p(4.0, 0.0),
    ))));
    doc.add(EntityKind::Curve(Curve::Poly(Box::new(PolyCurve::new(
        vec![
            Curve::Line(LineSeg::from_endpoints(p(0.0, 0.0), p(2.0, 0.0))),
            Curve::Arc(CircularArc::new(p(2.0, 1.0), 1.0, -FRAC_PI_2, FRAC_PI_2)),
        ],
    )))));
    doc.add(EntityKind::Hatch {
        boundary: vec![
            Curve::Line(LineSeg::from_endpoints(p(0.0, 0.0), p(4.0, 0.0))),
            Curve::Line(LineSeg::from_endpoints(p(4.0, 0.0), p(4.0, 4.0))),
            Curve::Line(LineSeg::from_endpoints(p(4.0, 4.0), p(0.0, 4.0))),
            Curve::Line(LineSeg::from_endpoints(p(0.0, 4.0), p(0.0, 0.0))),
        ],
        holes: vec![vec![Curve::Arc(CircularArc::new(
            p(2.0, 2.0),
            1.0,
            0.0,
            TAU,
        ))]],
        fill: (200, 60, 60),
        pattern: HatchPattern::Lines {
            angle_deg: 45.0,
            spacing: 2.0,
        },
    });
    doc.add(EntityKind::Point(p(7.0, 7.0)));
    doc.add(EntityKind::Text {
        anchor: p(1.0, 1.0),
        content: "hello world".into(),
        height: 2.5,
        rotation: 0.2,
        font: None,
    });
    doc.add(EntityKind::Dimension {
        p1: p(0.0, 0.0),
        p2: p(10.0, 0.0),
        line: p(5.0, 2.0),
        height: 2.5,
        override_text: Some("10.0".into()),
    });
    doc.add_constraint(SketchConstraint::pair(ConstraintKind::Parallel, a, b));
    doc
}

fn all_finite(doc: &Document) -> bool {
    doc.iter().all(|e| e.kind.is_finite())
}

/// Values with a history: non-finite literals, count overflows, the usize
/// ceiling that once spun the hatch-hole loop, and token soup.
const JUNK: &[&str] = &[
    "nan",
    "inf",
    "-inf",
    "1e999",
    "-1",
    "0",
    "18446744073709551615",
    "abc",
    "",
    ";",
    "1;nan",
    "1/0",
    "-0.0",
];

/// Applies one structural mutation to a valid file text. Seeds are ASCII, so
/// byte-index truncation is safe.
fn mutate(
    seed: &str,
    mode: u8,
    i: prop::sample::Index,
    j: prop::sample::Index,
    junk: &str,
) -> String {
    let lines: Vec<&str> = seed.lines().collect();
    match mode % 5 {
        0 => seed[..i.index(seed.len() + 1).min(seed.len())].to_string(),
        1 => {
            // Replace one whitespace token on one line.
            let li = i.index(lines.len());
            let mut out = Vec::with_capacity(lines.len());
            for (n, line) in lines.iter().enumerate() {
                if n == li {
                    let mut toks: Vec<&str> = line.split(' ').collect();
                    let ti = j.index(toks.len());
                    toks[ti] = junk;
                    out.push(toks.join(" "));
                } else {
                    out.push((*line).to_string());
                }
            }
            out.join("\n")
        }
        2 => {
            let li = i.index(lines.len());
            let mut out = lines.clone();
            out.remove(li);
            out.join("\n")
        }
        3 => {
            let li = i.index(lines.len());
            let mut out = lines.clone();
            out.insert(li, lines[li]);
            out.join("\n")
        }
        _ => {
            let li = i.index(lines.len() + 1);
            let mut out = lines.clone();
            let injected = format!("E ARC 0 bylayer 0;0 {junk} {junk} 0 ByLayer bylayer");
            out.insert(li.min(lines.len()), &injected);
            out.join("\n")
        }
    }
}

/// Whatever the salvage pass accepted must survive its own save/load cycle.
fn assert_resave_stable(doc: &Document) -> Result<(), TestCaseError> {
    let again = from_o2d(&to_o2d(doc));
    prop_assert!(again.is_ok(), "accepted document failed to re-load");
    let again = again.unwrap();
    prop_assert_eq!(
        again.iter().count(),
        doc.iter().count(),
        "entities lost between save and re-load"
    );
    prop_assert!(all_finite(&again));
    Ok(())
}

proptest! {
    #[test]
    fn loaders_survive_random_bytes(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(doc) = from_o2d(&text) {
            prop_assert!(all_finite(&doc));
        }
        prop_assert!(all_finite(&import_dxf(&text)));
        prop_assert!(all_finite(&import_svg(&text)));
    }

    #[test]
    fn o2d_loader_survives_mutated_valid_files(
        mode in 0u8..5,
        i in any::<prop::sample::Index>(),
        j in any::<prop::sample::Index>(),
        junk in prop::sample::select(JUNK),
    ) {
        let text = mutate(&to_o2d(&seed_doc()), mode, i, j, junk);
        if let Ok(doc) = from_o2d(&text) {
            prop_assert!(all_finite(&doc));
            assert_resave_stable(&doc)?;
        }
    }

    #[test]
    fn dxf_importer_survives_mutated_exports(
        mode in 0u8..5,
        i in any::<prop::sample::Index>(),
        j in any::<prop::sample::Index>(),
        junk in prop::sample::select(JUNK),
    ) {
        let text = mutate(&export_dxf(&seed_doc()), mode, i, j, junk);
        prop_assert!(all_finite(&import_dxf(&text)));
    }

    #[test]
    fn svg_importer_survives_mutated_exports(
        mode in 0u8..5,
        i in any::<prop::sample::Index>(),
        j in any::<prop::sample::Index>(),
        junk in prop::sample::select(JUNK),
    ) {
        let text = mutate(&export_svg(&seed_doc()), mode, i, j, junk);
        prop_assert!(all_finite(&import_svg(&text)));
    }

    #[test]
    fn random_finite_docs_round_trip_exactly(
        lines in prop::collection::vec(
            (-1e9..1e9f64, -1e9..1e9f64, -1e9..1e9f64, -1e9..1e9f64),
            0..12,
        ),
        arcs in prop::collection::vec(
            (-1e9..1e9f64, -1e9..1e9f64, 1e-3..1e6f64, -10.0..10.0f64, 0.01..TAU),
            0..8,
        ),
    ) {
        let mut doc = Document::new();
        for &(x0, y0, x1, y1) in &lines {
            doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                p(x0, y0),
                p(x1, y1),
            ))));
        }
        for &(cx, cy, r, start, sweep) in &arcs {
            doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
                p(cx, cy),
                r,
                start,
                start + sweep,
            ))));
        }

        let loaded = from_o2d(&to_o2d(&doc));
        prop_assert!(loaded.is_ok());
        let loaded = loaded.unwrap();
        prop_assert_eq!(loaded.iter().count(), lines.len() + arcs.len());

        // Floats are written in Rust's shortest round-trip form, so the
        // geometry must come back bit-exact, not merely close.
        for (orig, back) in doc.iter().zip(loaded.iter()) {
            match (&orig.kind, &back.kind) {
                (
                    EntityKind::Curve(Curve::Line(l0)),
                    EntityKind::Curve(Curve::Line(l1)),
                ) => {
                    prop_assert_eq!(l0.p0, l1.p0);
                    prop_assert_eq!(l0.p1, l1.p1);
                }
                (
                    EntityKind::Curve(Curve::Arc(a0)),
                    EntityKind::Curve(Curve::Arc(a1)),
                ) => {
                    prop_assert_eq!(a0.center, a1.center);
                    prop_assert_eq!(a0.radius, a1.radius);
                    prop_assert_eq!(a0.start_angle, a1.start_angle);
                    prop_assert_eq!(a0.end_angle, a1.end_angle);
                }
                (o, b) => prop_assert!(false, "kind changed in round trip: {o:?} -> {b:?}"),
            }
        }
    }
}

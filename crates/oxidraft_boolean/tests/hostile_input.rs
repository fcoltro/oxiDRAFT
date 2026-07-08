//! Hostile-input battery for the boolean pipeline. The cad layer feeds
//! `Region` whatever loops the user picked as hatch/boolean operands —
//! self-intersecting quads, unclosed rings, zero-area slivers, coincident
//! copies — and `Region::new` validates none of it. Every case here must
//! produce *some* finite answer (empty output is fine); a panic or a
//! non-finite region escaping into tessellation/rendering is a defect.
//!
//! Companion to `prop_invariants.rs`, which checks the boolean algebra on
//! well-formed shapes; this file checks survival on malformed ones.

use oxidraft_boolean::{Region, WELD_TOL, difference, intersection, union, weld_region, xor};
use oxidraft_geometry::{CircularArc, Curve, LineSeg, Point2d};

fn p(x: f64, y: f64) -> Point2d {
    Point2d::from_f64(x, y)
}

/// Closed polygonal region through the given vertices.
fn poly(pts: &[(f64, f64)]) -> Region {
    let n = pts.len();
    Region::new(
        (0..n)
            .map(|i| {
                let (ax, ay) = pts[i];
                let (bx, by) = pts[(i + 1) % n];
                Curve::Line(LineSeg::from_endpoints(p(ax, ay), p(bx, by)))
            })
            .collect(),
    )
}

/// Open polyline region: consecutive segments only, last never rejoins first.
fn open_poly(pts: &[(f64, f64)]) -> Region {
    Region::new(
        pts.windows(2)
            .map(|w| {
                Curve::Line(LineSeg::from_endpoints(
                    p(w[0].0, w[0].1),
                    p(w[1].0, w[1].1),
                ))
            })
            .collect(),
    )
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Region {
    poly(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
}

fn circle(x: f64, y: f64, r: f64) -> Region {
    Region::new(vec![Curve::Arc(CircularArc::new(
        p(x, y),
        r,
        0.0,
        std::f64::consts::TAU,
    ))])
}

/// Runs every boolean op on the pair plus welding and the query API on each
/// operand, and demands finite results throughout.
fn exercise(label: &str, a: &Region, b: &Region) {
    for r in [a, b] {
        assert!(
            r.signed_area_f64().is_finite() || !region_is_finite(r),
            "{label}: finite input produced non-finite area"
        );
        // Must answer, not panic — the value itself is unspecified here.
        let _ = r.contains_point(0.5, 0.5);
        let _ = r.winding_number(0.5, 0.5);
        let _ = weld_region(r, WELD_TOL);
    }
    for (op, out) in [
        ("union", union(a, b)),
        ("intersection", intersection(a, b)),
        ("difference", difference(a, b)),
        ("xor", xor(a, b)),
    ] {
        for reg in &out {
            let area = reg.signed_area_f64();
            assert!(area.is_finite(), "{label}/{op}: non-finite area {area}");
            assert!(
                region_is_finite(reg),
                "{label}/{op}: non-finite boundary escaped: {reg:?}"
            );
        }
    }
}

fn region_is_finite(r: &Region) -> bool {
    r.outer.iter().all(Curve::is_finite) && r.holes.iter().flatten().all(Curve::is_finite)
}

#[test]
fn zero_area_operands_dont_panic() {
    let ok = rect(0.0, 0.0, 4.0, 3.0);
    exercise("zero-width rect", &rect(1.0, 1.0, 0.0, 2.0), &ok);
    exercise("zero-height rect", &rect(1.0, 1.0, 2.0, 0.0), &ok);
    // A zero-radius arc can't exist as a value — try_new rejects it, which is
    // the geometry crate's contract for untrusted input — so the nearest
    // constructible degenerate is a vanishingly small circle.
    assert!(CircularArc::try_new(p(1.0, 1.0), 0.0, 0.0, 1.0).is_err());
    exercise("tiny-radius circle", &circle(1.0, 1.0, 1e-30), &ok);
    exercise("point-degenerate poly", &poly(&[(1.0, 1.0); 4]), &ok);
    exercise(
        "collinear poly",
        &poly(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)]),
        &ok,
    );
}

#[test]
fn self_intersecting_bowtie_doesnt_panic() {
    let bowtie = poly(&[(0.0, 0.0), (4.0, 3.0), (4.0, 0.0), (0.0, 3.0)]);
    exercise("bowtie vs rect", &bowtie, &rect(1.0, 1.0, 5.0, 5.0));
    exercise("bowtie vs bowtie", &bowtie, &bowtie.clone());
}

#[test]
fn unclosed_and_underfilled_rings_dont_panic() {
    let ok = rect(0.0, 0.0, 4.0, 3.0);
    exercise(
        "open ring",
        &open_poly(&[(0.0, 0.0), (3.0, 0.0), (3.0, 3.0)]),
        &ok,
    );
    exercise("empty region", &Region::new(Vec::new()), &ok);
    exercise(
        "single segment",
        &Region::new(vec![Curve::Line(LineSeg::from_endpoints(
            p(0.0, 0.0),
            p(2.0, 2.0),
        ))]),
        &ok,
    );
    exercise(
        "both empty",
        &Region::new(Vec::new()),
        &Region::new(Vec::new()),
    );
}

#[test]
fn coincident_and_shared_edge_operands_dont_panic() {
    let a = rect(0.0, 0.0, 4.0, 3.0);
    exercise("identical rects", &a, &a.clone());
    exercise("shared edge", &a, &rect(4.0, 0.0, 4.0, 3.0));
    exercise("shared corner", &a, &rect(4.0, 3.0, 2.0, 2.0));
    let c = circle(0.0, 0.0, 2.0);
    exercise("identical circles", &c, &c.clone());
    exercise("internally tangent circles", &c, &circle(1.0, 0.0, 1.0));
}

#[test]
fn duplicate_vertices_and_zero_length_edges_dont_panic() {
    let stutter = poly(&[
        (0.0, 0.0),
        (0.0, 0.0),
        (4.0, 0.0),
        (4.0, 0.0),
        (4.0, 3.0),
        (0.0, 3.0),
        (0.0, 3.0),
    ]);
    exercise("stuttering ring", &stutter, &rect(1.0, 1.0, 5.0, 5.0));
    let zero_arc = Region::new(vec![Curve::Arc(CircularArc::new(
        p(0.0, 0.0),
        2.0,
        1.0,
        1.0,
    ))]);
    exercise("zero-sweep arc", &zero_arc, &rect(0.0, 0.0, 4.0, 3.0));
}

#[test]
fn non_finite_coordinates_dont_panic_or_escape() {
    let ok = rect(0.0, 0.0, 4.0, 3.0);
    exercise(
        "NaN vertex",
        &poly(&[(0.0, 0.0), (4.0, 0.0), (f64::NAN, 3.0)]),
        &ok,
    );
    exercise(
        "infinite vertex",
        &poly(&[(0.0, 0.0), (4.0, 0.0), (f64::INFINITY, 3.0)]),
        &ok,
    );
    // A NaN radius is rejected at construction; a NaN *center* is not, so
    // that arc reaches the pipeline and must be scrubbed there.
    assert!(CircularArc::try_new(p(0.0, 0.0), f64::NAN, 0.0, 1.0).is_err());
    exercise("NaN-center circle", &circle(f64::NAN, f64::NAN, 1.0), &ok);
}

#[test]
fn extreme_scales_dont_panic() {
    exercise(
        "huge vs tiny",
        &rect(1e15, 1e15, 1e15, 1e15),
        &rect(0.0, 0.0, 1e-12, 1e-12),
    );
    exercise(
        "huge overlap",
        &rect(1e15, 1e15, 2e15, 2e15),
        &rect(2e15, 2e15, 2e15, 2e15),
    );
    exercise(
        "sliver",
        &rect(0.0, 0.0, 10.0, 1e-13),
        &rect(1.0, -1.0, 2.0, 2.0),
    );
}

#[test]
fn degenerate_holes_dont_panic() {
    let outer4 = || {
        vec![
            Curve::Line(LineSeg::from_endpoints(p(0.0, 0.0), p(8.0, 0.0))),
            Curve::Line(LineSeg::from_endpoints(p(8.0, 0.0), p(8.0, 8.0))),
            Curve::Line(LineSeg::from_endpoints(p(8.0, 8.0), p(0.0, 8.0))),
            Curve::Line(LineSeg::from_endpoints(p(0.0, 8.0), p(0.0, 0.0))),
        ]
    };
    let hole = |x: f64, y: f64, w: f64| {
        vec![
            Curve::Line(LineSeg::from_endpoints(p(x, y), p(x + w, y))),
            Curve::Line(LineSeg::from_endpoints(p(x + w, y), p(x + w, y + w))),
            Curve::Line(LineSeg::from_endpoints(p(x + w, y + w), p(x, y + w))),
            Curve::Line(LineSeg::from_endpoints(p(x, y + w), p(x, y))),
        ]
    };
    let ok = rect(1.0, 1.0, 4.0, 3.0);
    exercise(
        "hole outside outer",
        &Region::with_holes(outer4(), vec![hole(20.0, 20.0, 2.0)]),
        &ok,
    );
    exercise(
        "hole equals outer",
        &Region::with_holes(outer4(), vec![outer4()]),
        &ok,
    );
    exercise(
        "empty hole ring",
        &Region::with_holes(outer4(), vec![Vec::new()]),
        &ok,
    );
    exercise(
        "overlapping holes",
        &Region::with_holes(outer4(), vec![hole(1.0, 1.0, 3.0), hole(2.0, 2.0, 3.0)]),
        &ok,
    );
}

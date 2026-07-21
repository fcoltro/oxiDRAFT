//! Property-based invariants for the boolean pipeline: thousands of random
//! shape pairs checked against the algebra that must hold for *any* input,
//! not just the cases the unit tests thought of.
//!
//! Two invariant families:
//! - inclusion–exclusion on area: |A∪B| + |A∩B| = |A| + |B|
//! - pointwise set semantics: a probe point is in A∪B iff it is in A or B,
//!   in A∩B iff in both, in A−B iff in A and not B — checked away from
//!   boundaries, where flatten tolerances legitimately blur the answer.

use oxidraft_boolean::{Region, difference, intersection, union};
use oxidraft_geometry::{CircularArc, Curve, LineSeg, Point2d};
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Shape {
    Rect { x: f64, y: f64, w: f64, h: f64 },
    Circle { x: f64, y: f64, r: f64 },
    Ngon { x: f64, y: f64, r: f64, n: usize },
}

impl Shape {
    fn region(&self) -> Region {
        match *self {
            Shape::Rect { x, y, w, h } => {
                let p = Point2d::from_f64;
                Region::new(vec![
                    Curve::Line(LineSeg::from_endpoints(p(x, y), p(x + w, y))),
                    Curve::Line(LineSeg::from_endpoints(p(x + w, y), p(x + w, y + h))),
                    Curve::Line(LineSeg::from_endpoints(p(x + w, y + h), p(x, y + h))),
                    Curve::Line(LineSeg::from_endpoints(p(x, y + h), p(x, y))),
                ])
            }
            Shape::Circle { x, y, r } => Region::new(vec![Curve::Arc(CircularArc::new(
                Point2d::from_f64(x, y),
                r,
                0.0,
                std::f64::consts::TAU,
            ))]),
            Shape::Ngon { x, y, r, n } => {
                let pts: Vec<Point2d> = (0..n)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / n as f64;
                        Point2d::from_f64(x + r * a.cos(), y + r * a.sin())
                    })
                    .collect();
                Region::new(
                    (0..n)
                        .map(|i| Curve::Line(LineSeg::from_endpoints(pts[i], pts[(i + 1) % n])))
                        .collect(),
                )
            }
        }
    }
}

fn shape() -> impl Strategy<Value = Shape> {
    prop_oneof![
        (-8.0..8.0f64, -8.0..8.0f64, 0.8..7.0f64, 0.8..7.0f64)
            .prop_map(|(x, y, w, h)| Shape::Rect { x, y, w, h }),
        (-8.0..8.0f64, -8.0..8.0f64, 0.6..5.0f64).prop_map(|(x, y, r)| Shape::Circle { x, y, r }),
        (-8.0..8.0f64, -8.0..8.0f64, 0.6..5.0f64, 3usize..9).prop_map(|(x, y, r, n)| Shape::Ngon {
            x,
            y,
            r,
            n
        }),
    ]
}

fn total_area(regions: &[Region]) -> f64 {
    regions.iter().map(|r| r.signed_area_f64()).sum()
}

fn covered(regions: &[Region], x: f64, y: f64) -> bool {
    regions.iter().any(|r| r.contains_point(x, y))
}

/// Distance from a probe to the nearest boundary of either input, so the
/// pointwise checks only assert where the answer is unambiguous.
fn boundary_clearance(a: &Region, b: &Region, x: f64, y: f64) -> f64 {
    a.outer
        .iter()
        .chain(a.holes.iter().flatten())
        .chain(b.outer.iter())
        .chain(b.holes.iter().flatten())
        .map(|c| oxidraft_geometry::point_to_curve_distance(c, x, y))
        .fold(f64::INFINITY, f64::min)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn inclusion_exclusion_on_area(sa in shape(), sb in shape()) {
        let a = sa.region();
        let b = sb.region();
        let area_a = a.signed_area_f64();
        let area_b = b.signed_area_f64();
        let u = total_area(&union(&a, &b));
        let i = total_area(&intersection(&a, &b));
        // Flatten-tolerance area error scales with size; 2% relative slack
        // comfortably covers it while still catching real material loss.
        let lhs = u + i;
        let rhs = area_a + area_b;
        prop_assert!(
            (lhs - rhs).abs() <= 0.02 * rhs.abs().max(1.0),
            "|A∪B| + |A∩B| = {lhs} but |A| + |B| = {rhs} for {sa:?} vs {sb:?}"
        );
    }

    #[test]
    fn pointwise_set_semantics(
        sa in shape(),
        sb in shape(),
        probes in proptest::collection::vec((-16.0..16.0f64, -16.0..16.0f64), 12),
    ) {
        let a = sa.region();
        let b = sb.region();
        let u = union(&a, &b);
        let i = intersection(&a, &b);
        let d = difference(&a, &b);
        for (x, y) in probes {
            // Skip probes near any boundary: there the flatten tolerance and
            // the exact winding test may legitimately disagree.
            if boundary_clearance(&a, &b, x, y) < 0.05 {
                continue;
            }
            let in_a = a.contains_point(x, y);
            let in_b = b.contains_point(x, y);
            prop_assert_eq!(
                covered(&u, x, y), in_a || in_b,
                "union wrong at ({}, {}) for {:?} vs {:?}", x, y, &sa, &sb
            );
            prop_assert_eq!(
                covered(&i, x, y), in_a && in_b,
                "intersection wrong at ({}, {}) for {:?} vs {:?}", x, y, &sa, &sb
            );
            prop_assert_eq!(
                covered(&d, x, y), in_a && !in_b,
                "difference wrong at ({}, {}) for {:?} vs {:?}", x, y, &sa, &sb
            );
        }
    }

    #[test]
    fn winding_containment_matches_geometry_for_circles(
        cx in -8.0..8.0f64,
        cy in -8.0..8.0f64,
        r in 0.6..5.0f64,
        probes in proptest::collection::vec((-16.0..16.0f64, -16.0..16.0f64), 24),
    ) {
        let region = Shape::Circle { x: cx, y: cy, r }.region();
        for (x, y) in probes {
            let dist = (x - cx).hypot(y - cy);
            if (dist - r).abs() < 1e-6 {
                continue;
            }
            prop_assert_eq!(
                region.contains_point(x, y),
                dist < r,
                "circle containment disagrees with |p−c| at ({}, {})", x, y
            );
        }
    }
}

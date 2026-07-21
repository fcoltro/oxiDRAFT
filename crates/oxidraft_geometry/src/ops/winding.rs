//! Winding-angle contribution of a rational Bézier piece — the signed angle it
//! subtends at a query point, summed over a closed region to test containment.

use crate::nurbs::RationalBezier;

/// Signed angle subtended at `(qx, qy)` by one rational Bézier piece, after
/// the generalized-winding-number construction of Spainhour et al., "Robust
/// Containment Queries over Collections of Rational Parametric Curves via
/// Generalized Winding Numbers" (ACM TOG 43(4), 2024): subdivide until every
/// control point of the piece lies in one open half-plane through the query
/// point. The convex-hull property then confines the whole piece to that
/// half-plane, so it cannot pass through the query point, sweeps less than π,
/// and its subtended angle is exactly the angle between the endpoint vectors.
///
/// Summed over a closed boundary the angles telescope to 2π × winding number —
/// with no tessellation, so containment is exact for arcs and ellipses. For a
/// boundary with small gaps the sum is merely *near* an integer multiple,
/// which is what makes callers that round robust to non-watertight input.
pub fn rational_winding_angle(seg: &RationalBezier, qx: f64, qy: f64) -> f64 {
    // A non-finite control point, weight, or query defeats the half-plane
    // cutoff below on every level — and de Casteljau splitting spreads a NaN
    // into both halves — so the depth cap would bound a full 2^40-leaf tree
    // instead of a thin path near the query. A poisoned segment subtends no
    // defined angle; contribute none.
    if !(qx.is_finite() && qy.is_finite())
        || seg.points.iter().any(|p| !p.is_finite())
        || seg.weights.iter().any(|w| !w.is_finite())
    {
        return 0.0;
    }
    // Bounded so a query exactly on the curve terminates; the work per query
    // grows linearly with depth (only the pieces nearest the query keep
    // splitting), so a generous cap costs little and classifies points down
    // to ~1e-9 of the curve size.
    const MAX_DEPTH: u32 = 40;
    angle_rec(seg, qx, qy, 0, MAX_DEPTH)
}

fn angle_rec(seg: &RationalBezier, qx: f64, qy: f64, depth: u32, max_depth: u32) -> f64 {
    if depth < max_depth && !separated_by_half_plane(seg, qx, qy) {
        let (l, r) = seg.split(0.5);
        return angle_rec(&l, qx, qy, depth + 1, max_depth)
            + angle_rec(&r, qx, qy, depth + 1, max_depth);
    }
    endpoint_angle(seg, qx, qy)
}

/// True when every control point lies strictly in one open half-plane through
/// the query point. The candidate normal is the sum of the normalized control
/// vectors — sufficient (not necessary), so a false negative just costs one
/// more subdivision, never a wrong angle.
fn separated_by_half_plane(seg: &RationalBezier, qx: f64, qy: f64) -> bool {
    let (mut ux, mut uy) = (0.0f64, 0.0f64);
    for p in &seg.points {
        let (vx, vy) = (p.x - qx, p.y - qy);
        let len = vx.hypot(vy);
        if len < 1e-300 {
            // The query sits on a control point; only subdivision can decide.
            return false;
        }
        ux += vx / len;
        uy += vy / len;
    }
    seg.points
        .iter()
        .all(|p| (p.x - qx) * ux + (p.y - qy) * uy > 0.0)
}

fn endpoint_angle(seg: &RationalBezier, qx: f64, qy: f64) -> f64 {
    let a = seg.points.first().expect("validated non-empty");
    let b = seg.points.last().expect("validated non-empty");
    let (ax, ay) = (a.x - qx, a.y - qy);
    let (bx, by) = (b.x - qx, b.y - qy);
    (ax * by - ay * bx).atan2(ax * bx + ay * by)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::Curve;
    use crate::nurbs::lower;
    use crate::point::Point2d;
    use crate::primitives::{CircularArc, LineSeg};

    fn winding_of(curves: &[Curve], x: f64, y: f64) -> f64 {
        curves
            .iter()
            .flat_map(lower)
            .map(|s| rational_winding_angle(&s, x, y))
            .sum::<f64>()
            / std::f64::consts::TAU
    }

    #[test]
    fn full_circle_winds_exactly_once_inside_and_zero_outside() {
        let circle = [Curve::Arc(CircularArc::new(
            Point2d::from_f64(3.0, -1.0),
            5.0,
            0.0,
            std::f64::consts::TAU,
        ))];
        for (x, y, want) in [
            (3.0, -1.0, 1.0),
            (7.9, -1.0, 1.0),
            (8.1, -1.0, 0.0),
            (30.0, 40.0, 0.0),
        ] {
            let w = winding_of(&circle, x, y);
            assert!(
                (w - want).abs() < 1e-9,
                "winding at ({x},{y}) = {w}, want {want}"
            );
        }
    }

    #[test]
    fn classification_is_sharp_near_the_rim() {
        // Tessellation-based classification was only accurate to the flatten
        // tolerance (~1e-3 of the size); the winding form is exact down to
        // the subdivision floor.
        let circle = [Curve::Arc(CircularArc::new(
            Point2d::from_f64(0.0, 0.0),
            5.0,
            0.0,
            std::f64::consts::TAU,
        ))];
        let eps = 1e-8;
        let inside = winding_of(&circle, 5.0 - eps, 0.0);
        let outside = winding_of(&circle, 5.0 + eps, 0.0);
        assert!(inside > 0.5, "just inside the rim: winding {inside}");
        assert!(outside < 0.5, "just outside the rim: winding {outside}");
    }

    #[test]
    fn gapped_boundary_still_classifies() {
        // A square whose last edge stops short of closing (5% gap): the
        // winding sum is no longer an integer but stays close enough that
        // rounding classifies correctly — the paper's headline robustness.
        let p = |x: f64, y: f64| Point2d::from_f64(x, y);
        let square = [
            Curve::Line(LineSeg::from_endpoints(p(0.0, 0.0), p(10.0, 0.0))),
            Curve::Line(LineSeg::from_endpoints(p(10.0, 0.0), p(10.0, 10.0))),
            Curve::Line(LineSeg::from_endpoints(p(10.0, 10.0), p(0.0, 10.0))),
            Curve::Line(LineSeg::from_endpoints(p(0.0, 10.0), p(0.0, 0.5))),
        ];
        let inside = winding_of(&square, 5.0, 5.0);
        let outside = winding_of(&square, 15.0, 5.0);
        let far = winding_of(&square, -3.0, -3.0);
        assert!((inside - 1.0).abs() < 0.05, "center winding {inside}");
        assert!(outside.abs() < 0.05, "right of square winding {outside}");
        assert!(far.abs() < 0.05, "far corner winding {far}");
    }
}

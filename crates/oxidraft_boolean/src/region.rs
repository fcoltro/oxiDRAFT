//! The [`Region`] type: a closed area bounded by an outer loop of curves with
//! optional inner holes — the operand of every boolean operation. Flattened
//! rings (for area) and exact rational-Bézier boundaries (for containment) are
//! cached and self-validate against edits.

use oxidraft_geometry::nurbs::lower;
use oxidraft_geometry::{
    Curve, CurveSegment, Point2d, RationalBezier, rational_winding_angle, tessellate_curve,
};
use std::sync::{Arc, PoisonError, RwLock};

/// A filled planar region: an outer boundary loop, minus any hole loops.
pub struct Region {
    /// The outer boundary, a closed loop of curves.
    pub outer: Vec<Curve>,
    /// Hole boundaries, each a closed loop cut out of the region.
    pub holes: Vec<Vec<Curve>>,
    // Prepared boundary forms, keyed by a content hash of the curves:
    // flattened rings for area, and the exact rational-Bézier lowering for
    // winding-number containment. The hash key makes the cache
    // self-validating: mutating `outer` or `holes` in place just triggers a
    // recompute, never serves a stale boundary.
    ring_cache: RwLock<Option<(u64, Arc<Rings>)>>,
}

struct Rings {
    outer: Vec<Point2d>,
    holes: Vec<Vec<Point2d>>,
    // Each boundary loop (outer first, then holes) lowered to rational
    // Béziers — exact for arcs and ellipses. Kept per loop so containment
    // can combine loops by parity, independent of their orientation.
    rational: Vec<Vec<RationalBezier>>,
}

impl Clone for Region {
    fn clone(&self) -> Self {
        // Carry the warm cache over (the Arc clone is cheap).
        let cache = self
            .ring_cache
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        Region {
            outer: self.outer.clone(),
            holes: self.holes.clone(),
            ring_cache: RwLock::new(cache),
        }
    }
}

impl std::fmt::Debug for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Region")
            .field("outer", &self.outer)
            .field("holes", &self.holes)
            .finish()
    }
}

impl Region {
    /// A region bounded by `outer` with no holes.
    pub fn new(outer: Vec<Curve>) -> Self {
        Region::with_holes(outer, Vec::new())
    }

    /// A region bounded by `outer` with the given `holes` cut out.
    pub fn with_holes(outer: Vec<Curve>, holes: Vec<Vec<Curve>>) -> Self {
        Region {
            outer,
            holes,
            ring_cache: RwLock::new(None),
        }
    }

    /// The signed area (outer area minus hole areas); the sign reflects the
    /// outer loop's winding direction.
    pub fn signed_area_f64(&self) -> f64 {
        let rings = self.rings();
        ring_signed_area(&rings.outer)
            - rings
                .holes
                .iter()
                .map(|h| ring_signed_area(h).abs())
                .sum::<f64>()
    }

    /// Winding number computed on the exact lowered boundary (generalized
    /// winding numbers after Spainhour et al., TOG 2024): no tessellation, so
    /// classification near a curved rim is sharp instead of limited by a
    /// flatten tolerance, and rounding the angle sum keeps the answer stable
    /// for boundaries that fail to close by a small gap.
    pub fn winding_number(&self, px: f64, py: f64) -> i32 {
        let rings = self.rings();
        let total: f64 = rings
            .rational
            .iter()
            .flatten()
            .map(|s| rational_winding_angle(s, px, py))
            .sum();
        (total / std::f64::consts::TAU).round() as i32
    }

    /// Inside the region's material: enclosed by an odd number of boundary
    /// loops. Parity (rather than the summed winding) makes the answer
    /// independent of each loop's orientation — callers hand us holes wound
    /// either way.
    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        let rings = self.rings();
        let enclosing = rings
            .rational
            .iter()
            .filter(|lp| {
                let w: f64 = lp
                    .iter()
                    .map(|s| rational_winding_angle(s, px, py))
                    .sum::<f64>()
                    / std::f64::consts::TAU;
                w.round() as i32 != 0
            })
            .count();
        enclosing % 2 == 1
    }

    fn rings(&self) -> Arc<Rings> {
        let key = self.content_hash();
        if let Some((k, rings)) = self
            .ring_cache
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            && *k == key
        {
            return rings.clone();
        }
        // A loop containing any non-finite curve has no defined boundary, and
        // preparing it would poison every query built on these rings — most
        // expensively the winding recursion, where NaN control points defeat
        // the half-plane cutoff at every level and the depth cap becomes a
        // 2^40-leaf tree. Such a loop contributes empty prepared forms
        // instead: the same answers an absent loop would give.
        let finite = |lp: &[Curve]| lp.iter().all(Curve::is_finite);
        let rings = Arc::new(Rings {
            outer: if finite(&self.outer) {
                boundary_ring(&self.outer)
            } else {
                Vec::new()
            },
            holes: self
                .holes
                .iter()
                .map(|h| {
                    if finite(h) {
                        boundary_ring(h)
                    } else {
                        Vec::new()
                    }
                })
                .collect(),
            rational: std::iter::once(&self.outer)
                .chain(self.holes.iter())
                .map(|lp| {
                    if finite(lp) {
                        lp.iter().flat_map(lower).collect()
                    } else {
                        Vec::new()
                    }
                })
                .collect(),
        });
        *self
            .ring_cache
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some((key, rings.clone()));
        rings
    }

    /// FNV over the exact defining floats of every boundary curve. A collision
    /// would serve a stale ring — the same accepted 2⁻⁶⁴ trade-off as the
    /// NURBS decomposition cache.
    fn content_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        feed(&mut h, self.outer.len() as u64);
        for c in &self.outer {
            hash_curve(&mut h, c);
        }
        for hole in &self.holes {
            feed(&mut h, hole.len() as u64);
            for c in hole {
                hash_curve(&mut h, c);
            }
        }
        h
    }
}

fn feed(h: &mut u64, v: u64) {
    *h ^= v;
    *h = h.wrapping_mul(0x100000001b3);
}

fn hash_pt(h: &mut u64, p: &Point2d) {
    feed(h, p.x.to_bits());
    feed(h, p.y.to_bits());
}

/// Exhaustive over `Curve` so a new variant cannot silently skip hashing.
fn hash_curve(h: &mut u64, c: &Curve) {
    match c {
        Curve::Line(l) => {
            feed(h, 1);
            hash_pt(h, &l.p0);
            hash_pt(h, &l.p1);
        }
        Curve::Arc(a) => {
            feed(h, 2);
            hash_pt(h, &a.center);
            feed(h, a.radius.to_bits());
            feed(h, a.start_angle.to_bits());
            feed(h, a.end_angle.to_bits());
        }
        Curve::Ellipse(e) => {
            feed(h, 3);
            hash_pt(h, &e.center);
            feed(h, e.semi_major.to_bits());
            feed(h, e.semi_minor.to_bits());
            feed(h, e.rotation.to_bits());
            feed(h, e.start_angle.to_bits());
            feed(h, e.end_angle.to_bits());
        }
        Curve::Bezier(b) => {
            feed(h, 4);
            hash_pt(h, &b.p0);
            hash_pt(h, &b.p1);
            hash_pt(h, &b.p2);
            hash_pt(h, &b.p3);
        }
        Curve::Poly(p) => {
            feed(h, 5);
            feed(h, p.segments.len() as u64);
            for s in &p.segments {
                hash_curve(h, s);
            }
        }
        Curve::Rational(r) => {
            feed(h, 6);
            feed(h, r.points.len() as u64);
            for p in &r.points {
                hash_pt(h, p);
            }
            for w in &r.weights {
                feed(h, w.to_bits());
            }
        }
        Curve::Nurbs(n) => {
            feed(h, 7);
            feed(h, n.control.len() as u64);
            for p in &n.control {
                hash_pt(h, p);
            }
            for w in &n.weights {
                feed(h, w.to_bits());
            }
        }
    }
}

fn flatten_segment(seg: &Curve) -> Vec<Point2d> {
    let bb = seg.bounding_box();
    let diag = ((bb.max.x - bb.min.x).powi(2) + (bb.max.y - bb.min.y).powi(2)).sqrt();
    let tol = (diag * 1e-4).max(1e-9);
    tessellate_curve(seg, tol)
}

/// Flattens every boundary segment into one continuous vertex ring. Shared
/// endpoints between consecutive segments are de-duplicated so the ring carries
/// no zero-length edges, and the ring is meant to be read as *closed*: the edge
/// from the last vertex back to the first must be walked too.
///
/// Closing the ring matters because a full-circle arc tessellates to a polyline
/// whose ends differ by a rounding gap (`sin(2π) ≈ -1.2e-16`, not 0). Walking
/// only the within-segment edges drops the crossing that lives in that seam, so
/// a horizontal ray whose `y` lands inside the gap miscounts and reports an
/// outside point as inside.
fn boundary_ring(boundary: &[Curve]) -> Vec<Point2d> {
    let mut ring: Vec<Point2d> = Vec::new();
    for seg in boundary {
        let poly = flatten_segment(seg);
        let mut iter = poly.into_iter();
        if let Some(first) = iter.next() {
            // Skip the first point when it coincides with the previous segment's end.
            if ring
                .last()
                .is_none_or(|l| (l.x - first.x).abs() > 1e-12 || (l.y - first.y).abs() > 1e-12)
            {
                ring.push(first);
            }
            ring.extend(iter);
        }
    }
    // Drop a trailing point that coincides with the start: the closing edge added
    // by the wraparound walk would otherwise be zero-length (and the seam crossing
    // would still be missed).
    if ring.len() >= 2 {
        let (f, l) = (ring[0], *ring.last().unwrap());
        if (f.x - l.x).abs() <= 1e-12 && (f.y - l.y).abs() <= 1e-12 {
            ring.pop();
        }
    }
    ring
}

fn ring_signed_area(ring: &[Point2d]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        area += (a.x + b.x) * (b.y - a.y);
    }
    area / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_geometry::{CircularArc, Curve, LineSeg, Point2d};

    fn square_region() -> Region {
        Region::new(vec![
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(0, 0),
                Point2d::from_i64(2, 0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(2, 0),
                Point2d::from_i64(2, 2),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(2, 2),
                Point2d::from_i64(0, 2),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(0, 2),
                Point2d::from_i64(0, 0),
            )),
        ])
    }

    #[test]
    fn interior_point() {
        let r = square_region();
        assert!(r.contains_point(1.0, 1.0));
    }

    #[test]
    fn exterior_point() {
        let r = square_region();
        assert!(!r.contains_point(5.0, 5.0));
    }

    #[test]
    fn signed_area_positive_ccw() {
        let r = square_region();
        let area = r.signed_area_f64();
        assert!(
            area > 0.0,
            "CCW boundary should have positive area, got {}",
            area
        );
        assert!((area - 4.0).abs() < 0.1, "area≈{}", area);
    }

    #[test]
    fn circle_region_area_and_classification() {
        let r = Region::new(vec![Curve::Arc(CircularArc::new(
            Point2d::from_i64(0, 0),
            3.0,
            0.0,
            std::f64::consts::TAU,
        ))]);
        let area = r.signed_area_f64();
        let expected = std::f64::consts::PI * 9.0;
        // The inscribed polygon under-estimates by about sagitta·perimeter/2;
        // with the minimal-vertex arc flattening that bound is ~tol·π·r.
        assert!(
            (area - expected).abs() < 2e-2,
            "circle area ≈ {expected}, got {area}"
        );
        assert!(r.contains_point(0.0, 0.0), "centre is inside");
        assert!(r.contains_point(2.9, 0.0), "just inside the rim");
        assert!(!r.contains_point(3.1, 0.0), "just outside the rim");
        assert!(!r.contains_point(10.0, 10.0), "far point is outside");
    }

    #[test]
    fn full_circle_seam_does_not_leak_winding() {
        // A full-circle arc tessellates to a polyline whose ends differ by a
        // rounding gap (sin(2π) ≈ -1.2e-16). A horizontal ray whose y lands inside
        // that seam must still see both rim crossings; before closing the ring, the
        // seam crossing was dropped and a far-outside point read as inside, leaking
        // hatch lines beyond the circle.
        let r = Region::new(vec![Curve::Arc(CircularArc::new(
            Point2d::from_i64(0, 0),
            5.0,
            0.0,
            std::f64::consts::TAU,
        ))]);
        for &y in &[0.0, -0.0, 1e-16, -1e-16, -1e-300, 1e-9, -1e-9] {
            assert!(
                !r.contains_point(-9.571, y),
                "point far left of the circle must be outside (y={y:+e})"
            );
            assert!(
                !r.contains_point(9.571, y),
                "point far right of the circle must be outside (y={y:+e})"
            );
            assert!(
                r.contains_point(0.0, y),
                "the centre line must be inside (y={y:+e})"
            );
        }
    }

    #[test]
    fn non_watertight_boundary_still_classifies() {
        // The last edge stops 0.5 short of closing the square. Ray-parity
        // classification flips arbitrarily when a ray threads the gap; the
        // winding-angle sum only drifts slightly off 1.0 and rounds correctly.
        let r = Region::new(vec![
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
                Point2d::from_f64(0.0, 10.0),
                Point2d::from_f64(0.0, 0.5),
            )),
        ]);
        assert!(r.contains_point(5.0, 5.0), "center is inside");
        assert!(!r.contains_point(-1.0, 0.25), "left of the gap is outside");
        assert!(!r.contains_point(15.0, 5.0), "right of square is outside");
    }

    #[test]
    fn curved_rim_classification_is_sharp() {
        // Tessellation-based containment was only reliable to the flatten
        // tolerance (~1e-3 of the size); the exact winding form classifies
        // points a million times closer to the rim.
        let r = Region::new(vec![Curve::Arc(CircularArc::new(
            Point2d::from_i64(0, 0),
            5.0,
            0.0,
            std::f64::consts::TAU,
        ))]);
        assert!(r.contains_point(5.0 - 1e-8, 0.0), "just inside the rim");
        assert!(!r.contains_point(5.0 + 1e-8, 0.0), "just outside the rim");
    }

    #[test]
    fn rotated_diamond_classification_uses_robust_orientation() {
        let d = Region::new(vec![
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(3, 0),
                Point2d::from_i64(0, 3),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(0, 3),
                Point2d::from_i64(-3, 0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(-3, 0),
                Point2d::from_i64(0, -3),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(0, -3),
                Point2d::from_i64(3, 0),
            )),
        ]);
        assert!(d.contains_point(0.0, 0.0), "centre inside");
        assert!(d.contains_point(1.4, 1.4), "just inside the x+y=3 edge");
        assert!(!d.contains_point(1.6, 1.6), "just outside the x+y=3 edge");
        assert!(!d.contains_point(2.0, 2.0), "corner-diagonal point outside");
    }
}

//! Curve-preserving booleans: trim-and-stitch on the original boundary
//! curves, so arcs stay arcs and Béziers stay Béziers through
//! union/intersection/difference. The pipeline is
//!
//!   intersect original curves → split at hit parameters → classify each
//!   sub-curve by winding-number containment of its midpoint → stitch the
//!   kept pieces into loops → group loops into regions by containment depth.
//!
//! Every step that cannot be decided confidently (coincident boundaries,
//! ambiguous stitch junctions, crossings through curve endpoints that leave
//! no verifiable cut) returns `None`, and the caller falls back to the
//! tessellating Greiner–Hormann clipper, which handles those degeneracies by
//! perturbation. Exactness for the common transversal case, robustness for
//! the rest.

use crate::boolean_ops::{interior_point, point_in_ring, poly_area, signed_poly_area};
use crate::clip::BoolOp;
use crate::region::Region;
use crate::weld::{WELD_TOL, weld_region};
use oxidraft_geometry::{
    Curve, CurveSegment, Point2d, intersect, point_to_curve_distance, reverse_curve, split_curve,
    tessellate_curve,
};

pub(crate) fn clip_curved(a: &Region, b: &Region, op: BoolOp) -> Option<Vec<Region>> {
    let a = weld_region(a, WELD_TOL);
    let b = weld_region(b, WELD_TOL);
    let a_loops = region_loops(&a);
    let b_loops = region_loops(&b);
    if a_loops.is_empty() || b_loops.is_empty() {
        return None;
    }
    let diag = loops_diag(&a_loops).max(loops_diag(&b_loops)).max(1e-12);
    let contact_eps = diag * 1e-9;
    let join_tol = diag * 1e-7;

    // Cut fractions per curve on each side. A hit lands as a domain fraction
    // in (0, 1); hits at a curve endpoint are dropped here (the endpoint is
    // already a joint) but remembered, because a loop whose only contacts
    // were dropped cannot be classified whole with confidence.
    let mut a_cuts = cut_table(&a_loops);
    let mut b_cuts = cut_table(&b_loops);
    let mut a_dropped = vec![false; a_loops.len()];
    let mut b_dropped = vec![false; b_loops.len()];
    for (li, la) in a_loops.iter().enumerate() {
        for (ci, ca) in la.iter().enumerate() {
            for (lj, lb) in b_loops.iter().enumerate() {
                for (cj, cb) in lb.iter().enumerate() {
                    for h in intersect(ca, cb) {
                        match domain_fraction(ca, h.t1) {
                            Some(f) => a_cuts[li][ci].push(f),
                            None => a_dropped[li] = true,
                        }
                        match domain_fraction(cb, h.t2) {
                            Some(f) => b_cuts[lj][cj].push(f),
                            None => b_dropped[lj] = true,
                        }
                    }
                }
            }
        }
    }

    let mut final_loops: Vec<Vec<Curve>> = Vec::new();
    let mut pool: Vec<Curve> = Vec::new();
    for (side_is_a, loops, cuts, dropped, other, other_loops) in [
        (true, &a_loops, &mut a_cuts, &a_dropped, &b, &b_loops),
        (false, &b_loops, &mut b_cuts, &b_dropped, &a, &a_loops),
    ] {
        for (li, lp) in loops.iter().enumerate() {
            let mut subs: Vec<(Curve, bool)> = Vec::new();
            let mut any_cut = false;
            for (ci, c) in lp.iter().enumerate() {
                let fr = &mut cuts[li][ci];
                fr.sort_by(f64::total_cmp);
                fr.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
                any_cut |= !fr.is_empty();
                for piece in split_at_fractions(c, fr) {
                    let (t0, t1) = piece.domain();
                    let (mx, my) = piece.evaluate_f64(0.5 * (t0 + t1));
                    // A midpoint sitting on the other boundary means the
                    // boundaries coincide along a stretch — not decidable
                    // by sidedness.
                    if boundary_distance(other_loops, mx, my) < contact_eps {
                        return None;
                    }
                    let inside = other.contains_point(mx, my);
                    let keep = match op {
                        BoolOp::Union => !inside,
                        BoolOp::Intersection => inside,
                        BoolOp::Difference => side_is_a != inside,
                    };
                    subs.push((piece, keep));
                }
            }
            if any_cut {
                pool.extend(subs.into_iter().filter(|(_, k)| *k).map(|(c, _)| c));
            } else if subs.iter().all(|(_, k)| *k) {
                if dropped[li] {
                    // The other boundary touched this loop only through
                    // endpoints we discarded; a whole-loop verdict could be
                    // wrong on one side of that touch.
                    return None;
                }
                final_loops.push(lp.clone());
            } else if subs.iter().any(|(_, k)| *k) {
                // Mixed verdicts without cuts: the other boundary passes
                // through this loop's existing vertices. The kept whole
                // curves still stitch against the other side's cut pieces.
                pool.extend(subs.into_iter().filter(|(_, k)| *k).map(|(c, _)| c));
            } else if dropped[li] {
                return None;
            }
        }
    }

    final_loops.extend(stitch(pool, join_tol)?);
    Some(curve_loops_to_regions(final_loops, diag * 1e-3))
}

fn cut_table(loops: &[Vec<Curve>]) -> Vec<Vec<Vec<f64>>> {
    loops.iter().map(|l| vec![Vec::new(); l.len()]).collect()
}

/// Boundary loops with composite curves exploded one level: `Poly` chains
/// become their child segments and NURBS become their rational Bézier spans,
/// so every curve that reaches the splitter has a *linear* domain-fraction
/// parameterization (successive `split_curve` calls stay exact).
fn region_loops(r: &Region) -> Vec<Vec<Curve>> {
    fn explode(c: &Curve, out: &mut Vec<Curve>) {
        match c {
            Curve::Poly(p) => {
                for s in &p.segments {
                    explode(s, out);
                }
            }
            Curve::Nurbs(n) => out.extend(n.segments().into_iter().map(Curve::Rational)),
            other => out.push(other.clone()),
        }
    }
    let mut loops = Vec::with_capacity(1 + r.holes.len());
    for lp in std::iter::once(&r.outer).chain(r.holes.iter()) {
        let mut flat = Vec::with_capacity(lp.len());
        for c in lp {
            explode(c, &mut flat);
        }
        if !flat.is_empty() {
            loops.push(flat);
        }
    }
    loops
}

/// Converts a hit parameter (in the curve's own domain) to a fraction of the
/// domain; `None` for hits at the ends, which are joints, not cuts.
fn domain_fraction(c: &Curve, t: f64) -> Option<f64> {
    let (d0, d1) = c.domain();
    if (d1 - d0).abs() < 1e-300 {
        return None;
    }
    let f = (t - d0) / (d1 - d0);
    (1e-9..=1.0 - 1e-9).contains(&f).then_some(f)
}

fn split_at_fractions(c: &Curve, fractions: &[f64]) -> Vec<Curve> {
    if fractions.is_empty() {
        return vec![c.clone()];
    }
    let mut out = Vec::with_capacity(fractions.len() + 1);
    let mut rest = c.clone();
    let mut consumed = 0.0;
    for &f in fractions {
        let local = ((f - consumed) / (1.0 - consumed)).clamp(0.0, 1.0);
        let (left, right) = split_curve(&rest, local);
        out.push(left);
        rest = right;
        consumed = f;
    }
    out.push(rest);
    out
}

fn boundary_distance(loops: &[Vec<Curve>], x: f64, y: f64) -> f64 {
    loops
        .iter()
        .flatten()
        .map(|c| point_to_curve_distance(c, x, y))
        .fold(f64::INFINITY, f64::min)
}

fn ends(c: &Curve) -> (Point2d, Point2d) {
    let (t0, t1) = c.domain();
    let (sx, sy) = c.evaluate_f64(t0);
    let (ex, ey) = c.evaluate_f64(t1);
    (Point2d::from_f64(sx, sy), Point2d::from_f64(ex, ey))
}

/// Chains the kept pieces into closed loops by endpoint proximity. In a clean
/// transversal result exactly two kept pieces meet at every junction; any
/// other count means a tangency or coincidence slipped through — bail out.
fn stitch(mut pool: Vec<Curve>, tol: f64) -> Option<Vec<Vec<Curve>>> {
    let tol_sq = tol * tol;
    let mut loops = Vec::new();
    while let Some(first) = pool.pop() {
        let (loop_start, mut cur_end) = ends(&first);
        let mut lp = vec![first];
        loop {
            if cur_end.dist_sq(&loop_start) <= tol_sq {
                loops.push(lp);
                break;
            }
            let mut candidate: Option<(usize, bool)> = None;
            for (i, c) in pool.iter().enumerate() {
                let (s, e) = ends(c);
                let fwd = s.dist_sq(&cur_end) <= tol_sq;
                let rev = e.dist_sq(&cur_end) <= tol_sq;
                if fwd || rev {
                    if candidate.is_some() {
                        return None;
                    }
                    candidate = Some((i, rev && !fwd));
                }
            }
            let (i, needs_reverse) = candidate?;
            let mut next = pool.swap_remove(i);
            if needs_reverse {
                next = reverse_curve(&next);
            }
            cur_end = ends(&next).1;
            lp.push(next);
        }
    }
    Some(loops)
}

/// The curve-loop sibling of `loops_to_regions`: flatten each loop only to
/// *classify* it (containment depth, orientation), then emit the original
/// curves. Even-depth loops become region outers (oriented CCW), odd-depth
/// loops become holes of their innermost container (oriented CW).
fn curve_loops_to_regions(loops: Vec<Vec<Curve>>, flat_tol: f64) -> Vec<Region> {
    let loops: Vec<Vec<Curve>> = loops.into_iter().filter(|l| !l.is_empty()).collect();
    if loops.is_empty() {
        return Vec::new();
    }
    let rings: Vec<Vec<Point2d>> = loops
        .iter()
        .map(|l| flatten_curve_loop(l, flat_tol))
        .collect();
    let n = loops.len();
    let reps: Vec<(f64, f64)> = rings.iter().map(|r| interior_point(r)).collect();
    let areas: Vec<f64> = rings.iter().map(|r| poly_area(r)).collect();

    let mut depth = vec![0usize; n];
    let mut parent = vec![usize::MAX; n];
    for i in 0..n {
        for j in 0..n {
            if i == j || rings[j].len() < 3 || !point_in_ring(reps[i], &rings[j]) {
                continue;
            }
            depth[i] += 1;
            if parent[i] == usize::MAX || areas[j] < areas[parent[i]] {
                parent[i] = j;
            }
        }
    }

    let orient = |lp: &[Curve], ring: &[Point2d], ccw: bool| -> Vec<Curve> {
        if (signed_poly_area(ring) >= 0.0) == ccw {
            lp.to_vec()
        } else {
            lp.iter().rev().map(reverse_curve).collect()
        }
    };

    let mut outer_slot = vec![usize::MAX; n];
    let mut outers: Vec<Vec<Curve>> = Vec::new();
    let mut holes_for: Vec<Vec<Vec<Curve>>> = Vec::new();
    for i in 0..n {
        if depth[i].is_multiple_of(2) {
            outer_slot[i] = outers.len();
            outers.push(orient(&loops[i], &rings[i], true));
            holes_for.push(Vec::new());
        }
    }
    for i in 0..n {
        if !depth[i].is_multiple_of(2) {
            let slot = if parent[i] != usize::MAX {
                outer_slot[parent[i]]
            } else {
                usize::MAX
            };
            if slot == usize::MAX {
                continue;
            }
            holes_for[slot].push(orient(&loops[i], &rings[i], false));
        }
    }
    outers
        .into_iter()
        .zip(holes_for)
        .map(|(o, holes)| Region::with_holes(o, holes))
        .collect()
}

fn flatten_curve_loop(lp: &[Curve], tol: f64) -> Vec<Point2d> {
    let mut ring: Vec<Point2d> = Vec::new();
    for c in lp {
        for q in tessellate_curve(c, tol) {
            if ring.last().is_none_or(|l| l.dist_sq(&q) > 1e-18) {
                ring.push(q);
            }
        }
    }
    if ring.len() >= 2 && ring[0].dist_sq(ring.last().unwrap()) < 1e-18 {
        ring.pop();
    }
    ring
}

fn loops_diag(loops: &[Vec<Curve>]) -> f64 {
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for c in loops.iter().flatten() {
        let bb = c.bounding_box();
        xmin = xmin.min(bb.min.x);
        xmax = xmax.max(bb.max.x);
        ymin = ymin.min(bb.min.y);
        ymax = ymax.max(bb.max.y);
    }
    if xmin > xmax {
        return 0.0;
    }
    (xmax - xmin).hypot(ymax - ymin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_geometry::{CircularArc, LineSeg};

    fn circle(cx: f64, cy: f64, r: f64) -> Region {
        Region::new(vec![Curve::Arc(CircularArc::new(
            Point2d::from_f64(cx, cy),
            r,
            0.0,
            std::f64::consts::TAU,
        ))])
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
        let p = Point2d::from_f64;
        Region::new(vec![
            Curve::Line(LineSeg::from_endpoints(p(x0, y0), p(x1, y0))),
            Curve::Line(LineSeg::from_endpoints(p(x1, y0), p(x1, y1))),
            Curve::Line(LineSeg::from_endpoints(p(x1, y1), p(x0, y1))),
            Curve::Line(LineSeg::from_endpoints(p(x0, y1), p(x0, y0))),
        ])
    }

    fn covered(regions: &[Region], x: f64, y: f64) -> bool {
        regions.iter().any(|r| r.contains_point(x, y))
    }

    fn all_segments(regions: &[Region]) -> Vec<&Curve> {
        regions
            .iter()
            .flat_map(|r| r.outer.iter().chain(r.holes.iter().flatten()))
            .collect()
    }

    #[test]
    fn union_of_overlapping_circles_keeps_arcs() {
        let out = clip_curved(
            &circle(0.0, 0.0, 4.0),
            &circle(5.0, 0.0, 4.0),
            BoolOp::Union,
        )
        .expect("transversal circles must take the curved path");
        assert_eq!(out.len(), 1, "one merged region");
        let segs = all_segments(&out);
        assert!(
            segs.iter().all(|c| matches!(c, Curve::Arc(_))),
            "a union of circles is bounded by arcs, not polylines"
        );
        assert!(segs.len() <= 4, "two arc pieces (plus seam splits) at most");
        assert!(covered(&out, 0.0, 0.0) && covered(&out, 5.0, 0.0));
        assert!(covered(&out, 2.5, 0.0), "the lens is inside");
        assert!(!covered(&out, -5.0, 0.0) && !covered(&out, 10.0, 0.0));
        // The rim is still exactly round: winding flips within 1e-9 of r=4.
        assert!(covered(&out, 0.0, 4.0 - 1e-9));
        assert!(!covered(&out, 0.0, 4.0 + 1e-9));
    }

    #[test]
    fn circle_minus_square_keeps_the_arc() {
        let out = clip_curved(
            &circle(0.0, 0.0, 5.0),
            &square(0.0, -6.0, 6.0, 6.0),
            BoolOp::Difference,
        )
        .expect("transversal case must take the curved path");
        assert_eq!(out.len(), 1);
        let segs = all_segments(&out);
        assert!(
            segs.iter().any(|c| matches!(c, Curve::Arc(_))),
            "the round side survives as an arc"
        );
        assert!(
            segs.iter().any(|c| matches!(c, Curve::Line(_))),
            "the cut side is a straight edge"
        );
        assert!(covered(&out, -2.5, 0.0), "left half-disc stays");
        assert!(!covered(&out, 2.5, 0.0), "right half is cut away");
    }

    #[test]
    fn disjoint_circles_union_is_exact_passthrough() {
        let out = clip_curved(
            &circle(0.0, 0.0, 2.0),
            &circle(10.0, 0.0, 3.0),
            BoolOp::Union,
        )
        .expect("disjoint inputs are trivially non-degenerate");
        assert_eq!(out.len(), 2);
        for r in &out {
            assert_eq!(r.outer.len(), 1, "original single-arc boundary kept");
            assert!(matches!(r.outer[0], Curve::Arc(_)));
        }
    }

    #[test]
    fn coincident_boundaries_fall_back() {
        // Identical squares share every boundary point — sidedness cannot
        // decide anything and the curved path must decline.
        let a = square(0.0, 0.0, 4.0, 4.0);
        let b = square(0.0, 0.0, 4.0, 4.0);
        assert!(clip_curved(&a, &b, BoolOp::Union).is_none());
    }

    #[test]
    fn intersection_of_disjoint_is_empty_not_fallback() {
        let out = clip_curved(
            &circle(0.0, 0.0, 2.0),
            &circle(10.0, 0.0, 3.0),
            BoolOp::Intersection,
        )
        .expect("disjoint inputs classify cleanly");
        assert!(out.is_empty());
    }

    #[test]
    fn donut_intersection_with_circle_keeps_curves() {
        // Donut: outer circle r=5 with a square hole; clip with a circle
        // overlapping rim and hole.
        let hole = square(-2.0, -2.0, 2.0, 2.0).outer;
        let donut = Region::with_holes(circle(0.0, 0.0, 5.0).outer, vec![hole]);
        let cutter = circle(4.0, 0.0, 2.5);
        let out = clip_curved(&donut, &cutter, BoolOp::Intersection)
            .expect("transversal donut case must take the curved path");
        assert!(covered(&out, 4.0, 1.0), "ring material inside the cutter");
        assert!(!covered(&out, 1.9, 0.0), "the hole stays empty");
        assert!(!covered(&out, 0.0, 4.0), "ring material outside the cutter");
        assert!(
            all_segments(&out)
                .iter()
                .any(|c| matches!(c, Curve::Arc(_))),
            "curved pieces survive"
        );
    }
}

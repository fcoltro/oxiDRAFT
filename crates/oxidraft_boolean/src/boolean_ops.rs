use crate::clip::{BoolOp, clip};
use crate::region::Region;
use crate::weld::{WELD_TOL, weld_region};
use oxidraft_geometry::{Curve, CurveSegment, LineSeg, Point2d, tessellate_curve};

// Boolean results are a *set* of disjoint regions: a union of separated
// shapes, or xor's two lobes, cannot be modelled by a single outer boundary.
// The old single-Region API silently dropped every piece whose representative
// point fell outside the largest loop.
//
// Each op first tries the curve-preserving trim-and-stitch path (arcs stay
// arcs); it declines degenerate configurations — coincident boundaries,
// tangencies, vertex-only contacts — and those fall back to the tessellating
// clipper, which resolves them by perturbation.

pub fn union(a: &Region, b: &Region) -> Vec<Region> {
    crate::curved::clip_curved(a, b, BoolOp::Union)
        .unwrap_or_else(|| loops_to_regions(clip_regions(a, b, BoolOp::Union)))
}

pub fn intersection(a: &Region, b: &Region) -> Vec<Region> {
    crate::curved::clip_curved(a, b, BoolOp::Intersection)
        .unwrap_or_else(|| loops_to_regions(clip_regions(a, b, BoolOp::Intersection)))
}

pub fn difference(a: &Region, b: &Region) -> Vec<Region> {
    crate::curved::clip_curved(a, b, BoolOp::Difference)
        .unwrap_or_else(|| loops_to_regions(clip_regions(a, b, BoolOp::Difference)))
}

pub fn xor(a: &Region, b: &Region) -> Vec<Region> {
    let mut out = difference(a, b);
    out.extend(difference(b, a));
    out
}

fn clip_regions(a: &Region, b: &Region, op: BoolOp) -> Vec<Vec<Point2d>> {
    let a = weld_region(a, WELD_TOL);
    let b = weld_region(b, WELD_TOL);
    let pa = flatten_region_rings(&a);
    let pb = flatten_region_rings(&b);
    if pa.is_empty() || pb.is_empty() {
        return Vec::new();
    }
    clip(&pa, &pb, op)
}

fn flatten_region_rings(r: &Region) -> Vec<Vec<Point2d>> {
    use rayon::prelude::*;
    let outer = flatten_loop(&r.outer);
    if outer.len() < 3 {
        return Vec::new();
    }
    let mut rings = vec![outer];
    // Holes flatten independently, and rayon's collect keeps their order.
    rings.extend(
        r.holes
            .par_iter()
            .map(|h| flatten_loop(h))
            .filter(|p| p.len() >= 3)
            .collect::<Vec<_>>(),
    );
    rings
}

fn flatten_loop(curves: &[Curve]) -> Vec<Point2d> {
    let tol = (loop_diag(curves) * 1e-3).max(1e-6);
    let mut pts: Vec<Point2d> = Vec::new();
    for c in curves {
        for q in tessellate_curve(c, tol) {
            if pts.last().is_none_or(|l| dist2(l, &q) > 1e-18) {
                pts.push(q);
            }
        }
    }
    if pts.len() >= 2 && dist2(&pts[0], pts.last().unwrap()) < 1e-18 {
        pts.pop();
    }
    pts
}

/// Groups traced loops into disjoint regions by containment depth: a loop
/// contained by an even number of other loops is a component's outer
/// boundary; an odd-depth loop is a hole of its innermost container.
fn loops_to_regions(loops: Vec<Vec<Point2d>>) -> Vec<Region> {
    let loops: Vec<Vec<Point2d>> = loops.into_iter().filter(|l| l.len() >= 3).collect();
    if loops.is_empty() {
        return Vec::new();
    }
    let n = loops.len();
    let reps: Vec<(f64, f64)> = loops.iter().map(|l| interior_point(l)).collect();
    let areas: Vec<f64> = loops.iter().map(|l| poly_area(l)).collect();

    let mut depth = vec![0usize; n];
    let mut parent = vec![usize::MAX; n];
    for i in 0..n {
        for j in 0..n {
            if i == j || !point_in_ring(reps[i], &loops[j]) {
                continue;
            }
            depth[i] += 1;
            if parent[i] == usize::MAX || areas[j] < areas[parent[i]] {
                parent[i] = j;
            }
        }
    }

    let mut outer_slot = vec![usize::MAX; n];
    let mut outers: Vec<Vec<Point2d>> = Vec::new();
    let mut holes_for: Vec<Vec<Vec<Curve>>> = Vec::new();
    for i in 0..n {
        if depth[i].is_multiple_of(2) {
            outer_slot[i] = outers.len();
            let mut l = loops[i].clone();
            if signed_poly_area(&l) < 0.0 {
                l.reverse();
            }
            outers.push(l);
            holes_for.push(Vec::new());
        }
    }
    for i in 0..n {
        if depth[i] % 2 == 1 {
            // The innermost container of an odd-depth loop is even-depth in a
            // proper nesting; a malformed loop set just drops the hole.
            let slot = if parent[i] != usize::MAX {
                outer_slot[parent[i]]
            } else {
                usize::MAX
            };
            if slot == usize::MAX {
                continue;
            }
            let mut l = loops[i].clone();
            if signed_poly_area(&l) > 0.0 {
                l.reverse();
            }
            holes_for[slot].push(poly_to_lines(&l));
        }
    }
    outers
        .into_iter()
        .zip(holes_for)
        .map(|(o, holes)| Region::with_holes(poly_to_lines(&o), holes))
        .collect()
}

/// A point strictly inside the loop: the midpoint of its longest edge nudged
/// toward the interior. Loop *vertices* are the traced intersection points
/// and sit on several result loops at once, so they can't classify anything.
pub(crate) fn interior_point(l: &[Point2d]) -> (f64, f64) {
    let n = l.len();
    let mut bi = 0;
    let mut best = -1.0;
    for i in 0..n {
        let d = l[i].dist_sq(&l[(i + 1) % n]);
        if d > best {
            best = d;
            bi = i;
        }
    }
    let a = l[bi];
    let b = l[(bi + 1) % n];
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy).max(1e-300);
    // The left normal of a CCW loop points inward.
    let sgn = if signed_poly_area(l) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in l {
        xmin = xmin.min(p.x);
        xmax = xmax.max(p.x);
        ymin = ymin.min(p.y);
        ymax = ymax.max(p.y);
    }
    let eps = ((xmax - xmin).hypot(ymax - ymin)).max(1e-12) * 1e-9;
    (
        0.5 * (a.x + b.x) - dy / len * sgn * eps,
        0.5 * (a.y + b.y) + dx / len * sgn * eps,
    )
}

pub(crate) fn point_in_ring((px, py): (f64, f64), ring: &[Point2d]) -> bool {
    let n = ring.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (ring[i].x, ring[i].y);
        let (xj, yj) = (ring[j].x, ring[j].y);
        if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn poly_to_lines(pts: &[Point2d]) -> Vec<Curve> {
    (0..pts.len())
        .map(|i| Curve::Line(LineSeg::from_endpoints(pts[i], pts[(i + 1) % pts.len()])))
        .collect()
}

pub(crate) fn poly_area(pts: &[Point2d]) -> f64 {
    signed_poly_area(pts).abs()
}

pub(crate) fn signed_poly_area(pts: &[Point2d]) -> f64 {
    let mut a = 0.0;
    for i in 0..pts.len() {
        let (x0, y0) = pts[i].to_f64();
        let (x1, y1) = pts[(i + 1) % pts.len()].to_f64();
        a += x0 * y1 - x1 * y0;
    }
    a / 2.0
}

fn loop_diag(curves: &[Curve]) -> f64 {
    let mut min = (f64::MAX, f64::MAX);
    let mut max = (f64::MIN, f64::MIN);
    for c in curves {
        let bb = c.bounding_box();
        min.0 = min.0.min(bb.min.x);
        min.1 = min.1.min(bb.min.y);
        max.0 = max.0.max(bb.max.x);
        max.1 = max.1.max(bb.max.y);
    }
    ((max.0 - min.0).powi(2) + (max.1 - min.1).powi(2)).sqrt()
}

fn dist2(a: &Point2d, b: &Point2d) -> f64 {
    let (ax, ay) = a.to_f64();
    let (bx, by) = b.to_f64();
    (ax - bx).powi(2) + (ay - by).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_geometry::{CubicBezier, LineSeg, Point2d};

    fn square(x0: i64, y0: i64, x1: i64, y1: i64) -> Region {
        Region::new(vec![
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(x0, y0),
                Point2d::from_i64(x1, y0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(x1, y0),
                Point2d::from_i64(x1, y1),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(x1, y1),
                Point2d::from_i64(x0, y1),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(x0, y1),
                Point2d::from_i64(x0, y0),
            )),
        ])
    }

    fn covered(regions: &[Region], x: f64, y: f64) -> bool {
        regions.iter().any(|r| r.contains_point(x, y))
    }

    #[test]
    fn union_of_disjoint_squares_keeps_both() {
        let u = union(&square(0, 0, 2, 2), &square(5, 5, 7, 7));
        assert_eq!(u.len(), 2, "disjoint union is two separate regions");
        assert!(covered(&u, 1.0, 1.0), "first square survives");
        assert!(covered(&u, 6.0, 6.0), "second square survives");
        assert!(!covered(&u, 3.5, 3.5), "gap between them stays empty");
    }

    #[test]
    fn union_of_edge_adjacent_squares_covers_both() {
        // Two squares sharing the full edge x=4: the classic degenerate CAD
        // input that Greinerâ€“Hormann misclassifies without hardening.
        let u = union(&square(0, 0, 4, 4), &square(4, 0, 8, 4));
        assert!(covered(&u, 2.0, 2.0), "left square material");
        assert!(covered(&u, 6.0, 2.0), "right square material");
        assert!(!covered(&u, 9.0, 2.0), "right of both stays empty");
        assert!(!covered(&u, 2.0, 5.0), "above stays empty");
    }

    #[test]
    fn partial_shared_edge_union_and_difference() {
        // B sits on top of A sharing part of A's top edge (a T-contact).
        let a = square(0, 0, 4, 4);
        let b = square(1, 4, 3, 6);
        let u = union(&a, &b);
        assert!(covered(&u, 2.0, 2.0), "A material");
        assert!(covered(&u, 2.0, 5.0), "B material");
        assert!(!covered(&u, 0.5, 5.0), "beside B stays empty");
        let d = difference(&a, &b);
        assert!(covered(&d, 2.0, 2.0), "A keeps its interior");
        assert!(!covered(&d, 2.0, 5.0), "B region was never in A");
    }

    #[test]
    fn vertex_on_edge_crossing_is_handled() {
        // A diamond whose left/right vertices lie exactly ON A's top edge
        // while its body crosses it: the vertex-on-edge crossing case.
        let a = square(0, 0, 4, 4);
        let diamond = Region::new(vec![
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(2, 3),
                Point2d::from_i64(3, 4),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(3, 4),
                Point2d::from_i64(2, 5),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(2, 5),
                Point2d::from_i64(1, 4),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_i64(1, 4),
                Point2d::from_i64(2, 3),
            )),
        ]);
        let u = union(&a, &diamond);
        assert!(covered(&u, 2.0, 2.0), "square interior");
        assert!(covered(&u, 2.0, 4.5), "diamond tip above the square");
        assert!(!covered(&u, 3.5, 4.5), "outside both");
        let d = difference(&a, &diamond);
        assert!(covered(&d, 0.5, 0.5), "square keeps its far corner");
        assert!(!covered(&d, 2.0, 3.7), "diamond bite is removed");
    }

    #[test]
    fn xor_keeps_both_lobes() {
        let x = xor(&square(0, 0, 3, 3), &square(2, 2, 5, 5));
        assert!(covered(&x, 1.0, 1.0), "A-only lobe survives");
        assert!(covered(&x, 4.0, 4.0), "B-only lobe survives");
        assert!(!covered(&x, 2.5, 2.5), "the overlap is excluded");
    }

    #[test]
    fn difference_excludes_overlap() {
        let d = difference(&square(0, 0, 4, 4), &square(2, 2, 6, 6));
        assert!(covered(&d, 1.0, 1.0), "A-only region stays");
        assert!(!covered(&d, 3.0, 3.0), "the overlap corner is removed");
        assert!(!covered(&d, 5.0, 5.0), "B-only was never in A");
    }

    #[test]
    fn intersection_keeps_only_overlap() {
        let i = intersection(&square(0, 0, 3, 3), &square(2, 2, 5, 5));
        assert!(covered(&i, 2.5, 2.5), "overlap is inside");
        assert!(!covered(&i, 1.0, 1.0), "A-only excluded");
        assert!(!covered(&i, 4.0, 4.0), "B-only excluded");
    }

    #[test]
    fn union_covers_both() {
        let u = union(&square(0, 0, 3, 3), &square(2, 2, 5, 5));
        assert!(covered(&u, 1.0, 1.0), "deep in A");
        assert!(covered(&u, 4.0, 4.0), "deep in B");
        assert!(covered(&u, 2.5, 2.5), "the overlap");
        assert!(!covered(&u, 4.0, 1.0), "between the squares, outside both");
        assert!(!covered(&u, 10.0, 10.0), "far outside");
    }

    #[test]
    fn boolean_welds_open_input_boundary() {
        let g = 1e-9;
        let a = Region::new(vec![
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(0.0, 0.0),
                Point2d::from_f64(4.0, 0.0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(4.0, 0.0),
                Point2d::from_f64(4.0, 4.0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(4.0, 4.0),
                Point2d::from_f64(0.0, 4.0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(g, 4.0),
                Point2d::from_f64(g, g),
            )),
        ]);
        let d = difference(&a, &square(2, 2, 6, 6));
        assert!(
            covered(&d, 1.0, 1.0),
            "welded Aâˆ’B keeps the A-only region"
        );
        assert!(!covered(&d, 3.0, 3.0), "the overlap corner is removed");
    }

    #[test]
    fn boolean_over_bezier_boundary_is_fast() {
        let a = Region::new(vec![
            Curve::Bezier(CubicBezier::new(
                Point2d::from_f64(0.0, 0.0),
                Point2d::from_f64(1.0, 3.0),
                Point2d::from_f64(3.0, -3.0),
                Point2d::from_f64(4.0, 0.0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(4.0, 0.0),
                Point2d::from_f64(4.0, 4.0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(4.0, 4.0),
                Point2d::from_f64(0.0, 4.0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(0.0, 4.0),
                Point2d::from_f64(0.0, 0.0),
            )),
        ]);
        let b = square(1, 1, 3, 5);
        let t = std::time::Instant::now();
        let _ = difference(&a, &b);
        let _ = union(&a, &b);
        let _ = intersection(&a, &b);
        assert!(
            t.elapsed().as_millis() < 500,
            "boolean over BÃ©zier too slow: {:?}",
            t.elapsed()
        );
    }

    fn ngon(cx: f64, cy: f64, r: f64, n: usize) -> Region {
        let pts: Vec<Point2d> = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                Point2d::from_f64(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        let segs = (0..n)
            .map(|i| Curve::Line(LineSeg::from_endpoints(pts[i], pts[(i + 1) % n])))
            .collect();
        Region::new(segs)
    }

    #[test]
    fn union_of_overlapping_circles_classifies_correctly() {
        let u = union(&ngon(7.0, 6.0, 4.0, 48), &ngon(12.0, 6.0, 4.0, 48));
        assert!(!u.is_empty(), "union must produce a boundary");
        assert!(
            covered(&u, 7.0, 6.0),
            "center of circle 1 is inside the union"
        );
        assert!(
            covered(&u, 12.0, 6.0),
            "center of circle 2 is inside the union"
        );
        assert!(covered(&u, 9.5, 6.0), "the lens is inside the union");
        assert!(!covered(&u, 0.0, 6.0), "far-left point is outside");
        assert!(!covered(&u, 20.0, 6.0), "far-right point is outside");
        assert!(!covered(&u, 9.5, 20.0), "far-above point is outside");
    }

    #[test]
    fn xor_excludes_overlap() {
        let x = xor(&square(0, 0, 3, 3), &square(2, 2, 5, 5));
        assert!(!x.is_empty(), "xor of overlapping squares is non-empty");
        assert!(!covered(&x, 2.5, 2.5), "the overlap is excluded from xor");
        assert!(!covered(&x, 10.0, 10.0), "far outside is excluded");
    }

    fn donut(outer_bounds: (i64, i64, i64, i64), hole_bounds: (i64, i64, i64, i64)) -> Region {
        let (x0, y0, x1, y1) = outer_bounds;
        let (hx0, hy0, hx1, hy1) = hole_bounds;
        let outer = square(x0, y0, x1, y1).outer;
        let hole = square(hx0, hy0, hx1, hy1).outer;
        Region::with_holes(outer, vec![hole])
    }

    #[test]
    fn donut_intersect_solid_circle_is_crescent_region() {
        let d = donut((0, 0, 10, 10), (3, 3, 7, 7));
        let c = ngon(7.5, 5.0, 3.0, 64);
        let r = intersection(&d, &c);
        assert!(
            covered(&r, 8.5, 5.0),
            "inside rect, in circle, outside hole"
        );
        assert!(!covered(&r, 5.0, 5.0), "inside the hole is excluded");
        assert!(!covered(&r, 1.0, 1.0), "outside the circle is excluded");
    }

    #[test]
    fn donut_union_solid_square_partially_fills_hole() {
        let d = donut((0, 0, 10, 10), (3, 3, 7, 7));
        let b = square(4, 4, 6, 6);
        let u = union(&d, &b);
        assert!(covered(&u, 5.0, 5.0), "B fills part of A's hole");
        assert!(!covered(&u, 3.5, 5.0), "still a hole where B doesn't reach");
        assert!(covered(&u, 1.0, 1.0), "A's solid material stays");
    }

    #[test]
    fn donut_difference_solid_rectangle_keeps_hole() {
        let d = donut((0, 0, 10, 10), (3, 3, 7, 7));
        let cut = square(8, 8, 9, 9);
        let r = difference(&d, &cut);
        assert!(!covered(&r, 5.0, 5.0), "hole must survive");
        assert!(!covered(&r, 8.5, 8.5), "cut corner removed");
        assert!(covered(&r, 1.0, 1.0), "solid material stays");
    }

    #[test]
    fn donut_xor_solid_circle() {
        let d = donut((0, 0, 10, 10), (3, 3, 7, 7));
        let c = ngon(8.5, 1.5, 1.0, 64);
        let x = xor(&d, &c);
        assert!(
            !covered(&x, 8.5, 1.5),
            "circle fully inside A's solid material is excluded"
        );
        assert!(!covered(&x, 5.0, 5.0), "the original hole stays excluded");
        assert!(
            covered(&x, 1.0, 1.0),
            "donut material away from the circle stays"
        );
    }

    #[test]
    fn solid_circle_fully_inside_donut_hole() {
        let d = donut((0, 0, 10, 10), (3, 3, 7, 7));
        let c = ngon(5.0, 5.0, 1.0, 32);

        let u = union(&d, &c);
        assert!(
            covered(&u, 5.0, 5.0),
            "circle survives as a disjoint island"
        );
        assert!(covered(&u, 1.0, 1.0), "donut solid material stays");
        assert!(!covered(&u, 3.5, 5.0), "the rest of the hole stays empty");

        let i = intersection(&d, &c);
        assert!(
            !covered(&i, 5.0, 5.0),
            "circle in the hole never overlaps donut material"
        );

        let diff = difference(&d, &c);
        assert!(covered(&diff, 1.0, 1.0), "donut is unmodified");
        assert!(!covered(&diff, 5.0, 5.0), "hole is still a hole");
    }
}

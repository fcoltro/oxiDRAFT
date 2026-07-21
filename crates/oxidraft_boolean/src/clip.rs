//! The polygon clipper: a Greiner–Hormann style boolean over tessellated
//! polygon rings, using robust orientation predicates. The fallback path when
//! the curve-preserving boolean declines a degenerate configuration.

use oxidraft_geometry::{Point2d, point_segment_dist_sq};
use robust::{Coord, orient2d};

/// Which boolean operation the clipper performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    /// Keep area in either polygon.
    Union,
    /// Keep area in both polygons.
    Intersection,
    /// Keep subject area not in the clip polygon.
    Difference,
}

#[derive(Clone)]
struct Node {
    x: f64,
    y: f64,
    next: usize,
    prev: usize,
    intersection: bool,
    entry: bool,
    neighbour: usize,
    visited: bool,
}

const NONE: usize = usize::MAX;

/// Clips `subject` polygon rings against `clip_poly` rings under `op`,
/// returning the resulting rings. Rings are lists of vertices; a non-finite
/// vertex makes a ring undefined and it is dropped.
pub fn clip(subject: &[Vec<Point2d>], clip_poly: &[Vec<Point2d>], op: BoolOp) -> Vec<Vec<Point2d>> {
    // A ring with a non-finite vertex has no defined geometry, and NaN makes
    // the entry-marking parity below inconsistent — the traversal in `trace`
    // can then orbit between intersections forever. Drop such rings like the
    // too-short ones.
    let usable =
        |r: &Vec<(f64, f64)>| r.len() >= 3 && r.iter().all(|p| p.0.is_finite() && p.1.is_finite());
    let subj: Vec<Vec<(f64, f64)>> = subject
        .iter()
        .map(|r| r.iter().map(|p| p.to_f64()).collect())
        .filter(usable)
        .collect();
    let mut clp: Vec<Vec<(f64, f64)>> = clip_poly
        .iter()
        .map(|r| r.iter().map(|p| p.to_f64()).collect())
        .filter(usable)
        .collect();
    if subj.is_empty() || clp.is_empty() {
        return Vec::new();
    }

    // Greiner–Hormann cannot handle contact degeneracies (shared edges,
    // vertices exactly on the other boundary — the bread and butter of CAD
    // input). Resolve them by nudging the clip rings a hair: the shift is four
    // orders of magnitude below the curve-flattening tolerance upstream, so
    // the result is identical within pipeline accuracy, but every contact
    // becomes either a proper crossing or a clean separation.
    let diag = rings_diag(&subj).max(rings_diag(&clp));
    let contact_eps_sq = (diag * 1e-9).powi(2);
    for attempt in 0..4 {
        if !has_degenerate_contact(&subj, &clp, contact_eps_sq) {
            break;
        }
        let d = diag * 1e-7 * 3f64.powi(attempt);
        for ring in &mut clp {
            for p in ring.iter_mut() {
                p.0 += d;
                p.1 += d * 0.754_877_666;
            }
        }
    }

    let mut nodes: Vec<Node> = Vec::new();
    let s_starts: Vec<usize> = subj.iter().map(|r| build_ring(&mut nodes, r)).collect();
    let c_starts: Vec<usize> = clp.iter().map(|r| build_ring(&mut nodes, r)).collect();

    let crossings = insert_intersections(&mut nodes, &s_starts, &c_starts);
    if crossings == 0 {
        return no_crossing_result(&subj, &clp, op);
    }

    mark_entries(&mut nodes, &s_starts, &clp, op, true);
    mark_entries(&mut nodes, &c_starts, &subj, op, false);

    trace(&mut nodes)
}

fn build_ring(nodes: &mut Vec<Node>, poly: &[(f64, f64)]) -> usize {
    let base = nodes.len();
    let n = poly.len();
    for (i, &(x, y)) in poly.iter().enumerate() {
        nodes.push(Node {
            x,
            y,
            next: base + (i + 1) % n,
            prev: base + (i + n - 1) % n,
            intersection: false,
            entry: false,
            neighbour: NONE,
            visited: false,
        });
    }
    base
}

fn insert_intersections(nodes: &mut Vec<Node>, s_starts: &[usize], c_starts: &[usize]) -> usize {
    let s_edges: Vec<usize> = s_starts
        .iter()
        .flat_map(|&s| original_edges(nodes, s))
        .collect();
    let c_edges: Vec<usize> = c_starts
        .iter()
        .flat_map(|&c| original_edges(nodes, c))
        .collect();

    struct Hit {
        se: usize,
        ce: usize,
        a_s: f64,
        a_c: f64,
        x: f64,
        y: f64,
    }
    // Finding intersections is read-only over `nodes` and independent per edge
    // pair, and the loops below re-sort hits by parametric position before any
    // node is inserted, so the order of `hits` never affects the result. Only
    // go parallel past a pair-count threshold: typical documents are small and
    // rayon's dispatch overhead would dominate below it.
    const PAR_PAIRS: usize = 4096;
    let nodes_ro: &[Node] = nodes;
    let hits_on_edge = |&si: &usize| {
        let a0 = (nodes_ro[si].x, nodes_ro[si].y);
        let a1 = (nodes_ro[nodes_ro[si].next].x, nodes_ro[nodes_ro[si].next].y);
        c_edges.iter().filter_map(move |&ci| {
            let b0 = (nodes_ro[ci].x, nodes_ro[ci].y);
            let b1 = (nodes_ro[nodes_ro[ci].next].x, nodes_ro[nodes_ro[ci].next].y);
            seg_intersect(a0, a1, b0, b1).map(|(t, u, x, y)| Hit {
                se: si,
                ce: ci,
                a_s: t,
                a_c: u,
                x,
                y,
            })
        })
    };
    let hits: Vec<Hit> = if s_edges.len().saturating_mul(c_edges.len()) >= PAR_PAIRS {
        use rayon::prelude::*;
        s_edges.par_iter().flat_map_iter(hits_on_edge).collect()
    } else {
        s_edges.iter().flat_map(hits_on_edge).collect()
    };
    if hits.is_empty() {
        return 0;
    }

    let mut s_node = vec![NONE; hits.len()];
    let mut c_node = vec![NONE; hits.len()];

    for &se in &s_edges {
        let mut grp: Vec<usize> = (0..hits.len()).filter(|&h| hits[h].se == se).collect();
        grp.sort_by(|&a, &b| hits[a].a_s.total_cmp(&hits[b].a_s));
        let mut prev = se;
        let after = nodes[se].next;
        for &h in &grp {
            let idx = nodes.len();
            nodes.push(Node {
                x: hits[h].x,
                y: hits[h].y,
                next: after,
                prev,
                intersection: true,
                entry: false,
                neighbour: NONE,
                visited: false,
            });
            nodes[prev].next = idx;
            prev = idx;
            s_node[h] = idx;
        }
        nodes[after].prev = prev;
    }

    for &ce in &c_edges {
        let mut grp: Vec<usize> = (0..hits.len()).filter(|&h| hits[h].ce == ce).collect();
        grp.sort_by(|&a, &b| hits[a].a_c.total_cmp(&hits[b].a_c));
        let mut prev = ce;
        let after = nodes[ce].next;
        for &h in &grp {
            let idx = nodes.len();
            nodes.push(Node {
                x: hits[h].x,
                y: hits[h].y,
                next: after,
                prev,
                intersection: true,
                entry: false,
                neighbour: NONE,
                visited: false,
            });
            nodes[prev].next = idx;
            prev = idx;
            c_node[h] = idx;
        }
        nodes[after].prev = prev;
    }

    for h in 0..hits.len() {
        let (sn, cn) = (s_node[h], c_node[h]);
        nodes[sn].neighbour = cn;
        nodes[cn].neighbour = sn;
    }
    hits.len()
}

fn original_edges(nodes: &[Node], start: usize) -> Vec<usize> {
    let mut out = vec![start];
    let mut cur = nodes[start].next;
    while cur != start {
        out.push(cur);
        cur = nodes[cur].next;
    }
    out
}

fn point_in_region(x: f64, y: f64, rings: &[Vec<(f64, f64)>]) -> bool {
    rings.iter().filter(|r| point_in_poly(x, y, r)).count() % 2 == 1
}

fn mark_entries(
    nodes: &mut [Node],
    starts: &[usize],
    other: &[Vec<(f64, f64)>],
    op: BoolOp,
    is_subject: bool,
) {
    let flip = match op {
        BoolOp::Intersection => false,
        BoolOp::Union => true,
        BoolOp::Difference => is_subject,
    };
    for &start in starts {
        let mut inside = point_in_region(nodes[start].x, nodes[start].y, other);
        let mut cur = start;
        loop {
            if nodes[cur].intersection {
                nodes[cur].entry = (!inside) ^ flip;
                inside = !inside;
            }
            cur = nodes[cur].next;
            if cur == start {
                break;
            }
        }
    }
}

fn trace(nodes: &mut [Node]) -> Vec<Vec<Point2d>> {
    let mut result = Vec::new();
    // A valid traced loop visits each node at most once, so this is far above
    // any legitimate trace. Entry marking left inconsistent by a degenerate
    // contact the nudge loop failed to clear can otherwise orbit between
    // intersections without ever returning to `start`; a runaway loop is
    // geometric garbage, so it is discarded, and the visited flags it set
    // keep the outer scan making progress.
    let budget = nodes.len().saturating_mul(4).max(16);
    while let Some(start) = (0..nodes.len()).find(|&i| nodes[i].intersection && !nodes[i].visited) {
        let mut loop_pts: Vec<Point2d> = Vec::new();
        let mut cur = start;
        let mut steps = 0usize;
        'one_loop: loop {
            nodes[cur].visited = true;
            let nb = nodes[cur].neighbour;
            if nb != NONE {
                nodes[nb].visited = true;
            }
            let forward = nodes[cur].entry;
            loop {
                cur = if forward {
                    nodes[cur].next
                } else {
                    nodes[cur].prev
                };
                steps += 1;
                if steps > budget {
                    loop_pts.clear();
                    break 'one_loop;
                }
                loop_pts.push(Point2d::from_f64(nodes[cur].x, nodes[cur].y));
                if nodes[cur].intersection {
                    break;
                }
            }
            cur = nodes[cur].neighbour;
            if cur == NONE {
                break;
            }
            if cur == start {
                break;
            }
        }
        if loop_pts.len() >= 3 {
            result.push(loop_pts);
        }
    }
    result
}

fn no_crossing_result(
    subj: &[Vec<(f64, f64)>],
    clp: &[Vec<(f64, f64)>],
    op: BoolOp,
) -> Vec<Vec<Point2d>> {
    let to_pts = |rings: &[Vec<(f64, f64)>]| -> Vec<Vec<Point2d>> {
        rings
            .iter()
            .map(|r| r.iter().map(|&(x, y)| Point2d::from_f64(x, y)).collect())
            .collect()
    };
    let s_in_c = point_in_region(subj[0][0].0, subj[0][0].1, clp);
    let c_in_s = point_in_region(clp[0][0].0, clp[0][0].1, subj);
    match op {
        BoolOp::Union => {
            if s_in_c {
                to_pts(clp)
            } else if c_in_s {
                to_pts(subj)
            } else {
                let mut out = to_pts(subj);
                out.extend(to_pts(clp));
                out
            }
        }
        BoolOp::Intersection => {
            if s_in_c {
                to_pts(subj)
            } else if c_in_s {
                to_pts(clp)
            } else {
                Vec::new()
            }
        }
        BoolOp::Difference => {
            if s_in_c {
                Vec::new()
            } else if c_in_s {
                let mut out = to_pts(subj);
                out.extend(to_pts(clp));
                out
            } else {
                to_pts(subj)
            }
        }
    }
}

fn rings_diag(rings: &[Vec<(f64, f64)>]) -> f64 {
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for r in rings {
        for &(x, y) in r {
            xmin = xmin.min(x);
            xmax = xmax.max(x);
            ymin = ymin.min(y);
            ymax = ymax.max(y);
        }
    }
    if xmin > xmax {
        return 1.0;
    }
    ((xmax - xmin).powi(2) + (ymax - ymin).powi(2))
        .sqrt()
        .max(1e-12)
}

/// True when any vertex of one polygon set sits (within tolerance) on the
/// boundary of the other — the configurations Greiner–Hormann misclassifies.
fn has_degenerate_contact(a: &[Vec<(f64, f64)>], b: &[Vec<(f64, f64)>], eps_sq: f64) -> bool {
    let vertex_on_boundary = |verts: &[Vec<(f64, f64)>], edges: &[Vec<(f64, f64)>]| {
        verts.iter().flatten().any(|&v| {
            edges.iter().any(|ring| {
                let n = ring.len();
                (0..n).any(|i| point_segment_dist_sq(v, ring[i], ring[(i + 1) % n]) < eps_sq)
            })
        })
    };
    vertex_on_boundary(a, b) || vertex_on_boundary(b, a)
}

fn seg_intersect(
    a0: (f64, f64),
    a1: (f64, f64),
    b0: (f64, f64),
    b1: (f64, f64),
) -> Option<(f64, f64, f64, f64)> {
    // Exact proper-crossing test: each segment's endpoints must lie strictly
    // on opposite sides of the other segment. `orient2d` is adaptive-precision
    // so near-degenerate long/thin configurations classify correctly; contact
    // degeneracies (an orientation of exactly zero) were already resolved by
    // the perturbation pass and are rejected here.
    let c = |p: (f64, f64)| Coord { x: p.0, y: p.1 };
    let d1 = orient2d(c(b0), c(b1), c(a0));
    let d2 = orient2d(c(b0), c(b1), c(a1));
    let d3 = orient2d(c(a0), c(a1), c(b0));
    let d4 = orient2d(c(a0), c(a1), c(b1));
    let strictly_opposite = |x: f64, y: f64| (x > 0.0 && y < 0.0) || (x < 0.0 && y > 0.0);
    if !strictly_opposite(d1, d2) || !strictly_opposite(d3, d4) {
        return None;
    }
    let r = (a1.0 - a0.0, a1.1 - a0.1);
    let s = (b1.0 - b0.0, b1.1 - b0.1);
    let denom = r.0 * s.1 - r.1 * s.0;
    if denom.abs() < f64::MIN_POSITIVE {
        return None;
    }
    let qp = (b0.0 - a0.0, b0.1 - a0.1);
    let t = ((qp.0 * s.1 - qp.1 * s.0) / denom).clamp(0.0, 1.0);
    let u = ((qp.0 * r.1 - qp.1 * r.0) / denom).clamp(0.0, 1.0);
    Some((t, u, a0.0 + r.0 * t, a0.1 + r.1 * t))
}

fn point_in_poly(x: f64, y: f64, poly: &[(f64, f64)]) -> bool {
    let n = poly.len();
    if n == 0 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly(pts: &[(f64, f64)]) -> Vec<Point2d> {
        pts.iter().map(|&(x, y)| Point2d::from_f64(x, y)).collect()
    }

    fn ngon(cx: f64, cy: f64, r: f64, n: usize) -> Vec<Point2d> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                Point2d::from_f64(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }

    fn loops_contain(loops: &[Vec<Point2d>], x: f64, y: f64) -> bool {
        let mut c = 0;
        for l in loops {
            let p: Vec<(f64, f64)> = l.iter().map(|q| q.to_f64()).collect();
            if point_in_poly(x, y, &p) {
                c += 1;
            }
        }
        c % 2 == 1
    }

    #[test]
    fn union_of_overlapping_squares() {
        let a = poly(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        let b = poly(&[(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)]);
        let r = clip(&[a], &[b], BoolOp::Union);
        assert!(!r.is_empty());
        assert!(loops_contain(&r, 1.0, 1.0), "deep in A");
        assert!(loops_contain(&r, 5.0, 5.0), "deep in B");
        assert!(loops_contain(&r, 3.0, 3.0), "in the overlap");
        assert!(!loops_contain(&r, 5.0, 1.0), "outside both");
        assert!(!loops_contain(&r, 10.0, 10.0), "far outside");
    }

    #[test]
    fn intersection_of_overlapping_squares() {
        let a = poly(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        let b = poly(&[(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)]);
        let r = clip(&[a], &[b], BoolOp::Intersection);
        assert!(loops_contain(&r, 3.0, 3.0), "overlap is inside");
        assert!(!loops_contain(&r, 1.0, 1.0), "A-only is excluded");
        assert!(!loops_contain(&r, 5.0, 5.0), "B-only is excluded");
    }

    #[test]
    fn difference_of_overlapping_squares() {
        let a = poly(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        let b = poly(&[(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)]);
        let r = clip(&[a], &[b], BoolOp::Difference);
        assert!(loops_contain(&r, 1.0, 1.0), "A-only stays");
        assert!(!loops_contain(&r, 3.0, 3.0), "overlap removed");
        assert!(!loops_contain(&r, 5.0, 5.0), "B-only never in A");
    }

    #[test]
    fn union_of_overlapping_circles_is_a_single_clean_region() {
        let r = clip(
            &[ngon(7.0, 6.0, 4.0, 64)],
            &[ngon(12.0, 6.0, 4.0, 64)],
            BoolOp::Union,
        );
        assert!(!r.is_empty(), "union must produce a boundary");
        assert!(loops_contain(&r, 7.0, 6.0), "center of circle 1");
        assert!(loops_contain(&r, 12.0, 6.0), "center of circle 2");
        assert!(loops_contain(&r, 9.5, 6.0), "the lens");
        assert!(!loops_contain(&r, 0.0, 6.0), "far left outside");
        assert!(!loops_contain(&r, 20.0, 6.0), "far right outside");
    }

    #[test]
    fn intersection_of_overlapping_circles() {
        let r = clip(
            &[ngon(7.0, 6.0, 4.0, 64)],
            &[ngon(12.0, 6.0, 4.0, 64)],
            BoolOp::Intersection,
        );
        assert!(
            loops_contain(&r, 9.5, 6.0),
            "lens is inside the intersection"
        );
        assert!(
            !loops_contain(&r, 5.0, 6.0),
            "circle-1-only is not in the intersection"
        );
    }

    #[test]
    fn union_of_disjoint_squares_returns_both() {
        let a = poly(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        let b = poly(&[(5.0, 5.0), (6.0, 5.0), (6.0, 6.0), (5.0, 6.0)]);
        let r = clip(&[a], &[b], BoolOp::Union);
        assert_eq!(r.len(), 2, "disjoint union keeps both loops");
        assert!(loops_contain(&r, 0.5, 0.5));
        assert!(loops_contain(&r, 5.5, 5.5));
    }

    #[test]
    fn union_with_nested_square_returns_outer() {
        let outer = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let inner = poly(&[(3.0, 3.0), (6.0, 3.0), (6.0, 6.0), (3.0, 6.0)]);
        let r = clip(&[outer], &[inner], BoolOp::Union);
        assert_eq!(r.len(), 1);
        assert!(loops_contain(&r, 4.5, 4.5), "nested point still filled");
        assert!(loops_contain(&r, 0.5, 0.5), "outer ring filled");
    }

    #[test]
    fn point_in_empty_poly_is_false_not_panic() {
        assert!(!point_in_poly(0.0, 0.0, &[]));
    }

    #[test]
    fn donut_intersect_solid_circle_is_crescent() {
        let outer = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let hole = poly(&[(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)]);
        let circle = ngon(7.5, 5.0, 3.0, 64);
        let r = clip(&[outer, hole], &[circle], BoolOp::Intersection);
        assert!(!r.is_empty(), "crescent must be produced");
        assert!(
            loops_contain(&r, 8.5, 5.0),
            "inside rect, in circle, outside hole"
        );
        assert!(!loops_contain(&r, 5.0, 5.0), "inside the hole is excluded");
        assert!(
            !loops_contain(&r, 1.0, 1.0),
            "outside the circle is excluded"
        );
    }

    #[test]
    fn donut_union_donut_partially_fills_hole() {
        let outer_a = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let hole_a = poly(&[(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)]);
        let outer_b = poly(&[(4.0, 4.0), (4.0, 6.0), (6.0, 6.0), (6.0, 4.0)]);
        let r = clip(&[outer_a, hole_a], &[outer_b], BoolOp::Union);
        assert!(loops_contain(&r, 5.0, 5.0), "B fills part of A's hole");
        assert!(
            !loops_contain(&r, 3.5, 5.0),
            "still a hole where B doesn't reach"
        );
        assert!(loops_contain(&r, 1.0, 1.0), "A's solid material stays");
    }

    #[test]
    fn donut_difference_solid_rectangle_keeps_hole() {
        let outer = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let hole = poly(&[(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)]);
        let cut = poly(&[(8.0, 8.0), (8.0, 9.0), (9.0, 9.0), (9.0, 8.0)]);
        let r = clip(&[outer, hole], &[cut], BoolOp::Difference);
        assert!(!loops_contain(&r, 5.0, 5.0), "hole must survive");
        assert!(!loops_contain(&r, 8.5, 8.5), "cut corner removed");
        assert!(loops_contain(&r, 1.0, 1.0), "solid material stays");
    }

    #[test]
    fn donut_xor_solid_circle() {
        let outer = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let hole = poly(&[(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)]);
        let circle = ngon(7.5, 5.0, 3.0, 64);
        let mut r = clip(
            &[outer.clone(), hole.clone()],
            std::slice::from_ref(&circle),
            BoolOp::Difference,
        );
        r.extend(clip(&[circle], &[outer, hole], BoolOp::Difference));
        assert!(loops_contain(&r, 10.2, 5.0), "circle-only part stays");
        assert!(!loops_contain(&r, 8.5, 5.0), "overlap excluded from xor");
    }

    #[test]
    fn circle_fully_inside_donut_hole_no_crossing_variants() {
        let outer = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let hole = poly(&[(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)]);
        let circle = ngon(5.0, 5.0, 1.0, 32);

        let u = clip(
            &[outer.clone(), hole.clone()],
            std::slice::from_ref(&circle),
            BoolOp::Union,
        );
        assert!(
            loops_contain(&u, 5.0, 5.0),
            "circle survives as a disjoint island"
        );
        assert!(loops_contain(&u, 1.0, 1.0), "donut solid material stays");
        assert!(
            !loops_contain(&u, 3.5, 5.0),
            "the rest of the hole stays empty"
        );

        let i = clip(
            &[outer.clone(), hole.clone()],
            std::slice::from_ref(&circle),
            BoolOp::Intersection,
        );
        assert!(
            i.is_empty(),
            "circle in the hole never overlaps donut material"
        );

        let d = clip(&[outer, hole], &[circle], BoolOp::Difference);
        assert!(loops_contain(&d, 1.0, 1.0), "donut is unmodified");
        assert!(!loops_contain(&d, 5.0, 5.0), "hole is still a hole");
    }
}

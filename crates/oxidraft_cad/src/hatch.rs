//! Hatching: finding the region under a pick, filling it with a pattern, and
//! triangulating it for rendering. Includes closed-loop boundary detection and
//! a planar-arrangement fallback that traces a face out of unjoined edges.

use oxidraft_boolean::{Region, intersection, union};
use oxidraft_document::{Document, Entity, EntityId, EntityKind, HatchPattern};
use oxidraft_geometry::{Curve, CurveSegment, LineSeg, MinTracker, Point2d, tessellate_curve};
use std::collections::HashMap;

const TAU: f64 = std::f64::consts::TAU;

type P = (f64, f64);

/// The closed boundary loop of an entity that can bound a hatch (a closed
/// polycurve, full circle/ellipse, or existing hatch), or `None`.
pub fn boundary_loop(e: &Entity) -> Option<Vec<Curve>> {
    match &e.kind {
        EntityKind::Curve(Curve::Poly(pc)) if is_closed_poly(pc) => Some(pc.segments.clone()),
        EntityKind::Curve(Curve::Arc(a)) if is_full_span(a.start_angle, a.end_angle) => {
            Some(vec![Curve::Arc(*a)])
        }
        EntityKind::Curve(Curve::Ellipse(el)) if is_full_span(el.start_angle, el.end_angle) => {
            Some(vec![Curve::Ellipse(*el)])
        }
        EntityKind::Hatch { boundary, .. } => Some(boundary.clone()),
        _ => None,
    }
}

fn is_full_span(start: f64, end: f64) -> bool {
    ((end - start).abs() - TAU).abs() < 1e-9
}

fn is_closed_poly(pc: &oxidraft_geometry::PolyCurve) -> bool {
    let segs = &pc.segments;
    if segs.len() < 2 {
        return false;
    }
    let (s0, _) = segs[0].domain();
    let start = segs[0].evaluate_f64(s0);
    let (_, t1) = segs[segs.len() - 1].domain();
    let end = segs[segs.len() - 1].evaluate_f64(t1);
    (start.0 - end.0).hypot(start.1 - end.1) < 1e-6
}

/// True when `(x, y)` is inside `boundary` but outside all `holes`.
pub fn region_contains(boundary: &[Curve], holes: &[Vec<Curve>], x: f64, y: f64) -> bool {
    let (outer, hole_regions) = regions_of(boundary, holes);
    regions_contain(&outer, &hole_regions, x, y)
}

/// Prebuilt `Region`s for repeated containment queries. `Region` caches its
/// ring decomposition internally, so hot loops (pattern fills sample up to
/// a million points) must build these once instead of per query.
fn regions_of(boundary: &[Curve], holes: &[Vec<Curve>]) -> (Region, Vec<Region>) {
    (
        Region::new(boundary.to_vec()),
        holes.iter().map(|h| Region::new(h.to_vec())).collect(),
    )
}

fn regions_contain(outer: &Region, holes: &[Region], x: f64, y: f64) -> bool {
    outer.contains_point(x, y) && !holes.iter().any(|h| h.contains_point(x, y))
}

fn region_edges(boundary: &[Curve], holes: &[Vec<Curve>]) -> Vec<(P, P)> {
    let mut edges = Vec::new();
    let mut add_loop = |segs: &[Curve]| {
        for seg in segs {
            let bb = seg.bounding_box();
            let diag = (bb.max.x - bb.min.x).hypot(bb.max.y - bb.min.y);
            // Matches `default_flatten_tol`'s relative tolerance — 0.02 (2%)
            // here gave a visibly faceted clip boundary for the pattern
            // lines/dots even though the outline itself renders smooth.
            let pts = tessellate_curve(seg, (diag * 5e-4).max(1e-4));
            for w in pts.windows(2) {
                edges.push(((w[0].x, w[0].y), (w[1].x, w[1].y)));
            }
        }
    };
    add_loop(boundary);
    for h in holes {
        add_loop(h);
    }
    edges
}

fn region_bbox(boundary: &[Curve]) -> (f64, f64, f64, f64) {
    let mut bb: Option<oxidraft_geometry::BoundingBox> = None;
    for c in boundary {
        let b = c.bounding_box();
        bb = Some(match bb {
            Some(acc) => acc.union(&b),
            None => b,
        });
    }
    match bb {
        Some(b) => (b.min.x, b.min.y, b.max.x, b.max.y),
        None => (0.0, 0.0, 0.0, 0.0),
    }
}

fn seg_param(a: P, b: P, p: P, q: P) -> Option<f64> {
    let r = (b.0 - a.0, b.1 - a.1);
    let s = (q.0 - p.0, q.1 - p.1);
    let denom = r.0 * s.1 - r.1 * s.0;
    if denom.abs() < 1e-12 {
        return None;
    }
    let qp = (p.0 - a.0, p.1 - a.1);
    let t = (qp.0 * s.1 - qp.1 * s.0) / denom;
    let u = (qp.0 * r.1 - qp.1 * r.0) / denom;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

fn clip_to_region(
    a: P,
    b: P,
    edges: &[(P, P)],
    outer: &Region,
    holes: &[Region],
) -> Vec<(Point2d, Point2d)> {
    let mut ts: Vec<f64> = vec![0.0, 1.0];
    for &(p, q) in edges {
        if let Some(t) = seg_param(a, b, p, q) {
            ts.push(t);
        }
    }
    ts.sort_by(f64::total_cmp);
    let lerp = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    let mut out = Vec::new();
    for w in ts.windows(2) {
        if w[1] - w[0] < 1e-9 {
            continue;
        }
        let (mx, my) = lerp((w[0] + w[1]) * 0.5);
        if regions_contain(outer, holes, mx, my) {
            let s = lerp(w[0]);
            let e = lerp(w[1]);
            out.push((Point2d::from_f64(s.0, s.1), Point2d::from_f64(e.0, e.1)));
        }
    }
    out
}

/// The hatch line segments (clipped to the region) for a `Lines`/`Cross`
/// pattern. Empty for other patterns or an absurdly dense fill.
pub fn pattern_lines(
    boundary: &[Curve],
    holes: &[Vec<Curve>],
    pattern: HatchPattern,
) -> Vec<(Point2d, Point2d)> {
    let (angles, spacing): (Vec<f64>, f64) = match pattern {
        HatchPattern::Lines { angle_deg, spacing } => (vec![angle_deg], spacing),
        HatchPattern::Cross { angle_deg, spacing } => (vec![angle_deg, angle_deg + 90.0], spacing),
        _ => return Vec::new(),
    };
    if !(spacing.is_finite() && spacing > 1e-9) || boundary.is_empty() {
        return Vec::new();
    }
    let (minx, miny, maxx, maxy) = region_bbox(boundary);
    let (cx, cy) = ((minx + maxx) * 0.5, (miny + maxy) * 0.5);
    let diag = (maxx - minx).hypot(maxy - miny).max(1.0);
    // Decline a fill that would take absurdly many strokes outright:
    // trickling out a partial pattern after a multi-minute freeze helps
    // nobody, and past ~1e6·spacing the `k += spacing` walk below can
    // stall under f64 ulp and spin uselessly. diag bounds pmax−pmin for
    // every angle.
    if diag / spacing > 100_000.0 {
        return Vec::new();
    }
    let edges = region_edges(boundary, holes);
    let (outer, hole_regions) = regions_of(boundary, holes);
    let mut out = Vec::new();
    for angle_deg in angles {
        let a = angle_deg.to_radians();
        let (dx, dy) = (a.cos(), a.sin());
        let (nx, ny) = (-dy, dx);
        let corners = [(minx, miny), (maxx, miny), (maxx, maxy), (minx, maxy)];
        let projs: Vec<f64> = corners.iter().map(|&(x, y)| x * nx + y * ny).collect();
        let pmin = projs.iter().cloned().fold(f64::MAX, f64::min);
        let pmax = projs.iter().cloned().fold(f64::MIN, f64::max);
        let cp = cx * nx + cy * ny;
        let mut k = (pmin / spacing).floor() * spacing;
        let mut guard = 0;
        while k <= pmax + spacing && guard < 100_000 {
            guard += 1;
            let bx = cx + (k - cp) * nx;
            let by = cy + (k - cp) * ny;
            let pa = (bx - dx * diag, by - dy * diag);
            let pb = (bx + dx * diag, by + dy * diag);
            out.extend(clip_to_region(pa, pb, &edges, &outer, &hole_regions));
            k += spacing;
        }
    }
    out
}

/// The dot positions (inside the region) for a `Dots` pattern. Empty for other
/// patterns or an absurdly dense grid.
pub fn pattern_dots(
    boundary: &[Curve],
    holes: &[Vec<Curve>],
    pattern: HatchPattern,
) -> Vec<Point2d> {
    let spacing = match pattern {
        HatchPattern::Dots { spacing } => spacing,
        _ => return Vec::new(),
    };
    if !(spacing.is_finite() && spacing > 1e-9) || boundary.is_empty() {
        return Vec::new();
    }
    let (minx, miny, maxx, maxy) = region_bbox(boundary);
    // Same up-front bound as `pattern_lines`: decline instead of freezing
    // on a grid the walk below could never finish.
    let (nx, ny) = ((maxx - minx) / spacing, (maxy - miny) / spacing);
    if !(nx.is_finite() && ny.is_finite()) || (nx + 1.0) * (ny + 1.0) > 1_000_000.0 {
        return Vec::new();
    }
    let (outer, hole_regions) = regions_of(boundary, holes);
    let mut out = Vec::new();
    let mut y = (miny / spacing).floor() * spacing;
    let mut guard = 0;
    while y <= maxy && guard < 1_000_000 {
        let mut x = (minx / spacing).floor() * spacing;
        while x <= maxx && guard < 1_000_000 {
            guard += 1;
            if regions_contain(&outer, &hole_regions, x, y) {
                out.push(Point2d::from_f64(x, y));
            }
            x += spacing;
        }
        y += spacing;
    }
    out
}

/// Triangulates the filled region (boundary minus holes) for solid rendering,
/// choosing a flatten tolerance from the region size.
pub fn triangulate(boundary: &[Curve], holes: &[Vec<Curve>]) -> Vec<[Point2d; 3]> {
    triangulate_with_tol(boundary, holes, default_flatten_tol(boundary))
}

/// [`triangulate`] with an explicit curve-flattening tolerance `tol`.
pub fn triangulate_with_tol(
    boundary: &[Curve],
    holes: &[Vec<Curve>],
    tol: f64,
) -> Vec<[Point2d; 3]> {
    let floor = (region_diag(boundary) * 1e-7).max(1e-9);
    let tol = tol.max(floor);
    let mut outer = loop_polygon(boundary, tol);
    if outer.len() < 3 {
        return Vec::new();
    }
    if signed_area(&outer) < 0.0 {
        outer.reverse();
    }

    let hole_polys: Vec<Vec<P>> = holes
        .iter()
        .map(|h| {
            let mut p = loop_polygon(h, tol);
            if signed_area(&p) > 0.0 {
                p.reverse();
            }
            p
        })
        .filter(|p| p.len() >= 3)
        .collect();

    let Some(merged) = bridge_holes(outer, hole_polys) else {
        return Vec::new();
    };
    ear_clip(&merged)
}

/// Triangulates a set of nested contour loops, classifying each as fill or hole
/// by its containment depth (even depth fills, odd depth is a hole).
pub fn triangulate_contours(contours: &[Curve], tol: f64) -> Vec<[Point2d; 3]> {
    let polys: Vec<Vec<P>> = contours
        .iter()
        .map(|c| loop_polygon(std::slice::from_ref(c), tol))
        .filter(|p| p.len() >= 3)
        .collect();
    let n = polys.len();
    if n == 0 {
        return Vec::new();
    }
    let depth: Vec<usize> = (0..n)
        .map(|i| {
            let (px, py) = polys[i][0];
            (0..n)
                .filter(|&j| j != i && point_in_poly(&polys[j], px, py))
                .count()
        })
        .collect();
    let immediate_parent = |k: usize| -> Option<usize> {
        let (px, py) = polys[k][0];
        (0..n)
            .filter(|&j| j != k && point_in_poly(&polys[j], px, py))
            .max_by_key(|&j| depth[j])
    };

    let mut tris = Vec::new();
    for i in 0..n {
        if !depth[i].is_multiple_of(2) {
            continue;
        }
        let mut outer = polys[i].clone();
        if signed_area(&outer) < 0.0 {
            outer.reverse();
        }
        let hole_polys: Vec<Vec<P>> = (0..n)
            .filter(|&k| depth[k] == depth[i] + 1 && immediate_parent(k) == Some(i))
            .map(|k| {
                let mut h = polys[k].clone();
                if signed_area(&h) > 0.0 {
                    h.reverse();
                }
                h
            })
            .collect();
        if let Some(merged) = bridge_holes(outer, hole_polys) {
            tris.extend(ear_clip(&merged));
        }
    }
    tris
}

/// The region's boundary and hole loops flattened to point rings (for drawing
/// the hatch outline).
pub fn outline_loops(boundary: &[Curve], holes: &[Vec<Curve>], tol: f64) -> Vec<Vec<Point2d>> {
    let floor = (region_diag(boundary) * 1e-7).max(1e-9);
    let tol = tol.max(floor);
    let to_pts = |ring: Vec<P>| -> Vec<Point2d> {
        ring.into_iter()
            .map(|(x, y)| Point2d::from_f64(x, y))
            .collect()
    };
    let mut loops = Vec::new();
    let outer = loop_polygon(boundary, tol);
    if outer.len() >= 2 {
        loops.push(to_pts(outer));
    }
    for h in holes {
        let ring = loop_polygon(h, tol);
        if ring.len() >= 2 {
            loops.push(to_pts(ring));
        }
    }
    loops
}

fn region_diag(boundary: &[Curve]) -> f64 {
    let mut min = (f64::MAX, f64::MAX);
    let mut max = (f64::MIN, f64::MIN);
    for c in boundary {
        let bb = c.bounding_box();
        min.0 = min.0.min(bb.min.x);
        min.1 = min.1.min(bb.min.y);
        max.0 = max.0.max(bb.max.x);
        max.1 = max.1.max(bb.max.y);
    }
    ((max.0 - min.0).powi(2) + (max.1 - min.1).powi(2)).sqrt()
}

fn default_flatten_tol(boundary: &[Curve]) -> f64 {
    (region_diag(boundary) * 5e-4).max(1e-6)
}

fn bridge_holes(outer: Vec<P>, mut holes: Vec<Vec<P>>) -> Option<Vec<P>> {
    holes.sort_by(|a, b| {
        max_x(b)
            .partial_cmp(&max_x(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut poly = outer;
    for hole in holes {
        poly = merge_one_hole(poly, &hole)?;
    }
    Some(poly)
}

fn bridge_edge_crossing(outer: &[P], my: f64, mx: f64) -> Option<usize> {
    let n = outer.len();
    let mut best = MinTracker::new();
    for i in 0..n {
        let a = outer[i];
        let b = outer[(i + 1) % n];
        if (a.1 <= my && b.1 > my) || (b.1 <= my && a.1 > my) {
            let t = (my - a.1) / (b.1 - a.1);
            let x = a.0 + t * (b.0 - a.0);
            if x >= mx - 1e-9 {
                best.offer(x, i);
            }
        }
    }
    best.value()
}

fn merge_one_hole(outer: Vec<P>, hole: &[P]) -> Option<Vec<P>> {
    let mi = (0..hole.len())
        .max_by(|&a, &b| {
            hole[a]
                .0
                .partial_cmp(&hole[b].0)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap();
    let m = hole[mi];
    let n = outer.len();
    let eps = m.1.abs().max(1.0) * 1e-9;
    let ei = bridge_edge_crossing(&outer, m.1, m.0)
        .or_else(|| bridge_edge_crossing(&outer, m.1 + eps, m.0))
        .or_else(|| bridge_edge_crossing(&outer, m.1 - eps, m.0))?;
    let (a, b) = (outer[ei], outer[(ei + 1) % n]);
    let pidx = if a.0 >= b.0 { ei } else { (ei + 1) % n };

    let mut out = Vec::with_capacity(n + hole.len() + 2);
    out.extend_from_slice(&outer[..=pidx]);
    for k in 0..hole.len() {
        out.push(hole[(mi + k) % hole.len()]);
    }
    out.push(hole[mi]);
    out.push(outer[pidx]);
    out.extend_from_slice(&outer[pidx + 1..]);
    Some(out)
}

fn ear_clip(poly: &[P]) -> Vec<[Point2d; 3]> {
    let mut poly = poly.to_vec();
    if poly.len() < 3 {
        return Vec::new();
    }
    if signed_area(&poly) < 0.0 {
        poly.reverse();
    }
    let mut idx: Vec<usize> = (0..poly.len()).collect();
    let mut tris = Vec::new();
    let max_iter = poly.len() * poly.len() + 1;
    let mut guard = 0;
    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (ip, ic, inx) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (poly[ip], poly[ic], poly[inx]);
            if cross(a, b, c) <= 1e-9 {
                continue;
            }
            let near = |u: P, v: P| (u.0 - v.0).hypot(u.1 - v.1) < 1e-9;
            let blocked = idx.iter().any(|&oi| {
                if oi == ip || oi == ic || oi == inx {
                    return false;
                }
                let q = poly[oi];
                if near(q, a) || near(q, b) || near(q, c) {
                    return false;
                }
                point_in_tri(q, a, b, c)
            });
            if blocked {
                continue;
            }
            tris.push([pt(a), pt(b), pt(c)]);
            idx.remove(k);
            clipped = true;
            break;
        }
        guard += 1;
        if !clipped || guard > max_iter {
            break;
        }
    }
    if idx.len() == 3 {
        tris.push([pt(poly[idx[0]]), pt(poly[idx[1]]), pt(poly[idx[2]])]);
    }
    tris
}

/// Why [`trace_pick_region`] could not produce a fillable region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickRegionError {
    /// No enclosed region surrounds the pick point.
    NotFound,
    /// Too many edges to trace an arrangement within the budget.
    TooComplex,
}

/// Finds the region enclosing the pick `(px, py)`: its outer boundary and any
/// island holes, recovering the original curves. Tries closed loops first, then
/// a planar-arrangement trace over unjoined edges.
pub fn trace_pick_region(
    doc: &Document,
    px: f64,
    py: f64,
) -> Result<(Vec<Curve>, Vec<Vec<Curve>>), PickRegionError> {
    if let Some(region) = region_from_closed_loops(doc, px, py) {
        return Ok(region);
    }
    trace_pick_region_arrangement(doc, px, py)
}

fn region_from_closed_loops(
    doc: &Document,
    px: f64,
    py: f64,
) -> Option<(Vec<Curve>, Vec<Vec<Curve>>)> {
    let entries: Vec<(EntityId, Vec<Curve>)> = doc
        .editable_entities()
        .filter(|e| !matches!(e.kind, EntityKind::Hatch { .. }))
        .filter_map(|e| boundary_loop(e).map(|l| (e.id, l)))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    if entries.is_empty() {
        return None;
    }
    let ids: Vec<EntityId> = entries.iter().map(|(id, _)| *id).collect();
    let loops: Vec<Vec<Curve>> = entries.into_iter().map(|(_, l)| l).collect();
    let regions: Vec<Region> = loops.iter().map(|l| Region::new(l.clone())).collect();

    let outer_idx = (0..loops.len())
        .filter(|&i| regions[i].contains_point(px, py))
        .min_by(|&a, &b| {
            regions[a]
                .signed_area_f64()
                .abs()
                .partial_cmp(&regions[b].signed_area_f64().abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;

    if loop_subdivided(doc, &loops[outer_idx], ids[outer_idx]) {
        return None;
    }

    let islands: Vec<Vec<Curve>> = (0..loops.len())
        .filter(|&i| i != outer_idx)
        .filter(|&i| match interior_point(&loops[i]) {
            Some((ix, iy)) => {
                regions[outer_idx].contains_point(ix, iy) && !regions[i].contains_point(px, py)
            }
            None => false,
        })
        .map(|i| loops[i].clone())
        .collect();

    Some((loops[outer_idx].clone(), merge_islands(islands)))
}

fn merge_islands(islands: Vec<Vec<Curve>>) -> Vec<Vec<Curve>> {
    let mut regs: Vec<Region> = islands.into_iter().map(Region::new).collect();
    let mut changed = true;
    while changed {
        changed = false;
        'pairs: for i in 0..regs.len() {
            for j in (i + 1)..regs.len() {
                if !intersection(&regs[i], &regs[j]).is_empty() {
                    let merged = union(&regs[i], &regs[j]);
                    // i < j, so remove j first to keep the index valid.
                    regs.remove(j);
                    regs.remove(i);
                    regs.extend(merged);
                    changed = true;
                    break 'pairs;
                }
            }
        }
    }
    regs.into_iter()
        .map(|r| r.outer)
        .filter(|l| !l.is_empty())
        .collect()
}

fn loop_subdivided(doc: &Document, outer: &[Curve], outer_id: EntityId) -> bool {
    let po = loop_polygon(outer, default_flatten_tol(outer));
    if po.len() < 3 {
        return false;
    }
    for e in doc.editable_entities() {
        if e.id == outer_id {
            continue;
        }
        let EntityKind::Curve(c) = &e.kind else {
            continue;
        };
        let parts: Vec<Curve> = match c {
            Curve::Poly(pc) => pc.segments.clone(),
            other => vec![other.clone()],
        };
        for part in &parts {
            let pts = flatten(part, 3e-3);
            for w in pts.windows(2) {
                for i in 0..po.len() {
                    let (a0, a1) = (po[i], po[(i + 1) % po.len()]);
                    if segments_cross(a0, a1, w[0], w[1]) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn segments_cross(a0: P, a1: P, b0: P, b1: P) -> bool {
    let cross = |p: P, q: P, r: P| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
    let d1 = cross(a0, a1, b0);
    let d2 = cross(a0, a1, b1);
    let d3 = cross(b0, b1, a0);
    let d4 = cross(b0, b1, a1);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

fn interior_point(loop_: &[Curve]) -> Option<P> {
    let poly = loop_polygon(loop_, default_flatten_tol(loop_));
    if poly.len() < 3 {
        return None;
    }
    let region = Region::new(loop_.to_vec());
    let n = poly.len() as f64;
    let cx = poly.iter().map(|p| p.0).sum::<f64>() / n;
    let cy = poly.iter().map(|p| p.1).sum::<f64>() / n;
    if region.contains_point(cx, cy) {
        return Some((cx, cy));
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in &poly {
        minx = minx.min(x);
        miny = miny.min(y);
        maxx = maxx.max(x);
        maxy = maxy.max(y);
    }
    for gy in 1..8 {
        for gx in 1..8 {
            let x = minx + (maxx - minx) * gx as f64 / 8.0;
            let y = miny + (maxy - miny) * gy as f64 / 8.0;
            if region.contains_point(x, y) {
                return Some((x, y));
            }
        }
    }
    None
}

fn trace_pick_region_arrangement(
    doc: &Document,
    px: f64,
    py: f64,
) -> Result<(Vec<Curve>, Vec<Vec<Curve>>), PickRegionError> {
    let segs = split_at_intersections(collect_segments(doc));
    if segs.len() < 3 {
        return Err(PickRegionError::NotFound);
    }
    if segs.len() > 4000 {
        return Err(PickRegionError::TooComplex);
    }

    let mut nodes: Vec<P> = Vec::new();
    let mut index: HashMap<(i64, i64), usize> = HashMap::new();
    let node_of = |p: P, nodes: &mut Vec<P>, index: &mut HashMap<(i64, i64), usize>| -> usize {
        let key = ((p.0 * 1e6).round() as i64, (p.1 * 1e6).round() as i64);
        *index.entry(key).or_insert_with(|| {
            nodes.push(p);
            nodes.len() - 1
        })
    };
    let mut undirected: Vec<(usize, usize)> = Vec::new();
    for (a, b) in &segs {
        let (ia, ib) = (
            node_of(*a, &mut nodes, &mut index),
            node_of(*b, &mut nodes, &mut index),
        );
        if ia != ib {
            undirected.push((ia, ib));
        }
    }
    undirected.sort_unstable();
    undirected.dedup();

    let mut out: HashMap<usize, Vec<(f64, usize)>> = HashMap::new();
    for &(a, b) in &undirected {
        let ang_ab = (nodes[b].1 - nodes[a].1).atan2(nodes[b].0 - nodes[a].0);
        let ang_ba = (nodes[a].1 - nodes[b].1).atan2(nodes[a].0 - nodes[b].0);
        out.entry(a).or_default().push((ang_ab, b));
        out.entry(b).or_default().push((ang_ba, a));
    }
    for v in out.values_mut() {
        v.sort_by(|x, y| x.0.total_cmp(&y.0));
    }

    let mut visited: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let mut faces: Vec<Vec<usize>> = Vec::new();
    for (s0, s1) in undirected.iter().flat_map(|&(a, b)| [(a, b), (b, a)]) {
        if visited.contains(&(s0, s1)) {
            continue;
        }
        let mut cycle = Vec::new();
        let (mut from, mut to) = (s0, s1);
        let mut steps = 0;
        loop {
            if !visited.insert((from, to)) {
                break;
            }
            cycle.push(from);
            let ring = &out[&to];
            let pos = ring.iter().position(|&(_, w)| w == from).unwrap_or(0);
            let nxt = ring[(pos + ring.len() - 1) % ring.len()].1;
            from = to;
            to = nxt;
            steps += 1;
            if (from, to) == (s0, s1) || steps > undirected.len() * 2 + 4 {
                break;
            }
        }
        if cycle.len() >= 3 {
            faces.push(cycle);
        }
    }

    let polys: Vec<Vec<P>> = faces
        .iter()
        .map(|f| f.iter().map(|&i| nodes[i]).collect::<Vec<P>>())
        .filter(|p| signed_area(p) > 1e-9)
        .collect();
    let outer = polys
        .iter()
        .filter(|p| point_in_poly(p, px, py))
        .min_by(|a, b| {
            signed_area(a)
                .partial_cmp(&signed_area(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or(PickRegionError::NotFound)?
        .clone();

    let parts = source_parts(doc);
    let holes: Vec<Vec<Curve>> = polys
        .iter()
        .filter(|p| !polys_equal(p, &outer) && p.iter().all(|&(x, y)| point_in_poly(&outer, x, y)))
        .map(|p| recurve_loop(&poly_to_curves(p), &parts))
        .collect();

    Ok((recurve_loop(&poly_to_curves(&outer), &parts), holes))
}

fn collect_segments(doc: &Document) -> Vec<(P, P)> {
    let mut segs = Vec::new();
    for e in doc.editable_entities() {
        let curves: Vec<&Curve> = match &e.kind {
            EntityKind::Curve(c) => vec![c],
            _ => continue,
        };
        for c in curves {
            let parts: Vec<Curve> = match c {
                Curve::Poly(pc) => pc.segments.clone(),
                other => vec![other.clone()],
            };
            for part in &parts {
                let pts = flatten(part, 3e-3);
                for w in pts.windows(2) {
                    if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 {
                        segs.push((w[0], w[1]));
                    }
                }
            }
        }
    }
    segs
}

fn flatten(c: &Curve, tol: f64) -> Vec<P> {
    tessellate_curve(c, tol.max(1e-6))
        .into_iter()
        .map(|p| (p.x, p.y))
        .collect()
}

fn split_at_intersections(segs: Vec<(P, P)>) -> Vec<(P, P)> {
    let n = segs.len();
    if n > 4000 {
        return segs;
    }
    let mut out = Vec::with_capacity(n);
    for (i, &(a, b)) in segs.iter().enumerate() {
        let mut ts: Vec<f64> = vec![0.0, 1.0];
        for (j, &(c, d)) in segs.iter().enumerate() {
            if i == j {
                continue;
            }
            if let Some((t, _)) = seg_intersect(a, b, c, d)
                && t > 1e-9
                && t < 1.0 - 1e-9
            {
                ts.push(t);
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        for w in ts.windows(2) {
            let p0 = (a.0 + w[0] * (b.0 - a.0), a.1 + w[0] * (b.1 - a.1));
            let p1 = (a.0 + w[1] * (b.0 - a.0), a.1 + w[1] * (b.1 - a.1));
            out.push((p0, p1));
        }
    }
    out
}

fn seg_intersect(a0: P, a1: P, b0: P, b1: P) -> Option<(f64, f64)> {
    let (r, s) = ((a1.0 - a0.0, a1.1 - a0.1), (b1.0 - b0.0, b1.1 - b0.1));
    let denom = r.0 * s.1 - r.1 * s.0;
    if denom.abs() < 1e-12 {
        return None;
    }
    let qp = (b0.0 - a0.0, b0.1 - a0.1);
    let t = (qp.0 * s.1 - qp.1 * s.0) / denom;
    let u = (qp.0 * r.1 - qp.1 * r.0) / denom;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some((t, u))
    } else {
        None
    }
}

fn pt((x, y): P) -> Point2d {
    Point2d::from_f64(x, y)
}
fn max_x(p: &[P]) -> f64 {
    p.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max)
}

fn poly_to_curves(p: &[P]) -> Vec<Curve> {
    (0..p.len())
        .map(|i| {
            let a = p[i];
            let b = p[(i + 1) % p.len()];
            Curve::Line(LineSeg::from_endpoints(pt(a), pt(b)))
        })
        .collect()
}

fn subcurve(c: &Curve, a: f64, b: f64) -> Curve {
    use oxidraft_geometry::split_curve;
    let (a, b) = (a.clamp(0.0, 1.0), b.clamp(0.0, 1.0));
    if b <= a + 1e-9 {
        return c.clone();
    }
    let right = if a > 1e-9 {
        split_curve(c, a).1
    } else {
        c.clone()
    };
    if b < 1.0 - 1e-9 {
        let bb = ((b - a) / (1.0 - a)).clamp(0.0, 1.0);
        split_curve(&right, bb).0
    } else {
        right
    }
}

fn curve_closed(c: &Curve) -> bool {
    let (t0, t1) = c.domain();
    let s = c.evaluate_f64(t0);
    let e = c.evaluate_f64(t1);
    (s.0 - e.0).hypot(s.1 - e.1) < 1e-7
}

fn arc_pieces(c: &Curve, a: f64, b: f64, closed: bool) -> Vec<Curve> {
    if !closed || (a >= -1e-9 && b <= 1.0 + 1e-9) {
        return vec![subcurve(c, a.clamp(0.0, 1.0), b.clamp(0.0, 1.0))];
    }
    let mut pieces = Vec::new();
    let mut s = a;
    while s < b - 1e-9 {
        let base = s.floor();
        let seg_hi = (base + 1.0).min(b);
        pieces.push(subcurve(c, s - base, seg_hi - base));
        s = seg_hi;
    }
    pieces
}

fn loop_diag(verts: &[P]) -> f64 {
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in verts {
        minx = minx.min(x);
        miny = miny.min(y);
        maxx = maxx.max(x);
        maxy = maxy.max(y);
    }
    if minx > maxx {
        0.0
    } else {
        (maxx - minx).hypot(maxy - miny)
    }
}

fn recurve_loop(loop_: &[Curve], parts: &[Curve]) -> Vec<Curve> {
    use oxidraft_geometry::{project_point_onto_curve, reverse_curve};
    let m = loop_.len();
    if m < 3 || parts.is_empty() {
        return loop_.to_vec();
    }
    let verts: Vec<P> = loop_
        .iter()
        .map(|c| {
            let (t0, _) = c.domain();
            c.evaluate_f64(t0)
        })
        .collect();
    let tol = (loop_diag(&verts) * 5e-3).max(5e-3);

    let assign = |p: P| -> Option<(usize, f64)> {
        let mut best = MinTracker::new();
        for (i, c) in parts.iter().enumerate() {
            let pr = project_point_onto_curve(c, p.0, p.1);
            let d = (pr.point.0 - p.0).hypot(pr.point.1 - p.1);
            if d <= tol {
                let (t0, t1) = c.domain();
                let span = t1 - t0;
                let nt = if span.abs() < 1e-12 {
                    0.0
                } else {
                    ((pr.t - t0) / span).clamp(0.0, 1.0)
                };
                best.offer(d, (i, nt));
            }
        }
        best.value()
    };
    let va: Vec<Option<(usize, f64)>> = verts.iter().map(|&p| assign(p)).collect();

    let edge_part = |k: usize| -> Option<usize> {
        let (ia, _) = va[k]?;
        let (ib, _) = va[(k + 1) % m]?;
        if ia != ib {
            return None;
        }
        let (pa, pb) = (verts[k], verts[(k + 1) % m]);
        let mid = ((pa.0 + pb.0) * 0.5, (pa.1 + pb.1) * 0.5);
        let pr = project_point_onto_curve(&parts[ia], mid.0, mid.1);
        ((pr.point.0 - mid.0).hypot(pr.point.1 - mid.1) <= tol).then_some(ia)
    };
    let ep: Vec<Option<usize>> = (0..m).map(edge_part).collect();

    if ep[0].is_some() && ep.iter().all(|&e| e == ep[0]) {
        return vec![parts[ep[0].unwrap()].clone()];
    }

    let start = (0..m).find(|&k| ep[k] != ep[(k + m - 1) % m]).unwrap_or(0);

    let mut out: Vec<Curve> = Vec::new();
    let mut k = 0;
    while k < m {
        let e = (start + k) % m;
        let Some(idx) = ep[e] else {
            out.push(loop_[e].clone());
            k += 1;
            continue;
        };
        let mut len = 1;
        while k + len < m && ep[(start + k + len) % m] == Some(idx) {
            len += 1;
        }
        let vend = (e + len) % m;
        let closed = curve_closed(&parts[idx]);
        let mut u: Vec<f64> = Vec::with_capacity(len + 1);
        u.push(va[e].unwrap().1);
        for j in 1..=len {
            let raw = va[(e + j) % m].unwrap().1;
            let prev = *u.last().unwrap();
            u.push(if closed {
                raw + (prev - raw).round()
            } else {
                raw
            });
        }
        let (t0, t1) = (u[0], u[len]);
        let inc = t1 >= t0;
        let mut mono = (t1 - t0).abs() > 1e-6;
        for w in u.windows(2) {
            if (inc && w[1] + 1e-9 < w[0]) || (!inc && w[1] > w[0] + 1e-9) {
                mono = false;
                break;
            }
        }
        let pieces = mono
            .then(|| {
                let (a, b) = if inc { (t0, t1) } else { (t1, t0) };
                let mut ps = arc_pieces(&parts[idx], a, b, closed);
                if !inc {
                    ps.reverse();
                    for p in &mut ps {
                        *p = reverse_curve(p);
                    }
                }
                ps
            })
            .filter(|ps| {
                let (Some(first), Some(last)) = (ps.first(), ps.last()) else {
                    return false;
                };
                let (f0, _) = first.domain();
                let (_, l1) = last.domain();
                let sp = first.evaluate_f64(f0);
                let epp = last.evaluate_f64(l1);
                (sp.0 - verts[e].0).hypot(sp.1 - verts[e].1) <= tol * 2.0
                    && (epp.0 - verts[vend].0).hypot(epp.1 - verts[vend].1) <= tol * 2.0
            });
        match pieces {
            Some(ps) => out.extend(ps),
            None => {
                for j in 0..len {
                    out.push(loop_[(e + j) % m].clone());
                }
            }
        }
        k += len;
    }
    if out.is_empty() { loop_.to_vec() } else { out }
}

fn source_parts(doc: &Document) -> Vec<Curve> {
    let mut parts = Vec::new();
    for e in doc.editable_entities() {
        if let EntityKind::Curve(c) = &e.kind {
            match c {
                Curve::Poly(pc) => parts.extend(pc.segments.iter().cloned()),
                other => parts.push(other.clone()),
            }
        }
    }
    parts
}

fn polys_equal(a: &[P], b: &[P]) -> bool {
    a.len() == b.len()
        && (signed_area(a) - signed_area(b)).abs() < 1e-9
        && a.iter().all(|&(x, y)| {
            point_in_poly(b, x, y) || b.iter().any(|&(bx, by)| (x - bx).hypot(y - by) < 1e-6)
        })
}

fn loop_polygon(boundary: &[Curve], tol: f64) -> Vec<P> {
    let mut pts: Vec<P> = Vec::new();
    for seg in boundary {
        for p in flatten(seg, tol) {
            if pts
                .last()
                .is_none_or(|l| (l.0 - p.0).hypot(l.1 - p.1) > 1e-9)
            {
                pts.push(p);
            }
        }
    }
    if pts.len() >= 2 {
        let (f, l) = (pts[0], *pts.last().unwrap());
        if (f.0 - l.0).hypot(f.1 - l.1) < 1e-9 {
            pts.pop();
        }
    }
    pts
}

fn signed_area(p: &[P]) -> f64 {
    let mut a = 0.0;
    for i in 0..p.len() {
        let j = (i + 1) % p.len();
        a += p[i].0 * p[j].1 - p[j].0 * p[i].1;
    }
    a / 2.0
}

fn cross(a: P, b: P, c: P) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn point_in_tri(p: P, a: P, b: P, c: P) -> bool {
    let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

fn point_in_poly(poly: &[P], x: f64, y: f64) -> bool {
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
    use oxidraft_geometry::{CircularArc, LineSeg, PolyCurve};

    #[test]
    fn point_in_empty_poly_is_false_not_panic() {
        assert!(!point_in_poly(&[], 0.0, 0.0));
    }

    fn rect_poly(x0: i64, y0: i64, x1: i64, y1: i64) -> EntityKind {
        let segs = vec![
            Curve::Line(LineSeg::from_endpoints(pti(x0, y0), pti(x1, y0))),
            Curve::Line(LineSeg::from_endpoints(pti(x1, y0), pti(x1, y1))),
            Curve::Line(LineSeg::from_endpoints(pti(x1, y1), pti(x0, y1))),
            Curve::Line(LineSeg::from_endpoints(pti(x0, y1), pti(x0, y0))),
        ];
        EntityKind::Curve(Curve::Poly(Box::new(PolyCurve::new(segs))))
    }

    fn full_circle(cx: f64, cy: f64, r: f64) -> EntityKind {
        EntityKind::Curve(Curve::Arc(CircularArc::new(
            Point2d::from_f64(cx, cy),
            r,
            0.0,
            TAU,
        )))
    }

    fn pti(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    fn square() -> Vec<Curve> {
        vec![
            Curve::Line(LineSeg::from_endpoints(pti(0, 0), pti(4, 0))),
            Curve::Line(LineSeg::from_endpoints(pti(4, 0), pti(4, 4))),
            Curve::Line(LineSeg::from_endpoints(pti(4, 4), pti(0, 4))),
            Curve::Line(LineSeg::from_endpoints(pti(0, 4), pti(0, 0))),
        ]
    }

    fn tri_area(t: &[Point2d; 3]) -> f64 {
        0.5 * ((t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[1].y - t[0].y) * (t[2].x - t[0].x)).abs()
    }

    #[test]
    fn triangulate_square_covers_area() {
        let tris = triangulate(&square(), &[]);
        let total: f64 = tris.iter().map(tri_area).sum();
        assert!(
            (total - 16.0).abs() < 1e-6,
            "square fill area 16, got {total}"
        );
    }

    #[test]
    fn triangulate_square_with_hole_subtracts_it() {
        let hole = vec![
            Curve::Line(LineSeg::from_endpoints(pti(1, 1), pti(3, 1))),
            Curve::Line(LineSeg::from_endpoints(pti(3, 1), pti(3, 3))),
            Curve::Line(LineSeg::from_endpoints(pti(3, 3), pti(1, 3))),
            Curve::Line(LineSeg::from_endpoints(pti(1, 3), pti(1, 1))),
        ];
        let tris = triangulate(&square(), &[hole]);
        let total: f64 = tris.iter().map(tri_area).sum();
        assert!(
            (total - 12.0).abs() < 1e-5,
            "ring fill area 12, got {total}"
        );
    }

    fn square_contour(x0: i64, y0: i64, x1: i64, y1: i64) -> Curve {
        Curve::Poly(Box::new(PolyCurve::new(vec![
            Curve::Line(LineSeg::from_endpoints(pti(x0, y0), pti(x1, y0))),
            Curve::Line(LineSeg::from_endpoints(pti(x1, y0), pti(x1, y1))),
            Curve::Line(LineSeg::from_endpoints(pti(x1, y1), pti(x0, y1))),
            Curve::Line(LineSeg::from_endpoints(pti(x0, y1), pti(x0, y0))),
        ])))
    }

    #[test]
    fn triangulate_contours_classifies_holes_by_depth() {
        let contours = [
            square_contour(0, 0, 6, 6),
            square_contour(1, 1, 5, 5),
            square_contour(2, 2, 4, 4),
        ];
        let total: f64 = triangulate_contours(&contours, 0.01)
            .iter()
            .map(tri_area)
            .sum();
        assert!(
            (total - 24.0).abs() < 1e-5,
            "nested fill area 24, got {total}"
        );
    }

    #[test]
    fn triangulate_contours_handles_separate_regions() {
        let contours = [square_contour(0, 0, 2, 2), square_contour(5, 0, 7, 2)];
        let total: f64 = triangulate_contours(&contours, 0.01)
            .iter()
            .map(tri_area)
            .sum();
        assert!(
            (total - 8.0).abs() < 1e-5,
            "two-region fill area 8, got {total}"
        );
    }

    #[test]
    fn region_contains_respects_holes() {
        let hole = vec![
            Curve::Line(LineSeg::from_endpoints(pti(1, 1), pti(3, 1))),
            Curve::Line(LineSeg::from_endpoints(pti(3, 1), pti(3, 3))),
            Curve::Line(LineSeg::from_endpoints(pti(3, 3), pti(1, 3))),
            Curve::Line(LineSeg::from_endpoints(pti(1, 3), pti(1, 1))),
        ];
        assert!(
            region_contains(&square(), std::slice::from_ref(&hole), 0.5, 2.0),
            "in the ring"
        );
        assert!(
            !region_contains(&square(), &[hole], 2.0, 2.0),
            "in the hole → not filled"
        );
    }

    #[test]
    fn pick_region_from_unjoined_lines() {
        let mut doc = Document::new();
        for (a, b) in [
            ((0, 0), (4, 0)),
            ((4, 0), (4, 4)),
            ((4, 4), (0, 4)),
            ((0, 4), (0, 0)),
        ] {
            doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                pti(a.0, a.1),
                pti(b.0, b.1),
            ))));
        }
        let (boundary, holes) = trace_pick_region(&doc, 2.0, 2.0).expect("encloses the click");
        assert!(holes.is_empty());
        assert!(region_contains(&boundary, &[], 2.0, 2.0));
    }

    #[test]
    fn bridge_edge_crossing_misses_exact_vertex_tie() {
        let outer = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(
            bridge_edge_crossing(&outer, 10.0, 4.0).is_none(),
            "a ray exactly through the top edge's y misses every edge"
        );
        assert!(bridge_edge_crossing(&outer, 9.999, 4.0).is_some());
    }

    #[test]
    fn merge_one_hole_recovers_from_exact_vertex_tie() {
        let outer = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let hole = vec![(3.0, 8.5), (4.0, 10.0), (3.0, 9.5)];
        let merged = merge_one_hole(outer, &hole).expect("the epsilon retry finds a bridge edge");
        assert_eq!(merged.len(), 4 + 3 + 2, "outer + hole + bridge duplication");
    }

    #[test]
    fn trace_pick_region_reports_too_complex_not_not_found() {
        let mut doc = Document::new();
        let n = 4001;
        for i in 0..n {
            let a = TAU * i as f64 / n as f64;
            let b = TAU * (i + 1) as f64 / n as f64;
            doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(10.0 * a.cos(), 10.0 * a.sin()),
                Point2d::from_f64(10.0 * b.cos(), 10.0 * b.sin()),
            ))));
        }
        match trace_pick_region(&doc, 0.0, 0.0) {
            Err(PickRegionError::TooComplex) => {}
            other => panic!("expected TooComplex, got {other:?}"),
        }
    }

    #[test]
    fn pick_region_detects_island() {
        let mut doc = Document::new();
        for (a, b) in [
            ((0, 0), (6, 0)),
            ((6, 0), (6, 6)),
            ((6, 6), (0, 6)),
            ((0, 6), (0, 0)),
            ((2, 2), (4, 2)),
            ((4, 2), (4, 4)),
            ((4, 4), (2, 4)),
            ((2, 4), (2, 2)),
        ] {
            doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                pti(a.0, a.1),
                pti(b.0, b.1),
            ))));
        }
        let (boundary, holes) = trace_pick_region(&doc, 1.0, 1.0).expect("encloses the click");
        assert_eq!(holes.len(), 1, "the inner square is an island");
        assert!(
            region_contains(&boundary, &holes, 1.0, 1.0),
            "click is in the ring"
        );
        assert!(
            !region_contains(&boundary, &holes, 3.0, 3.0),
            "island interior is a hole"
        );
    }

    #[test]
    fn pick_region_merges_overlapping_island_circles() {
        let mut doc = Document::new();
        doc.add(rect_poly(0, 0, 20, 12));
        doc.add(full_circle(7.0, 6.0, 4.0));
        doc.add(full_circle(12.0, 6.0, 4.0));

        let (boundary, holes) = trace_pick_region(&doc, 2.0, 2.0).expect("corner is enclosed");
        assert_eq!(holes.len(), 1, "overlapping circles merge into one island");
        assert!(
            region_contains(&boundary, &holes, 2.0, 2.0),
            "corner is in the filled ring"
        );
        assert!(
            !region_contains(&boundary, &holes, 9.5, 6.0),
            "the lens (in both circles) must NOT be filled"
        );
        assert!(
            !region_contains(&boundary, &holes, 5.0, 6.0),
            "inside one circle (outside the lens) must NOT be filled"
        );
    }

    #[test]
    fn pick_inside_two_bare_overlapping_circles_fills_only_that_face() {
        let mut doc = Document::new();
        doc.add(full_circle(7.0, 6.0, 4.0));
        doc.add(full_circle(12.0, 6.0, 4.0));
        let (boundary, holes) =
            trace_pick_region(&doc, 4.0, 6.0).expect("the crescent encloses the click");
        assert!(
            region_contains(&boundary, &holes, 4.0, 6.0),
            "the picked crescent is filled"
        );
        assert!(
            !region_contains(&boundary, &holes, 9.5, 6.0),
            "the lens (shared with the other circle) must NOT be filled"
        );
    }

    #[test]
    fn arrangement_pick_recovers_source_arc() {
        let mut doc = Document::new();
        doc.add(full_circle(0.0, 0.0, 5.0));
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pti(-5, 0),
            pti(5, 0),
        ))));
        let (boundary, _holes) =
            trace_pick_region(&doc, 0.0, 2.0).expect("upper half-disc encloses the click");
        assert!(
            boundary.iter().any(|c| matches!(c, Curve::Arc(_))),
            "the curved side should be recovered as an arc, got {boundary:?}"
        );
        assert!(region_contains(&boundary, &[], 0.0, 2.0));
    }

    #[test]
    fn arrangement_pick_recovers_source_arc_lower_half() {
        let mut doc = Document::new();
        doc.add(full_circle(0.0, 0.0, 5.0));
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pti(-5, 0),
            pti(5, 0),
        ))));
        let (boundary, _holes) =
            trace_pick_region(&doc, 0.0, -2.0).expect("lower half-disc encloses the click");
        assert!(
            boundary.iter().any(|c| matches!(c, Curve::Arc(_))),
            "the curved side should be recovered as an arc, got {boundary:?}"
        );
        assert!(region_contains(&boundary, &[], 0.0, -2.0));
    }

    #[test]
    fn closed_loop_cut_by_open_line_uses_subface() {
        let mut doc = Document::new();
        doc.add(full_circle(0.0, 0.0, 5.0));
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pti(-5, 3),
            pti(5, 3),
        ))));
        let (boundary, holes) =
            trace_pick_region(&doc, 0.0, 4.0).expect("the cap encloses the click");
        assert!(holes.is_empty());
        assert!(region_contains(&boundary, &[], 0.0, 4.0), "cap is filled");
        assert!(
            !region_contains(&boundary, &[], 0.0, 0.0),
            "the circle center (below the chord) must NOT be in the picked cap"
        );
        assert!(
            boundary.iter().any(|c| matches!(c, Curve::Arc(_))),
            "the curved side of the cap is a real arc, got {boundary:?}"
        );
    }

    #[test]
    fn circle_triangulates() {
        let circle = vec![Curve::Arc(CircularArc::new(pti(0, 0), 5.0, 0.0, TAU))];
        let total: f64 = triangulate(&circle, &[]).iter().map(tri_area).sum();
        assert!(
            (total - std::f64::consts::PI * 25.0).abs() < 1.5,
            "disc area ≈ 78.5, got {total}"
        );
    }
}

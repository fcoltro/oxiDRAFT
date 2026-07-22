//! The interactive corner tool: detects fillet/chamfer-able corners in the
//! current selection (including adjacent segments of a single polycurve),
//! drives a live drag preview, and applies the result. Backs the
//! click-a-corner-and-drag UX rather than the FILLET/CHAMFER command-line tools.

use super::AppState;
use oxidraft_cad::edit::CornerEdge;
use oxidraft_document::{EntityId, EntityKind};
use oxidraft_geometry::{Curve, CurveSegment};

/// The geometry of one detected corner: the two edges meeting at it, their
/// directions and lengths away from the corner, and whether it's eligible
/// for a chamfer (both edges must be straight).
#[derive(Clone, Copy, Debug)]
pub struct CornerGeom {
    pub a: EntityId,
    pub b: EntityId,
    pub poly_seg: Option<usize>,
    pub corner: (f64, f64),
    pub dir_a: (f64, f64),
    pub len_a: f64,
    pub dir_b: (f64, f64),
    pub len_b: f64,
    pub chamfer_ok: bool,
    pub edge_a: CornerEdge,
    pub edge_b: CornerEdge,
}

/// Which corner operation is currently being applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerKind {
    Fillet,
    Chamfer,
}

/// An in-progress fillet/chamfer drag: the corner, which operation, and the
/// size dragged to so far.
#[derive(Clone, Copy, Debug)]
pub struct CornerAction {
    pub geom: CornerGeom,
    pub kind: CornerKind,
    pub size: f64,
}

impl CornerGeom {
    /// The largest fillet/chamfer size this corner alone can take before an
    /// edge would be consumed entirely.
    pub fn max_size(&self, kind: CornerKind) -> f64 {
        let min_len = self.len_a.min(self.len_b);
        match kind {
            CornerKind::Chamfer => min_len * 0.98,
            CornerKind::Fillet => {
                let half = self.interior_angle() * 0.5;
                (min_len * half.tan()).max(1e-6)
            }
        }
    }

    /// The angle between the two edges at the corner, in radians.
    pub fn interior_angle(&self) -> f64 {
        let cos = (self.dir_a.0 * self.dir_b.0 + self.dir_a.1 * self.dir_b.1).clamp(-1.0, 1.0);
        cos.acos()
    }
}

fn poly_uniform_max_size(group: &[CornerGeom], kind: CornerKind) -> f64 {
    use std::collections::HashMap;
    let by_seg: HashMap<usize, &CornerGeom> = group
        .iter()
        .filter_map(|c| c.poly_seg.map(|i| (i, c)))
        .collect();
    let mut segs: Vec<usize> = by_seg.keys().copied().collect();
    segs.sort_unstable();

    let mut cap = f64::INFINITY;
    for (k, &i) in segs.iter().enumerate() {
        let c = by_seg[&i];
        cap = cap.min(c.max_size(kind));
        let next = by_seg[&segs[(k + 1) % segs.len()]];
        if std::ptr::eq(c, next) {
            continue;
        }
        let l = c.len_b;
        let bound = match kind {
            CornerKind::Chamfer => l * 0.5,
            CornerKind::Fillet => {
                let cot = |g: &CornerGeom| 1.0 / (g.interior_angle() * 0.5).tan();
                let denom = cot(c) + cot(next);
                if denom > 1e-9 {
                    l / denom
                } else {
                    f64::INFINITY
                }
            }
        };
        cap = cap.min(bound);
    }
    (cap * 0.98).max(1e-3)
}

fn line_group_max_size(group: &[CornerGeom], kind: CornerKind) -> f64 {
    use std::collections::HashMap;
    let mut cap = group
        .iter()
        .map(|c| c.max_size(kind))
        .fold(f64::INFINITY, f64::min);

    let factor = |c: &CornerGeom| match kind {
        CornerKind::Chamfer => 1.0,
        CornerKind::Fillet => 1.0 / (c.interior_angle() * 0.5).tan(),
    };
    let mut uses: HashMap<EntityId, Vec<(f64, f64)>> = HashMap::new();
    for c in group {
        uses.entry(c.a).or_default().push((c.len_a, factor(c)));
        uses.entry(c.b).or_default().push((c.len_b, factor(c)));
    }
    for budget in uses.values() {
        if budget.len() < 2 {
            continue;
        }
        let l = budget.iter().map(|u| u.0).fold(f64::INFINITY, f64::min);
        let sum: f64 = budget.iter().map(|u| u.1).sum();
        if sum > 1e-9 {
            cap = cap.min(l / sum);
        }
    }
    (cap * 0.98).max(1e-3)
}

type Pt = (f64, f64);

/// Given a corner and the two unit directions leaving it along its edges,
/// returns the fillet arc's tangent points on each edge and its center, for
/// radius `r` — or `None` if the edges are (nearly) collinear or the corner
/// is degenerate.
pub fn fillet_arc(corner: Pt, da: Pt, db: Pt, r: f64) -> Option<(Pt, Pt, Pt)> {
    let cos = (da.0 * db.0 + da.1 * db.1).clamp(-1.0, 1.0);
    let half = cos.acos() * 0.5;
    let s = half.sin();
    let tan = half.tan();
    if s < 1e-6 || !tan.is_finite() || tan.abs() < 1e-9 {
        return None;
    }
    let t = r / tan;
    let p1 = (corner.0 + da.0 * t, corner.1 + da.1 * t);
    let p2 = (corner.0 + db.0 * t, corner.1 + db.1 * t);
    let (mut bx, mut by) = (da.0 + db.0, da.1 + db.1);
    let bl = (bx * bx + by * by).sqrt();
    if bl < 1e-9 {
        return None;
    }
    bx /= bl;
    by /= bl;
    let d = r / s;
    Some((p1, p2, (corner.0 + bx * d, corner.1 + by * d)))
}

struct EndInfo {
    pos: (f64, f64),
    dir: (f64, f64),
    len: f64,
    is_line: bool,
}

fn curve_ends(app: &AppState, id: EntityId) -> Vec<EndInfo> {
    let kind = match app.document.get(id) {
        Some(e) => &e.kind,
        None => return vec![],
    };
    match kind {
        EntityKind::Curve(Curve::Line(l)) => {
            let (p0, p1) = (l.p0.to_f64(), l.p1.to_f64());
            let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                return vec![];
            }
            let d = (dx / len, dy / len);
            vec![
                EndInfo {
                    pos: p0,
                    dir: d,
                    len,
                    is_line: true,
                },
                EndInfo {
                    pos: p1,
                    dir: (-d.0, -d.1),
                    len,
                    is_line: true,
                },
            ]
        }
        EntityKind::Curve(Curve::Arc(a)) => {
            let sweep = a.included_angle();
            if sweep >= std::f64::consts::TAU - 1e-6 {
                return vec![];
            }
            let len = a.radius * sweep;
            if len < 1e-9 {
                return vec![];
            }
            let (t0, t1) = (a.start_angle, a.end_angle);
            vec![
                EndInfo {
                    pos: a.start_point(),
                    dir: (-t0.sin(), t0.cos()),
                    len,
                    is_line: false,
                },
                EndInfo {
                    pos: a.end_point(),
                    dir: (t1.sin(), -t1.cos()),
                    len,
                    is_line: false,
                },
            ]
        }
        _ => vec![],
    }
}

fn seg_point(seg: &Curve, end: bool) -> (f64, f64) {
    let (t0, t1) = seg.domain();
    seg.evaluate_f64(if end { t1 } else { t0 })
}

fn edge_dir_len(edge: &CornerEdge, vertex: (f64, f64)) -> ((f64, f64), f64) {
    let sq = |p: (f64, f64)| (p.0 - vertex.0).powi(2) + (p.1 - vertex.1).powi(2);
    match *edge {
        CornerEdge::Line { p0, p1 } => {
            let far = if sq(p0) > sq(p1) { p0 } else { p1 };
            let (dx, dy) = (far.0 - vertex.0, far.1 - vertex.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                ((1.0, 0.0), 0.0)
            } else {
                ((dx / len, dy / len), len)
            }
        }
        CornerEdge::Arc {
            cx,
            cy,
            r,
            start,
            end,
        } => {
            let sp = (cx + r * start.cos(), cy + r * start.sin());
            let ep = (cx + r * end.cos(), cy + r * end.sin());
            let len = r * (end - start).abs();
            if sq(sp) <= sq(ep) {
                ((-start.sin(), start.cos()), len)
            } else {
                ((end.sin(), -end.cos()), len)
            }
        }
    }
}

impl AppState {
    /// Finds every fillet/chamfer-able corner in the current selection: both
    /// adjacent segments within a selected polycurve, and coincident
    /// endpoints between separately selected entities.
    pub fn detect_corners(&self) -> Vec<CornerGeom> {
        let mut out = Vec::new();

        for &id in &self.selection {
            if let Some(Curve::Poly(pc)) = self.document.get(id).and_then(|e| e.as_curve()) {
                self.poly_corners(id, pc, &mut out);
            }
            if out.len() > 64 {
                return out;
            }
        }

        if (2..=24).contains(&self.selection.len()) {
            let ends: Vec<(EntityId, CornerEdge, Vec<EndInfo>)> = self
                .selection
                .iter()
                .filter_map(|&id| {
                    let edge = CornerEdge::from_curve(self.document.get(id)?.as_curve()?)?;
                    Some((id, edge, curve_ends(self, id)))
                })
                .collect();
            let tol = 1e-6;
            for i in 0..ends.len() {
                for j in (i + 1)..ends.len() {
                    let (a, edge_a, ea) = &ends[i];
                    let (b, edge_b, eb) = &ends[j];
                    for fa in ea {
                        for fb in eb {
                            if (fa.pos.0 - fb.pos.0).hypot(fa.pos.1 - fb.pos.1) >= tol {
                                continue;
                            }
                            let cos = fa.dir.0 * fb.dir.0 + fa.dir.1 * fb.dir.1;
                            if cos.abs() > 0.999 {
                                continue;
                            }
                            out.push(CornerGeom {
                                a: *a,
                                b: *b,
                                poly_seg: None,
                                corner: fa.pos,
                                dir_a: fa.dir,
                                len_a: fa.len,
                                dir_b: fb.dir,
                                len_b: fb.len,
                                chamfer_ok: fa.is_line && fb.is_line,
                                edge_a: *edge_a,
                                edge_b: *edge_b,
                            });
                        }
                    }
                }
            }
        }
        out
    }

    fn poly_corners(
        &self,
        id: EntityId,
        pc: &oxidraft_geometry::PolyCurve,
        out: &mut Vec<CornerGeom>,
    ) {
        let n = pc.segments.len();
        if n < 2 {
            return;
        }
        let first = seg_point(&pc.segments[0], false);
        let last = seg_point(&pc.segments[n - 1], true);
        let closed = (first.0 - last.0).hypot(first.1 - last.1) < 1e-6;
        let count = if closed { n } else { n - 1 };
        for i in 0..count {
            let j = (i + 1) % n;
            let (edge_a, edge_b) = match (
                CornerEdge::from_curve(&pc.segments[i]),
                CornerEdge::from_curve(&pc.segments[j]),
            ) {
                (Some(a), Some(b)) => (a, b),
                _ => continue,
            };
            let ve = seg_point(&pc.segments[i], true);
            let vs = seg_point(&pc.segments[j], false);
            let corner = ((ve.0 + vs.0) * 0.5, (ve.1 + vs.1) * 0.5);
            let (dir_a, len_a) = edge_dir_len(&edge_a, corner);
            let (dir_b, len_b) = edge_dir_len(&edge_b, corner);
            let cos = dir_a.0 * dir_b.0 + dir_a.1 * dir_b.1;
            if cos.abs() > 0.999 {
                continue;
            }
            out.push(CornerGeom {
                a: id,
                b: id,
                poly_seg: Some(i),
                corner,
                dir_a,
                len_a,
                dir_b,
                len_b,
                chamfer_ok: edge_a.is_line() && edge_b.is_line(),
                edge_a,
                edge_b,
            });
        }
    }

    /// Other detected corners that share geometry with `geom` and must be
    /// resized together (the other corners of the same polycurve, or of the
    /// same chain of selected lines/arcs).
    pub fn corner_group(&self, geom: &CornerGeom, kind: CornerKind) -> Vec<CornerGeom> {
        let all = self.detect_corners();
        if geom.poly_seg.is_some() {
            all.into_iter()
                .filter(|c| c.poly_seg.is_some() && c.a == geom.a)
                .collect()
        } else {
            all.into_iter()
                .filter(|c| c.poly_seg.is_none())
                .filter(|c| kind == CornerKind::Fillet || c.chamfer_ok)
                .collect()
        }
    }

    /// The largest size `geom` can be dragged to without any edge in its
    /// [`corner_group`](Self::corner_group) being consumed entirely.
    pub fn corner_group_cap(&self, geom: &CornerGeom, kind: CornerKind) -> f64 {
        let group = self.corner_group(geom, kind);
        if group.len() <= 1 {
            return geom.max_size(kind);
        }
        if geom.poly_seg.is_some() {
            poly_uniform_max_size(&group, kind)
        } else {
            line_group_max_size(&group, kind)
        }
    }

    /// Starts an interactive fillet drag at `geom`, seeded with a starting
    /// size derived from the group's size cap.
    pub fn begin_corner_action(&mut self, geom: CornerGeom) {
        let cap = self.corner_group_cap(&geom, CornerKind::Fillet);
        let size = (cap * 0.3).max(1e-3);
        self.interaction.corner_action = Some(CornerAction {
            geom,
            kind: CornerKind::Fillet,
            size,
        });
    }

    /// Updates the in-progress corner action from the current cursor
    /// position: picks fillet or chamfer based on which side of the corner
    /// the cursor is on, and sets the size to the cursor's distance (capped).
    pub fn update_corner_drag(&mut self) {
        if let Some(mut ca) = self.interaction.corner_action {
            let (cx, cy) = ca.geom.corner;
            ca.kind = if !ca.geom.chamfer_ok || self.cursor_world.0 >= cx {
                CornerKind::Fillet
            } else {
                CornerKind::Chamfer
            };
            let d = (self.cursor_world.0 - cx).hypot(self.cursor_world.1 - cy);
            let cap = self.corner_group_cap(&ca.geom, ca.kind);
            ca.size = d.clamp(1e-3, cap);
            self.interaction.corner_action = Some(ca);
        }
    }

    /// Sets the in-progress corner action's size directly (e.g. from typed
    /// keyboard input), clamped to the group's cap.
    pub fn set_corner_size(&mut self, val: f64) {
        if let Some(mut ca) = self.interaction.corner_action {
            let cap = self.corner_group_cap(&ca.geom, ca.kind);
            ca.size = val.clamp(1e-3, cap);
            self.interaction.corner_action = Some(ca);
        }
    }

    /// Commits the in-progress corner action: applies the fillet or chamfer
    /// to every corner in its group and snapshots the result for undo.
    pub fn apply_corner_action(&mut self) {
        let Some(ca) = self.interaction.corner_action.take() else {
            return;
        };
        let g = ca.geom;
        let kind = ca.kind;
        let group = self.corner_group(&g, kind);
        if group.is_empty() {
            return;
        }
        let size = self.corner_group_cap(&g, kind).min(ca.size).max(1e-3);

        self.history.snapshot(&self.document);
        let is_poly = g.poly_seg.is_some();
        if is_poly {
            let mut idx: Vec<usize> = group.iter().filter_map(|c| c.poly_seg).collect();
            idx.sort_unstable();
            idx.dedup();
            for i in idx.into_iter().rev() {
                match kind {
                    CornerKind::Fillet => {
                        oxidraft_cad::edit::fillet_poly_corner(&mut self.document, g.a, i, size);
                    }
                    CornerKind::Chamfer => {
                        oxidraft_cad::edit::chamfer_poly_corner(&mut self.document, g.a, i, size);
                    }
                }
            }
        } else {
            for c in &group {
                match kind {
                    CornerKind::Fillet => {
                        if let Some(arc) = oxidraft_cad::edit::fillet(
                            &mut self.document,
                            c.a,
                            c.b,
                            size,
                            c.corner.0,
                            c.corner.1,
                        ) {
                            self.record_corner_constraints([c.a, c.b], arc, true);
                        }
                    }
                    CornerKind::Chamfer => {
                        if let Some(conn) =
                            oxidraft_cad::edit::chamfer(&mut self.document, c.a, c.b, size, size)
                        {
                            self.record_corner_constraints([c.a, c.b], conn, false);
                        }
                    }
                }
            }
            self.selection.clear();
        }
    }

    /// Aborts the in-progress corner action without modifying the document.
    pub fn cancel_corner_action(&mut self) {
        self.interaction.corner_action = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fillet_arc_right_angle() {
        let (p1, p2, c) = fillet_arc((0.0, 0.0), (1.0, 0.0), (0.0, 1.0), 2.0).unwrap();
        assert!((p1.0 - 2.0).abs() < 1e-9 && p1.1.abs() < 1e-9);
        assert!(p2.0.abs() < 1e-9 && (p2.1 - 2.0).abs() < 1e-9);
        assert!((c.0 - 2.0).abs() < 1e-9 && (c.1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn fillet_arc_rejects_straight() {
        assert!(fillet_arc((0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), 1.0).is_none());
    }
}

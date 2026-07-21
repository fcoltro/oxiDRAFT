//! A quadtree over curve bounding boxes, for fast rectangle, point, and
//! nearest-curve queries against a drawing.

use oxidraft_geometry::{BoundingBox, Curve, CurveSegment};

/// How a quadtree cell relates to the geometry it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellClass {
    /// No curve touches the cell.
    Empty,
    /// The cell is entirely inside covered geometry.
    Full,
    /// A curve crosses the cell's region.
    Boundary,
}

/// One node of the quadtree: its region, the curves indexed there, its
/// classification, and its four children (empty when a leaf).
#[derive(Clone, Debug)]
pub struct QuadNode {
    /// The rectangular region this node covers.
    pub bounds: BoundingBox,
    /// Indices (into [`Quadtree::curves`]) of curves overlapping this node.
    pub curve_indices: Vec<usize>,
    /// This node's classification.
    pub class: CellClass,
    /// Child nodes; empty for a leaf.
    pub children: Vec<QuadNode>,
}

impl QuadNode {
    /// True when this node has no children.
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

/// A quadtree index over a set of curves, subdividing space to answer spatial
/// queries without scanning every curve.
pub struct Quadtree {
    /// The root node covering the whole indexed region.
    pub root: QuadNode,
    /// All curves inserted, addressed by index.
    pub curves: Vec<Curve>,
    /// Per-curve bounding boxes, parallel to `curves`. Cached at insert:
    /// recomputing every box on every insert made building O(n²) in
    /// bounding-box evaluations.
    bbs: Vec<BoundingBox>,
    /// Deepest level the tree may subdivide to.
    pub max_depth: u32,
    /// A leaf splits once it holds more than this many curves (until
    /// `max_depth`).
    pub max_curves_per_leaf: usize,
}

impl Quadtree {
    /// An empty quadtree covering `bounds`, subdividing at most `max_depth`
    /// levels deep.
    pub fn new(bounds: BoundingBox, max_depth: u32) -> Self {
        Quadtree {
            root: QuadNode {
                bounds,
                curve_indices: Vec::new(),
                class: CellClass::Empty,
                children: Vec::new(),
            },
            curves: Vec::new(),
            bbs: Vec::new(),
            max_depth,
            max_curves_per_leaf: 4,
        }
    }

    /// Inserts a curve and returns its index, subdividing cells as needed.
    pub fn insert(&mut self, curve: Curve) -> usize {
        let idx = self.curves.len();
        let bb = curve.bounding_box();
        self.curves.push(curve);
        self.bbs.push(bb);
        let max_depth = self.max_depth;
        let max_per_leaf = self.max_curves_per_leaf;
        Self::insert_into(
            &mut self.root,
            &self.bbs,
            idx,
            &bb,
            0,
            max_depth,
            max_per_leaf,
        );
        idx
    }

    fn insert_into(
        node: &mut QuadNode,
        curve_bbs: &[BoundingBox],
        idx: usize,
        curve_bb: &BoundingBox,
        depth: u32,
        max_depth: u32,
        max_per_leaf: usize,
    ) {
        if !node.bounds.intersects(curve_bb) {
            return;
        }

        if node.is_leaf() {
            node.curve_indices.push(idx);
            node.class = CellClass::Boundary;
            if node.curve_indices.len() > max_per_leaf && depth < max_depth {
                Self::split_node(node, curve_bbs, depth, max_depth, max_per_leaf);
            }
        } else {
            for child in node.children.iter_mut() {
                Self::insert_into(
                    child,
                    curve_bbs,
                    idx,
                    curve_bb,
                    depth + 1,
                    max_depth,
                    max_per_leaf,
                );
            }
        }
    }

    fn split_node(
        node: &mut QuadNode,
        curve_bbs: &[BoundingBox],
        _depth: u32,
        _max_depth: u32,
        _max_per_leaf: usize,
    ) {
        let (x0, y0) = node.bounds.min.to_f64();
        let (x1, y1) = node.bounds.max.to_f64();
        let mx = (x0 + x1) / 2.0;
        let my = (y0 + y1) / 2.0;

        let quads = [
            BoundingBox::from_corners(x0, y0, mx, my),
            BoundingBox::from_corners(mx, y0, x1, my),
            BoundingBox::from_corners(x0, my, mx, y1),
            BoundingBox::from_corners(mx, my, x1, y1),
        ];

        node.children = quads
            .into_iter()
            .map(|bb| QuadNode {
                bounds: bb,
                curve_indices: Vec::new(),
                class: CellClass::Empty,
                children: Vec::new(),
            })
            .collect();

        let indices = std::mem::take(&mut node.curve_indices);
        for &idx in &indices {
            if let Some(bb) = curve_bbs.get(idx) {
                for child in node.children.iter_mut() {
                    if child.bounds.intersects(bb) {
                        child.curve_indices.push(idx);
                        child.class = CellClass::Boundary;
                    }
                }
            }
        }
        // A split must actually separate something and must not strand a
        // curve outside every quadrant (a poisoned box intersects nothing).
        // Without the first check, curves that span every quadrant — think
        // page-sized construction lines — deepen the tree by one full level
        // per insert and balloon it toward 4^max_depth nodes; such nodes
        // stay fat leaves instead (later inserts may retry, which is O(n)
        // per attempt, not exponential).
        let separated = node
            .children
            .iter()
            .any(|ch| ch.curve_indices.len() < indices.len());
        let all_placed = indices.iter().all(|idx| {
            node.children
                .iter()
                .any(|ch| ch.curve_indices.contains(idx))
        });
        if !(separated && all_placed) {
            node.children.clear();
            node.curve_indices = indices;
        }
    }

    /// Indices of all curves whose bounding box overlaps `query_bb`.
    pub fn query_rect(&self, query_bb: &BoundingBox) -> Vec<usize> {
        let mut candidates = Vec::new();
        Self::query_node(&self.root, query_bb, &mut candidates);
        candidates.sort_unstable();
        candidates.dedup();
        candidates.retain(|&idx| {
            self.curves
                .get(idx)
                .map(|c| c.bounding_box().intersects(query_bb))
                .unwrap_or(false)
        });
        candidates
    }

    fn query_node(node: &QuadNode, query_bb: &BoundingBox, results: &mut Vec<usize>) {
        if !node.bounds.intersects(query_bb) {
            return;
        }
        if node.is_leaf() {
            results.extend_from_slice(&node.curve_indices);
        } else {
            let mut ordered: Vec<(u64, usize)> = node
                .children
                .iter()
                .enumerate()
                .map(|(i, ch)| {
                    let (cx, cy) = ch.bounds.min.to_f64();
                    let gx = (cx * 1000.0).max(0.0) as u32;
                    let gy = (cy * 1000.0).max(0.0) as u32;
                    (crate::morton::morton_code(gx, gy), i)
                })
                .collect();
            ordered.sort_by_key(|&(m, _)| m);
            for (_, i) in ordered {
                Self::query_node(&node.children[i], query_bb, results);
            }
        }
    }

    /// The deepest leaf node containing point `(px, py)`, or `None` if outside
    /// the tree's bounds.
    pub fn query_point(&self, px: f64, py: f64) -> Option<&QuadNode> {
        Self::find_leaf(&self.root, px, py)
    }

    fn find_leaf(node: &QuadNode, px: f64, py: f64) -> Option<&QuadNode> {
        if !node.bounds.contains_point_f64(px, py) {
            return None;
        }
        if node.is_leaf() {
            return Some(node);
        }
        for child in &node.children {
            if let Some(leaf) = Self::find_leaf(child, px, py) {
                return Some(leaf);
            }
        }
        None
    }

    /// Index of the curve nearest to point `(px, py)`, using the tree to prune
    /// far-away candidates, or `None` when the tree is empty.
    pub fn nearest_curve(&self, px: f64, py: f64) -> Option<usize> {
        use oxidraft_geometry::point_to_curve_distance;
        // A non-finite query would make every distance NaN and the winner
        // arbitrary; there is no meaningful "nearest" to such a point.
        if !(px.is_finite() && py.is_finite()) {
            return None;
        }
        let mut candidates = {
            let mut bb = BoundingBox::from_corners(px - 0.001, py - 0.001, px + 0.001, py + 0.001);
            let mut result = Vec::new();
            for _ in 0..8 {
                result = self.query_rect(&bb);
                if !result.is_empty() {
                    break;
                }
                let (bx0, by0) = bb.min.to_f64();
                let (bx1, by1) = bb.max.to_f64();
                let w = bx1 - bx0;
                bb = BoundingBox::from_corners(bx0 - w, by0 - w, bx1 + w, by1 + w);
            }
            result
        };
        // The ring search only reaches a few units out; a drawing whose
        // curves are all farther away still has a nearest one — fall back
        // to brute force so the answer stays total.
        if candidates.is_empty() {
            candidates = (0..self.curves.len()).collect();
        }
        candidates.into_iter().min_by(|&a, &b| {
            let da = point_to_curve_distance(&self.curves[a], px, py);
            let db = point_to_curve_distance(&self.curves[b], px, py);
            da.total_cmp(&db)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_geometry::{LineSeg, Point2d};

    fn make_tree() -> Quadtree {
        Quadtree::new(BoundingBox::from_corners(-100.0, -100.0, 100.0, 100.0), 12)
    }

    #[test]
    fn insert_and_query() {
        let mut qt = make_tree();
        let seg = Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(0, 0),
            Point2d::from_i64(10, 10),
        ));
        let idx = qt.insert(seg);
        let results = qt.query_rect(&BoundingBox::from_corners(0.0, 0.0, 15.0, 15.0));
        assert!(results.contains(&idx));
    }

    #[test]
    fn query_empty_region() {
        let mut qt = make_tree();
        qt.insert(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(50, 50),
            Point2d::from_i64(60, 60),
        )));
        let results = qt.query_rect(&BoundingBox::from_corners(-10.0, -10.0, 0.0, 0.0));
        assert!(results.is_empty());
    }

    #[test]
    fn morton_codes_order() {
        use crate::morton::morton_code;
        assert!(morton_code(0, 0) < morton_code(1, 0));
        assert!(morton_code(0, 0) < morton_code(0, 1));
        assert!(morton_code(1, 1) == morton_code(1, 0) + morton_code(0, 1));
    }
}

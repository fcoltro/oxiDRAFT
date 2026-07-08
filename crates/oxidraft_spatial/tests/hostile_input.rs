//! Hostile-input battery for the spatial index. Companion to the other
//! crates' batteries: whatever gets inserted — page-spanning construction
//! lines, poisoned coordinates, coincident stacks — the tree must stay
//! bounded in size, never panic or hang, and keep healthy curves findable.

use oxidraft_geometry::{BoundingBox, Curve, LineSeg, Point2d};
use oxidraft_spatial::{QuadNode, Quadtree};

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Curve {
    Curve::Line(LineSeg::from_endpoints(
        Point2d::from_f64(x0, y0),
        Point2d::from_f64(x1, y1),
    ))
}

fn tree() -> Quadtree {
    Quadtree::new(BoundingBox::from_corners(-100.0, -100.0, 100.0, 100.0), 12)
}

fn count_nodes(n: &QuadNode) -> usize {
    1 + n.children.iter().map(count_nodes).sum::<usize>()
}

#[test]
fn broad_curves_cannot_balloon_the_tree() {
    let mut qt = tree();
    // Every curve's box spans all four quadrants at every depth: splitting
    // separates nothing, so the node must stay a fat leaf instead of
    // deepening one level per insert toward 4^12 nodes.
    for i in 0..64 {
        qt.insert(line(-99.0, -99.0 + i as f64 * 0.01, 99.0, 99.0));
    }
    let nodes = count_nodes(&qt.root);
    assert!(nodes < 100, "tree ballooned to {nodes} nodes");
    let all = qt.query_rect(&BoundingBox::from_corners(-100.0, -100.0, 100.0, 100.0));
    assert_eq!(all.len(), 64, "every broad curve stays findable");
}

#[test]
fn mixed_broad_and_local_curves_stay_bounded_and_queryable() {
    let mut qt = tree();
    for i in 0..8 {
        qt.insert(line(-99.0, -99.0 + i as f64, 99.0, 99.0));
    }
    let mut locals = Vec::new();
    for i in 0..200 {
        let x = -90.0 + (i % 20) as f64 * 9.0;
        let y = -90.0 + (i / 20) as f64 * 18.0;
        locals.push(qt.insert(line(x, y, x + 1.0, y + 1.0)));
    }
    let nodes = count_nodes(&qt.root);
    assert!(nodes < 4000, "mixed tree ballooned to {nodes} nodes");
    // A local query returns its local curve (broad extras are fine).
    let hits = qt.query_rect(&BoundingBox::from_corners(-90.5, -90.5, -88.5, -88.5));
    assert!(hits.contains(&locals[0]), "local curve findable: {hits:?}");
}

#[test]
fn poisoned_curves_do_not_break_queries() {
    let mut qt = tree();
    qt.insert(line(f64::NAN, 0.0, 1.0, 1.0));
    qt.insert(line(f64::NEG_INFINITY, -1.0, f64::INFINITY, 1.0));
    qt.insert(line(1.0, 1.0, 1.0, 1.0)); // zero length
    let good = qt.insert(line(50.0, 50.0, 60.0, 60.0));

    let nan_rect = BoundingBox::from_corners(f64::NAN, f64::NAN, f64::NAN, f64::NAN);
    let _ = qt.query_rect(&nan_rect);
    let _ = qt.query_rect(&BoundingBox::from_corners(10.0, 10.0, -10.0, -10.0));
    let _ = qt.query_point(f64::NAN, f64::NAN);
    let _ = qt.query_point(1e300, -1e300);
    assert_eq!(qt.nearest_curve(f64::NAN, 0.0), None, "no nearest to NaN");

    let hits = qt.query_rect(&BoundingBox::from_corners(45.0, 45.0, 65.0, 65.0));
    assert!(hits.contains(&good), "poison must not hide healthy curves");
    assert_eq!(qt.nearest_curve(55.0, 56.0), Some(good));
}

#[test]
fn nearest_curve_reaches_distant_curves() {
    let mut qt = tree();
    let far = qt.insert(line(90.0, 90.0, 95.0, 95.0));
    // The ring search around (-90, -90) never gets close; the brute-force
    // fallback must still name the (only) curve.
    assert_eq!(qt.nearest_curve(-90.0, -90.0), Some(far));
    assert_eq!(tree().nearest_curve(0.0, 0.0), None, "empty tree has none");
}

#[test]
fn coincident_stacks_respect_the_depth_cap() {
    let mut qt = tree();
    for _ in 0..50 {
        qt.insert(line(3.0, 3.0, 3.1, 3.1));
    }
    fn depth(n: &QuadNode) -> u32 {
        1 + n.children.iter().map(depth).max().unwrap_or(0)
    }
    assert!(depth(&qt.root) <= 13, "depth cap held");
    let nodes = count_nodes(&qt.root);
    assert!(nodes < 200, "stacked inserts ballooned to {nodes} nodes");
    let hits = qt.query_rect(&BoundingBox::from_corners(2.9, 2.9, 3.2, 3.2));
    assert_eq!(hits.len(), 50, "the whole stack stays findable");
}

#[test]
fn hostile_tree_bounds_never_panic() {
    for bounds in [
        BoundingBox::from_corners(f64::NAN, f64::NAN, f64::NAN, f64::NAN),
        BoundingBox::from_corners(0.0, 0.0, 0.0, 0.0),
        BoundingBox::from_corners(10.0, 10.0, -10.0, -10.0),
    ] {
        let mut qt = Quadtree::new(bounds, 12);
        for i in 0..10 {
            qt.insert(line(0.0, i as f64, 4.0, i as f64));
        }
        let _ = qt.query_rect(&BoundingBox::from_corners(-5.0, -5.0, 5.0, 5.0));
        let _ = qt.query_point(0.0, 0.0);
        let _ = qt.nearest_curve(0.0, 0.0);
    }
}

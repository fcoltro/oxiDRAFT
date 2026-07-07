//! Boolean pipeline benchmarks: the curved trim-and-stitch path, the
//! polyline fallback, and winding-number containment.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use oxidraft_boolean::{Region, union};
use oxidraft_geometry::{CircularArc, Curve, LineSeg, Point2d};

fn circle(cx: f64, cy: f64, r: f64) -> Region {
    Region::new(vec![Curve::Arc(CircularArc::new(
        Point2d::from_f64(cx, cy),
        r,
        0.0,
        std::f64::consts::TAU,
    ))])
}

fn ngon(cx: f64, cy: f64, r: f64, n: usize) -> Region {
    let pts: Vec<Point2d> = (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            Point2d::from_f64(cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    Region::new(
        (0..n)
            .map(|i| Curve::Line(LineSeg::from_endpoints(pts[i], pts[(i + 1) % n])))
            .collect(),
    )
}

fn curved_union(c: &mut Criterion) {
    let a = circle(0.0, 0.0, 4.0);
    let b = circle(5.0, 0.0, 4.0);
    c.bench_function("union_two_circles_curved_path", |bch| {
        bch.iter(|| union(black_box(&a), black_box(&b)))
    });
}

fn polygon_union(c: &mut Criterion) {
    let a = ngon(0.0, 0.0, 4.0, 64);
    let b = ngon(5.0, 0.0, 4.0, 64);
    c.bench_function("union_two_64gons", |bch| {
        bch.iter(|| union(black_box(&a), black_box(&b)))
    });
}

fn containment_queries(c: &mut Criterion) {
    let region = circle(0.0, 0.0, 5.0);
    // Warm the prepared-boundary cache once, then measure steady-state.
    region.contains_point(0.0, 0.0);
    c.bench_function("contains_point_circle_1k_queries", |bch| {
        bch.iter(|| {
            let mut hits = 0u32;
            for k in 0..1000 {
                let x = -8.0 + 16.0 * (k as f64 / 999.0);
                if region.contains_point(black_box(x), 0.7) {
                    hits += 1;
                }
            }
            black_box(hits)
        })
    });
}

criterion_group!(benches, curved_union, polygon_union, containment_queries);
criterion_main!(benches);

//! Kernel micro-benchmarks guarding the hot paths the performance work
//! optimised: run `cargo bench -p oxidraft_geometry` before and after
//! touching them.

use criterion::{Criterion, criterion_group, criterion_main};
use oxidraft_geometry::{
    CircularArc, Curve, CurveSegment, LineSeg, NurbsCurve, Point2d, intersect,
    project_point_onto_curve, tessellate_curve,
};
use std::hint::black_box;

fn arc_tessellation(c: &mut Criterion) {
    let arc = Curve::Arc(CircularArc::new(
        Point2d::from_f64(0.0, 0.0),
        50.0,
        0.0,
        std::f64::consts::TAU,
    ));
    c.bench_function("tessellate_full_circle_r50_tol_1e-3", |b| {
        b.iter(|| tessellate_curve(black_box(&arc), 1e-3))
    });
}

fn nurbs_evaluation(c: &mut Criterion) {
    let cvs: Vec<Point2d> = (0..24)
        .map(|i| Point2d::from_f64(i as f64, if i % 2 == 0 { -1.0 } else { 1.0 }))
        .collect();
    let spline = Curve::Nurbs(NurbsCurve::uniform(cvs));
    c.bench_function("nurbs_24cv_evaluate_256_samples", |b| {
        b.iter(|| {
            let mut acc = 0.0;
            for k in 0..256 {
                let (x, y) = spline.evaluate_f64(k as f64 / 255.0);
                acc += x + y;
            }
            black_box(acc)
        })
    });
}

fn polyline_projection(c: &mut Criterion) {
    use oxidraft_geometry::PolyCurve;
    let segs: Vec<Curve> = (0..100)
        .map(|i| {
            let x = i as f64;
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(x, (i % 2) as f64),
                Point2d::from_f64(x + 1.0, ((i + 1) % 2) as f64),
            ))
        })
        .collect();
    let poly = Curve::Poly(Box::new(PolyCurve::new(segs)));
    c.bench_function("project_point_onto_100_segment_polyline", |b| {
        b.iter(|| project_point_onto_curve(black_box(&poly), 73.4, 5.0))
    });
}

fn spline_line_intersection(c: &mut Criterion) {
    let cvs: Vec<Point2d> = (0..40)
        .map(|i| Point2d::from_f64(i as f64, if i % 2 == 0 { -1.0 } else { 1.0 }))
        .collect();
    let spline = Curve::Nurbs(NurbsCurve::uniform(cvs));
    let line = Curve::Line(LineSeg::from_endpoints(
        Point2d::from_f64(-1.0, 0.0),
        Point2d::from_f64(40.0, 0.0),
    ));
    c.bench_function("intersect_40cv_spline_with_line", |b| {
        b.iter(|| intersect(black_box(&spline), black_box(&line)))
    });
}

criterion_group!(
    benches,
    arc_tessellation,
    nurbs_evaluation,
    polyline_projection,
    spline_line_intersection
);
criterion_main!(benches);

use crate::curve::CurveSegment;
use crate::point::{BoundingBox, Point2d};

#[derive(Clone, Debug, PartialEq)]
pub struct CubicBezier {
    pub p0: Point2d,
    pub p1: Point2d,
    pub p2: Point2d,
    pub p3: Point2d,
}

impl CubicBezier {
    pub fn new(p0: Point2d, p1: Point2d, p2: Point2d, p3: Point2d) -> Self {
        CubicBezier { p0, p1, p2, p3 }
    }

    pub fn evaluate_exact(&self, t: f64) -> Point2d {
        let (x, y) = self.evaluate_f64(t);
        Point2d::new(x, y)
    }

    pub fn split_at_exact(&self, t: f64) -> (CubicBezier, CubicBezier) {
        let lerp = |a: &Point2d, b: &Point2d| a.lerp(b, t);

        let q0 = lerp(&self.p0, &self.p1);
        let q1 = lerp(&self.p1, &self.p2);
        let q2 = lerp(&self.p2, &self.p3);
        let r0 = lerp(&q0, &q1);
        let r1 = lerp(&q1, &q2);
        let s = lerp(&r0, &r1);

        (
            CubicBezier {
                p0: self.p0,
                p1: q0,
                p2: r0,
                p3: s,
            },
            CubicBezier {
                p0: s,
                p1: r1,
                p2: q2,
                p3: self.p3,
            },
        )
    }

    pub fn degree_elevate(&self) -> [Point2d; 5] {
        let q0 = self.p0;
        let q1 = self.p0.lerp(&self.p1, 0.75);
        let q2 = self.p1.lerp(&self.p2, 0.5);
        let q3 = self.p2.lerp(&self.p3, 0.25);
        let q4 = self.p3;
        [q0, q1, q2, q3, q4]
    }
}

impl CurveSegment for CubicBezier {
    fn domain(&self) -> (f64, f64) {
        (0.0, 1.0)
    }

    fn evaluate_f64(&self, t: f64) -> (f64, f64) {
        let (x0, y0) = self.p0.to_f64();
        let (x1, y1) = self.p1.to_f64();
        let (x2, y2) = self.p2.to_f64();
        let (x3, y3) = self.p3.to_f64();

        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;

        let t2 = t * t;
        let t3 = t2 * t;

        let x = mt3 * x0 + 3.0 * mt2 * t * x1 + 3.0 * mt * t2 * x2 + t3 * x3;
        let y = mt3 * y0 + 3.0 * mt2 * t * y1 + 3.0 * mt * t2 * y2 + t3 * y3;
        (x, y)
    }

    fn bounding_box(&self) -> BoundingBox {
        let (x0, y0) = self.p0.to_f64();
        let (x3, y3) = self.p3.to_f64();
        let mut xmin = x0.min(x3);
        let mut xmax = x0.max(x3);
        let mut ymin = y0.min(y3);
        let mut ymax = y0.max(y3);

        for &t in &deriv_roots(self.p0.x, self.p1.x, self.p2.x, self.p3.x) {
            let (x, _) = self.evaluate_f64(t);
            xmin = xmin.min(x);
            xmax = xmax.max(x);
        }
        for &t in &deriv_roots(self.p0.y, self.p1.y, self.p2.y, self.p3.y) {
            let (_, y) = self.evaluate_f64(t);
            ymin = ymin.min(y);
            ymax = ymax.max(y);
        }
        BoundingBox::from_corners(xmin, ymin, xmax, ymax)
    }

    fn tangent_f64(&self, t: f64) -> (f64, f64) {
        let (x0, y0) = self.p0.to_f64();
        let (x1, y1) = self.p1.to_f64();
        let (x2, y2) = self.p2.to_f64();
        let (x3, y3) = self.p3.to_f64();

        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let t2 = t * t;

        let x = 3.0 * mt2 * (x1 - x0) + 6.0 * mt * t * (x2 - x1) + 3.0 * t2 * (x3 - x2);
        let y = 3.0 * mt2 * (y1 - y0) + 6.0 * mt * t * (y2 - y1) + 3.0 * t2 * (y3 - y2);
        (x, y)
    }

    fn arc_length(&self) -> f64 {
        const NODES: [f64; 5] = [0.046910077, 0.230765346, 0.5, 0.769234654, 0.953089923];
        const WEIGHTS: [f64; 5] = [
            0.118463442,
            0.239314335,
            0.284444444,
            0.239314335,
            0.118463442,
        ];
        NODES.iter().zip(WEIGHTS.iter()).fold(0.0, |acc, (&t, &w)| {
            let (dx, dy) = self.tangent_f64(t);
            acc + w * (dx * dx + dy * dy).sqrt()
        })
    }
}

fn deriv_roots(c0: f64, c1: f64, c2: f64, c3: f64) -> Vec<f64> {
    let a = -c0 + 3.0 * c1 - 3.0 * c2 + c3;
    let b = 2.0 * c0 - 4.0 * c1 + 2.0 * c2;
    let c = c1 - c0;
    let mut out = Vec::new();
    let mut push = |t: f64| {
        if t > 0.0 && t < 1.0 {
            out.push(t);
        }
    };
    if a.abs() < 1e-12 {
        if b.abs() > 1e-12 {
            push(-c / b);
        }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let sq = disc.sqrt();
            push((-b + sq) / (2.0 * a));
            push((-b - sq) / (2.0 * a));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn evaluate_endpoints() {
        let bz = CubicBezier::new(pt(0, 0), pt(1, 2), pt(3, 4), pt(4, 0));
        let p0 = bz.evaluate_exact(0.0);
        let p1 = bz.evaluate_exact(1.0);
        assert_eq!(p0, pt(0, 0));
        assert_eq!(p1, pt(4, 0));
    }

    #[test]
    fn split_reconstructs_original() {
        let bz = CubicBezier::new(pt(0, 0), pt(1, 3), pt(3, 3), pt(4, 0));
        let (left, right) = bz.split_at_exact(0.5);
        assert_eq!(left.p0, bz.p0);
        assert_eq!(right.p3, bz.p3);
        let mid_orig = bz.evaluate_exact(0.5);
        assert_eq!(left.p3, mid_orig);
        assert_eq!(right.p0, mid_orig);
    }

    #[test]
    fn degree_elevate_preserves_shape() {
        let bz = CubicBezier::new(pt(0, 0), pt(1, 2), pt(3, 2), pt(4, 0));
        let elevated = bz.degree_elevate();
        assert_eq!(elevated[0], bz.p0);
        assert_eq!(elevated[4], bz.p3);
        for &t_val in &[0.25f64, 0.5, 0.75] {
            let orig = bz.evaluate_exact(t_val);
            let (ox, oy) = orig.to_f64();
            let (ex, ey) = bz.evaluate_f64(t_val);
            assert!((ox - ex).abs() < 1e-10, "x mismatch at t={}", t_val);
            assert!((oy - ey).abs() < 1e-10, "y mismatch at t={}", t_val);
        }
    }

    #[test]
    fn bounding_box_contains_all_points() {
        let bz = CubicBezier::new(pt(0, 0), pt(2, 4), pt(3, 4), pt(5, 0));
        let bb = bz.bounding_box();
        for i in 0..=20 {
            let t = i as f64 / 20.0;
            let (x, y) = bz.evaluate_f64(t);
            assert!(
                bb.contains_point_f64(x, y),
                "t={}: ({},{}) outside {:?}",
                t,
                x,
                y,
                bb
            );
        }
    }

    #[test]
    fn arc_length_straight_line() {
        let bz = CubicBezier::new(
            Point2d::from_i64(0, 0),
            Point2d::new(4.0 / 3.0, 0.0),
            Point2d::new(8.0 / 3.0, 0.0),
            Point2d::from_i64(4, 0),
        );
        assert!(
            (bz.arc_length() - 4.0).abs() < 1e-5,
            "length={}",
            bz.arc_length()
        );
    }
}

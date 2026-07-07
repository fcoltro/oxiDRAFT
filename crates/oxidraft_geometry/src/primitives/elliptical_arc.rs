use crate::curve::CurveSegment;
use crate::point::{BoundingBox, Point2d};

#[derive(Clone, Copy, Debug)]
pub struct EllipticalArc {
    pub center: Point2d,
    pub semi_major: f64,
    pub semi_minor: f64,
    pub rotation: f64,
    pub start_angle: f64,
    pub end_angle: f64,
}

impl EllipticalArc {
    pub fn new(
        center: Point2d,
        semi_major: f64,
        semi_minor: f64,
        rotation: f64,
        start_angle: f64,
        end_angle: f64,
    ) -> Self {
        EllipticalArc {
            center,
            semi_major,
            semi_minor,
            rotation,
            start_angle,
            end_angle,
        }
    }

    pub fn axis_aligned(
        center: Point2d,
        semi_major: f64,
        semi_minor: f64,
        start_angle: f64,
        end_angle: f64,
    ) -> Self {
        EllipticalArc {
            center,
            semi_major,
            semi_minor,
            rotation: 0.0,
            start_angle,
            end_angle,
        }
    }

    pub fn foci(&self) -> ((f64, f64), (f64, f64)) {
        let a = self.semi_major;
        let b = self.semi_minor;
        let c = (a * a - b * b).max(0.0).sqrt();
        let (cx, cy) = self.center.to_f64();
        let phi = self.rotation;
        let f1 = (cx + c * phi.cos(), cy + c * phi.sin());
        let f2 = (cx - c * phi.cos(), cy - c * phi.sin());
        (f1, f2)
    }

    pub fn eccentricity(&self) -> f64 {
        let a = self.semi_major;
        let b = self.semi_minor;
        let c = (a * a - b * b).max(0.0).sqrt();
        c / a
    }

    pub fn included_angle(&self) -> f64 {
        let mut a = self.end_angle - self.start_angle;
        while a <= 0.0 {
            a += 2.0 * std::f64::consts::PI;
        }
        a
    }
}

impl CurveSegment for EllipticalArc {
    fn domain(&self) -> (f64, f64) {
        (self.start_angle, self.end_angle)
    }

    fn evaluate_f64(&self, t: f64) -> (f64, f64) {
        let (cx, cy) = self.center.to_f64();
        let a = self.semi_major;
        let b = self.semi_minor;
        let phi = self.rotation;
        let u = a * t.cos();
        let v = b * t.sin();
        let x = cx + u * phi.cos() - v * phi.sin();
        let y = cy + u * phi.sin() + v * phi.cos();
        (x, y)
    }

    fn bounding_box(&self) -> BoundingBox {
        let steps = 64usize;
        let (t0, t1) = (self.start_angle, self.start_angle + self.included_angle());
        let mut xmin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymin = f64::INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        for i in 0..=steps {
            let t = t0 + (t1 - t0) * i as f64 / steps as f64;
            let (x, y) = self.evaluate_f64(t);
            xmin = xmin.min(x);
            xmax = xmax.max(x);
            ymin = ymin.min(y);
            ymax = ymax.max(y);
        }
        BoundingBox::from_corners(xmin, ymin, xmax, ymax)
    }

    fn tangent_f64(&self, t: f64) -> (f64, f64) {
        let a = self.semi_major;
        let b = self.semi_minor;
        let phi = self.rotation;
        let du = -a * t.sin();
        let dv = b * t.cos();
        let dx = du * phi.cos() - dv * phi.sin();
        let dy = du * phi.sin() + dv * phi.cos();
        (dx, dy)
    }

    fn arc_length(&self) -> f64 {
        let steps = 128usize;
        let (t0, t1) = (self.start_angle, self.start_angle + self.included_angle());
        let dt = (t1 - t0) / steps as f64;
        let mut length = 0.0;
        for i in 0..steps {
            let t = t0 + dt * (i as f64 + 0.5);
            let (dx, dy) = self.tangent_f64(t);
            length += (dx * dx + dy * dy).sqrt() * dt;
        }
        length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foci_axis_aligned() {
        let ell = EllipticalArc::axis_aligned(
            Point2d::from_i64(0, 0),
            5.0,
            4.0,
            0.0,
            2.0 * std::f64::consts::PI,
        );
        let ((f1x, f1y), (f2x, f2y)) = ell.foci();
        assert!((f1x.abs() - 3.0).abs() < 1e-8);
        assert!(f1y.abs() < 1e-8);
        assert!((f2x.abs() - 3.0).abs() < 1e-8);
        assert!(f2y.abs() < 1e-8);
    }

    #[test]
    fn eccentricity() {
        let circle = EllipticalArc::axis_aligned(
            Point2d::from_i64(0, 0),
            5.0,
            5.0,
            0.0,
            2.0 * std::f64::consts::PI,
        );
        assert!(circle.eccentricity().abs() < 1e-10);
    }

    #[test]
    fn foci_finite_when_minor_exceeds_major() {
        let ell = EllipticalArc::axis_aligned(
            Point2d::from_i64(0, 0),
            4.0,
            5.0,
            0.0,
            2.0 * std::f64::consts::PI,
        );
        let ((f1x, f1y), (f2x, f2y)) = ell.foci();
        assert!(f1x.is_finite() && f1y.is_finite() && f2x.is_finite() && f2y.is_finite());
    }
}

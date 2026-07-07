use crate::curve::CurveSegment;
use crate::error::GeomError;
use crate::point::{BoundingBox, Point2d};

#[derive(Clone, Copy, Debug)]
pub struct CircularArc {
    pub center: Point2d,
    pub radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
}

impl CircularArc {
    /// Trusted-caller constructor. Panics on a non-positive radius; use
    /// [`CircularArc::try_new`] when the radius comes from untrusted input.
    pub fn new(center: Point2d, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        Self::try_new(center, radius, start_angle, end_angle).expect("Radius must be positive")
    }

    /// Fallible constructor: returns [`GeomError::NonPositiveRadius`] instead of panicking.
    pub fn try_new(
        center: Point2d,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    ) -> Result<Self, GeomError> {
        if radius.is_nan() || radius <= 0.0 {
            return Err(GeomError::NonPositiveRadius(radius));
        }
        Ok(CircularArc {
            center,
            radius,
            start_angle,
            end_angle,
        })
    }

    pub fn from_three_points(p1: &Point2d, p2: &Point2d, p3: &Point2d) -> Option<Self> {
        let ax = p2.x - p1.x;
        let ay = p2.y - p1.y;
        let bx = p3.x - p2.x;
        let by = p3.y - p2.y;

        let r1 = (ax * (p1.x + p2.x) + ay * (p1.y + p2.y)) / 2.0;
        let r2 = (bx * (p2.x + p3.x) + by * (p2.y + p3.y)) / 2.0;

        // Relative collinearity test: an absolute 1e-12 floor wrongly flags large-
        // coordinate triangles as collinear. Scale the determinant tolerance by the
        // magnitudes of the two edge vectors it is built from.
        let det = ax * by - ay * bx;
        let scale = (ax * ax + ay * ay).sqrt() * (bx * bx + by * by).sqrt();
        if det.abs() <= 1e-12 * scale.max(1.0) {
            return None;
        }

        let cx = (r1 * by - r2 * ay) / det;
        let cy = (ax * r2 - bx * r1) / det;

        let center = Point2d { x: cx, y: cy };
        let radius = center.dist_f64(p1);

        let angle_of = |p: &Point2d| (p.y - center.y).atan2(p.x - center.x);
        let a1 = angle_of(p1);
        let a2 = angle_of(p2);
        let a3 = angle_of(p3);

        let pi2 = 2.0 * std::f64::consts::PI;
        let lift = |start: f64, mut end: f64| {
            while end <= start {
                end += pi2;
            }
            end
        };
        let on_arc = |start: f64, end: f64, mut a: f64| {
            while a < start {
                a += pi2;
            }
            a <= end + 1e-12
        };

        let (start_angle, end_angle) = {
            let e1 = lift(a1, a3);
            if on_arc(a1, e1, a2) {
                (a1, e1)
            } else {
                (a3, lift(a3, a1))
            }
        };

        Some(CircularArc {
            center,
            radius,
            start_angle,
            end_angle,
        })
    }

    pub fn start_point(&self) -> (f64, f64) {
        self.evaluate_f64(self.start_angle)
    }

    pub fn end_point(&self) -> (f64, f64) {
        self.evaluate_f64(self.end_angle)
    }

    pub fn included_angle(&self) -> f64 {
        crate::util::positive_sweep(self.end_angle - self.start_angle)
    }

    pub fn sagitta(&self) -> f64 {
        let r = self.radius;
        r - r * (self.included_angle() / 2.0).cos()
    }
}

impl CurveSegment for CircularArc {
    fn domain(&self) -> (f64, f64) {
        (self.start_angle, self.end_angle)
    }

    fn evaluate_f64(&self, t: f64) -> (f64, f64) {
        let (cx, cy) = self.center.to_f64();
        let r = self.radius;
        (cx + r * t.cos(), cy + r * t.sin())
    }

    fn bounding_box(&self) -> BoundingBox {
        let (sx, sy) = self.start_point();
        let (ex, ey) = self.end_point();

        let mut xmin = sx.min(ex);
        let mut xmax = sx.max(ex);
        let mut ymin = sy.min(ey);
        let mut ymax = sy.max(ey);

        // The extrema of a circular arc are its endpoints plus whichever of the four
        // cardinal directions (k·90°) fall inside the swept range.
        for k in 0..4 {
            let angle = k as f64 * std::f64::consts::FRAC_PI_2;
            let rel = crate::util::wrap_tau(angle - self.start_angle);
            if rel <= self.included_angle() + 1e-12 {
                let (x, y) = self.evaluate_f64(self.start_angle + rel);
                xmin = xmin.min(x);
                xmax = xmax.max(x);
                ymin = ymin.min(y);
                ymax = ymax.max(y);
            }
        }

        BoundingBox::from_corners(xmin, ymin, xmax, ymax)
    }

    fn tangent_f64(&self, t: f64) -> (f64, f64) {
        let r = self.radius;
        (-r * t.sin(), r * t.cos())
    }

    fn arc_length(&self) -> f64 {
        self.radius * self.included_angle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_point_construction() {
        let p1 = Point2d::from_f64(4.0, 2.0);
        let p2 = Point2d::from_f64(1.0, 5.0);
        let p3 = Point2d::from_f64(-2.0, 2.0);
        let arc = CircularArc::from_three_points(&p1, &p2, &p3).unwrap();

        let (cx, cy) = arc.center.to_f64();
        assert!((cx - 1.0).abs() < 1e-6, "cx={}", cx);
        assert!((cy - 2.0).abs() < 1e-6, "cy={}", cy);
        assert!((arc.radius - 3.0).abs() < 1e-4, "r={}", arc.radius);
    }

    #[test]
    fn try_new_rejects_non_positive_radius() {
        assert_eq!(
            CircularArc::try_new(Point2d::from_i64(0, 0), 0.0, 0.0, 1.0).unwrap_err(),
            GeomError::NonPositiveRadius(0.0)
        );
        assert!(CircularArc::try_new(Point2d::from_i64(0, 0), 2.0, 0.0, 1.0).is_ok());
    }

    #[test]
    fn arc_length_quarter_circle() {
        let arc = CircularArc::new(
            Point2d::from_i64(0, 0),
            5.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        );
        let expected = 5.0 * std::f64::consts::FRAC_PI_2;
        assert!((arc.arc_length() - expected).abs() < 1e-10);
    }

    #[test]
    fn sagitta_semicircle() {
        let arc = CircularArc::new(Point2d::from_i64(0, 0), 4.0, 0.0, std::f64::consts::PI);
        assert!((arc.sagitta() - 4.0).abs() < 1e-10);
    }
}

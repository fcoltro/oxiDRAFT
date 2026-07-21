//! The elliptical arc primitive, [`EllipticalArc`]. The arc is a rotated,
//! non-uniformly scaled circle; its `[start_angle, end_angle]` parameter is the
//! eccentric angle, and the sweep direction follows the same signed convention
//! as [`crate::CircularArc`].

use crate::curve::CurveSegment;
use crate::error::GeomError;
use crate::point::{BoundingBox, Point2d};

/// An elliptical arc: an ellipse (semi-axes `semi_major`/`semi_minor`, tilted by
/// `rotation`) swept between two eccentric angles.
#[derive(Clone, Copy, Debug)]
pub struct EllipticalArc {
    /// Centre of the ellipse.
    pub center: Point2d,
    /// Semi-axis length along the (rotated) major direction.
    pub semi_major: f64,
    /// Semi-axis length along the (rotated) minor direction.
    pub semi_minor: f64,
    /// Rotation of the major axis from +x, in radians.
    pub rotation: f64,
    /// Eccentric angle of the start point.
    pub start_angle: f64,
    /// Eccentric angle of the end point.
    pub end_angle: f64,
}

impl EllipticalArc {
    /// Builds a rotated elliptical arc from all six parameters (trusted caller;
    /// use [`EllipticalArc::try_new`] for untrusted input).
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

    /// Builds an axis-aligned elliptical arc (zero rotation).
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

    /// Fallible constructor for untrusted input (file import, transforms of
    /// unknown provenance), mirroring [`crate::CircularArc::try_new`]:
    /// rejects non-positive/NaN semi-axes and any non-finite coordinate or
    /// angle, so degenerate geometry can't evaluate to NaN downstream.
    pub fn try_new(
        center: Point2d,
        semi_major: f64,
        semi_minor: f64,
        rotation: f64,
        start_angle: f64,
        end_angle: f64,
    ) -> Result<Self, GeomError> {
        if !semi_major.is_finite() || semi_major <= 0.0 {
            return Err(GeomError::NonPositiveAxis(semi_major));
        }
        if !semi_minor.is_finite() || semi_minor <= 0.0 {
            return Err(GeomError::NonPositiveAxis(semi_minor));
        }
        if !center.is_finite()
            || !rotation.is_finite()
            || !start_angle.is_finite()
            || !end_angle.is_finite()
        {
            return Err(GeomError::NonFiniteValue);
        }
        Ok(EllipticalArc {
            center,
            semi_major,
            semi_minor,
            rotation,
            start_angle,
            end_angle,
        })
    }

    /// The ellipse's two focal points, in world coordinates. For a circle (or
    /// where `semi_minor ≥ semi_major`) both foci coincide with the centre.
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

    /// The ellipse's eccentricity in `[0, 1)` — `0` for a circle, approaching
    /// `1` as it flattens.
    pub fn eccentricity(&self) -> f64 {
        let a = self.semi_major;
        let b = self.semi_minor;
        let c = (a * a - b * b).max(0.0).sqrt();
        c / a
    }

    /// Absolute angular span, independent of traversal direction — see
    /// [`crate::CircularArc::included_angle`] for why `positive_sweep`
    /// is wrong here (reversed arcs would report the complement).
    pub fn included_angle(&self) -> f64 {
        (self.end_angle - self.start_angle).abs()
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
        // Sample from the lower domain end so a reversed arc (end < start)
        // samples the span it actually covers.
        let lo = self.start_angle.min(self.end_angle);
        let (t0, t1) = (lo, lo + self.included_angle());
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
        let lo = self.start_angle.min(self.end_angle);
        let (t0, t1) = (lo, lo + self.included_angle());
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
    fn try_new_rejects_degenerate_and_non_finite_input() {
        let c = Point2d::from_i64(0, 0);
        assert!(matches!(
            EllipticalArc::try_new(c, 0.0, 1.0, 0.0, 0.0, 1.0),
            Err(GeomError::NonPositiveAxis(_))
        ));
        assert!(matches!(
            EllipticalArc::try_new(c, 2.0, f64::NAN, 0.0, 0.0, 1.0),
            Err(GeomError::NonPositiveAxis(_))
        ));
        assert!(matches!(
            EllipticalArc::try_new(c, 2.0, 1.0, f64::INFINITY, 0.0, 1.0),
            Err(GeomError::NonFiniteValue)
        ));
        assert!(EllipticalArc::try_new(c, 2.0, 1.0, 0.0, 0.0, 1.0).is_ok());
    }

    #[test]
    fn reversed_elliptical_arc_matches_forward_metrics() {
        let fwd = EllipticalArc::axis_aligned(
            Point2d::from_i64(0, 0),
            5.0,
            3.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        );
        let rev = EllipticalArc::axis_aligned(
            Point2d::from_i64(0, 0),
            5.0,
            3.0,
            std::f64::consts::FRAC_PI_2,
            0.0,
        );
        assert!(
            (rev.included_angle() - fwd.included_angle()).abs() < 1e-12,
            "included angle must be the span, not its complement: {}",
            rev.included_angle()
        );
        assert!((rev.arc_length() - fwd.arc_length()).abs() < 1e-9);
        let (bf, br) = (fwd.bounding_box(), rev.bounding_box());
        assert!(
            (bf.min.x - br.min.x).abs() < 1e-9
                && (bf.min.y - br.min.y).abs() < 1e-9
                && (bf.max.x - br.max.x).abs() < 1e-9
                && (bf.max.y - br.max.y).abs() < 1e-9,
            "reversed bbox {br:?} must equal forward bbox {bf:?}"
        );
    }

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

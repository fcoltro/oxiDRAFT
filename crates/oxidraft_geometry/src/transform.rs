//! Affine transforms of the plane, [`Transform2d`] — the 2×3 matrix behind
//! move/rotate/scale/mirror, applied to points, vectors, and whole curves.
//! [`Transform2d::compose`] combines two transforms; the arc/ellipse fast paths
//! in [`Transform2d::apply_curve`] rely on [`Transform2d::is_conformal`].

use crate::curve::Curve;
use crate::nurbs::{NurbsCurve, RationalBezier};
use crate::point::Point2d;
use crate::primitives::{CircularArc, CubicBezier, EllipticalArc, LineSeg, PolyCurve};

/// A 2D affine transform: the linear 2×2 part (`m00 m01 / m10 m11`) plus a
/// translation (`tx`, `ty`). Maps `(x, y)` to
/// `(m00·x + m01·y + tx, m10·x + m11·y + ty)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform2d {
    /// Row 0, column 0 of the linear part.
    pub m00: f64,
    /// Row 0, column 1 of the linear part.
    pub m01: f64,
    /// Horizontal translation.
    pub tx: f64,
    /// Row 1, column 0 of the linear part.
    pub m10: f64,
    /// Row 1, column 1 of the linear part.
    pub m11: f64,
    /// Vertical translation.
    pub ty: f64,
}

impl Transform2d {
    /// True when every matrix entry is finite — callers gate application on
    /// this, since degenerate constructors (e.g. a mirror across a zero-length
    /// line) can yield non-finite transforms rather than panicking.
    pub fn is_finite(&self) -> bool {
        [self.m00, self.m01, self.tx, self.m10, self.m11, self.ty]
            .iter()
            .all(|v| v.is_finite())
    }

    /// The identity transform (leaves every point unchanged).
    pub fn identity() -> Self {
        Transform2d {
            m00: 1.0,
            m01: 0.0,
            tx: 0.0,
            m10: 0.0,
            m11: 1.0,
            ty: 0.0,
        }
    }

    /// A pure translation by `(dx, dy)`.
    pub fn translation(dx: f64, dy: f64) -> Self {
        let mut t = Self::identity();
        t.tx = dx;
        t.ty = dy;
        t
    }

    /// A scaling about the origin by `sx` horizontally and `sy` vertically.
    pub fn scale(sx: f64, sy: f64) -> Self {
        Transform2d {
            m00: sx,
            m01: 0.0,
            tx: 0.0,
            m10: 0.0,
            m11: sy,
            ty: 0.0,
        }
    }

    /// A uniform scaling about the origin by factor `s` on both axes.
    pub fn scale_uniform(s: f64) -> Self {
        Self::scale(s, s)
    }

    /// A scaling by `(sx, sy)` about `center` instead of the origin (the point
    /// stays fixed).
    pub fn scale_about(center: &Point2d, sx: f64, sy: f64) -> Self {
        Self::translation(center.x, center.y)
            .compose(&Self::scale(sx, sy))
            .compose(&Self::translation(-center.x, -center.y))
    }

    /// Reflection across the x-axis (flips y).
    pub fn mirror_x() -> Self {
        Self::scale(1.0, -1.0)
    }

    /// Reflection across the line through `p0` and `p1`. Coincident (or
    /// non-finite) points define no axis; the result is then non-finite,
    /// and callers gate application on [`Transform2d::is_finite`] — two
    /// identical mirror picks are a click away, so this must not panic.
    pub fn mirror_line(p0: &Point2d, p1: &Point2d) -> Self {
        let dx = p1.x - p0.x;
        let dy = p1.y - p0.y;
        let len_sq = dx * dx + dy * dy;
        let r00 = (dx * dx - dy * dy) / len_sq;
        let r01 = (2.0 * dx * dy) / len_sq;
        let r11 = (dy * dy - dx * dx) / len_sq;
        let refl = Transform2d {
            m00: r00,
            m01: r01,
            tx: 0.0,
            m10: r01,
            m11: r11,
            ty: 0.0,
        };
        Self::translation(p0.x, p0.y)
            .compose(&refl)
            .compose(&Self::translation(-p0.x, -p0.y))
    }

    /// Rotation by `n` quarter turns (90° each) about the origin, using exact
    /// `0`/`±1` matrix entries so right-angle rotations stay exact.
    pub fn rotation_quarter_turns(n: i32) -> Self {
        let (c, s) = match n.rem_euclid(4) {
            0 => (1.0, 0.0),
            1 => (0.0, 1.0),
            2 => (-1.0, 0.0),
            _ => (0.0, -1.0),
        };
        Transform2d {
            m00: c,
            m01: -s,
            tx: 0.0,
            m10: s,
            m11: c,
            ty: 0.0,
        }
    }

    /// Rotation by `angle` radians (counter-clockwise) about the origin.
    pub fn rotation(angle: f64) -> Self {
        let c = angle.cos();
        let s = angle.sin();
        Transform2d {
            m00: c,
            m01: -s,
            tx: 0.0,
            m10: s,
            m11: c,
            ty: 0.0,
        }
    }

    /// Rotation by `angle` about `center` instead of the origin.
    pub fn rotation_about(center: &Point2d, angle: f64) -> Self {
        Self::translation(center.x, center.y)
            .compose(&Self::rotation(angle))
            .compose(&Self::translation(-center.x, -center.y))
    }

    /// Composition `self ∘ other`: the transform that applies `other` first,
    /// then `self`.
    pub fn compose(&self, other: &Transform2d) -> Transform2d {
        Transform2d {
            m00: self.m00 * other.m00 + self.m01 * other.m10,
            m01: self.m00 * other.m01 + self.m01 * other.m11,
            tx: self.m00 * other.tx + self.m01 * other.ty + self.tx,
            m10: self.m10 * other.m00 + self.m11 * other.m10,
            m11: self.m10 * other.m01 + self.m11 * other.m11,
            ty: self.m10 * other.tx + self.m11 * other.ty + self.ty,
        }
    }

    /// Maps a position through the full transform (linear part + translation).
    #[inline]
    pub fn apply_point(&self, p: &Point2d) -> Point2d {
        Point2d {
            x: self.m00 * p.x + self.m01 * p.y + self.tx,
            y: self.m10 * p.x + self.m11 * p.y + self.ty,
        }
    }

    /// Applies only the linear (rotation/scale/shear) part to a direction vector,
    /// ignoring translation. Use this for directions/normals rather than positions.
    #[inline]
    pub fn apply_vector(&self, dx: f64, dy: f64) -> (f64, f64) {
        (self.m00 * dx + self.m01 * dy, self.m10 * dx + self.m11 * dy)
    }

    /// Determinant of the linear part: the signed area-scaling factor (negative
    /// under a reflection).
    #[inline]
    pub fn determinant(&self) -> f64 {
        self.m00 * self.m11 - self.m01 * self.m10
    }

    /// The uniform length-scaling factor `√|det|` — exact for a similarity,
    /// a reasonable summary otherwise.
    pub fn scale_factor(&self) -> f64 {
        self.determinant().abs().sqrt()
    }

    /// The rotation angle of the linear part, in radians.
    pub fn rotation_angle(&self) -> f64 {
        self.m10.atan2(self.m00)
    }

    /// True when the transform flips orientation (mirrors), i.e. its
    /// determinant is negative.
    pub fn is_reflection(&self) -> bool {
        self.determinant() < 0.0
    }

    /// True when the linear part is a similarity (uniform scale + rotation, possibly
    /// reflected): its columns are orthogonal and equal length. Only such transforms
    /// map a circle to a circle and an ellipse to a similar ellipse, so the closed-form
    /// `apply_arc`/`apply_ellipse` fast paths are valid exactly when this holds.
    pub fn is_conformal(&self) -> bool {
        let col_a = self.m00 * self.m00 + self.m10 * self.m10;
        let col_b = self.m01 * self.m01 + self.m11 * self.m11;
        let dot = self.m00 * self.m01 + self.m10 * self.m11;
        let scale = col_a.max(col_b);
        if scale < 1e-24 {
            return true; // degenerate (≈zero) linear part; nothing to preserve
        }
        (col_a - col_b).abs() <= 1e-9 * scale && dot.abs() <= 1e-9 * scale
    }
}

impl Transform2d {
    /// Maps a whole curve through the transform, returning the transformed
    /// curve. Arcs/ellipses under a similarity keep their kind via a closed-form
    /// fast path; a general affine lowers them to rational form first.
    pub fn apply_curve(&self, curve: &Curve) -> Curve {
        match curve {
            Curve::Line(l) => Curve::Line(LineSeg::from_endpoints(
                self.apply_point(&l.p0),
                self.apply_point(&l.p1),
            )),
            Curve::Bezier(b) => Curve::Bezier(CubicBezier::new(
                self.apply_point(&b.p0),
                self.apply_point(&b.p1),
                self.apply_point(&b.p2),
                self.apply_point(&b.p3),
            )),
            Curve::Arc(a) => self.apply_arc(a),
            Curve::Ellipse(e) => self.apply_ellipse(e),
            Curve::Poly(pc) => {
                let segs = pc.segments.iter().map(|s| self.apply_curve(s)).collect();
                Curve::Poly(Box::new(PolyCurve::new(segs)))
            }
            Curve::Rational(rb) => {
                let points = rb.points.iter().map(|p| self.apply_point(p)).collect();
                Curve::Rational(RationalBezier::new(points, rb.weights.clone()))
            }
            Curve::Nurbs(nc) => {
                let control = nc.control().iter().map(|p| self.apply_point(p)).collect();
                Curve::Nurbs(NurbsCurve::new(control, nc.weights().to_vec()))
            }
        }
    }

    fn apply_arc(&self, a: &CircularArc) -> Curve {
        if !self.is_conformal() {
            // A non-uniform scale or shear turns a circle into an ellipse that a
            // CircularArc cannot represent. Lower to exact rational quadratics and
            // transform their control points instead — that map is exact for any affine.
            return self.apply_lowered(&Curve::Arc(*a));
        }
        let new_center = self.apply_point(&a.center);
        let new_radius = a.radius * self.scale_factor();
        let rot = self.rotation_angle();
        // Negate, do NOT swap. Swapping keeps `start < end` but silently
        // re-orients the mirrored arc, so its domain-start evaluates to the
        // mirror of the *end* point. Lines and beziers map endpoint-to-
        // endpoint, and `apply_ellipse` negates without swapping, so an arc
        // that swaps desynchronises any mixed chain: mirroring a rounded
        // rectangle or filleted profile broke G0 at every arc.
        let (start, end) = if self.is_reflection() {
            (-a.start_angle + rot, -a.end_angle + rot)
        } else {
            (a.start_angle + rot, a.end_angle + rot)
        };
        // A degenerate conformal map (zero or non-finite scale) collapses the
        // radius below CircularArc's positivity contract; lower to the exact
        // rational form instead of panicking in the trusted constructor.
        match CircularArc::try_new(new_center, new_radius, start, end) {
            Ok(arc) => Curve::Arc(arc),
            Err(_) => self.apply_lowered(&Curve::Arc(*a)),
        }
    }

    fn apply_ellipse(&self, e: &EllipticalArc) -> Curve {
        if !self.is_conformal() {
            // Under a general affine the ellipse stays an ellipse, but its axes rotate
            // and rescale independently (and may shear) — not expressible by scaling both
            // semi-axes uniformly. Transform the exact rational form instead.
            return self.apply_lowered(&Curve::Ellipse(*e));
        }
        let new_center = self.apply_point(&e.center);
        let sf = self.scale_factor();
        let new_major = e.semi_major * sf;
        let new_minor = e.semi_minor * sf;
        let rot = self.rotation_angle();
        // Unlike a circular arc, an ellipse carries its orientation in its own
        // `rotation` field, so the map must go there — NOT into the eccentric
        // angles as well. With P(t) = C + R(ψ)·(a cos t, b sin t):
        //
        //   rotation  M = s·R(φ):  M·P(t) = C' + R(φ+ψ)·(sa cos t, sb sin t)
        //                          ⇒ rotation ψ+φ, angles unchanged.
        //   reflection M = s·R(φ)·F, using F·R(ψ) = R(−ψ)·F:
        //                          M·P(t) = C' + s·R(φ−ψ)·(a cos(−t), b sin(−t))
        //                          ⇒ rotation φ−ψ, angles negated.
        //
        // Adding `rot` to the angles too (the circular-arc rule, where there is
        // no rotation field to absorb it) rotated the ellipse twice.
        let (new_rotation, start, end) = if self.is_reflection() {
            (rot - e.rotation, -e.start_angle, -e.end_angle)
        } else {
            (e.rotation + rot, e.start_angle, e.end_angle)
        };
        // Same degenerate-scale guard as apply_arc: don't hand back an
        // ellipse with collapsed axes that evaluates to NaN downstream.
        match EllipticalArc::try_new(new_center, new_major, new_minor, new_rotation, start, end) {
            Ok(el) => Curve::Ellipse(el),
            Err(_) => self.apply_lowered(&Curve::Ellipse(*e)),
        }
    }

    /// Lowers a conic to its exact rational-Bézier segments and transforms those,
    /// yielding a `Poly` of `Rational`s. Used when the transform is non-conformal.
    fn apply_lowered(&self, curve: &Curve) -> Curve {
        let segs: Vec<Curve> = crate::nurbs::lower(curve)
            .into_iter()
            .map(Curve::Rational)
            .collect();
        self.apply_curve(&Curve::Poly(Box::new(PolyCurve::new(segs))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::CurveSegment;

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn translate_point_exact() {
        let t = Transform2d::translation(3.0, -2.0);
        assert_eq!(t.apply_point(&pt(5, 5)), pt(8, 3));
    }

    #[test]
    fn scale_about_center() {
        let t = Transform2d::scale_about(&pt(1, 1), 2.0, 2.0);
        assert_eq!(t.apply_point(&pt(3, 3)), pt(5, 5));
        assert_eq!(t.apply_point(&pt(1, 1)), pt(1, 1));
    }

    #[test]
    fn quarter_turn_exact() {
        let t = Transform2d::rotation_quarter_turns(1);
        assert_eq!(t.apply_point(&pt(1, 0)), pt(0, 1));
        assert_eq!(t.apply_point(&pt(0, 1)), pt(-1, 0));
    }

    #[test]
    fn mirror_x_axis() {
        let t = Transform2d::mirror_x();
        assert_eq!(t.apply_point(&pt(3, 4)), pt(3, -4));
        assert!(t.is_reflection());
    }

    #[test]
    fn mirror_diagonal_line() {
        let t = Transform2d::mirror_line(&pt(0, 0), &pt(1, 1));
        assert_eq!(t.apply_point(&pt(3, 0)), pt(0, 3));
    }

    #[test]
    fn zero_scale_collapses_conics_without_panicking() {
        // scale_uniform(0.0) is conformal (nothing to preserve), so the arc
        // fast path used to compute radius 0 and panic in CircularArc::new;
        // the ellipse fast path silently produced zero axes instead. Both
        // must fall back to the lowered rational form: finite, no panic.
        let zero = Transform2d::scale_uniform(0.0);
        let arc = Curve::Arc(CircularArc::new(pt(1, 2), 5.0, 0.0, 1.0));
        let ell = Curve::Ellipse(EllipticalArc::new(pt(1, 2), 5.0, 3.0, 0.3, 0.0, 1.0));
        for c in [zero.apply_curve(&arc), zero.apply_curve(&ell)] {
            assert!(c.is_finite(), "collapsed conic must stay finite: {c:?}");
        }
    }

    #[test]
    fn compose_translate_then_scale() {
        let t = Transform2d::scale(2.0, 2.0).compose(&Transform2d::translation(1.0, 1.0));
        assert_eq!(t.apply_point(&pt(2, 3)), pt(6, 8));
    }

    #[test]
    fn bezier_is_affine_invariant() {
        let bz = Curve::Bezier(CubicBezier::new(pt(0, 0), pt(1, 2), pt(3, 2), pt(4, 0)));
        let t = Transform2d::translation(10.0, 5.0);
        let moved = t.apply_curve(&bz);
        let (x, y) = bz.evaluate_f64(0.5);
        let (mx, my) = moved.evaluate_f64(0.5);
        assert!((mx - (x + 10.0)).abs() < 1e-9 && (my - (y + 5.0)).abs() < 1e-9);
    }

    #[test]
    fn line_transform_endpoints() {
        let l = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(2, 0)));
        let t = Transform2d::rotation_quarter_turns(1);
        if let Curve::Line(moved) = t.apply_curve(&l) {
            assert_eq!(moved.p0, pt(0, 0));
            assert_eq!(moved.p1, pt(0, 2));
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn arc_translate_and_scale() {
        let arc = Curve::Arc(CircularArc::new(pt(0, 0), 2.0, 0.0, std::f64::consts::PI));
        let t = Transform2d::scale_uniform(3.0);
        if let Curve::Arc(a) = t.apply_curve(&arc) {
            assert!((a.radius - 6.0).abs() < 1e-6);
            assert_eq!(a.center, pt(0, 0));
        } else {
            panic!("expected arc");
        }
    }

    #[test]
    fn non_uniform_scale_turns_circle_into_exact_ellipse() {
        // scale(2,1) on a unit circle: every transformed point must satisfy
        // (x/2)^2 + y^2 = 1. The closed-form CircularArc path would wrongly keep a
        // circle of radius sqrt(2); the lowered path is exact.
        let circle = Curve::Arc(CircularArc::new(
            Point2d::from_i64(0, 0),
            1.0,
            0.0,
            std::f64::consts::TAU,
        ));
        let t = Transform2d::scale(2.0, 1.0);
        assert!(!t.is_conformal());
        let out = t.apply_curve(&circle);
        for i in 0..=64 {
            let s = i as f64 / 64.0;
            let (x, y) = out.evaluate_f64(s);
            let f = (x / 2.0).powi(2) + y.powi(2);
            assert!((f - 1.0).abs() < 1e-9, "off ellipse at s={s}: f={f}");
        }
    }

    #[test]
    fn uniform_scale_keeps_arc_type() {
        let arc = Curve::Arc(CircularArc::new(pt(0, 0), 2.0, 0.0, std::f64::consts::PI));
        let out = Transform2d::scale_uniform(3.0).apply_curve(&arc);
        assert!(matches!(out, Curve::Arc(_)), "conformal scale keeps an Arc");
    }

    #[test]
    fn mirror_arc_reverses_sweep() {
        let arc = CircularArc::new(pt(0, 0), 2.0, 0.0, std::f64::consts::FRAC_PI_2);
        let mirrored = match Transform2d::mirror_x().apply_curve(&Curve::Arc(arc)) {
            Curve::Arc(a) => a,
            _ => panic!("expected arc"),
        };

        // Endpoint correspondence: the mirrored arc's start must be the mirror
        // of the ORIGINAL start, not of its end. This used to swap them, which
        // kept `start < end` but reversed traversal — and since lines and
        // beziers map endpoint-to-endpoint, mirroring a mixed chain then broke
        // G0 at every arc (see `mirroring_a_chain_keeps_it_connected`).
        let (sx, sy) = mirrored.evaluate_f64(mirrored.start_angle);
        let (ex, ey) = mirrored.evaluate_f64(mirrored.end_angle);
        assert!(
            (sx - 2.0).abs() < 1e-6 && (sy - 0.0).abs() < 1e-6,
            "mirrored start {:?}, want the mirror of (2,0)",
            (sx, sy)
        );
        assert!(
            (ex - 0.0).abs() < 1e-6 && (ey + 2.0).abs() < 1e-6,
            "mirrored end {:?}, want the mirror of (0,2)",
            (ex, ey)
        );

        assert!(
            (mirrored.included_angle() - std::f64::consts::FRAC_PI_2).abs() < 1e-6,
            "included angle {}",
            mirrored.included_angle()
        );
    }

    /// Worst distance from the true image of the source curve to the curve the
    /// transform produced. Checks the *point set*, which is what the older
    /// tests missed: they only asserted a field or a single endpoint, so a
    /// wrong rotation/angle pairing that still produced a plausible ellipse
    /// went unnoticed.
    fn worst_image_deviation(xf: &Transform2d, src: &Curve) -> f64 {
        let (t0, t1) = src.domain();
        let out = xf.apply_curve(src);
        let mut worst: f64 = 0.0;
        for i in 0..=60 {
            let t = t0 + (t1 - t0) * i as f64 / 60.0;
            let (x, y) = src.evaluate_f64(t);
            let truth = xf.apply_point(&Point2d::from_f64(x, y));
            // True distance to the curve via the kernel's projection — a
            // nearest-sample scan would floor out at half the sample spacing
            // and hide (or fake) errors at this magnitude.
            worst = worst.max(crate::ops::point_to_curve_distance(&out, truth.x, truth.y));
        }
        worst
    }

    /// Regression: mirroring a mixed line+arc chain must keep it connected.
    /// Lines and beziers map endpoint-to-endpoint under a reflection, so an
    /// arc that also swapped its endpoints desynchronised the chain — every
    /// rounded rectangle, slot or filleted profile came apart at the arcs when
    /// mirrored.
    #[test]
    fn mirroring_a_chain_keeps_it_connected() {
        let half_pi = std::f64::consts::FRAC_PI_2;
        for xf in [
            Transform2d::mirror_x(),
            Transform2d::mirror_line(&pt(0, 0), &Point2d::from_f64(1.0, 1.0)),
        ] {
            let chain = Curve::Poly(Box::new(PolyCurve::new(vec![
                Curve::Line(LineSeg::from_endpoints(pt(-2, 0), pt(0, 0))),
                // Arc from (0,0) round to (2,2)-ish, then a line onward.
                Curve::Arc(CircularArc::new(pt(0, 2), 2.0, -half_pi, 0.0)),
                Curve::Line(LineSeg::from_endpoints(pt(2, 2), pt(4, 2))),
            ])));
            let Curve::Poly(before) = &chain else {
                panic!("expected a Poly")
            };
            assert!(before.check_g0(1e-6), "fixture must start connected");

            let Curve::Poly(after) = xf.apply_curve(&chain) else {
                panic!("a mirrored Poly must stay a Poly")
            };
            assert!(
                after.check_g0(1e-6),
                "mirroring broke the chain's continuity"
            );
        }
    }

    /// Regression: an ellipse carries its orientation in its `rotation` field,
    /// so a rotation must not also shift the eccentric angles. Doing both
    /// rotated the ellipse twice and moved the curve off its true image by
    /// several units.
    #[test]
    fn rotating_a_partial_ellipse_keeps_its_point_set() {
        let e = EllipticalArc::new(pt(0, 0), 3.0, 1.0, 0.0, 0.0, std::f64::consts::FRAC_PI_2);
        let src = Curve::Ellipse(e);
        for turns in [0.25, 0.5, 1.0, -0.3] {
            let xf = Transform2d::rotation(turns * std::f64::consts::PI);
            let dev = worst_image_deviation(&xf, &src);
            assert!(dev < 1e-6, "rotation by {turns}pi deviated {dev}");
        }
    }

    /// Regression: reflecting an ellipse negates its rotation (φ − ψ, not
    /// φ + ψ) and negates the eccentric angles. Getting either wrong moved the
    /// produced ellipse clean off the mirrored point set.
    #[test]
    fn mirroring_a_rotated_ellipse_keeps_its_point_set() {
        for psi in [0.0, 0.5, 1.2, -0.8] {
            let e = EllipticalArc::new(pt(0, 0), 3.0, 1.0, psi, 0.0, std::f64::consts::FRAC_PI_2);
            let src = Curve::Ellipse(e);
            let dev = worst_image_deviation(&Transform2d::mirror_x(), &src);
            assert!(dev < 1e-6, "mirror of rotation={psi} deviated {dev}");
        }
    }

    /// A rotated ellipse under a rotation+scale must still land on its image.
    #[test]
    fn rotate_and_scale_a_rotated_ellipse() {
        let e = EllipticalArc::new(pt(1, -2), 4.0, 2.5, 0.7, -0.4, 1.9);
        let src = Curve::Ellipse(e);
        // Uniform scale only: a non-uniform one is non-conformal and lowers the
        // ellipse to rational segments, which is a different code path.
        let xf = Transform2d::rotation(0.6).compose(&Transform2d::scale_about(
            &Point2d::from_f64(0.0, 0.0),
            1.7,
            1.7,
        ));
        let dev = worst_image_deviation(&xf, &src);
        assert!(dev < 1e-6, "rotate+scale deviated {dev}");
    }
}

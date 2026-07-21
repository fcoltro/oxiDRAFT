//! Cutting a curve at a parameter ([`split_curve`]) and reversing its direction
//! ([`reverse_curve`]), each preserving the exact shape of the input.

use crate::curve::Curve;
use crate::nurbs::NurbsCurve;
use crate::primitives::{CircularArc, CubicBezier, EllipticalArc, PolyCurve};

/// Cuts `curve` at parameter `t ∈ [0, 1]`, returning the `[0, t]` and `[t, 1]`
/// pieces. Each piece has the same kind as the input (a split polycurve stays a
/// polycurve) and together they trace exactly the original shape.
pub fn split_curve(curve: &Curve, t: f64) -> (Curve, Curve) {
    match curve {
        Curve::Line(l) => {
            let (a, b) = l.split_at_exact(t);
            (Curve::Line(a), Curve::Line(b))
        }
        Curve::Arc(a) => {
            let mid_angle = a.start_angle + t * (a.end_angle - a.start_angle);
            let left = CircularArc::new(a.center, a.radius, a.start_angle, mid_angle);
            let right = CircularArc::new(a.center, a.radius, mid_angle, a.end_angle);
            (Curve::Arc(left), Curve::Arc(right))
        }
        Curve::Ellipse(e) => {
            let mid_angle = e.start_angle + t * (e.end_angle - e.start_angle);
            let left = EllipticalArc::new(
                e.center,
                e.semi_major,
                e.semi_minor,
                e.rotation,
                e.start_angle,
                mid_angle,
            );
            let right = EllipticalArc::new(
                e.center,
                e.semi_major,
                e.semi_minor,
                e.rotation,
                mid_angle,
                e.end_angle,
            );
            (Curve::Ellipse(left), Curve::Ellipse(right))
        }
        Curve::Bezier(bz) => {
            let (a, b) = bz.split_at_exact(t);
            (Curve::Bezier(a), Curve::Bezier(b))
        }
        Curve::Poly(pc) => {
            let n = pc.segments.len();
            if n == 0 {
                return (
                    Curve::Poly(Box::new(PolyCurve::new(vec![]))),
                    Curve::Poly(Box::new(PolyCurve::new(vec![]))),
                );
            }
            let seg_idx = ((t * n as f64) as usize).min(n - 1);
            let t_local = (t * n as f64 - seg_idx as f64).clamp(0.0, 1.0);

            let (seg_left, seg_right) = split_curve(&pc.segments[seg_idx], t_local);

            let mut left_segs: Vec<Curve> = pc.segments[..seg_idx].to_vec();
            left_segs.push(seg_left);

            let mut right_segs = vec![seg_right];
            right_segs.extend(pc.segments[seg_idx + 1..].to_vec());

            (
                Curve::Poly(Box::new(PolyCurve::new(left_segs))),
                Curve::Poly(Box::new(PolyCurve::new(right_segs))),
            )
        }
        Curve::Rational(rb) => {
            let (a, b) = rb.split(t);
            (Curve::Rational(a), Curve::Rational(b))
        }
        Curve::Nurbs(nc) => {
            let segs = nc.segments().into_iter().map(Curve::Rational).collect();
            split_curve(&Curve::Poly(Box::new(PolyCurve::new(segs))), t)
        }
    }
}

/// Returns `curve` traced in the opposite direction — same geometry, swapped
/// start and end. Arcs come back with `end < start` (a reversed arc), which the
/// kernel treats as a first-class span, not an error.
pub fn reverse_curve(curve: &Curve) -> Curve {
    match curve {
        Curve::Line(l) => Curve::Line(l.reverse()),
        Curve::Arc(a) => Curve::Arc(CircularArc::new(
            a.center,
            a.radius,
            a.end_angle,
            a.start_angle,
        )),
        Curve::Ellipse(e) => Curve::Ellipse(EllipticalArc::new(
            e.center,
            e.semi_major,
            e.semi_minor,
            e.rotation,
            e.end_angle,
            e.start_angle,
        )),
        Curve::Bezier(bz) => Curve::Bezier(CubicBezier::new(bz.p3, bz.p2, bz.p1, bz.p0)),
        Curve::Poly(pc) => {
            let reversed_segs: Vec<Curve> = pc.segments.iter().rev().map(reverse_curve).collect();
            Curve::Poly(Box::new(PolyCurve::new(reversed_segs)))
        }
        Curve::Rational(rb) => Curve::Rational(rb.reverse()),
        Curve::Nurbs(nc) => Curve::Nurbs(NurbsCurve::new(
            nc.control.iter().rev().cloned().collect(),
            nc.weights.iter().rev().cloned().collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::CurveSegment;
    use crate::point::Point2d;
    use crate::primitives::LineSeg;

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn split_line_at_half() {
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 6)));
        let (left, right) = split_curve(&line, 0.5);
        let (lx, ly) = left.evaluate_f64(1.0);
        let (rx, ry) = right.evaluate_f64(0.0);
        assert!((lx - 2.0).abs() < 1e-9 && (ly - 3.0).abs() < 1e-9);
        assert!((rx - 2.0).abs() < 1e-9 && (ry - 3.0).abs() < 1e-9);
    }

    #[test]
    fn split_bezier_preserves_shape() {
        let bz = Curve::Bezier(CubicBezier::new(pt(0, 0), pt(1, 3), pt(3, 3), pt(4, 0)));
        let (left, right) = split_curve(&bz, 0.5);
        let (lx0, ly0) = left.evaluate_f64(0.0);
        assert!((lx0 - 0.0).abs() < 1e-9 && (ly0 - 0.0).abs() < 1e-9);
        let (rx1, ry1) = right.evaluate_f64(1.0);
        assert!((rx1 - 4.0).abs() < 1e-9 && (ry1 - 0.0).abs() < 1e-9);
        let (lx1, ly1) = left.evaluate_f64(1.0);
        let (rx0, ry0) = right.evaluate_f64(0.0);
        assert!((lx1 - rx0).abs() < 1e-9 && (ly1 - ry0).abs() < 1e-9);
    }

    #[test]
    fn reverse_line_swaps_endpoints() {
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(5, 5)));
        let rev = reverse_curve(&line);
        let (x0, y0) = rev.evaluate_f64(0.0);
        let (x1, y1) = rev.evaluate_f64(1.0);
        assert!((x0 - 5.0).abs() < 1e-9 && (y0 - 5.0).abs() < 1e-9);
        assert!((x1 - 0.0).abs() < 1e-9 && (y1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn reverse_bezier_swaps_endpoints() {
        let bz = CubicBezier::new(pt(0, 0), pt(1, 3), pt(3, 3), pt(4, 0));
        let rev = reverse_curve(&Curve::Bezier(bz.clone()));
        if let Curve::Bezier(r) = rev {
            assert_eq!(r.p0, bz.p3);
            assert_eq!(r.p3, bz.p0);
        }
    }
}

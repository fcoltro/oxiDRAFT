use crate::curve::{Curve, CurveSegment};

pub fn tangent_at(curve: &Curve, t: f64) -> (f64, f64) {
    curve.tangent_f64(t)
}

pub fn normal_at(curve: &Curve, t: f64) -> (f64, f64) {
    let (tx, ty) = curve.tangent_f64(t);
    (-ty, tx)
}

pub fn curvature_at(curve: &Curve, t: f64) -> Option<f64> {
    let (t0, t1) = curve.domain();
    let (lo, hi) = (t0.min(t1), t0.max(t1));
    let span = (hi - lo).max(1e-9);
    let h = span * 1e-4;
    let tc = t.clamp(lo + h, hi - h);
    let (xm, ym) = curve.evaluate_f64(tc - h);
    let (xc, yc) = curve.evaluate_f64(tc);
    let (xp, yp) = curve.evaluate_f64(tc + h);

    let dx = (xp - xm) / (2.0 * h);
    let dy = (yp - ym) / (2.0 * h);
    let ddx = (xp - 2.0 * xc + xm) / (h * h);
    let ddy = (yp - 2.0 * yc + ym) / (h * h);

    let speed_sq = dx * dx + dy * dy;
    if speed_sq < 1e-20 {
        return None;
    }
    // speed_sq^1.5 == speed_sq * sqrt(speed_sq); avoids a generic powf in the hot path.
    let k = (dx * ddy - dy * ddx) / (speed_sq * speed_sq.sqrt());
    k.is_finite().then_some(k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::Point2d;
    use crate::primitives::{CircularArc, LineSeg};

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn curvature_of_circle_is_1_over_r() {
        let r_val = 3.0;
        let arc = CircularArc::new(pt(0, 0), 3.0, 0.0, 2.0 * std::f64::consts::PI);
        let c = Curve::Arc(arc);
        let kappa = curvature_at(&c, 0.0).unwrap();
        assert!(
            (kappa.abs() - 1.0 / r_val).abs() < 1e-6,
            "κ={}, expected ±{}",
            kappa,
            1.0 / r_val
        );
    }

    #[test]
    fn curvature_of_line_is_zero() {
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 0)));
        let kappa = curvature_at(&line, 0.5).unwrap();
        assert!(kappa.abs() < 1e-6, "κ={}", kappa);
    }

    #[test]
    fn tangent_perpendicular_to_normal() {
        let arc = Curve::Arc(CircularArc::new(
            pt(0, 0),
            2.0,
            0.0,
            2.0 * std::f64::consts::PI,
        ));
        let (tx, ty) = tangent_at(&arc, 0.0);
        let (nx, ny) = normal_at(&arc, 0.0);
        let dot = tx * nx + ty * ny;
        assert!(dot.abs() < 1e-10, "dot={}", dot);
    }
}

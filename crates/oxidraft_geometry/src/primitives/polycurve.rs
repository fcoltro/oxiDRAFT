use crate::curve::{Curve, CurveSegment};
use crate::point::BoundingBox;

#[derive(Clone, Debug)]
pub struct PolyCurve {
    pub segments: Vec<Curve>,
}

impl PolyCurve {
    pub fn new(segments: Vec<Curve>) -> Self {
        PolyCurve { segments }
    }

    pub fn check_g0(&self, tol: f64) -> bool {
        for i in 0..self.segments.len().saturating_sub(1) {
            let (_, t1) = self.segments[i].domain();
            let (t0, _) = self.segments[i + 1].domain();
            let (ex, ey) = self.segments[i].evaluate_f64(t1);
            let (sx, sy) = self.segments[i + 1].evaluate_f64(t0);
            let d = ((ex - sx).powi(2) + (ey - sy).powi(2)).sqrt();
            if d > tol {
                return false;
            }
        }
        true
    }

    pub fn merge_collinear(&self, tol: f64) -> PolyCurve {
        use crate::primitives::LineSeg;
        let mut result: Vec<Curve> = Vec::new();
        let mut i = 0;
        while i < self.segments.len() {
            if let Some(l0) = self.segments[i].as_line() {
                let mut end = l0.p1;
                let start = l0.p0;
                let mut j = i + 1;
                while j < self.segments.len() {
                    if let Some(l1) = self.segments[j].as_line() {
                        let (tx0, ty0) = (l0.p1.x - l0.p0.x, l0.p1.y - l0.p0.y);
                        let (tx1, ty1) = (l1.p1.x - l1.p0.x, l1.p1.y - l1.p0.y);
                        let cross = tx0 * ty1 - ty0 * tx1;
                        let len = ((tx0 * tx0 + ty0 * ty0) * (tx1 * tx1 + ty1 * ty1))
                            .sqrt()
                            .max(1e-15);
                        if cross.abs() / len < tol {
                            end = l1.p1;
                            j += 1;
                            continue;
                        }
                    }
                    break;
                }
                result.push(Curve::Line(LineSeg::from_endpoints(start, end)));
                i = j;
            } else {
                result.push(self.segments[i].clone());
                i += 1;
            }
        }
        PolyCurve::new(result)
    }
}

impl CurveSegment for PolyCurve {
    fn domain(&self) -> (f64, f64) {
        (0.0, 1.0)
    }

    fn evaluate_f64(&self, t: f64) -> (f64, f64) {
        let n = self.segments.len();
        if n == 0 {
            return (0.0, 0.0);
        }
        let seg_idx = ((t * n as f64) as usize).min(n - 1);
        let t_local = t * n as f64 - seg_idx as f64;
        let (t0, t1) = self.segments[seg_idx].domain();
        let t_mapped = t0 + t_local * (t1 - t0);
        self.segments[seg_idx].evaluate_f64(t_mapped)
    }

    fn bounding_box(&self) -> BoundingBox {
        if self.segments.is_empty() {
            return BoundingBox::from_corners(0.0, 0.0, 0.0, 0.0);
        }
        self.segments
            .iter()
            .skip(1)
            .fold(self.segments[0].bounding_box(), |acc, seg| {
                acc.union(&seg.bounding_box())
            })
    }

    fn tangent_f64(&self, t: f64) -> (f64, f64) {
        let n = self.segments.len();
        if n == 0 {
            return (1.0, 0.0);
        }
        let seg_idx = ((t * n as f64) as usize).min(n - 1);
        let t_local = t * n as f64 - seg_idx as f64;
        let (t0, t1) = self.segments[seg_idx].domain();
        let t_mapped = t0 + t_local * (t1 - t0);
        self.segments[seg_idx].tangent_f64(t_mapped)
    }

    fn arc_length(&self) -> f64 {
        self.segments.iter().map(|s| s.arc_length()).sum()
    }

    /// Walks the segments and recurses into the one containing `s` — the
    /// poly parameter allots 1/n per segment regardless of segment length,
    /// so the trait's uniform chord walk would misplace points badly on
    /// mixed-length chains.
    fn param_at_length(&self, s: f64) -> f64 {
        let n = self.segments.len();
        if n == 0 || !s.is_finite() || s <= 0.0 {
            return 0.0;
        }
        let mut acc = 0.0;
        for (i, seg) in self.segments.iter().enumerate() {
            let len = seg.arc_length();
            if len > 1e-12 && acc + len >= s {
                let (t0, t1) = seg.domain();
                let tl = seg.param_at_length(s - acc);
                let f = if (t1 - t0).abs() > 1e-12 {
                    ((tl - t0) / (t1 - t0)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                return (i as f64 + f) / n as f64;
            }
            // A poisoned segment length (NaN) fails the comparison above
            // and must not poison the accumulator either.
            if len.is_finite() {
                acc += len;
            }
        }
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::Point2d;
    use crate::primitives::LineSeg;

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }
    fn seg(x0: i64, y0: i64, x1: i64, y1: i64) -> Curve {
        Curve::Line(LineSeg::from_endpoints(pt(x0, y0), pt(x1, y1)))
    }

    #[test]
    fn g0_continuity_connected() {
        let pc = PolyCurve::new(vec![seg(0, 0, 1, 1), seg(1, 1, 2, 0)]);
        assert!(pc.check_g0(1e-9));
    }

    #[test]
    fn g0_continuity_disconnected() {
        let pc = PolyCurve::new(vec![seg(0, 0, 1, 1), seg(2, 0, 3, 1)]);
        assert!(!pc.check_g0(1e-9));
    }

    #[test]
    fn merge_collinear_lines() {
        let pc = PolyCurve::new(vec![seg(0, 0, 1, 0), seg(1, 0, 2, 0), seg(2, 0, 5, 0)]);
        let merged = pc.merge_collinear(1e-9);
        assert_eq!(merged.segments.len(), 1);
        if let Some(l) = merged.segments[0].as_line() {
            assert_eq!(l.p0, pt(0, 0));
            assert_eq!(l.p1, pt(5, 0));
        } else {
            panic!("Expected a line segment");
        }
    }

    #[test]
    fn total_arc_length() {
        let pc = PolyCurve::new(vec![seg(0, 0, 3, 4), seg(3, 4, 6, 0)]);
        assert!((pc.arc_length() - 10.0).abs() < 1e-8);
    }

    #[test]
    fn poly_ending_in_arc_has_normalized_domain() {
        use crate::primitives::CircularArc;
        let arc = Curve::Arc(CircularArc::new(
            pt(1, 0),
            1.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        ));
        let pc = PolyCurve::new(vec![seg(0, 0, 1, 0), arc]);
        assert_eq!(pc.domain(), (0.0, 1.0));
        let start = pc.evaluate_f64(0.0);
        let end = pc.evaluate_f64(1.0);
        assert!((start.0 - 0.0).abs() < 1e-9 && (start.1 - 0.0).abs() < 1e-9);
        assert!((end.0 - 1.0).abs() < 1e-9 && (end.1 - 1.0).abs() < 1e-9);
        for k in 0..=20 {
            let t = 0.5 + 0.5 * k as f64 / 20.0;
            let (_x, y) = pc.evaluate_f64(t);
            assert!(y >= -1e-9, "arc sample dipped below the corner: y={y}");
        }
    }
}

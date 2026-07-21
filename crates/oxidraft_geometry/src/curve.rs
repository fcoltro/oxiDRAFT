use crate::nurbs::{NurbsCurve, RationalBezier};
use crate::point::BoundingBox;
use crate::primitives::{CircularArc, CubicBezier, EllipticalArc, LineSeg, PolyCurve};

pub trait CurveSegment {
    fn domain(&self) -> (f64, f64);

    fn evaluate_f64(&self, t: f64) -> (f64, f64);

    fn bounding_box(&self) -> BoundingBox;

    fn tangent_f64(&self, t: f64) -> (f64, f64);

    fn normal_f64(&self, t: f64) -> (f64, f64) {
        let (tx, ty) = self.tangent_f64(t);
        (-ty, tx)
    }

    fn arc_length(&self) -> f64;

    /// The domain parameter `t` as a clamped 0..1 fraction, or `None` when
    /// the domain has collapsed to a point.
    fn normalized_param(&self, t: f64) -> Option<f64> {
        let (t0, t1) = self.domain();
        ((t1 - t0).abs() > 1e-12).then(|| ((t - t0) / (t1 - t0)).clamp(0.0, 1.0))
    }

    /// [`CurveSegment::param_at_length`] for many distances at once,
    /// results in input order. The default serves every query from one
    /// cumulative chord walk — per-query walks would cost N×256 curve
    /// evaluations for a DIVIDE/MEASURE over a spline. Uniform-speed kinds
    /// override with their per-query closed forms.
    fn param_at_lengths(&self, distances: &[f64]) -> Vec<f64> {
        let (t0, t1) = self.domain();
        // Past-the-end queries clamp to the end, like the singular form.
        let mut out = vec![t1; distances.len()];
        // The singular form short-circuits non-finite and non-positive
        // distances to the start; do the same here and keep only the
        // positive-finite queries for the walk (so the ascending sort the
        // cursor relies on never has to order a NaN).
        let mut order: Vec<usize> = (0..distances.len())
            .filter(|&i| {
                let keep = distances[i].is_finite() && distances[i] > 0.0;
                if !keep {
                    out[i] = t0;
                }
                keep
            })
            .collect();
        order.sort_by(|&a, &b| distances[a].total_cmp(&distances[b]));

        const N: usize = 256;
        let mut prev = self.evaluate_f64(t0);
        let mut acc = 0.0;
        let mut qi = 0;
        for i in 1..=N {
            if qi >= order.len() {
                break;
            }
            let t = t0 + (t1 - t0) * i as f64 / N as f64;
            let p = self.evaluate_f64(t);
            let d = (p.0 - prev.0).hypot(p.1 - prev.1);
            while qi < order.len() && acc + d >= distances[order[qi]] {
                let s = distances[order[qi]];
                let f = if d > 1e-12 { (s - acc) / d } else { 1.0 };
                out[order[qi]] = t0 + (t1 - t0) * ((i - 1) as f64 + f) / N as f64;
                qi += 1;
            }
            acc += d;
            prev = p;
        }
        out
    }

    /// Parameter at which the arc length measured from the curve's start
    /// reaches `s`, clamped to the domain: `s ≤ 0` (or non-finite) gives
    /// the start, `s` past the total length gives the end. Uniform
    /// *parameter* steps are not uniform *spacing* on beziers and NURBS;
    /// this is the inverse mapping that DIVIDE/MEASURE-style operations
    /// need to place points evenly along the curve itself.
    fn param_at_length(&self, s: f64) -> f64 {
        let (t0, t1) = self.domain();
        if !s.is_finite() || s <= 0.0 {
            return t0;
        }
        // Cumulative chord walk: kind-agnostic, and the returned parameter
        // is exact on the curve — only the spacing carries the ~1/N chord
        // error, in line with the tolerance-based style used elsewhere.
        // Uniform-speed kinds (lines, circular arcs) and polycurves
        // override with closed forms.
        const N: usize = 256;
        let mut prev = self.evaluate_f64(t0);
        let mut acc = 0.0;
        for i in 1..=N {
            let t = t0 + (t1 - t0) * i as f64 / N as f64;
            let p = self.evaluate_f64(t);
            let d = (p.0 - prev.0).hypot(p.1 - prev.1);
            if acc + d >= s {
                let f = if d > 1e-12 { (s - acc) / d } else { 1.0 };
                return t0 + (t1 - t0) * ((i - 1) as f64 + f) / N as f64;
            }
            acc += d;
            prev = p;
        }
        t1
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Curve {
    Line(LineSeg),
    Arc(CircularArc),
    Ellipse(EllipticalArc),
    Bezier(CubicBezier),
    Poly(Box<PolyCurve>),
    Rational(RationalBezier),
    Nurbs(NurbsCurve),
}

impl Curve {
    pub fn as_line(&self) -> Option<&LineSeg> {
        if let Curve::Line(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// True when every defining number is finite. A NaN/inf smuggled in from
    /// a corrupt file can't be caught via `bounding_box()` — `f64::min`/`max`
    /// return the non-NaN operand, so a poisoned corner masks itself — hence
    /// this explicit walk over the raw fields.
    pub fn is_finite(&self) -> bool {
        match self {
            Curve::Line(l) => l.p0.is_finite() && l.p1.is_finite(),
            Curve::Arc(a) => {
                a.center.is_finite()
                    && a.radius.is_finite()
                    && a.start_angle.is_finite()
                    && a.end_angle.is_finite()
            }
            Curve::Ellipse(e) => {
                e.center.is_finite()
                    && e.semi_major.is_finite()
                    && e.semi_minor.is_finite()
                    && e.rotation.is_finite()
                    && e.start_angle.is_finite()
                    && e.end_angle.is_finite()
            }
            Curve::Bezier(b) => {
                b.p0.is_finite() && b.p1.is_finite() && b.p2.is_finite() && b.p3.is_finite()
            }
            Curve::Poly(pc) => pc.segments.iter().all(|s| s.is_finite()),
            Curve::Rational(r) => {
                r.points.iter().all(|p| p.is_finite()) && r.weights.iter().all(|w| w.is_finite())
            }
            Curve::Nurbs(n) => {
                n.control.iter().all(|p| p.is_finite()) && n.weights.iter().all(|w| w.is_finite())
            }
        }
    }
}

/// Forwards a `CurveSegment` call to whichever variant `Curve` holds, binding the
/// inner value to `$v`. Keeps the seven-arm match in exactly one place so a new
/// `Curve` variant can never silently miss a method.
macro_rules! dispatch {
    ($self:ident, $v:ident => $body:expr) => {
        match $self {
            Curve::Line($v) => $body,
            Curve::Arc($v) => $body,
            Curve::Ellipse($v) => $body,
            Curve::Bezier($v) => $body,
            Curve::Poly($v) => $body,
            Curve::Rational($v) => $body,
            Curve::Nurbs($v) => $body,
        }
    };
}

impl CurveSegment for Curve {
    fn domain(&self) -> (f64, f64) {
        dispatch!(self, v => v.domain())
    }
    fn evaluate_f64(&self, t: f64) -> (f64, f64) {
        dispatch!(self, v => v.evaluate_f64(t))
    }
    fn bounding_box(&self) -> BoundingBox {
        dispatch!(self, v => v.bounding_box())
    }
    fn tangent_f64(&self, t: f64) -> (f64, f64) {
        dispatch!(self, v => v.tangent_f64(t))
    }
    fn arc_length(&self) -> f64 {
        dispatch!(self, v => v.arc_length())
    }
    fn param_at_length(&self, s: f64) -> f64 {
        dispatch!(self, v => v.param_at_length(s))
    }
    fn param_at_lengths(&self, distances: &[f64]) -> Vec<f64> {
        dispatch!(self, v => v.param_at_lengths(distances))
    }
}

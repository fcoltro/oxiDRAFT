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
}

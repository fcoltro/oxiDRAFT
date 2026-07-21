//! The primitive curve types — the concrete shapes the [`crate::Curve`] enum
//! wraps: straight segments, circular and elliptical arcs, cubic Béziers, and
//! polycurves (chains of the others). Each implements [`crate::CurveSegment`].

pub mod circular_arc;
pub mod cubic_bezier;
pub mod elliptical_arc;
pub mod line_seg;
pub mod polycurve;

pub use circular_arc::CircularArc;
pub use cubic_bezier::CubicBezier;
pub use elliptical_arc::EllipticalArc;
pub use line_seg::LineSeg;
pub use polycurve::PolyCurve;

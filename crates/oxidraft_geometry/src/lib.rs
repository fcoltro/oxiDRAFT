//! oxiDRAFT's 2D geometry kernel.
//!
//! An `f64`-with-tolerance kernel for planar CAD geometry: points and bounding
//! boxes ([`point`]), the primitive curves lines/arcs/ellipses/Béziers
//! ([`primitives`]), rational Béziers and NURBS ([`nurbs`]), the unifying
//! [`Curve`] enum and its [`CurveSegment`] trait ([`curve`]), affine transforms
//! ([`transform`]), and the geometric operations — intersection, distance,
//! offset, blend, tangency, curvature, winding, split/reverse ([`ops`]).
//!
//! The kernel is deliberately float-based rather than exact-arithmetic: every
//! comparison that matters is made against an explicit tolerance, which keeps it
//! fast and NURBS-ready while staying robust to the degenerate inputs a drawing
//! program throws at it. The commonly used types and functions are re-exported
//! at the crate root.

pub mod curve;
pub mod error;
pub mod nurbs;
pub mod ops;
pub mod point;
pub mod primitives;
pub mod transform;
pub mod util;

pub use curve::{Curve, CurveSegment};
pub use error::GeomError;
pub use nurbs::{NurbsCurve, RationalBezier, cv_spline_segments, lower, tessellate_curve};
pub use ops::{
    Continuity, CurveIntersection, ProjectionResult, blend_curves, circle_through_three_points,
    common_tangent_segments, curvature_at, curve_to_curve_distance, intersect,
    intersect_circle_circle, intersect_line_circle, intersect_line_line, intersect_lines_unbounded,
    normal_at, offset_curve, point_to_curve_distance, project_point_onto_curve,
    rational_winding_angle, refit_nurbs_subcurve, reverse_curve, split_curve, tangent_at,
    tangent_circle_ttr, tangent_circle_ttt, tangent_points_from_point,
};
pub use point::{BoundingBox, Point2d};
pub use primitives::{CircularArc, CubicBezier, EllipticalArc, LineSeg, PolyCurve};
pub use transform::Transform2d;
pub use util::{
    MinTracker, point_segment_dist, point_segment_dist_sq, positive_sweep, wrap_deg360, wrap_from,
    wrap_pi, wrap_tau,
};

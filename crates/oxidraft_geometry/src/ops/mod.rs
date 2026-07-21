pub mod blend;
pub mod curvature;
pub mod distance;
pub mod intersect;
pub mod offset;
pub mod split_reverse;
pub mod tangent;
pub mod winding;

pub use blend::{Continuity, blend_curves};
pub use curvature::{curvature_at, normal_at, tangent_at};
pub use distance::{
    ProjectionResult, curve_to_curve_distance, point_to_curve_distance, project_point_onto_curve,
};
pub use intersect::{
    CurveIntersection, intersect, intersect_circle_circle, intersect_general,
    intersect_line_circle, intersect_line_line, intersect_lines_unbounded,
};
pub use offset::{offset_curve, refit_nurbs_subcurve};
pub use split_reverse::{reverse_curve, split_curve};
pub use tangent::{
    circle_through_three_points, common_tangent_segments, tangent_circle_ttr, tangent_circle_ttt,
    tangent_points_from_point,
};
pub use winding::rational_winding_angle;

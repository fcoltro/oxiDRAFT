pub mod constrain;
pub mod draw;
pub mod edit;
pub mod grips;
pub mod hatch;
pub mod infer;
pub mod inquiry;
pub mod selection;
pub mod snap;

pub use constrain::{
    ConstrainError, DofSummary, constrain_angle, constrain_block, constrain_coincident_points,
    constrain_concentric, constrain_distance, constrain_equal_radius, constrain_fixed,
    constrain_line_distance, constrain_lines, constrain_point_distance, constrain_point_pair,
    constrain_radius, constrain_symmetric_points, diagnose_conflict, dof_report,
    resolve_after_edit, resolve_after_transform, selection_validity,
};
pub use draw as commands;
pub use grips::{Grip, GripRole, apply_grip, apply_grip_value, grip_value_label, grips_for};
pub use hatch::{
    PickRegionError, boundary_loop, outline_loops as hatch_outline_loops,
    pattern_dots as hatch_pattern_dots, pattern_lines as hatch_pattern_lines, region_contains,
    trace_pick_region, triangulate as triangulate_hatch, triangulate_contours,
    triangulate_with_tol as triangulate_hatch_with_tol,
};
pub use infer::{Guide, GuideKind, InferResult, infer_axis};
pub use oxidraft_document::ConstraintKind;
pub use selection::{pick_at, select_by, select_crossing, select_fence, select_window};
pub use snap::{
    SNAP_KINDS, SnapKind, SnapPoint, SnapSettings, best_snap, find_snaps, find_snaps_excluding,
};

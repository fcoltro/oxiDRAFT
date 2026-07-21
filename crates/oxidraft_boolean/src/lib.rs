//! 2D boolean operations on filled regions.
//!
//! A [`Region`] is an outer boundary with optional holes; [`union`],
//! [`intersection`], [`difference`], and [`xor`] combine two of them, returning
//! a set of disjoint result regions. A curve-preserving path keeps arcs and
//! ellipses exact where it can, falling back to the tessellating polygon
//! clipper ([`clip::clip`]) for degenerate cases. The [`weld`] module closes
//! tiny boundary gaps first.

pub mod boolean_ops;
pub mod clip;
mod curved;
pub mod region;
pub mod weld;
pub use boolean_ops::{difference, intersection, union, xor};
pub use clip::{BoolOp, clip};
pub use region::Region;
pub use weld::{WELD_TOL, weld_region};

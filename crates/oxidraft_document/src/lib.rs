//! oxiDRAFT's document model — the in-memory drawing.
//!
//! [`Document`] owns everything a drawing contains: [`entity`]s (curves, text,
//! dimensions, hatches, inserts) with their display [`properties`], [`layer`]s,
//! blocks, saved views, [`Settings`], and sketch [`constraint`]s. [`dimension`]
//! turns dimension entities into measured values and labels. This crate is pure
//! data and bookkeeping; the CAD, I/O, and UI crates operate on it.

pub mod constraint;
pub mod dimension;
pub mod document;
pub mod entity;
pub mod layer;
pub mod properties;

pub use constraint::{ANCHOR_DERIVED, ConstraintKind, SketchConstraint, normalize_angle_deg};
pub use dimension::{AngularSweep, angular_sweep, label_text, linear_orientation, measured_value};
pub use document::{Block, DIMENSION_LAYER, DimStyle, Document, NamedView, Settings, Units};
pub use entity::{Entity, EntityId, EntityKind, HatchPattern, TangentRef};
pub use layer::{Layer, LayerTable};
pub use properties::{Color, LineTypeDef, LineTypeRef, LineWeight, XData};

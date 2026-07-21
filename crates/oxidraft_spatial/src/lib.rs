//! Spatial acceleration structures for the drawing.
//!
//! A [`quadtree`] over entity bounding boxes for fast rectangle / point /
//! nearest queries, and [`morton`] Z-order interleaving used to key cells.

pub mod morton;
pub mod quadtree;
pub use morton::morton_code;
pub use quadtree::{CellClass, QuadNode, Quadtree};

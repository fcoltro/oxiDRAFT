//! The egui-based UI layer: application state ([`state`]), the drawing
//! canvas and panels ([`view`]), interactive drawing/edit tools ([`tools`]),
//! the command line ([`command`]), undo/redo ([`history`]), crash-recovery
//! autosaving ([`autosave`]), and shared visual styling ([`theme`], [`icons`],
//! [`fonts`]).

pub mod autosave;
pub mod command;
pub mod fonts;
pub mod history;
pub mod icons;
pub mod state;
pub mod theme;
pub mod tools;
pub mod view;
pub mod view_transform;

pub use command::{Command, parse_command};
pub use egui;
pub use history::History;
pub use state::{AppState, UiPrefs};
pub use tools::{Tool, ToolEvent};
pub use view::{UiState, draw_ui};
pub use view_transform::ViewTransform;

//! Pure state, layout, protocol and theming for herdr-plus.
//!
//! This crate holds everything the runtime (server) and the terminal client
//! both need, without depending on a terminal library: pane trees and their
//! geometry, the session model, the wire protocol, agent detection, themes,
//! `@path` mention parsing and text utilities.

pub mod config;
pub mod detect;
pub mod geom;
pub mod ids;
pub mod layout;
pub mod mention;
pub mod model;
pub mod paths;
pub mod protocol;
pub mod text;
pub mod theme;

pub use config::Config;
pub use geom::Rect;
pub use ids::{PaneId, TabId, WorkspaceId};
pub use layout::{Node, PanePlacement, SplitDir};
pub use model::{PaneKind, PaneMeta, SessionState, Tab, Workspace};
pub use theme::Theme;

/// Version reported over the socket and by `--version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

//! UI state and logic in plain Rust, outside the gpui views, so it is unit tested without a window
//! or a GPU.

pub mod config;
pub mod editor;
pub mod explorer;
pub mod find;
pub mod git;
pub mod git_branches;
pub mod git_gutter;
pub mod git_sync;
pub mod goto;
pub mod launch;
pub mod locale;
pub mod minimap;
pub mod open_queue;
pub mod recents;
pub mod settings;
pub mod status;
pub mod syntax;
pub mod tabs;
pub mod wrap;

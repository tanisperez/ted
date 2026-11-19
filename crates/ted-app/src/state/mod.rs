//! UI state and logic in plain Rust, outside the gpui views, so it is unit tested without a window
//! or a GPU.

pub mod config;
pub mod explorer;
pub mod file_icon;
pub mod find_files;
pub mod git;
pub mod git_branches;
pub mod git_diff;
pub mod git_gutter;
pub mod git_sync;
pub mod language_picker;
pub mod launch;
pub mod locale;
pub mod markdown_view;
pub mod open_queue;
pub mod palette;
pub mod recents;
pub mod settings;
pub mod shortcuts;
pub mod status;
pub mod tabs;
pub mod update;
pub mod whats_new;
pub mod window;

//! The gpui views. Kept thin: state and logic live in `crate::state`.

mod choice;
mod document;
mod editor;
mod explorer;
mod find;
mod git;
mod goto;
mod location;
mod logo;
mod root;
mod settings;
mod settings_git;
mod status_bar;
mod tabs;
mod title_bar;
mod welcome;

pub use editor::bind_keys;
pub use explorer::bind_keys as bind_explorer_keys;
pub use find::bind_keys as bind_find_keys;
pub use goto::bind_keys as bind_goto_keys;
pub use root::AppView;
pub use settings::apply_editor_settings;

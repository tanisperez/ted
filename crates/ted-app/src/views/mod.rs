//! The gpui views. Kept thin: state and logic live in `crate::state`.

mod choice;
mod document;
mod editor;
mod explorer;
mod rail;
mod root;
mod status_bar;
mod title_bar;
mod welcome;

pub use editor::bind_keys;
pub use explorer::bind_keys as bind_explorer_keys;
pub use root::AppView;

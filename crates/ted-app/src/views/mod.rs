//! The gpui views. Kept thin: state and logic live in `crate::state`.

mod editor;
mod root;
mod status_bar;
mod title_bar;
mod welcome;

pub use editor::bind_keys;
pub use root::AppView;

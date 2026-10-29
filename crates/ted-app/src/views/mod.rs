//! The gpui views. Kept thin: state and logic live in `crate::state`.

mod root;
mod status_bar;
mod title_bar;

pub use root::AppView;

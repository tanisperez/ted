//! The text editor component: [`EditorView`] owns an [`EditorState`] (a rope buffer, cursors,
//! undo) and paints it with its own gpui element, which lays out and paints only the visible
//! lines. Syntax colors, folding, soft wrap, the minimap, the git gutter, the find bar and Go to
//! line come with it.
//!
//! Its strings are its own (`locales/`), in English and Spanish, and follow the process wide
//! `rust_i18n` locale the app sets.

rust_i18n::i18n!("locales", fallback = "en");

pub mod metrics;
pub mod state;
pub mod syntax_colors;
pub mod views;

pub use state::editor::EditorState;
pub use state::gutter;
pub use syntax_colors::SyntaxColors;
pub use views::editor::{EditorDisplay, EditorView, bind_keys};

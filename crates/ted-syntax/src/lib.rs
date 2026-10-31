//! Language detection and syntax highlighting: tree-sitter for code, a light highlighter for
//! logs and batch files. No `gpui`; the work is blocking and meant for a background executor, and only the
//! visible range is queried per frame.
//!
//! The flow for a document:
//!
//! 1. [`Language::detect`] from the path, shown in the status bar with [`Language::name`].
//! 2. [`SyntaxTree::parse`] on a background thread with a clone of the buffer (cloning is O(1)).
//! 3. On every frame, [`SyntaxTree::highlights`] for the visible byte range.
//! 4. After edits, take the buffer's changes (`Buffer::track_changes` once, then
//!    `Buffer::take_changes`), call [`SyntaxTree::edited`] to get a tree the editor can paint at
//!    once (its ranges follow the text), and reparse in the background passing that tree as
//!    `previous`.
//!
//! Logs, batch files and the line based config files have no tree: [`highlight_log_line`] and
//! the highlighters of [`Language::line_highlighter`] work on the text of one line.

mod batch;
mod editorconfig;
mod gitignore;
mod grammar;
mod highlight;
mod language;
mod log;
mod tree;

pub use batch::highlight_batch_line;
pub use editorconfig::highlight_editorconfig_line;
pub use gitignore::highlight_gitignore_line;
pub use highlight::{Highlight, HighlightSpan};
pub use language::Language;
pub use log::highlight_log_line;
pub use tree::SyntaxTree;

//! The text buffer of ted: the rope, positions, edits and their undo history, line endings and
//! indentation detection.
//! No IO and no `gpui`, so everything here is unit tested.
//!
//! Every index is a byte offset into the UTF-8 text, as in `ropey` 2. Lines are split on LF; a
//! CRLF line ends with a `\r` that [`Buffer::line`] leaves out.

mod buffer;
mod cleanup;
mod detect;
mod history;

pub use buffer::{Buffer, Position};
pub use cleanup::cleanup_edits;
pub use detect::{Indent, LineEnding, SAMPLE_LEN, detect_indent, detect_line_ending};
pub use history::{Edit, EditKind, History, Selection};
pub use ropey::{Rope, RopeSlice};

//! The text buffer of ted: the rope, positions, edits, line endings and indentation detection.
//! No IO and no `gpui`, so everything here is unit tested.
//!
//! Every index is a byte offset into the UTF-8 text, as in `ropey` 2. Lines are split on LF; a
//! CRLF line ends with a `\r` that [`Buffer::line`] leaves out.

mod buffer;
mod detect;

pub use buffer::{Buffer, Position};
pub use detect::{Indent, LineEnding, SAMPLE_LEN, detect_indent, detect_line_ending};
pub use ropey::{Rope, RopeSlice};

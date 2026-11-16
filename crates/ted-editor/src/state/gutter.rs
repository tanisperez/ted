//! The change marks of the editor's gutter: which lines differ from a base, such as the last
//! commit. The app computes them off the main thread and hands them to the editor
//! ([`EditorState::git_marks`](crate::EditorState::git_marks)); the editor only looks up the
//! marks of its visible lines.

use std::ops::Range;
use std::sync::Arc;

/// What changed on a run of lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    /// Lines that are new.
    Added,
    /// Lines that replace others.
    Modified,
    /// Lines were removed just before the first line of the mark (its range is empty).
    Deleted,
}

/// A run of lines with the same change, 0 based. A [`MarkKind::Deleted`] mark has an empty range
/// at the line that follows the removed ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GutterMark {
    pub lines: Range<usize>,
    pub kind: MarkKind,
}

/// The marks of one buffer, valid while the buffer is still at `version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitMarks {
    /// The buffer version the marks were computed for: any edit hides them until the next save.
    pub version: u64,
    /// In line order.
    pub marks: Arc<Vec<GutterMark>>,
}

impl GitMarks {
    /// The marks that touch `lines`: a binary search, then only the visible ones.
    pub fn on_lines(&self, lines: Range<usize>) -> impl Iterator<Item = &GutterMark> {
        let first = self
            .marks
            .partition_point(|mark| mark.lines.end.max(mark.lines.start + 1) <= lines.start);
        self.marks[first..]
            .iter()
            .take_while(move |mark| mark.lines.start < lines.end.max(lines.start + 1))
    }
}

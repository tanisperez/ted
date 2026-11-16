//! The change marks of the editor's gutter: which lines of the file on disk differ from the last
//! commit. Computed from the file's diff (`telaio_git::parse_file_diff`) on the background
//! executor after a load, a save or a status change, never per frame; the editor only looks up
//! the marks of its visible lines.

pub use telaio_editor::gutter::{GitMarks, GutterMark, MarkKind};
use telaio_git::FileDiff;

/// The marks of a parsed diff from `HEAD` to the file on disk.
pub fn gutter_marks(diff: &FileDiff) -> Vec<GutterMark> {
    if diff.binary {
        return Vec::new();
    }
    diff.blocks
        .iter()
        .map(|block| {
            let start = (block.new_start as usize).saturating_sub(1);
            let (kind, end) = match (block.removed, block.added) {
                (_, 0) => (MarkKind::Deleted, start),
                (0, added) => (MarkKind::Added, start + added),
                (_, added) => (MarkKind::Modified, start + added),
            };
            GutterMark {
                lines: start..end,
                kind,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;
    use telaio_git::parse_file_diff;

    const PATCH: &str = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,8 +1,8 @@\n one\n-two\n+TWO\n three\n+new\n four\n-five\n six\n seven\n";

    #[test]
    fn blocks_become_added_modified_and_deleted_marks() {
        let marks = gutter_marks(&parse_file_diff(PATCH));
        assert_eq!(
            marks,
            vec![
                GutterMark {
                    lines: 1..2,
                    kind: MarkKind::Modified
                },
                GutterMark {
                    lines: 3..4,
                    kind: MarkKind::Added
                },
                GutterMark {
                    lines: 5..5,
                    kind: MarkKind::Deleted
                },
            ]
        );
    }

    #[test]
    fn only_the_marks_on_the_visible_lines_are_listed() {
        let marks = GitMarks {
            version: 0,
            marks: Arc::new(gutter_marks(&parse_file_diff(PATCH))),
        };
        let kinds = |lines| {
            marks
                .on_lines(lines)
                .map(|mark| mark.kind)
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds(0..1), vec![]);
        assert_eq!(kinds(0..3), vec![MarkKind::Modified]);
        assert_eq!(kinds(3..6), vec![MarkKind::Added, MarkKind::Deleted]);
        assert_eq!(kinds(6..8), vec![]);
    }
}

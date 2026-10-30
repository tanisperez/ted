//! The whitespace cleanup applied on save: trailing spaces and tabs removed from every line, and a
//! line break added at the end of a text that has none. Computed as [`Edit`]s, so the editor
//! applies them as one undo step, and from a snapshot of the buffer, so a big file can be scanned
//! on a background thread.

use ropey::LineType;

use crate::{Buffer, Edit};

/// The edits that clean `buffer` up, sorted by offset: one per line with trailing whitespace
/// when `trim`, and the final line break when `final_newline`. Empty when there is nothing to do.
pub fn cleanup_edits(buffer: &Buffer, trim: bool, final_newline: bool) -> Vec<Edit> {
    let rope = buffer.rope();
    let mut edits = Vec::new();
    if trim {
        let mut start = 0;
        for line in rope.lines(LineType::LF) {
            let len = line.len();
            // The end of the content: before `\n` or `\r\n`.
            let mut end = len;
            if end > 0 && line.byte(end - 1) == b'\n' {
                end -= 1;
                if end > 0 && line.byte(end - 1) == b'\r' {
                    end -= 1;
                }
            }
            let mut trimmed = end;
            while trimmed > 0 && matches!(line.byte(trimmed - 1), b' ' | b'\t') {
                trimmed -= 1;
            }
            if trimmed < end {
                edits.push(Edit {
                    start: start + trimmed,
                    old_text: line.slice(trimmed..end).to_string(),
                    new_text: String::new(),
                });
            }
            start += len;
        }
    }
    let len = rope.len();
    if final_newline && len > 0 && rope.byte(len - 1) != b'\n' {
        edits.push(Edit {
            start: len,
            old_text: String::new(),
            new_text: buffer.line_ending().as_str().to_string(),
        });
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditKind, History, Selection};
    use pretty_assertions::assert_eq;

    fn cleaned(text: &str, trim: bool, final_newline: bool) -> String {
        let mut buffer = Buffer::from_text(text);
        let edits = cleanup_edits(&buffer, trim, final_newline);
        let cursor = vec![Selection::cursor(0)];
        History::default().edit_many(&mut buffer, edits, EditKind::Other, cursor.clone(), cursor);
        buffer.to_text()
    }

    #[test]
    fn trailing_whitespace_goes_and_line_endings_stay() {
        assert_eq!(cleaned("a  \nb\t\n  \n", true, false), "a\nb\n\n");
        assert_eq!(cleaned("a \r\nb\r\n", true, false), "a\r\nb\r\n");
        assert_eq!(cleaned("  indented\n", true, false), "  indented\n");
        assert_eq!(cleaned("last  ", true, false), "last");
    }

    #[test]
    fn a_final_line_break_in_the_document_ending() {
        assert_eq!(cleaned("a\nb", false, true), "a\nb\n");
        assert_eq!(cleaned("a\r\nb", false, true), "a\r\nb\r\n");
        assert_eq!(cleaned("a\n", false, true), "a\n");
        assert_eq!(cleaned("", false, true), "");
        assert_eq!(cleaned("x  ", true, true), "x\n");
    }

    #[test]
    fn nothing_to_do_is_no_edit() {
        assert!(cleanup_edits(&Buffer::from_text("clean\n"), true, true).is_empty());
        assert!(cleanup_edits(&Buffer::from_text("dirty  "), false, false).is_empty());
    }
}

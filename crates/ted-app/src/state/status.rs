//! The status bar's texts for the open document, computed from the editor state. Everything here
//! costs the same for any file size: it runs on every render.

use rust_i18n::t;
use ted_core::Indent;
use ted_syntax::Language;

use super::editor::EditorState;

/// The status bar items of a document, left to right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentStatus {
    /// `Ln 12, Col 5`.
    pub position: String,
    /// `Spaces: 4` or `Tabs`, `None` when the file has no indented line.
    pub indent: Option<String>,
    /// `UTF-8`, `UTF-8 with BOM`, or `UTF-8 (read only)` for a lossily decoded file.
    pub encoding: String,
    /// `LF` or `CRLF`.
    pub line_ending: &'static str,
    /// `Rust`, `JSON`, `Log`, `Plain text`, ...
    pub language: String,
}

impl DocumentStatus {
    pub fn of(state: &EditorState) -> Self {
        let cursor = state.cursor;
        let column = state.buffer.display_column(cursor);
        let line = (cursor.line + 1).to_string();
        let column = column.to_string();
        let indent = state.buffer.indent().map(|indent| match indent {
            Indent::Tabs => t!("status.tabs").into_owned(),
            Indent::Spaces(count) => t!("status.spaces", count = count).into_owned(),
        });
        let encoding = if state.read_only {
            t!("status.encoding_lossy").into_owned()
        } else if state.bom {
            t!("status.encoding_bom").into_owned()
        } else {
            "UTF-8".to_string()
        };
        Self {
            position: t!("status.position", line = line, column = column).into_owned(),
            indent,
            encoding,
            line_ending: state.buffer.line_ending().label(),
            language: match state.syntax.language {
                Language::PlainText => t!("status.plain_text").into_owned(),
                language => language.name().to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use ted_core::{Buffer, Position};

    #[test]
    fn a_crlf_file_indented_with_two_spaces() {
        let mut state = EditorState::new(Buffer::from_text("a:\r\n  b: 1\r\n"));
        state.cursor = Position::new(1, 2);
        let status = DocumentStatus::of(&state);
        assert_eq!(status.line_ending, "CRLF");
        assert_eq!(status.encoding, "UTF-8");
        assert!(status.position.contains('2') && status.position.contains('3'));
        assert!(status.indent.is_some_and(|indent| indent.contains('2')));
    }

    #[test]
    fn the_language_is_named() {
        let mut state = EditorState::new(Buffer::from_text("{}"));
        state.syntax = crate::state::syntax::Syntax::new(Language::Json, 0);
        assert_eq!(DocumentStatus::of(&state).language, "JSON");
    }

    #[test]
    fn no_indent_item_without_indented_lines() {
        let state = EditorState::new(Buffer::from_text("plain\ntext\n"));
        assert_eq!(DocumentStatus::of(&state).indent, None);
    }
}

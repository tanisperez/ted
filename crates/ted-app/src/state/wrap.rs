//! Soft wrap: where a line breaks into rows for a number of columns. Plain functions on one
//! line, so the editor only wraps the lines it shows (and the ones it scrolls over), never the
//! whole document.
//!
//! A row ends at a word boundary when one fits: after the last space before the limit. A word
//! longer than a row breaks anywhere. A tab advances to the next multiple of the tab size, from
//! the start of its row, as the painted row expands it.

use std::ops::Range;
use std::sync::Arc;

use ted_syntax::Language;

use super::editor::tab_size;

/// Lines longer than this, in bytes, wrap every `columns` characters with no word boundaries
/// and no table of row starts: a minified file of megabytes on one line costs nothing to wrap.
pub const WRAP_WORDS_MAX: usize = 256 * 1024;

/// How a line breaks into rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineRows {
    /// The byte where each row starts, the first one 0.
    Words(Arc<[usize]>),
    /// A long line: a row every `columns` characters, `chars` characters in all.
    Chars { columns: usize, chars: usize },
}

impl LineRows {
    /// The number of rows, at least one.
    pub fn count(&self) -> usize {
        match self {
            LineRows::Words(starts) => starts.len(),
            LineRows::Chars { columns, chars } => chars.div_ceil(*columns).max(1),
        }
    }
}

/// Whether a document of `language` wraps when Alt+Z did not say: Markdown and plain text
/// always, code as the `word_wrap` setting (`global`) says.
pub fn wraps_by_default(language: Language, global: bool) -> bool {
    matches!(language, Language::Markdown | Language::PlainText) || global
}

/// The byte where each row of `line` starts when it wraps at `columns` columns (at least one).
pub fn wrap_line(line: &str, columns: usize) -> Vec<usize> {
    let columns = columns.max(1);
    let mut starts = vec![0];
    let mut row_start = 0;
    // The display column inside the row, and the byte after the row's last space.
    let mut column = 0;
    let mut boundary: Option<usize> = None;
    for (index, ch) in line.char_indices() {
        if column + width(ch, column) > columns && index > row_start {
            let start = match boundary {
                Some(at) if at > row_start => at,
                _ => index,
            };
            starts.push(start);
            row_start = start;
            boundary = None;
            // What came after the space starts the new row.
            column = recount(&line[start..index]);
        }
        column += width(ch, column);
        if ch == ' ' || ch == '\t' {
            boundary = Some(index + ch.len_utf8());
        }
    }
    starts
}

/// The columns `ch` takes at display column `column` of its row.
fn width(ch: char, column: usize) -> usize {
    match ch {
        '\t' => {
            let tab = tab_size();
            tab - column % tab
        }
        _ => 1,
    }
}

/// The display width of `text` from the start of a row.
fn recount(text: &str) -> usize {
    text.chars()
        .fold(0, |column, ch| column + width(ch, column))
}

/// The rows of a line of `len` bytes and `chars` characters whose text `text` gives when it is
/// short enough to wrap at words.
pub fn line_rows(
    len: usize,
    chars: usize,
    columns: usize,
    text: impl FnOnce() -> String,
) -> LineRows {
    if len > WRAP_WORDS_MAX {
        LineRows::Chars {
            columns: columns.max(1),
            chars,
        }
    } else {
        LineRows::Words(wrap_line(&text(), columns).into())
    }
}

/// The byte range of row `row` among `starts` in a line of `len` bytes.
pub fn row_range(starts: &[usize], row: usize, len: usize) -> Range<usize> {
    let start = starts.get(row).copied().unwrap_or(len);
    let end = starts.get(row + 1).copied().unwrap_or(len);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(line: &str, columns: usize) -> Vec<&str> {
        let starts = wrap_line(line, columns);
        (0..starts.len())
            .map(|row| &line[row_range(&starts, row, line.len())])
            .collect()
    }

    #[test]
    fn prose_wraps_by_default_and_code_follows_the_setting() {
        assert!(wraps_by_default(Language::Markdown, false));
        assert!(wraps_by_default(Language::PlainText, false));
        assert!(!wraps_by_default(Language::Rust, false));
        assert!(!wraps_by_default(Language::Log, false));
        assert!(wraps_by_default(Language::Rust, true));
    }

    #[test]
    fn short_lines_are_one_row() {
        assert_eq!(wrap_line("", 10), [0]);
        assert_eq!(wrap_line("hello", 10), [0]);
        assert_eq!(wrap_line("0123456789", 10), [0]);
    }

    #[test]
    fn breaks_after_the_last_space_that_fits() {
        assert_eq!(rows("the quick brown fox", 10), ["the quick ", "brown fox"]);
        assert_eq!(
            rows("aaa bbb ccc ddd eee", 8),
            ["aaa bbb ", "ccc ddd ", "eee"]
        );
    }

    #[test]
    fn long_words_break_anywhere() {
        assert_eq!(rows("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(rows("ab abcdefghij", 4), ["ab ", "abcd", "efgh", "ij"]);
    }

    #[test]
    fn accents_count_as_one_column() {
        assert_eq!(rows("ñññññ ñññ", 6), ["ñññññ ", "ñññ"]);
    }

    #[test]
    fn long_lines_wrap_by_characters() {
        let rows = line_rows(WRAP_WORDS_MAX + 1, 1000, 80, || unreachable!());
        assert_eq!(rows.count(), 13);
        assert_eq!(line_rows(5, 5, 80, || "hello".to_string()).count(), 1);
    }
}

//! Word boundaries inside one line, for Ctrl+Left, Ctrl+Right, Ctrl+Backspace and Ctrl+Delete.
//!
//! A line is a sequence of runs: word characters (letters, digits and `_`), punctuation, or
//! whitespace. A word movement skips the whitespace next to the cursor and then one run of
//! word characters or punctuation, as most editors do.

/// The kind of run a character belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Word,
    Space,
    Punctuation,
}

fn class(ch: char) -> Class {
    if ch.is_alphanumeric() || ch == '_' {
        Class::Word
    } else if ch.is_whitespace() {
        Class::Space
    } else {
        Class::Punctuation
    }
}

/// The byte column where the word before `column` starts: the whitespace before `column` and
/// then one run of word characters or punctuation are skipped. Zero at the line start.
pub fn previous_word_start(line: &str, column: usize) -> usize {
    let column = line.floor_char_boundary(column.min(line.len()));
    let mut chars = line[..column].char_indices().rev().peekable();
    while chars
        .next_if(|&(_, ch)| class(ch) == Class::Space)
        .is_some()
    {}
    let Some(&(_, first)) = chars.peek() else {
        return 0;
    };
    let target = class(first);
    let mut start = column;
    while let Some((index, _)) = chars.next_if(|&(_, ch)| class(ch) == target) {
        start = index;
    }
    start
}

/// The byte column where the word after `column` ends: the whitespace after `column` and then
/// one run of word characters or punctuation are skipped. The line length at the line end.
pub fn next_word_end(line: &str, column: usize) -> usize {
    let column = line.floor_char_boundary(column.min(line.len()));
    let mut chars = line[column..].char_indices().peekable();
    while chars
        .next_if(|&(_, ch)| class(ch) == Class::Space)
        .is_some()
    {}
    let Some(&(_, first)) = chars.peek() else {
        return line.len();
    };
    let target = class(first);
    let mut end = column;
    while let Some((index, ch)) = chars.next_if(|&(_, ch)| class(ch) == target) {
        end = column + index + ch.len_utf8();
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every stop of repeated movements from one end of `line` to the other.
    fn stops(line: &str, forward: bool) -> Vec<usize> {
        let mut column = if forward { 0 } else { line.len() };
        let mut stops = Vec::new();
        loop {
            let next = if forward {
                next_word_end(line, column)
            } else {
                previous_word_start(line, column)
            };
            if next == column {
                return stops;
            }
            stops.push(next);
            column = next;
        }
    }

    #[test]
    fn moves_over_words_and_punctuation() {
        let line = "let x = foo.bar_baz(1);";
        assert_eq!(stops(line, true), [3, 5, 7, 11, 12, 19, 20, 21, 23]);
        assert_eq!(stops(line, false), [21, 20, 19, 12, 11, 8, 6, 4, 0]);
    }

    #[test]
    fn skips_leading_whitespace_and_tabs() {
        let line = "\t\t  value  ";
        assert_eq!(next_word_end(line, 0), 9);
        assert_eq!(next_word_end(line, 9), line.len());
        assert_eq!(previous_word_start(line, line.len()), 4);
        assert_eq!(previous_word_start(line, 4), 0);
    }

    #[test]
    fn handles_accents_and_wide_characters() {
        let line = "año más 日本語";
        assert_eq!(stops(line, true), [4, 9, line.len()]);
        assert_eq!(stops(line, false), [10, 5, 0]);
        // A column inside a character counts from its start.
        assert_eq!(next_word_end(line, 2), 4);
    }

    #[test]
    fn stays_at_the_line_ends() {
        assert_eq!(previous_word_start("abc", 0), 0);
        assert_eq!(next_word_end("abc", 3), 3);
        assert_eq!(next_word_end("", 0), 0);
        assert_eq!(previous_word_start("   ", 3), 0);
    }

    #[test]
    fn punctuation_runs_are_one_stop() {
        let line = "a => b";
        assert_eq!(stops(line, true), [1, 4, 6]);
        assert_eq!(stops(line, false), [5, 2, 0]);
    }
}

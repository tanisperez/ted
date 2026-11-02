//! Finding a text or a pattern in a rope: [`SearchQuery`]. The rope is read in windows of
//! [`WINDOW`] bytes that overlap by the longest possible match, so a match across two chunks is
//! found, and no window copies more than a few MB whatever the size of the text.
//!
//! A literal query knows its longest match. A regular expression does not: its windows reach to
//! the end of the line they stop in (at most [`REGEX_REACH`] bytes more), so a pattern that
//! matches within a line is always found whole. A match across lines is found when it does not
//! cross a window edge. Every window also reads the character before it, so `^`, `$` and `\b`
//! see the real text around a window and not its edges.

use std::ops::Range;

use regex::bytes::{Regex, RegexBuilder};
use ropey::Rope;

/// Bytes of the rope copied and searched at a time.
const WINDOW: usize = 1024 * 1024;
/// How far past a window a regular expression's window reaches to end its line.
const REGEX_REACH: usize = 64 * 1024;
/// Bytes read before a window: one character, for `^` and `\b` at its start.
const CONTEXT: usize = 4;

/// How a query matches, from the toggles of the find bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    /// Only matches with a word boundary on both sides.
    pub whole_word: bool,
    /// The text is a regular expression (Rust's `regex` syntax), not a literal.
    pub regex: bool,
}

/// Why a text gives no query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    /// Nothing to find.
    Empty,
    /// Not a valid regular expression; the message says why.
    Invalid(String),
}

/// A text to find, compiled once.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    regex: Regex,
    /// The longest a match can be in bytes: a case insensitive match may use longer encodings
    /// of the same letters (`K` and the Kelvin sign). `None` for a regular expression.
    max_len: Option<usize>,
}

impl SearchQuery {
    /// The query for `text`, matched literally. `None` for an empty text.
    pub fn new(text: &str, case_sensitive: bool) -> Option<Self> {
        let options = SearchOptions {
            case_sensitive,
            ..SearchOptions::default()
        };
        Self::with_options(text, options).ok()
    }

    /// The query for `text` with `options`.
    pub fn with_options(text: &str, options: SearchOptions) -> Result<Self, QueryError> {
        if text.is_empty() {
            return Err(QueryError::Empty);
        }
        let pattern = if options.regex {
            text.to_string()
        } else {
            regex::escape(text)
        };
        let pattern = if options.whole_word {
            format!(r"\b(?:{pattern})\b")
        } else {
            pattern
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!options.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|error| QueryError::Invalid(error.to_string()))?;
        let max_len = (!options.regex).then(|| text.chars().count() * 4);
        Ok(Self { regex, max_len })
    }

    /// The text that replaces `matched`, a match of this query: `with`, with `$1` and `${name}`
    /// expanded from the match's groups for a regular expression (`$$` is a `$`), and in the case
    /// of `matched` when `preserve_case` is set (see [`preserve_case`]).
    pub fn replacement(&self, matched: &str, with: &str, preserve_case: bool) -> String {
        let text = if self.max_len.is_none() {
            match self.regex.captures(matched.as_bytes()) {
                Some(captures) => {
                    let mut expanded = Vec::new();
                    captures.expand(with.as_bytes(), &mut expanded);
                    String::from_utf8_lossy(&expanded).into_owned()
                }
                None => with.to_string(),
            }
        } else {
            with.to_string()
        };
        if preserve_case {
            self::preserve_case(matched, &text)
        } else {
            text
        }
    }

    /// Calls `found` with every match that starts in `range`, in order and without overlaps,
    /// until it returns `false`. A match may end after `range`.
    pub fn find_in(
        &self,
        rope: &Rope,
        range: Range<usize>,
        mut found: impl FnMut(Range<usize>) -> bool,
    ) {
        let len = rope.len();
        let end = range.end.min(len);
        let mut start = rope.floor_char_boundary(range.start.min(end));
        while start < end {
            let core_end = (start + WINDOW).min(end);
            let window_end = rope.ceil_char_boundary(self.window_end(rope, core_end));
            let window_start = rope.floor_char_boundary(start.saturating_sub(CONTEXT));
            let mut bytes = Vec::with_capacity(window_end - window_start);
            for chunk in rope.slice(window_start..window_end).chunks() {
                bytes.extend_from_slice(chunk.as_bytes());
            }
            let mut next = core_end;
            let mut at = start - window_start;
            while let Some(m) = self.regex.find_at(&bytes, at) {
                if m.start() == m.end() {
                    // An empty match (`^`, `a*`) finds nothing to show; look past it.
                    at = m.end() + 1;
                    if at > bytes.len() {
                        break;
                    }
                    continue;
                }
                let match_start = window_start + m.start();
                if match_start >= core_end {
                    break;
                }
                let matched = match_start..window_start + m.end();
                // The next window starts after this match, so matches never overlap.
                next = next.max(matched.end);
                if !found(matched) {
                    return;
                }
                at = m.end();
            }
            start = next;
        }
    }

    /// Where the window that searches up to `core_end` stops: past it by the longest match of a
    /// literal, at the end of its line for a regular expression.
    fn window_end(&self, rope: &Rope, core_end: usize) -> usize {
        let len = rope.len();
        match self.max_len {
            Some(max_len) => (core_end + max_len).min(len),
            None => {
                let reach = (core_end + REGEX_REACH).min(len);
                let mut end = core_end;
                for chunk in rope
                    .slice(rope.floor_char_boundary(core_end)..reach)
                    .chunks()
                {
                    match chunk.as_bytes().iter().position(|byte| *byte == b'\n') {
                        Some(newline) => return (end + newline + 1).min(len),
                        None => end += chunk.len(),
                    }
                }
                reach
            }
        }
    }

    /// The first match that starts at or after `offset`, before `limit`.
    pub fn next_from(&self, rope: &Rope, offset: usize, limit: usize) -> Option<Range<usize>> {
        let mut first = None;
        self.find_in(rope, offset..limit, |matched| {
            first = Some(matched);
            false
        });
        first
    }

    /// The last match that starts before `offset`, looking back no further than `limit`.
    pub fn previous_before(
        &self,
        rope: &Rope,
        offset: usize,
        limit: usize,
    ) -> Option<Range<usize>> {
        let mut end = offset;
        while end > limit {
            let start = end.saturating_sub(WINDOW).max(limit);
            let mut last = None;
            self.find_in(rope, start..end, |matched| {
                last = Some(matched);
                true
            });
            if last.is_some() {
                return last;
            }
            end = start;
        }
        None
    }
}

/// `replacement` in the case of `matched`: all capitals when `matched` is (and has two letters
/// or more), a capital first letter when `matched` starts with one, a small first letter when it
/// starts with a small one. Otherwise `replacement` as it is.
pub fn preserve_case(matched: &str, replacement: &str) -> String {
    let letters: Vec<char> = matched.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return replacement.to_uppercase();
    }
    let mut chars = replacement.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    match letters.first() {
        Some(letter) if letter.is_uppercase() => first.to_uppercase().chain(chars).collect(),
        Some(letter) if letter.is_lowercase() => first.to_lowercase().chain(chars).collect(),
        _ => replacement.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn all(query: &SearchQuery, rope: &Rope) -> Vec<Range<usize>> {
        let mut found = Vec::new();
        query.find_in(rope, 0..rope.len(), |matched| {
            found.push(matched);
            true
        });
        found
    }

    #[test]
    fn literal_and_case() {
        let rope = Rope::from_str("Error: a.b ERROR error a*b");
        let sensitive = SearchQuery::new("error", true).unwrap();
        assert_eq!(all(&sensitive, &rope), vec![17..22]);
        let insensitive = SearchQuery::new("error", false).unwrap();
        assert_eq!(all(&insensitive, &rope), vec![0..5, 11..16, 17..22]);
        // Matched literally, not as a pattern.
        let dot = SearchQuery::new("a.b", true).unwrap();
        assert_eq!(all(&dot, &rope), vec![7..10]);
        assert!(SearchQuery::new("", true).is_none());
    }

    #[test]
    fn matches_never_overlap() {
        let rope = Rope::from_str("aaaaa");
        let query = SearchQuery::new("aa", true).unwrap();
        assert_eq!(all(&query, &rope), vec![0..2, 2..4]);
    }

    #[test]
    fn across_windows_and_chunks() {
        // A match right on every window edge, in a text far bigger than one window.
        let mut text = "x".repeat(WINDOW - 2);
        text.push_str("needle");
        text.push_str(&"y".repeat(WINDOW));
        text.push_str("needle");
        let rope = Rope::from_str(&text);
        let query = SearchQuery::new("needle", true).unwrap();
        let expected = vec![WINDOW - 2..WINDOW + 4, 2 * WINDOW + 4..2 * WINDOW + 10];
        assert_eq!(all(&query, &rope), expected);
        assert_eq!(
            query.next_from(&rope, 10, rope.len()),
            Some(expected[0].clone())
        );
        assert_eq!(
            query.next_from(&rope, WINDOW, rope.len()),
            Some(expected[1].clone())
        );
        assert_eq!(
            query.previous_before(&rope, rope.len(), 0),
            Some(expected[1].clone())
        );
        assert_eq!(
            query.previous_before(&rope, 2 * WINDOW, 0),
            Some(expected[0].clone())
        );
        assert_eq!(query.previous_before(&rope, 100, 0), None);
    }

    #[test]
    fn a_range_keeps_matches_that_start_inside_it() {
        let rope = Rope::from_str("ab ab ab");
        let query = SearchQuery::new("ab", true).unwrap();
        let mut found = Vec::new();
        query.find_in(&rope, 2..4, |matched| {
            found.push(matched);
            true
        });
        assert_eq!(found, vec![3..5]);
    }

    #[test]
    fn unicode_case_folding() {
        let rope = Rope::from_str("ÁRBOL árbol");
        let query = SearchQuery::new("árbol", false).unwrap();
        assert_eq!(all(&query, &rope).len(), 2);
    }

    fn query(text: &str, case_sensitive: bool, whole_word: bool, regex: bool) -> SearchQuery {
        let options = SearchOptions {
            case_sensitive,
            whole_word,
            regex,
        };
        SearchQuery::with_options(text, options).unwrap()
    }

    #[test]
    fn whole_words() {
        let rope = Rope::from_str("find find_open refind find.");
        let words = query("find", true, true, false);
        assert_eq!(all(&words, &rope), vec![0..4, 22..26]);
    }

    #[test]
    fn regular_expressions() {
        let rope = Rope::from_str("let a = 10;\nlet bb = 200;");
        let numbers = query(r"\d+", true, false, true);
        assert_eq!(all(&numbers, &rope), vec![8..10, 21..24]);
        // `^` is the start of a line, also when the search starts in the middle of one.
        let lets = query("^let", true, false, true);
        assert_eq!(all(&lets, &rope), vec![0..3, 12..15]);
        let mut found = Vec::new();
        lets.find_in(&rope, 1..rope.len(), |matched| {
            found.push(matched);
            true
        });
        assert_eq!(found, vec![12..15]);
        assert_eq!(
            SearchQuery::with_options(
                "(",
                SearchOptions {
                    regex: true,
                    ..SearchOptions::default()
                }
            )
            .err()
            .map(|error| matches!(error, QueryError::Invalid(_))),
            Some(true)
        );
        assert_eq!(
            SearchQuery::with_options("", SearchOptions::default()).err(),
            Some(QueryError::Empty)
        );
    }

    #[test]
    fn a_regex_match_is_whole_across_a_window_edge() {
        let mut text = "x".repeat(WINDOW - 3);
        text.push_str(" key=value_that_is_long\n");
        let rope = Rope::from_str(&text);
        let pairs = query(r"key=\w+", true, false, true);
        assert_eq!(all(&pairs, &rope), vec![WINDOW - 2..WINDOW + 20]);
    }

    #[test]
    fn empty_matches_are_skipped() {
        let rope = Rope::from_str("abc\n\nab");
        let stars = query("b*", true, false, true);
        assert_eq!(all(&stars, &rope), vec![1..2, 6..7]);
    }

    #[test]
    fn replacements_expand_groups_and_keep_case() {
        let groups = query(r"(\w+)=(\w+)", true, false, true);
        assert_eq!(groups.replacement("a=b", "$2=$1", false), "b=a");
        let literal = query("a=b", true, false, false);
        assert_eq!(literal.replacement("a=b", "$2", false), "$2");
        assert_eq!(preserve_case("FIND", "search"), "SEARCH");
        assert_eq!(preserve_case("Find", "search"), "Search");
        assert_eq!(preserve_case("find", "Search"), "search");
        assert_eq!(preserve_case("_x", "Search"), "search");
        assert_eq!(preserve_case("42", "Search"), "Search");
        assert_eq!(preserve_case("A", "search"), "Search");
    }
}

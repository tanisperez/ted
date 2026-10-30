//! Finding a literal text in a rope: [`SearchQuery`]. The rope is read in windows of
//! [`WINDOW`] bytes that overlap by the longest possible match, so a match across two chunks is
//! found, and no window copies more than a few MB whatever the size of the text.

use std::ops::Range;

use regex::bytes::{Regex, RegexBuilder};
use ropey::Rope;

/// Bytes of the rope copied and searched at a time.
const WINDOW: usize = 1024 * 1024;

/// A text to find, compiled once.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    regex: Regex,
    /// The longest a match can be in bytes: a case insensitive match may use longer encodings
    /// of the same letters (`K` and the Kelvin sign).
    max_len: usize,
}

impl SearchQuery {
    /// The query for `text`, matched literally. `None` for an empty text.
    pub fn new(text: &str, case_sensitive: bool) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let regex = RegexBuilder::new(&regex::escape(text))
            .case_insensitive(!case_sensitive)
            .build()
            .ok()?;
        Some(Self {
            regex,
            max_len: text.chars().count() * 4,
        })
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
        let mut start = range.start.min(end);
        while start < end {
            let core_end = (start + WINDOW).min(end);
            let window_end = rope.ceil_char_boundary((core_end + self.max_len).min(len));
            let window_start = rope.floor_char_boundary(start);
            let mut bytes = Vec::with_capacity(window_end - window_start);
            for chunk in rope.slice(window_start..window_end).chunks() {
                bytes.extend_from_slice(chunk.as_bytes());
            }
            let mut next = core_end;
            for m in self.regex.find_iter(&bytes[start - window_start..]) {
                let match_start = start + m.start();
                if match_start >= core_end {
                    break;
                }
                let matched = match_start..start + m.end();
                // The next window starts after this match, so matches never overlap.
                next = next.max(matched.end);
                if !found(matched) {
                    return;
                }
            }
            start = next;
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
}

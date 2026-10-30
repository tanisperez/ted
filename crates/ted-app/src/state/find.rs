//! Find in file, outside the gpui views: the query, the matches found so far and which one is
//! current. The view searches the whole text in the background in steps ([`STEP`]) and hands
//! each step's matches to [`Find::add`]; painting searches only the visible lines.
//!
//! Only the first [`MAX_KEPT`] matches are kept: a one letter query in a 100 MB log has millions.
//! Past them the count goes on, and next and previous search the text near the cursor instead,
//! which is quick there because the matches are that dense.

use std::ops::Range;

use rust_i18n::t;
use ted_core::{Rope, SearchQuery};

/// Bytes searched by one background step before its matches are handed over.
pub const STEP: usize = 8 * 1024 * 1024;
/// The most matches kept.
pub const MAX_KEPT: usize = 100_000;
/// How far next and previous search the text directly, past the kept matches.
const NEAR: usize = 4 * 1024 * 1024;

/// The state of the find bar of one editor.
#[derive(Debug, Clone, Default)]
pub struct Find {
    pub text: String,
    pub case_sensitive: bool,
    /// The compiled [`Self::text`], `None` when it is empty.
    pub query: Option<SearchQuery>,
    /// The first matches, in order, at most [`MAX_KEPT`].
    matches: Vec<Range<usize>>,
    /// Every match found so far, kept or not.
    count: usize,
    /// The whole text was searched.
    complete: bool,
    /// The buffer version the matches belong to.
    pub version: u64,
    /// Bumped by every new search, so the steps of an older one are dropped.
    pub generation: u64,
}

impl Find {
    /// Starts over for `text` on the buffer at `version`. Returns the new generation.
    pub fn restart(&mut self, version: u64) -> u64 {
        self.query = SearchQuery::new(&self.text, self.case_sensitive);
        self.matches.clear();
        self.count = 0;
        self.complete = self.query.is_none();
        self.version = version;
        self.generation += 1;
        self.generation
    }

    /// The matches of one background step of `generation`, and whether it was the last.
    pub fn add(&mut self, generation: u64, matches: Vec<Range<usize>>, done: bool) {
        if generation != self.generation {
            return;
        }
        self.count += matches.len();
        let room = MAX_KEPT.saturating_sub(self.matches.len());
        self.matches.extend(matches.into_iter().take(room));
        self.complete = done;
    }

    /// Whether every match is kept, so next and previous can wrap with the kept list alone.
    fn all_kept(&self) -> bool {
        self.complete && self.count == self.matches.len()
    }

    /// The match after the one at or past `offset` (`forward`), or before it, wrapping around
    /// the end of the text. `None` while it is not known yet.
    pub fn step(&self, rope: &Rope, offset: usize, forward: bool) -> Option<Range<usize>> {
        let kept = &self.matches;
        if forward {
            let index = kept.partition_point(|matched| matched.start < offset);
            if let Some(matched) = kept.get(index) {
                return Some(matched.clone());
            }
            if self.all_kept() {
                return kept.first().cloned();
            }
            // Past the kept matches: dense, so the next one is near.
            let query = self.query.as_ref()?;
            query.next_from(rope, offset, (offset + NEAR).min(rope.len()))
        } else {
            let index = kept.partition_point(|matched| matched.start < offset);
            let beyond_kept = !self.all_kept() && index == kept.len() && !kept.is_empty();
            if index > 0 && !beyond_kept {
                return Some(kept[index - 1].clone());
            }
            if self.all_kept() {
                return kept.last().cloned();
            }
            let query = self.query.as_ref()?;
            query.previous_before(rope, offset, offset.saturating_sub(NEAR))
        }
    }

    /// The counter of the find bar: `3 of 120`, `120` while the current match is not among the
    /// kept ones, `No results`, or a count followed by `…` while the search runs.
    pub fn label(&self, current: Option<&Range<usize>>) -> String {
        if self.query.is_none() {
            return String::new();
        }
        if self.complete && self.count == 0 {
            return t!("find.no_results").into_owned();
        }
        let total = if self.complete {
            self.count.to_string()
        } else {
            format!("{}\u{2026}", self.count)
        };
        let index = current.and_then(|current| {
            self.matches
                .binary_search_by_key(&current.start, |matched| matched.start)
                .ok()
        });
        match index {
            Some(index) => t!("find.position", index = index + 1, total = total).into_owned(),
            None => total,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn found(text: &str, query: &str) -> (Find, Rope) {
        let rope = Rope::from_str(text);
        let mut find = Find {
            text: query.to_string(),
            ..Find::default()
        };
        let generation = find.restart(1);
        let mut matches = Vec::new();
        if let Some(query) = &find.query {
            query.find_in(&rope, 0..rope.len(), |matched| {
                matches.push(matched);
                true
            });
        }
        find.add(generation, matches, true);
        (find, rope)
    }

    #[test]
    fn steps_wrap_around() {
        let (find, rope) = found("ab ab ab", "ab");
        assert_eq!(find.step(&rope, 1, true), Some(3..5));
        assert_eq!(find.step(&rope, 7, true), Some(0..2));
        assert_eq!(find.step(&rope, 3, false), Some(0..2));
        assert_eq!(find.step(&rope, 0, false), Some(6..8));
    }

    #[test]
    fn labels() {
        let (find, _) = found("ab ab ab", "ab");
        assert_eq!(find.label(Some(&(3..5))), "2 of 3");
        assert_eq!(find.label(None), "3");
        let (none, _) = found("ab", "zz");
        assert_eq!(none.label(None), "No results");
        let (empty, _) = found("ab", "");
        assert_eq!(empty.label(None), "");
    }

    #[test]
    fn a_stale_step_is_dropped_and_partial_counts_show_progress() {
        let mut find = Find {
            text: "a".to_string(),
            ..Find::default()
        };
        let old = find.restart(1);
        let new = find.restart(2);
        find.add(old, vec![0..1, 2..3], true);
        assert_eq!(find.label(None), "0\u{2026}");
        find.add(new, vec![0..1, 4..5], false);
        assert_eq!(find.label(None), "2\u{2026}");
    }

    #[test]
    fn past_the_kept_matches_it_searches_near_the_cursor() {
        let text = "a".repeat(MAX_KEPT + 10);
        let (find, rope) = found(&text, "a");
        assert_eq!(find.label(None), (MAX_KEPT + 10).to_string());
        assert_eq!(
            find.step(&rope, MAX_KEPT + 5, true),
            Some(MAX_KEPT + 5..MAX_KEPT + 6)
        );
        assert_eq!(
            find.step(&rope, MAX_KEPT + 5, false),
            Some(MAX_KEPT + 4..MAX_KEPT + 5)
        );
        assert_eq!(find.step(&rope, 5, false), Some(4..5));
    }
}

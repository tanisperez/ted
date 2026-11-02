//! Find and replace in files: the pieces that touch the disk or a whole text, for the Find in
//! files window of the app. [`FileMask`] picks the files by name, [`read_text`] reads one as
//! text (never a binary, a file that is not UTF-8 or a huge one), [`find_lines`] finds the
//! matches of a text with the line around each one and a little context for the preview, and
//! [`replace_in_file`] replaces every match of a file on disk, atomically.
//!
//! Every function blocks: the app calls them on its background executor.

use std::fs;
use std::ops::Range;
use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use ted_core::{Buffer, SearchQuery};

use crate::load::{FileContent, load_file};
use crate::save::{SaveError, save_file};

/// Files bigger than this are not searched.
pub const MAX_SEARCHED_FILE: u64 = 50 * 1024 * 1024;
/// The most bytes of a line kept for a result row or a preview line; a longer line is cut around
/// its match.
pub const MAX_LINE_BYTES: usize = 300;
/// Lines of context before and after a match, for the preview.
pub const CONTEXT_LINES: usize = 2;

/// Which files to search, by name: globs separated by commas, `!` before the ones to leave out
/// (`*.rs, *.toml, !*.lock`). A glob without `/` matches the file name, one with `/` the path
/// relative to the folder. An empty mask takes every file.
#[derive(Debug, Clone, Default)]
pub struct FileMask {
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl FileMask {
    /// The mask written as `text`. An invalid glob gives its error.
    pub fn new(text: &str) -> Result<Self, String> {
        let mut include = GlobSetBuilder::new();
        let mut exclude = GlobSetBuilder::new();
        let (mut includes, mut excludes) = (0, 0);
        for pattern in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (negated, pattern) = match pattern.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, pattern),
            };
            // `*.rs` matches the name in any folder: as a path glob it is `**/*.rs`.
            let pattern = if pattern.contains('/') {
                pattern.trim_start_matches('/').to_string()
            } else {
                format!("**/{pattern}")
            };
            let glob = Glob::new(&pattern).map_err(|error| error.to_string())?;
            if negated {
                exclude.add(glob);
                excludes += 1;
            } else {
                include.add(glob);
                includes += 1;
            }
        }
        let build = |builder: GlobSetBuilder, count| -> Result<Option<GlobSet>, String> {
            match count {
                0 => Ok(None),
                _ => builder.build().map(Some).map_err(|error| error.to_string()),
            }
        };
        Ok(Self {
            include: build(include, includes)?,
            exclude: build(exclude, excludes)?,
        })
    }

    /// Whether the file at `relative` (`/` separated, from the folder) is searched.
    pub fn matches(&self, relative: &str) -> bool {
        let included = self
            .include
            .as_ref()
            .is_none_or(|set| set.is_match(relative));
        let excluded = self
            .exclude
            .as_ref()
            .is_some_and(|set| set.is_match(relative));
        included && !excluded
    }
}

/// One match of a text, with what the results list and the preview show of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMatch {
    /// The match in the whole text, in bytes.
    pub offset: Range<usize>,
    /// Its line, from 0.
    pub line: usize,
    /// The line, cut to [`MAX_LINE_BYTES`] around the match when it is longer.
    pub text: String,
    /// The match in [`Self::text`]. It may end at the cut.
    pub range: Range<usize>,
    /// The lines around it for the preview, from [`CONTEXT_LINES`] before to as many after, each
    /// with its number and cut to [`MAX_LINE_BYTES`]. The match's own line is among them.
    pub context: Vec<(usize, String)>,
}

/// The matches of `query` in `buffer`, at most `limit`, each with its line and context.
pub fn find_lines(query: &SearchQuery, buffer: &Buffer, limit: usize) -> Vec<LineMatch> {
    let mut found = Vec::new();
    query.find_in(buffer.rope(), 0..buffer.len(), |offset| {
        found.push(offset);
        found.len() < limit
    });
    found
        .into_iter()
        .map(|offset| {
            let line = buffer.byte_to_line(offset.start);
            let start = buffer.line_to_byte(line);
            let full = buffer.line(line).to_string();
            let within = offset.start - start..(offset.end - start).min(full.len());
            let (text, range) = cut_around(&full, within);
            let first = line.saturating_sub(CONTEXT_LINES);
            let last = (line + CONTEXT_LINES).min(buffer.line_count() - 1);
            let context = (first..=last)
                .map(|number| {
                    let text = buffer.line(number).to_string();
                    let (text, _) = cut_around(&text, 0..0);
                    (number, text)
                })
                .collect();
            LineMatch {
                offset,
                line,
                text,
                range,
                context,
            }
        })
        .collect()
}

/// `line` cut to [`MAX_LINE_BYTES`] so that `range` shows, and `range` in the cut text.
fn cut_around(line: &str, range: Range<usize>) -> (String, Range<usize>) {
    if line.len() <= MAX_LINE_BYTES {
        return (line.to_string(), range);
    }
    // Some text before the match, the rest after it.
    let mut start = range.start.saturating_sub(MAX_LINE_BYTES / 4);
    while !line.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + MAX_LINE_BYTES).min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let cut = line[start..end].to_string();
    let range = range.start - start..(range.end.min(end)).saturating_sub(start);
    (cut, range)
}

/// The text of the file at `path`, or `None` when it is not searched: missing, unreadable,
/// bigger than [`MAX_SEARCHED_FILE`], binary or not valid UTF-8.
pub fn read_text(path: &Path) -> Option<Buffer> {
    let size = fs::metadata(path).ok()?.len();
    if size > MAX_SEARCHED_FILE {
        return None;
    }
    match load_file(path).ok()?.content {
        FileContent::Text {
            buffer,
            lossy: false,
            ..
        } => Some(buffer),
        _ => None,
    }
}

/// Replaces in the file at `path` every match of `query` that starts in one of `only` (all of
/// them when `None`), with what [`SearchQuery::replacement`] gives for `with`. The file is read
/// again and saved atomically, keeping its BOM. Returns how many matches were replaced: none
/// when the file is no longer text.
pub fn replace_in_file(
    path: &Path,
    query: &SearchQuery,
    with: &str,
    preserve_case: bool,
    only: Option<&[usize]>,
) -> Result<usize, SaveError> {
    let Ok(loaded) = load_file(path) else {
        return Ok(0);
    };
    let FileContent::Text {
        mut buffer,
        lossy: false,
        bom,
    } = loaded.content
    else {
        return Ok(0);
    };
    let mut ranges = Vec::new();
    query.find_in(buffer.rope(), 0..buffer.len(), |found| {
        if only.is_none_or(|starts| starts.contains(&found.start)) {
            ranges.push(found);
        }
        true
    });
    // From the end, so the earlier offsets stay right.
    for range in ranges.iter().rev() {
        let matched = buffer.text_in(range.clone());
        let text = query.replacement(&matched, with, preserve_case);
        buffer.replace(range.clone(), &text);
    }
    if !ranges.is_empty() {
        save_file(path, buffer.rope(), bom)?;
    }
    Ok(ranges.len())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use ted_core::SearchOptions;

    fn query(text: &str) -> SearchQuery {
        SearchQuery::with_options(
            text,
            SearchOptions {
                case_sensitive: true,
                ..SearchOptions::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn masks_pick_names_and_paths() {
        let all = FileMask::new("").unwrap();
        assert!(all.matches("src/main.rs"));
        let rust = FileMask::new("*.rs, *.toml, !*.lock").unwrap();
        assert!(rust.matches("main.rs"));
        assert!(rust.matches("crates/a/src/lib.rs"));
        assert!(rust.matches("Cargo.toml"));
        assert!(!rust.matches("Cargo.lock"));
        assert!(!rust.matches("README.md"));
        let tests = FileMask::new("!tests/**").unwrap();
        assert!(tests.matches("src/lib.rs"));
        assert!(!tests.matches("tests/fixtures/a.txt"));
        assert!(FileMask::new("[a").is_err());
    }

    #[test]
    fn lines_and_context() {
        let buffer = Buffer::from_text("one\ntwo find\nthree\nfour\nfive find\n");
        let found = find_lines(&query("find"), &buffer, 100);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[0].text, "two find");
        assert_eq!(found[0].range, 4..8);
        assert_eq!(found[0].offset, 8..12);
        let numbers: Vec<usize> = found[0].context.iter().map(|(n, _)| *n).collect();
        assert_eq!(numbers, [0, 1, 2, 3]);
        let numbers: Vec<usize> = found[1].context.iter().map(|(n, _)| *n).collect();
        assert_eq!(numbers, [2, 3, 4, 5]);
        assert_eq!(find_lines(&query("find"), &buffer, 1).len(), 1);
    }

    #[test]
    fn a_long_line_is_cut_around_its_match() {
        let line = format!("{}needle{}", "a".repeat(1000), "b".repeat(1000));
        let buffer = Buffer::from_text(&line);
        let found = find_lines(&query("needle"), &buffer, 10);
        let found = &found[0];
        assert!(found.text.len() <= MAX_LINE_BYTES);
        assert_eq!(&found.text[found.range.clone()], "needle");
    }

    #[test]
    fn binary_and_big_files_are_not_read() {
        let tmp = tempfile::tempdir().unwrap();
        let text = tmp.path().join("a.txt");
        fs::write(&text, "hello\n").unwrap();
        let binary = tmp.path().join("a.bin");
        fs::write(&binary, b"\x00\x01\x02hello").unwrap();
        let latin = tmp.path().join("latin.txt");
        fs::write(&latin, b"caf\xe9\n").unwrap();
        assert!(read_text(&text).is_some());
        assert!(read_text(&binary).is_none());
        assert!(read_text(&latin).is_none());
        assert!(read_text(&tmp.path().join("missing")).is_none());
    }

    #[test]
    fn replacing_on_disk_keeps_the_rest_and_the_bom() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.txt");
        fs::write(&path, b"\xEF\xBB\xBFfind one\r\nFind two find\r\n").unwrap();
        let options = SearchOptions::default();
        let query = SearchQuery::with_options("find", options).unwrap();
        assert_eq!(
            replace_in_file(&path, &query, "seek", true, None).unwrap(),
            3
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            b"\xEF\xBB\xBFseek one\r\nSeek two seek\r\n"
        );
        // Only the match that starts at byte 5, the second one.
        fs::write(&path, "find find find\n").unwrap();
        assert_eq!(
            replace_in_file(&path, &query, "x", false, Some(&[5])).unwrap(),
            1
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "find x find\n");
    }
}

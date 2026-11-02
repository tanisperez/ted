//! Find and replace in files (`Ctrl+Shift+F`, `Ctrl+Shift+R`), outside the gpui views: the
//! query and its options, where to search, the results found so far, the selected row and the
//! summary of the header. The view searches in the background, file by file, and hands each batch
//! of files with matches to [`FindFiles::add`]; a new search bumps the generation so the batches
//! of an older one are dropped.
//!
//! At most [`MAX_MATCHES`] matches are kept; past them the search stops and the summary says so.

use std::ops::Range;
use std::path::PathBuf;

use rust_i18n::t;
use ted_core::{QueryError, SearchOptions, SearchQuery};
use ted_workspace::{FileMask, LineMatch};

use crate::state::locale::plural;

/// The most matches a search keeps.
pub const MAX_MATCHES: usize = 10_000;

/// Where the search looks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Scope {
    /// Every file of the open folder.
    #[default]
    Folder,
    /// Every file under a directory the user picked.
    Directory(PathBuf),
    /// The documents open in tabs.
    OpenFiles,
}

/// The matches of one file.
#[derive(Debug, Clone)]
pub struct FileResult {
    pub path: PathBuf,
    /// The path shown, relative to the folder (`/` separated), or the full one outside it.
    pub relative: String,
    pub matches: Vec<LineMatch>,
}

impl FileResult {
    /// The file name, the last part of [`Self::relative`].
    pub fn name(&self) -> &str {
        self.relative.rsplit('/').next().unwrap_or(&self.relative)
    }

    /// The folder of the file, relative (`crates/ted-app/src`), empty at the top.
    pub fn dir(&self) -> &str {
        self.relative.rsplit_once('/').map_or("", |(dir, _)| dir)
    }
}

/// The state of the Find in files window.
#[derive(Debug, Clone, Default)]
pub struct FindFiles {
    pub text: String,
    pub options: SearchOptions,
    pub preserve_case: bool,
    /// The file mask as typed (`*.rs, !*.lock`).
    pub mask: String,
    pub scope: Scope,
    /// The replace field shows (`Ctrl+Shift+R`).
    pub replace: bool,
    /// Pinned: the window stays open after a result is opened and when the focus leaves it.
    pub pinned: bool,
    /// Each "Open in a tab" opens a new results tab instead of reusing the last one.
    pub new_tab: bool,
    /// The compiled query of the last search.
    pub query: Option<SearchQuery>,
    /// Why the text or the mask is not valid.
    pub invalid: Option<String>,
    results: Vec<FileResult>,
    /// Each row of the list: (file, match) indices into `results`.
    rows: Vec<(u32, u32)>,
    pub selected: Option<usize>,
    /// Bumped by every new search, so the batches of an older one are dropped.
    pub generation: u64,
    pub searching: bool,
    /// The search stopped at [`MAX_MATCHES`].
    pub truncated: bool,
}

impl FindFiles {
    /// Starts a new search: clears the results and compiles the query and the mask. Returns
    /// the new generation and what to search with, or `None` when there is nothing to search.
    pub fn restart(&mut self) -> (u64, Option<(SearchQuery, FileMask)>) {
        self.generation += 1;
        self.results.clear();
        self.rows.clear();
        self.selected = None;
        self.truncated = false;
        self.invalid = None;
        self.query = None;
        let query = match SearchQuery::with_options(&self.text, self.options) {
            Ok(query) => query,
            Err(QueryError::Empty) => {
                self.searching = false;
                return (self.generation, None);
            }
            Err(QueryError::Invalid(error)) => {
                self.invalid = Some(error);
                self.searching = false;
                return (self.generation, None);
            }
        };
        let mask = match FileMask::new(&self.mask) {
            Ok(mask) => mask,
            Err(error) => {
                self.invalid = Some(error);
                self.searching = false;
                return (self.generation, None);
            }
        };
        self.query = Some(query.clone());
        self.searching = true;
        (self.generation, Some((query, mask)))
    }

    /// The files with matches of one batch of `generation`. Returns how many more matches may
    /// still be kept.
    pub fn add(&mut self, generation: u64, files: Vec<FileResult>) -> usize {
        if generation != self.generation {
            return 0;
        }
        for file in files {
            let room = MAX_MATCHES - self.match_count();
            if room == 0 {
                self.truncated = true;
                break;
            }
            let mut file = file;
            if file.matches.len() > room {
                file.matches.truncate(room);
                self.truncated = true;
            }
            let index = self.results.len() as u32;
            self.rows
                .extend((0..file.matches.len() as u32).map(|line| (index, line)));
            self.results.push(file);
        }
        if self.selected.is_none() && !self.rows.is_empty() {
            self.selected = Some(0);
        }
        MAX_MATCHES - self.match_count()
    }

    /// The search of `generation` went through every file, or stopped at the limit.
    pub fn finish(&mut self, generation: u64) {
        if generation == self.generation {
            self.searching = false;
        }
    }

    pub fn match_count(&self) -> usize {
        self.rows.len()
    }

    pub fn files(&self) -> &[FileResult] {
        &self.results
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// The file and the match of row `row`.
    pub fn row(&self, row: usize) -> Option<(&FileResult, &LineMatch)> {
        let (file, line) = *self.rows.get(row)?;
        let file = self.results.get(file as usize)?;
        Some((file, file.matches.get(line as usize)?))
    }

    /// The selected row's file and match.
    pub fn current(&self) -> Option<(&FileResult, &LineMatch)> {
        self.row(self.selected?)
    }

    /// Moves the selection `delta` rows, stopping at the ends.
    pub fn select_by(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        let current = self.selected.map_or(-1, |row| row as isize);
        self.selected = Some((current + delta).clamp(0, last) as usize);
    }

    /// The header's summary: `9 matches in 5 files`, `10,000+ matches in 40 files`, `Searching…`,
    /// `No results`, the error of an invalid pattern, or nothing before a search.
    pub fn summary(&self) -> String {
        if self.invalid.is_some() {
            return t!("find.invalid").into_owned();
        }
        if self.query.is_none() {
            return String::new();
        }
        let matches = self.match_count();
        if matches == 0 {
            return match self.searching {
                true => t!("find_files.searching").into_owned(),
                false => t!("find.no_results").into_owned(),
            };
        }
        let count = telaio_app::number::format_integer(matches as u64);
        let matches_label = match self.truncated {
            true => t!("find_files.matches_more", count = count).into_owned(),
            false => plural("find_files.matches", matches),
        };
        let files = plural("find_files.files", self.results.len());
        let summary = t!("find_files.summary", matches = matches_label, files = files).into_owned();
        match self.searching {
            true => format!("{summary}\u{2026}"),
            false => summary,
        }
    }
}

/// `text` split for a row: before the match, the match and after it, with the indentation of
/// the line left out.
pub fn row_parts(text: &str, range: &Range<usize>) -> (String, String, String) {
    let trimmed = text.len() - text.trim_start().len();
    let start = range.start.max(trimmed).min(text.len());
    let end = range.end.clamp(start, text.len());
    (
        text[trimmed.min(start)..start].to_string(),
        text[start..end].to_string(),
        text[end..].to_string(),
    )
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn line(line: usize) -> LineMatch {
        LineMatch {
            offset: line * 10..line * 10 + 4,
            line,
            text: "    find it".to_string(),
            range: 4..8,
            context: Vec::new(),
        }
    }

    fn file(name: &str, lines: usize) -> FileResult {
        FileResult {
            path: PathBuf::from(format!("/root/{name}")),
            relative: name.to_string(),
            matches: (0..lines).map(line).collect(),
        }
    }

    fn searching(text: &str) -> (FindFiles, u64) {
        let mut find = FindFiles {
            text: text.to_string(),
            ..FindFiles::default()
        };
        let (generation, query) = find.restart();
        assert!(query.is_some());
        (find, generation)
    }

    #[test]
    fn rows_follow_the_files_and_the_summary_counts_them() {
        let (mut find, generation) = searching("find");
        assert_eq!(find.summary(), "Searching\u{2026}");
        find.add(generation, vec![file("src/a.rs", 2), file("b.rs", 1)]);
        assert_eq!(find.summary(), "3 matches in 2 files\u{2026}");
        find.finish(generation);
        assert_eq!(find.summary(), "3 matches in 2 files");
        assert_eq!(find.row_count(), 3);
        let (first, matched) = find.current().expect("the first row is selected");
        assert_eq!(
            (first.name(), first.dir(), matched.line),
            ("a.rs", "src", 0)
        );
        find.select_by(5);
        assert_eq!(find.current().map(|(file, _)| file.name()), Some("b.rs"));
        find.select_by(-1);
        assert_eq!(find.current().map(|(_, matched)| matched.line), Some(1));
    }

    #[test]
    fn a_stale_batch_is_dropped() {
        let (mut find, old) = searching("find");
        let (new, _) = find.restart();
        find.add(old, vec![file("a.rs", 2)]);
        assert_eq!(find.row_count(), 0);
        find.add(new, vec![file("a.rs", 1)]);
        find.finish(new);
        assert_eq!(find.summary(), "1 match in 1 file");
    }

    #[test]
    fn the_limit_stops_the_search() {
        let (mut find, generation) = searching("find");
        let room = find.add(generation, vec![file("a.rs", MAX_MATCHES - 1)]);
        assert_eq!(room, 1);
        let room = find.add(generation, vec![file("b.rs", 3), file("c.rs", 1)]);
        find.finish(generation);
        assert_eq!(room, 0);
        assert!(find.truncated);
        assert_eq!(find.files().len(), 2);
        assert_eq!(find.summary(), "10,000+ matches in 2 files");
    }

    #[test]
    fn nothing_to_search_and_invalid_input() {
        let mut find = FindFiles::default();
        assert!(find.restart().1.is_none());
        assert_eq!(find.summary(), "");
        find.text = "(".to_string();
        find.options.regex = true;
        assert!(find.restart().1.is_none());
        assert_eq!(find.summary(), "Invalid pattern");
        find.options.regex = false;
        find.mask = "[a".to_string();
        assert!(find.restart().1.is_none());
        assert_eq!(find.summary(), "Invalid pattern");
    }

    #[test]
    fn row_parts_leave_the_indentation_out() {
        assert_eq!(
            row_parts("    find it", &(4..8)),
            ("".to_string(), "find".to_string(), " it".to_string())
        );
        assert_eq!(
            row_parts("a find", &(2..6)),
            ("a ".to_string(), "find".to_string(), String::new())
        );
    }
}

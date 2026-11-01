//! Plain data behind the global search (`Ctrl+K`, the title bar's search box): the actions it
//! lists, and the fuzzy matching that filters and ranks the folder's files and the actions as
//! the user types. `views/search.rs` renders it; this module only holds the data and the
//! matching, so both are unit tested without a window.
//!
//! Ranking the files of a big folder is the expensive part: [`rank_paths`] makes no allocation
//! per path (two reused buffers) and keeps only the best [`MAX_FILE_RESULTS`], and the view runs
//! it on the background executor.

use rust_i18n::t;

/// How many files the search shows at most: the best ones. Rendering more is wasted work, no one
/// scrolls through thousands of results.
pub const MAX_FILE_RESULTS: usize = 50;

/// An app action reachable from the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionId {
    NewFile,
    OpenFile,
    OpenFolder,
    Save,
    CloseTab,
    OpenSettings,
    OpenShortcuts,
    ToggleExplorer,
    ToggleGit,
    Find,
    Replace,
    GoToLine,
    ToggleWrap,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    CopyPath,
    Reveal,
}

/// What is open, which decides the actions listed: one that would do nothing is left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Context {
    /// A folder is open (folder mode): the panel toggles.
    pub folder: bool,
    /// A document is shown: Save and Close.
    pub document: bool,
    /// The document shown is in an editor: Find, Go to line and Wrap.
    pub editor: bool,
}

impl ActionId {
    /// The actions available in `context`, in the order the search lists them.
    pub fn available(context: Context) -> Vec<ActionId> {
        use ActionId::*;
        let mut actions = vec![NewFile, OpenFile, OpenFolder];
        if context.document {
            actions.extend([Save, CloseTab]);
        }
        if context.editor {
            actions.extend([Find, Replace, GoToLine, ToggleWrap]);
        }
        if context.folder {
            actions.extend([ToggleExplorer, ToggleGit]);
        }
        if context.folder || context.document {
            actions.extend([CopyPath, Reveal]);
        }
        actions.extend([ZoomIn, ZoomOut, ZoomReset, OpenSettings, OpenShortcuts]);
        actions
    }

    /// The translated label.
    pub fn label(self) -> String {
        use ActionId::*;
        let key = match self {
            NewFile => "search.action.new_file",
            OpenFile => "search.action.open_file",
            OpenFolder => "search.action.open_folder",
            Save => "search.action.save",
            CloseTab => "search.action.close_tab",
            OpenSettings => "search.action.settings",
            OpenShortcuts => "search.action.shortcuts",
            ToggleExplorer => "search.action.toggle_explorer",
            ToggleGit => "search.action.toggle_git",
            Find => "search.action.find",
            Replace => "search.action.replace",
            GoToLine => "search.action.goto",
            ToggleWrap => "search.action.toggle_wrap",
            ZoomIn => "search.action.zoom_in",
            ZoomOut => "search.action.zoom_out",
            ZoomReset => "search.action.zoom_reset",
            CopyPath => "search.action.copy_path",
            Reveal => "search.action.reveal",
        };
        t!(key).into_owned()
    }
}

/// One match: the index of what matched, its score (higher is better) and the char indices of
/// the matched characters, for the view to highlight them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub index: usize,
    pub score: i32,
    pub positions: Vec<usize>,
}

/// Points for every matched character.
const CHAR_SCORE: i32 = 1;
/// Bonus for a matched character that starts a word: the first one, one after a separator
/// (`/`, `_`, `-`, `.`, a space) or a capital after a small letter (`camelCase`).
const WORD_START_BONUS: i32 = 8;
/// Bonus for a matched character right after the previous one.
const CONSECUTIVE_BONUS: i32 = 6;
/// Penalty per character skipped between two matched ones, up to [`MAX_GAP_PENALTY`].
const GAP_PENALTY: i32 = 1;
const MAX_GAP_PENALTY: i32 = 5;
/// Bonus when the whole query matches in the file name: `main` finds `src/main.rs` before
/// `src/maintenance/lib.rs`.
const FILE_NAME_BONUS: i32 = 20;

/// The query as matched: lowercase, with the spaces dropped (`root rs` finds `root.rs`).
pub fn normalize_query(query: &str) -> Vec<char> {
    query
        .chars()
        .filter(|character| !character.is_whitespace())
        .map(lower_char)
        .collect()
}

/// The lowercase of `character`, one char for one char so matched indices stay valid. Unlike
/// `to_ascii_lowercase` it also folds accented capitals (`Ó`), which the translated labels have.
fn lower_char(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

fn is_separator(character: char) -> bool {
    matches!(character, '/' | '\\' | '_' | '-' | '.' | ' ')
}

/// Reused buffers, so ranking many paths allocates nothing per path.
#[derive(Default)]
struct Scratch {
    chars: Vec<char>,
    lower: Vec<char>,
    positions: Vec<usize>,
}

impl Scratch {
    fn load(&mut self, text: &str) {
        self.chars.clear();
        self.lower.clear();
        self.chars.extend(text.chars());
        self.lower
            .extend(self.chars.iter().copied().map(lower_char));
    }

    /// Matches `query` as a subsequence of the loaded text from char `from`, greedily picking the
    /// earliest occurrence of each character. Leaves the positions in `self.positions` and
    /// returns the score, or `None` when some character is missing.
    fn match_from(&mut self, query: &[char], from: usize) -> Option<i32> {
        self.positions.clear();
        let mut next = from;
        for &wanted in query {
            let found = self.lower[next..]
                .iter()
                .position(|&character| character == wanted)?
                + next;
            self.positions.push(found);
            next = found + 1;
        }
        Some(self.score())
    }

    fn score(&self) -> i32 {
        let mut score = 0;
        let mut previous: Option<usize> = None;
        for &position in &self.positions {
            score += CHAR_SCORE;
            let word_start = position == 0 || {
                let before = self.chars[position - 1];
                let here = self.chars[position];
                is_separator(before) || (before.is_lowercase() && here.is_uppercase())
            };
            if word_start {
                score += WORD_START_BONUS;
            }
            if let Some(previous) = previous {
                let gap = (position - previous - 1) as i32;
                if gap == 0 {
                    score += CONSECUTIVE_BONUS;
                } else {
                    score -= (gap * GAP_PENALTY).min(MAX_GAP_PENALTY);
                }
            }
            previous = Some(position);
        }
        score
    }

    /// Matches a path: in its file name first (with [`FILE_NAME_BONUS`]), else in the whole path.
    fn match_path(&mut self, query: &[char]) -> Option<i32> {
        let name_start = self
            .chars
            .iter()
            .rposition(|&character| character == '/')
            .map_or(0, |slash| slash + 1);
        if name_start > 0
            && !query.contains(&'/')
            && let Some(score) = self.match_from(query, name_start)
        {
            return Some(score + FILE_NAME_BONUS);
        }
        let score = self.match_from(query, 0)?;
        // A path with no folder is all file name.
        Some(match name_start {
            0 => score + FILE_NAME_BONUS,
            _ => score,
        })
    }
}

/// Ranks `paths` (relative, `/` separated) against `query`: the best [`MAX_FILE_RESULTS`] (or
/// `limit`) matches, best first. Ties go to the shorter path, then to the earlier one. An empty
/// query matches nothing: the view shows the recent files instead.
pub fn rank_paths<'a>(
    query: &str,
    paths: impl IntoIterator<Item = &'a str>,
    limit: usize,
) -> Vec<Match> {
    let query = normalize_query(query);
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut scratch = Scratch::default();
    // (score, length, index) of every match; the positions are found again for the kept ones.
    let mut found: Vec<(i32, usize, usize)> = Vec::new();
    let mut kept: Vec<&'a str> = Vec::new();
    for (index, path) in paths.into_iter().enumerate() {
        // A cheap first test: the path has to be at least as long as the query.
        if path.len() < query.len() {
            continue;
        }
        scratch.load(path);
        if let Some(score) = scratch.match_path(&query) {
            found.push((score, path.len(), index));
            kept.push(path);
        }
    }
    let order = |a: &(i32, usize, usize), b: &(i32, usize, usize)| {
        b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
    };
    let mut slots: Vec<usize> = (0..found.len()).collect();
    if slots.len() > limit {
        slots.select_nth_unstable_by(limit - 1, |&a, &b| order(&found[a], &found[b]));
        slots.truncate(limit);
    }
    slots.sort_unstable_by(|&a, &b| order(&found[a], &found[b]));
    slots
        .into_iter()
        .map(|slot| {
            let (score, _, index) = found[slot];
            scratch.load(kept[slot]);
            scratch.match_path(&query);
            Match {
                index,
                score,
                positions: std::mem::take(&mut scratch.positions),
            }
        })
        .collect()
}

/// Ranks `labels` (the actions) against `query`: every match, best first, ties in their order.
/// An empty query matches every label, in order. The scores are on the scale of [`rank_paths`]
/// (a label counts as a file name), so the view can put first the group with the best match.
pub fn rank_labels(query: &str, labels: &[String]) -> Vec<Match> {
    let query = normalize_query(query);
    let mut scratch = Scratch::default();
    let mut matches: Vec<Match> = labels
        .iter()
        .enumerate()
        .filter_map(|(index, label)| {
            scratch.load(label);
            let score = match query.is_empty() {
                true => scratch.match_from(&query, 0)?,
                false => scratch.match_path(&query)?,
            };
            Some(Match {
                index,
                score,
                positions: scratch.positions.clone(),
            })
        })
        .collect();
    matches.sort_by_key(|found| std::cmp::Reverse(found.score));
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn ranked<'a>(query: &str, paths: &[&'a str]) -> Vec<&'a str> {
        rank_paths(query, paths.iter().copied(), MAX_FILE_RESULTS)
            .into_iter()
            .map(|found| paths[found.index])
            .collect()
    }

    #[test]
    fn an_empty_query_ranks_no_file() {
        assert!(ranked("", &["a.rs"]).is_empty());
        assert!(ranked("   ", &["a.rs"]).is_empty());
    }

    #[test]
    fn matches_a_subsequence_ignoring_case_and_spaces() {
        let paths = ["src/Views/Root.rs", "README.md"];
        assert_eq!(ranked("root", &paths), vec!["src/Views/Root.rs"]);
        assert_eq!(ranked("v root", &paths), vec!["src/Views/Root.rs"]);
        assert!(ranked("xyz", &paths).is_empty());
    }

    #[test]
    fn a_match_in_the_file_name_ranks_first() {
        let paths = ["src/main/lib.rs", "docs/main.md", "src/domain.rs"];
        assert_eq!(
            ranked("main", &paths),
            vec!["docs/main.md", "src/domain.rs", "src/main/lib.rs"]
        );
    }

    #[test]
    fn word_starts_beat_scattered_letters() {
        let paths = ["src/state/readme_tool.rs", "src/status_bar.rs"];
        assert_eq!(ranked("sb", &paths)[0], "src/status_bar.rs");
    }

    #[test]
    fn a_slash_in_the_query_matches_the_folders() {
        let paths = ["views/root.rs", "state/root.rs"];
        assert_eq!(ranked("state/root", &paths), vec!["state/root.rs"]);
    }

    #[test]
    fn ties_go_to_the_shorter_path() {
        let paths = ["a/bb/x.rs", "a/x.rs"];
        assert_eq!(ranked("x.rs", &paths), vec!["a/x.rs", "a/bb/x.rs"]);
    }

    #[test]
    fn keeps_only_the_best_matches() {
        let paths: Vec<String> = (0..200)
            .map(|number| format!("dir/file{number}.txt"))
            .collect();
        let matches = rank_paths("file1", paths.iter().map(String::as_str), 10);
        assert_eq!(matches.len(), 10);
        // The shortest, `file1.txt`, wins the tie.
        assert_eq!(paths[matches[0].index], "dir/file1.txt");
        assert!(
            matches
                .windows(2)
                .all(|pair| pair[0].score >= pair[1].score)
        );
    }

    #[test]
    fn reports_the_char_positions_of_the_match() {
        let matches = rank_paths("mr", ["src/main.rs"], 5);
        // `m` of main, then the `r` of `rs`.
        assert_eq!(matches[0].positions, vec![4, 9]);
        let accented = rank_paths("ó", ["dir/Ó.txt"], 5);
        assert_eq!(accented[0].positions, vec![4]);
    }

    #[test]
    fn labels_match_with_every_label_for_an_empty_query() {
        let labels = vec!["New file".to_string(), "Open folder".to_string()];
        let all: Vec<usize> = rank_labels("", &labels).iter().map(|m| m.index).collect();
        assert_eq!(all, vec![0, 1]);
        let folder: Vec<usize> = rank_labels("fold", &labels)
            .iter()
            .map(|m| m.index)
            .collect();
        assert_eq!(folder, vec![1]);
    }

    #[test]
    fn a_label_and_a_path_compare_on_one_scale() {
        let labels = vec!["Go to line".to_string()];
        let action = rank_labels("go line", &labels)[0].score;
        let file = rank_paths("go line", ["site/docs-src/pages/command-line.html"], 5)[0].score;
        assert!(action > file, "{action} <= {file}");
    }

    #[test]
    fn actions_follow_what_is_open() {
        let empty = ActionId::available(Context::default());
        assert!(empty.contains(&ActionId::OpenFolder));
        assert!(!empty.contains(&ActionId::Save));
        assert!(!empty.contains(&ActionId::ToggleGit));
        assert!(!empty.contains(&ActionId::CopyPath));
        let all = ActionId::available(Context {
            folder: true,
            document: true,
            editor: true,
        });
        assert_eq!(all.len(), 18);
    }

    #[test]
    fn every_action_has_a_label_in_both_languages() {
        let all = ActionId::available(Context {
            folder: true,
            document: true,
            editor: true,
        });
        for action in all {
            let label = action.label();
            assert!(!label.starts_with("search."), "{label}");
        }
        assert_eq!(t!("search.action.new_file", locale = "es"), "Nuevo archivo");
    }
}

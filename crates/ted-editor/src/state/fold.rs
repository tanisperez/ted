//! Folded blocks of text: which lines are hidden, how line numbers map to the rows on screen
//! without them, and where a block starts and ends.
//!
//! A block is found by indentation, reading only its own lines: a line starts a block when the
//! next line with text is indented deeper. A last line that only closes the block (`}`, `</div>`,
//! `end`) goes in it. Markdown also folds its heading sections and fenced code blocks, from a
//! table built on the background executor ([`markdown_blocks`]).
//!
//! With nothing folded, every mapping is the identity at no cost. The folds are kept up to date
//! from the buffer's line edits: an edit above a fold moves it, one that touches its hidden lines
//! unfolds it.

use std::ops::Range;

use ted_core::{Buffer, LineEdit};

use super::editor::tab_size;

/// How many blank lines are skipped looking for the next line with text, per visible line.
const MAX_BLANK_SCAN: usize = 1_000;
/// How far up the block around the cursor is searched for `Ctrl+Shift+[`, in lines.
pub const MAX_ENCLOSING_SCAN: usize = 2_000;
/// The longest closing line shown in a fold's marker, in bytes.
pub const MAX_CLOSER_LEN: usize = 24;

/// A folded block: `header` stays on screen, the lines after it up to `last` are hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fold {
    pub header: usize,
    pub last: usize,
    /// Whether `last` only closes the block, so the marker shows it (`⋯ }`).
    pub closes: bool,
}

/// A block of text, folded or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub last: usize,
    pub closes: bool,
}

/// The folds of a document, sorted and not overlapping.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FoldMap {
    folds: Vec<Fold>,
}

impl FoldMap {
    pub fn is_empty(&self) -> bool {
        self.folds.is_empty()
    }

    /// How many lines are hidden.
    pub fn hidden_lines(&self) -> usize {
        self.folds.iter().map(|fold| fold.last - fold.header).sum()
    }

    /// The fold whose header is `line`.
    pub fn fold_at(&self, line: usize) -> Option<Fold> {
        let index = self.folds.binary_search_by_key(&line, |f| f.header).ok()?;
        self.folds.get(index).copied()
    }

    /// The fold that hides `line`.
    fn hiding(&self, line: usize) -> Option<usize> {
        let index = self.folds.partition_point(|fold| fold.header < line);
        let fold = self.folds.get(index.checked_sub(1)?)?;
        (line <= fold.last).then_some(index - 1)
    }

    /// Whether every line of `lines` is hidden.
    pub fn hides_all(&self, lines: Range<usize>) -> bool {
        !lines.is_empty()
            && self
                .hiding(lines.start)
                .is_some_and(|index| self.folds[index].last + 1 >= lines.end)
    }

    /// Folds `block` under `header`. A fold inside it is absorbed: unfolding shows everything.
    pub fn fold(&mut self, header: usize, block: Block) {
        self.folds
            .retain(|fold| fold.header < header || fold.header > block.last);
        let index = self.folds.partition_point(|fold| fold.header < header);
        self.folds.insert(
            index,
            Fold {
                header,
                last: block.last,
                closes: block.closes,
            },
        );
    }

    /// Removes the fold whose header is `line`. Returns whether there was one.
    pub fn unfold(&mut self, line: usize) -> bool {
        match self.folds.binary_search_by_key(&line, |fold| fold.header) {
            Ok(index) => {
                self.folds.remove(index);
                true
            }
            Err(_) => false,
        }
    }

    /// Removes the fold that hides `line`. Returns whether there was one.
    pub fn reveal(&mut self, line: usize) -> bool {
        match self.hiding(line) {
            Some(index) => {
                self.folds.remove(index);
                true
            }
            None => false,
        }
    }

    /// Where `line` is among the lines on screen. A hidden line counts as its fold's header.
    pub fn display_line(&self, line: usize) -> usize {
        let mut hidden = 0;
        for fold in &self.folds {
            if fold.header >= line {
                break;
            }
            hidden += fold.last.min(line) - fold.header;
        }
        line - hidden
    }

    /// The line at place `display` among the lines on screen.
    pub fn buffer_line(&self, display: usize) -> usize {
        let mut line = display;
        for fold in &self.folds {
            if fold.header >= line {
                break;
            }
            line += fold.last - fold.header;
        }
        line
    }

    /// The first line on screen below `line`, which may be past the end of the document.
    pub fn next_visible(&self, line: usize) -> usize {
        match self.fold_at(line) {
            Some(fold) => fold.last + 1,
            None => match self.hiding(line + 1) {
                Some(index) => self.folds[index].last + 1,
                None => line + 1,
            },
        }
    }

    /// The first line on screen above `line`, which must not be the first of the document.
    pub fn previous_visible(&self, line: usize) -> usize {
        match self.hiding(line - 1) {
            Some(index) => self.folds[index].header,
            None => line - 1,
        }
    }

    /// The runs of consecutive lines behind the lines on screen `display`: where each run starts
    /// on screen and its lines.
    pub fn runs(&self, display: Range<usize>) -> Vec<(usize, Range<usize>)> {
        let mut runs: Vec<(usize, Range<usize>)> = Vec::new();
        if self.folds.is_empty() {
            if !display.is_empty() {
                runs.push((display.start, display));
            }
            return runs;
        }
        for place in display {
            let line = self.buffer_line(place);
            match runs.last_mut() {
                Some((_, lines)) if lines.end == line => lines.end = line + 1,
                _ => runs.push((place, line..line + 1)),
            }
        }
        runs
    }

    /// Follows one edit of the text.
    pub fn apply(&mut self, edit: LineEdit) {
        let delta = edit.new_end as isize - edit.old_end as isize;
        self.folds.retain_mut(|fold| {
            if edit.start > fold.last {
                return true;
            }
            // Inside the header's line: the fold stays unless the edit breaks that line.
            if edit.start == fold.header && edit.old_end == fold.header {
                return edit.new_end == fold.header;
            }
            if edit.old_end > fold.header {
                return false;
            }
            fold.header = fold.header.saturating_add_signed(delta);
            fold.last = fold.last.saturating_add_signed(delta);
            true
        });
    }
}

/// The indentation of a line in columns, `None` for a blank one.
fn indent_of(line: impl Iterator<Item = char>) -> Option<usize> {
    let mut column = 0;
    for ch in line {
        match ch {
            ' ' => column += 1,
            '\t' => column += tab_size() - column % tab_size(),
            '\r' | '\n' => return None,
            _ => return Some(column),
        }
    }
    None
}

fn indent_at(buffer: &Buffer, line: usize) -> Option<usize> {
    indent_of(buffer.line(line).chars())
}

/// The next line with text after `line`, and its indentation.
fn next_with_text(buffer: &Buffer, line: usize) -> Option<(usize, usize)> {
    let end = (line + 1 + MAX_BLANK_SCAN).min(buffer.line_count());
    (line + 1..end).find_map(|next| indent_at(buffer, next).map(|indent| (next, indent)))
}

/// Whether a block starts at `line`, by its indentation. Reads only the next lines with text.
pub fn starts_indented_block(buffer: &Buffer, line: usize) -> bool {
    let Some(indent) = indent_at(buffer, line) else {
        return false;
    };
    next_with_text(buffer, line).is_some_and(|(_, next)| next > indent)
}

/// The block by indentation that starts at `line`.
pub fn indented_block(buffer: &Buffer, line: usize) -> Option<Block> {
    let indent = indent_at(buffer, line)?;
    let (first, next) = next_with_text(buffer, line)?;
    if next <= indent {
        return None;
    }
    let mut last = first;
    for current in first + 1..buffer.line_count() {
        match indent_at(buffer, current) {
            None => {}
            Some(depth) if depth > indent => last = current,
            Some(depth) => {
                if depth == indent && only_closes(&buffer.line(current).to_string()) {
                    return Some(Block {
                        last: current,
                        closes: true,
                    });
                }
                break;
            }
        }
    }
    Some(Block {
        last,
        closes: false,
    })
}

/// Whether a line does nothing but close a block: `}`, `});`, `</div>`, `end`.
pub fn only_closes(line: &str) -> bool {
    let text = line.trim();
    if text.is_empty() {
        return false;
    }
    if text == "end" {
        return true;
    }
    if let Some(tag) = text.strip_prefix("</") {
        return tag.ends_with('>') && !tag[..tag.len() - 1].contains(['<', '>']);
    }
    let closers = text.trim_end_matches([';', ',']);
    !closers.is_empty() && closers.chars().all(|ch| matches!(ch, '}' | ')' | ']'))
}

/// The heading sections and fenced code blocks of a Markdown text, as `(first line, last line)`,
/// sorted by first line. Linear in the text: meant for the background executor.
pub fn markdown_blocks(buffer: &Buffer) -> Vec<(usize, usize)> {
    let mut blocks = Vec::new();
    // The open sections, as (level, first line), and the last line with text so far.
    let mut sections: Vec<(usize, usize)> = Vec::new();
    let mut fence: Option<(usize, char)> = None;
    let mut last_text = 0;
    let mut close = |sections: &mut Vec<(usize, usize)>, level: usize, last: usize| {
        while sections.last().is_some_and(|(open, _)| *open >= level) {
            if let Some((_, first)) = sections.pop()
                && last > first
            {
                blocks.push((first, last));
            }
        }
    };
    let mut fences = Vec::new();
    for line in 0..buffer.line_count() {
        let text = buffer.line(line).to_string();
        let trimmed = text.trim();
        if let Some((first, mark)) = fence {
            if trimmed.len() >= 3 && trimmed.chars().all(|ch| ch == mark) {
                fence = None;
                if line > first {
                    fences.push((first, line));
                }
            }
            last_text = line;
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = trimmed.chars().next().map(|mark| (line, mark));
            last_text = line;
            continue;
        }
        if let Some(level) = heading_level(&text) {
            close(&mut sections, level, last_text);
            sections.push((level, line));
        }
        if !trimmed.is_empty() {
            last_text = line;
        }
    }
    close(&mut sections, 0, last_text);
    blocks.append(&mut fences);
    blocks.sort_unstable();
    blocks
}

/// The level of a `#` heading.
fn heading_level(line: &str) -> Option<usize> {
    let level = line.bytes().take_while(|&byte| byte == b'#').count();
    let spaced = line[level..].starts_with([' ', '\t']);
    ((1..=6).contains(&level) && spaced).then_some(level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn block(last: usize) -> Block {
        Block {
            last,
            closes: false,
        }
    }

    fn map(folds: &[(usize, usize)]) -> FoldMap {
        let mut map = FoldMap::default();
        for &(header, last) in folds {
            map.fold(header, block(last));
        }
        map
    }

    #[test]
    fn nothing_folded_is_the_identity() {
        let map = FoldMap::default();
        assert_eq!(map.display_line(7), 7);
        assert_eq!(map.buffer_line(7), 7);
        assert_eq!(map.next_visible(7), 8);
        assert_eq!(map.previous_visible(7), 6);
        assert_eq!(map.runs(2..5), [(2, 2..5)]);
    }

    #[test]
    fn lines_map_to_the_screen_and_back() {
        // Lines 3..=5 and 8..=9 are hidden.
        let map = map(&[(2, 5), (7, 9)]);
        assert_eq!(map.hidden_lines(), 5);
        let visible = [0, 1, 2, 6, 7, 10, 11];
        for (display, line) in visible.into_iter().enumerate() {
            assert_eq!(map.buffer_line(display), line);
            assert_eq!(map.display_line(line), display);
            assert!(!map.hides_all(line..line + 1));
        }
        assert!(map.hides_all(3..6) && map.hides_all(9..10));
        assert_eq!(map.display_line(4), 2, "a hidden line counts as its header");
    }

    #[test]
    fn walking_skips_the_hidden_lines() {
        let map = map(&[(2, 5), (6, 7)]);
        assert_eq!(map.next_visible(1), 2);
        assert_eq!(map.next_visible(2), 6);
        assert_eq!(map.next_visible(6), 8);
        assert_eq!(map.previous_visible(8), 6);
        assert_eq!(map.previous_visible(6), 2);
        assert_eq!(map.runs(0..6), [(0, 0..3), (3, 6..7), (4, 8..10)]);
    }

    #[test]
    fn folding_around_a_fold_absorbs_it() {
        let mut map = map(&[(3, 4), (8, 9)]);
        map.fold(1, block(5));
        assert_eq!(map.fold_at(3), None);
        assert_eq!(map.hidden_lines(), 5);
        assert!(map.unfold(1));
        assert_eq!(map.hidden_lines(), 1);
        assert!(map.reveal(9));
        assert!(map.is_empty());
    }

    #[test]
    fn hides_all_needs_every_line_in_one_fold() {
        let map = map(&[(2, 5)]);
        assert!(map.hides_all(3..6));
        assert!(!map.hides_all(2..4));
        assert!(!map.hides_all(4..7));
    }

    fn edit(start: usize, old_end: usize, new_end: usize) -> LineEdit {
        LineEdit {
            start,
            old_end,
            new_end,
        }
    }

    #[test]
    fn edits_above_move_a_fold_and_inside_unfold_it() {
        let moved = |change| {
            let mut map = map(&[(5, 8)]);
            map.apply(change);
            map.folds.first().map(|fold| (fold.header, fold.last))
        };
        assert_eq!(moved(edit(1, 1, 3)), Some((7, 10)), "two lines added above");
        assert_eq!(
            moved(edit(1, 3, 1)),
            Some((3, 6)),
            "two lines removed above"
        );
        assert_eq!(moved(edit(5, 5, 5)), Some((5, 8)), "typing in the header");
        assert_eq!(moved(edit(9, 9, 12)), Some((5, 8)), "below it");
        assert_eq!(moved(edit(5, 5, 6)), None, "a line break in the header");
        assert_eq!(moved(edit(7, 7, 7)), None, "inside it");
        assert_eq!(moved(edit(4, 6, 4)), None, "across its start");
        assert_eq!(
            moved(edit(4, 5, 4)),
            Some((4, 7)),
            "joined to the line above"
        );
    }

    #[test]
    fn blocks_follow_the_indentation() {
        let text = "fn a() {\n    one\n\n    two\n}\nlet b;\nif c:\n  d\n\n\ne\n\t\n";
        let buffer = Buffer::from_text(text);
        assert!(starts_indented_block(&buffer, 0));
        assert!(!starts_indented_block(&buffer, 1));
        assert!(!starts_indented_block(&buffer, 2));
        assert_eq!(
            indented_block(&buffer, 0),
            Some(Block {
                last: 4,
                closes: true
            })
        );
        assert_eq!(indented_block(&buffer, 1), None);
        assert_eq!(
            indented_block(&buffer, 6),
            Some(block(7)),
            "no trailing blanks"
        );
    }

    #[test]
    fn a_line_that_opens_again_is_not_a_closer() {
        let buffer = Buffer::from_text("if a {\n  b\n} else {\n  c\n}\n");
        assert_eq!(indented_block(&buffer, 0), Some(block(1)));
        for closer in ["}", "});", "]", "),", "end", "</div>"] {
            assert!(only_closes(closer), "{closer}");
        }
        for other in ["} else {", "", "<div>", "ending", "</a><b>"] {
            assert!(!only_closes(other), "{other}");
        }
    }

    #[test]
    fn markdown_folds_sections_and_fences() {
        let text =
            "# One\n\ntext\n\n## Two\n\n```sh\n# not a heading\n```\n\n## Three\n\n# Four\nend\n";
        let blocks = markdown_blocks(&Buffer::from_text(text));
        assert_eq!(blocks, [(0, 10), (4, 8), (6, 8), (12, 13)]);
    }
}

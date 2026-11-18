//! Plain helpers of the diff tab, tested without a window: the syntax colors of every line of a
//! diff, worked out once in the background from the two sides of the file, and the styled pieces
//! of one line, where those colors meet the emphasis of the changed words.

use std::ops::Range;

use telaio_git::{FileDiff, LineKind};
use telaio_syntax::{Highlight, Language, SyntaxTree};
use telaio_text::Buffer;

/// Columns a tab is drawn with in the diff.
pub const TAB_WIDTH: usize = 4;

/// The syntax spans of one line: byte ranges of its text and their class.
pub type LineSpans = Vec<(Range<usize>, Highlight)>;

/// The syntax spans of every line of `diff`, byte ranges of each line's text. Each side of the
/// file (the old one: context and removed lines; the new one: context and added lines) is parsed
/// whole, so a line inside a block comment or a string is colored as in the editor. Empty when
/// the language has no grammar or a side is over `max_bytes`. Blocking: run it in the background.
pub fn diff_syntax(diff: &FileDiff, language: Language, max_bytes: usize) -> Vec<LineSpans> {
    if !language.has_grammar() || diff.lines.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![Vec::new(); diff.lines.len()];
    for skipped in [LineKind::Added, LineKind::Removed] {
        let side: Vec<usize> = (0..diff.lines.len())
            .filter(|&index| diff.lines[index].kind != skipped)
            .collect();
        let bytes: usize = side.iter().map(|&i| diff.lines[i].text.len() + 1).sum();
        if bytes > max_bytes {
            return Vec::new();
        }
        let mut text = String::with_capacity(bytes);
        let mut starts = Vec::with_capacity(side.len());
        for &index in &side {
            starts.push(text.len());
            text.push_str(&diff.lines[index].text);
            text.push('\n');
        }
        let buffer = Buffer::from_text(&text);
        let Some(tree) = SyntaxTree::parse(language, &buffer, None, max_bytes) else {
            return Vec::new();
        };
        for span in tree.highlights(&buffer, 0..text.len()) {
            // The side lines the span covers, each with its part of the span.
            let first = starts.partition_point(|&start| start <= span.range.start) - 1;
            for (position, &index) in side.iter().enumerate().skip(first) {
                let start = starts[position];
                if start >= span.range.end {
                    break;
                }
                let len = diff.lines[index].text.len();
                let from = span.range.start.max(start) - start;
                let to = (span.range.end - start).min(len);
                // Context lines get the new side's colors only, so they are not added twice.
                let context_again =
                    skipped == LineKind::Added && diff.lines[index].kind == LineKind::Context;
                if from < to && !context_again {
                    lines[index].push((from..to, span.highlight));
                }
            }
        }
    }
    lines
}

/// A piece of a line as drawn: its byte range in the text with tabs expanded, its syntax class
/// (`None` for plain text) and whether it is one of the changed words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub range: Range<usize>,
    pub highlight: Option<Highlight>,
    pub emphasis: bool,
}

/// The text of a line as drawn, with every tab as [`TAB_WIDTH`] spaces.
pub fn expand_tabs(text: &str) -> String {
    text.replace('\t', &" ".repeat(TAB_WIDTH))
}

/// The pieces of `text` to style: where `syntax` (sorted, non-overlapping byte ranges) and
/// `emphasis` (the changed words) change, with the ranges moved to the text with tabs expanded.
/// Plain pieces that are not emphasized are left out.
pub fn segments(
    text: &str,
    syntax: &[(Range<usize>, Highlight)],
    emphasis: &[Range<usize>],
) -> Vec<Segment> {
    let len = text.len();
    let mut cuts: Vec<usize> = vec![0, len];
    for (range, _) in syntax {
        cuts.extend([range.start.min(len), range.end.min(len)]);
    }
    for range in emphasis {
        cuts.extend([range.start.min(len), range.end.min(len)]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    // The drawn offset of every cut: each tab before it adds TAB_WIDTH - 1 bytes.
    let drawn = |offset: usize| {
        offset
            + text.as_bytes()[..offset]
                .iter()
                .filter(|&&byte| byte == b'\t')
                .count()
                * (TAB_WIDTH - 1)
    };
    let mut out: Vec<Segment> = Vec::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let highlight = syntax
            .iter()
            .find(|(range, _)| range.start <= start && end <= range.end)
            .map(|(_, highlight)| *highlight);
        let emphasized = emphasis
            .iter()
            .any(|range| range.start <= start && end <= range.end);
        if highlight.is_none() && !emphasized {
            continue;
        }
        let range = drawn(start)..drawn(end);
        match out.last_mut() {
            Some(last)
                if last.range.end == range.start
                    && last.highlight == highlight
                    && last.emphasis == emphasized =>
            {
                last.range.end = range.end;
            }
            _ => out.push(Segment {
                range,
                highlight,
                emphasis: emphasized,
            }),
        }
    }
    out
}

/// The 0 based line of the new file where change block `block` of `diff` starts: its first
/// added line, or for a pure removal the line that follows it (as Postino).
pub fn block_new_line(diff: &FileDiff, block: usize) -> Option<usize> {
    let block = diff.blocks.get(block)?;
    Some(block.new_start.saturating_sub(1) as usize)
}

/// The change block that line `row` of the diff is part of, `None` for a context line.
pub fn block_at_row(diff: &FileDiff, row: usize) -> Option<usize> {
    let index = diff
        .blocks
        .partition_point(|block| block.first_line <= row)
        .checked_sub(1)?;
    let block = &diff.blocks[index];
    (row < block.first_line + block.removed + block.added).then_some(index)
}

/// The item before or after `current` in `items`, with no wrap: the previous or next file.
pub fn neighbour<T>(items: &[T], current: usize, forward: bool) -> Option<&T> {
    if forward {
        items.get(current + 1)
    } else {
        current.checked_sub(1).and_then(|index| items.get(index))
    }
}

/// How much of the diff header fits the width of the diff area (design `ted 0.7.0`, Postino
/// #289).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderFit {
    /// Every label, from [`HEADER_FULL_WIDTH`].
    Full,
    /// Previous, Next and Open file as icons with tooltips, and the counter as `3/19`.
    Narrow,
}

/// The area width, in logical pixels, from which the header shows every label. Measured with
/// the Spanish labels, the longest, and a file name of 20 characters.
pub const HEADER_FULL_WIDTH: f32 = 700.0;

/// The header for an area `width` wide.
pub fn header_fit(width: f32) -> HeaderFit {
    if width >= HEADER_FULL_WIDTH {
        HeaderFit::Full
    } else {
        HeaderFit::Narrow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use telaio_git::parse_file_diff;

    fn segment(range: Range<usize>, highlight: Option<Highlight>, emphasis: bool) -> Segment {
        Segment {
            range,
            highlight,
            emphasis,
        }
    }

    #[test]
    fn syntax_and_emphasis_split_into_pieces() {
        // "let x = 1;" with `let` a keyword, `1` a number and "x = 1" changed.
        let got = segments(
            "let x = 1;",
            &[(0..3, Highlight::Keyword), (8..9, Highlight::Number)],
            &[Range { start: 4, end: 9 }],
        );
        assert_eq!(
            got,
            vec![
                segment(0..3, Some(Highlight::Keyword), false),
                segment(4..8, None, true),
                segment(8..9, Some(Highlight::Number), true),
            ]
        );
    }

    #[test]
    fn tabs_move_the_ranges() {
        let got = segments("\tfn", &[(1..3, Highlight::Keyword)], &[]);
        assert_eq!(got, vec![segment(4..6, Some(Highlight::Keyword), false)]);
        assert_eq!(expand_tabs("\tfn"), "    fn");
    }

    #[test]
    fn ranges_past_the_end_are_clipped() {
        let got = segments("ab", &[(0..9, Highlight::String)], &[]);
        assert_eq!(got, vec![segment(0..2, Some(Highlight::String), false)]);
    }

    #[test]
    fn each_side_is_colored_as_a_whole_file() {
        // A block comment opened on a context line keeps coloring the removed line inside it,
        // and the added line, which is code, gets its own colors.
        let patch = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,3 @@\n /*\n-old\n */\n+fn x() {}\n";
        let diff = parse_file_diff(patch);
        let kinds: Vec<_> = diff.lines.iter().map(|line| line.kind).collect();
        assert_eq!(
            kinds,
            [
                LineKind::Context,
                LineKind::Removed,
                LineKind::Context,
                LineKind::Added
            ]
        );
        let syntax = diff_syntax(&diff, Language::Rust, 1 << 20);
        assert_eq!(syntax.len(), 4);
        assert!(syntax[1].iter().any(|(_, h)| *h == Highlight::Comment));
        assert!(syntax[3].iter().any(|(_, h)| *h == Highlight::Keyword));
        // Context lines are colored once.
        assert_eq!(syntax[0].len(), 1);
    }

    #[test]
    fn finds_the_block_of_a_row_and_its_new_line() {
        // Lines: 0 context, 1 removed, 2 added, 3 context, 4 added.
        let patch = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1,3 +1,4 @@\n a\n-b\n+B\n c\n+d\n";
        let diff = parse_file_diff(patch);
        assert_eq!(diff.blocks.len(), 2);
        assert_eq!(block_at_row(&diff, 0), None);
        assert_eq!(block_at_row(&diff, 1), Some(0));
        assert_eq!(block_at_row(&diff, 2), Some(0));
        assert_eq!(block_at_row(&diff, 3), None);
        assert_eq!(block_at_row(&diff, 4), Some(1));
        assert_eq!(block_new_line(&diff, 0), Some(1));
        assert_eq!(block_new_line(&diff, 1), Some(3));
        assert_eq!(block_new_line(&diff, 2), None);
    }

    #[test]
    fn neighbours_stop_at_the_ends() {
        let items = [1, 2, 3];
        assert_eq!(neighbour(&items, 0, true), Some(&2));
        assert_eq!(neighbour(&items, 2, true), None);
        assert_eq!(neighbour(&items, 0, false), None);
        assert_eq!(neighbour(&items, 2, false), Some(&2));
    }

    #[test]
    fn the_header_drops_labels_when_narrow() {
        assert_eq!(header_fit(1046.0), HeaderFit::Full);
        assert_eq!(header_fit(HEADER_FULL_WIDTH), HeaderFit::Full);
        // The smallest window with the Git panel open leaves 674 px.
        assert_eq!(header_fit(674.0), HeaderFit::Narrow);
    }

    #[test]
    fn no_grammar_or_too_big_means_no_colors() {
        let patch =
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-a\n+fn b() {}\n";
        let diff = parse_file_diff(patch);
        assert!(diff_syntax(&diff, Language::PlainText, 1 << 20).is_empty());
        assert!(diff_syntax(&diff, Language::Rust, 4).is_empty());
    }
}

//! The minimap's geometry and content, outside the gpui element: which lines it shows, where the
//! slider (the visible part of the document) sits, which scroll a pointer position means, and
//! the colored blocks of one line. Plain Rust, unit tested.
//!
//! Every line is [`LINE_HEIGHT`] pixels high and every character one pixel wide. When the
//! document is taller than the minimap, the minimap scrolls with the editor: at the top of the
//! document it shows its first lines, at the bottom its last ones, in proportion in between, so
//! the slider moves through the whole minimap as the editor scrolls through the document.

use std::ops::Range;

use ted_syntax::{Highlight, HighlightSpan};

use super::editor::advance;

/// Height of one line in the minimap, in pixels.
pub const LINE_HEIGHT: f64 = 2.0;
/// The most columns of a line the minimap draws: one pixel each.
pub const MAX_COLUMNS: usize = 120;

/// Where the minimap is, for a document and a scroll position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapLayout {
    /// The minimap's own scroll: the document line at its top is `scroll / LINE_HEIGHT`.
    pub scroll: f64,
    /// The slider's top and height, relative to the minimap's top.
    pub slider_top: f64,
    pub slider_height: f64,
    /// How far the slider travels while the editor goes from the top to the bottom.
    pub travel: f64,
}

/// The editor's measures the minimap follows.
#[derive(Debug, Clone, Copy)]
pub struct EditorMeasures {
    pub lines: usize,
    pub line_height: f64,
    /// The editor's vertical scroll and its largest value, in pixels.
    pub scroll_y: f64,
    pub max_scroll_y: f64,
    /// The height of the editor's viewport, and of the minimap.
    pub viewport_height: f64,
}

impl MinimapLayout {
    pub fn new(editor: EditorMeasures) -> Self {
        let scale = LINE_HEIGHT / editor.line_height;
        let content = editor.lines as f64 * LINE_HEIGHT;
        let overflow = (content - editor.viewport_height).max(0.0);
        let fraction = if editor.max_scroll_y > 0.0 {
            (editor.scroll_y / editor.max_scroll_y).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let scroll = fraction * overflow;
        let slider_height = (editor.viewport_height * scale).max(LINE_HEIGHT * 4.0);
        let travel = (editor.max_scroll_y * scale - overflow).max(0.0);
        Self {
            scroll,
            slider_top: fraction * travel,
            slider_height,
            travel,
        }
    }

    /// The document lines the minimap shows, in a minimap `height` pixels high.
    pub fn lines(&self, total: usize, height: f64) -> Range<usize> {
        let first = (self.scroll / LINE_HEIGHT).floor() as usize;
        let count = (height / LINE_HEIGHT).ceil() as usize + 1;
        first.min(total)..(first + count).min(total)
    }

    /// The editor scroll fraction (0 at the top, 1 at the bottom) that puts the slider's top at
    /// `slider_top`, while it is dragged.
    pub fn fraction_for_slider(&self, slider_top: f64) -> f64 {
        if self.travel > 0.0 {
            (slider_top / self.travel).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The document line under `y`, relative to the minimap's top.
    pub fn line_at(&self, y: f64) -> usize {
        ((self.scroll + y.max(0.0)) / LINE_HEIGHT) as usize
    }
}

/// The blocks of one line: `(columns, class)`, one pixel per character, tabs expanded, spaces
/// left out, neighbours of the same class merged, at most [`MAX_COLUMNS`] columns. `spans` are
/// relative to the start of `text`.
pub fn line_blocks(text: &str, spans: &[HighlightSpan]) -> Vec<(Range<usize>, Option<Highlight>)> {
    let mut blocks: Vec<(Range<usize>, Option<Highlight>)> = Vec::new();
    let mut spans = spans.iter().peekable();
    let mut column = 0;
    for (index, ch) in text.char_indices() {
        if column >= MAX_COLUMNS {
            break;
        }
        let next = advance(column, ch);
        if !ch.is_whitespace() {
            while spans.next_if(|span| span.range.end <= index).is_some() {}
            let highlight = spans
                .peek()
                .filter(|span| span.range.start <= index)
                .map(|span| span.highlight);
            match blocks.last_mut() {
                Some((range, class)) if range.end == column && *class == highlight => {
                    range.end = next;
                }
                _ => blocks.push((column..next, highlight)),
            }
        }
        column = next;
    }
    if let Some((range, _)) = blocks.last_mut() {
        range.end = range.end.min(MAX_COLUMNS);
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn measures(lines: usize, scroll_y: f64) -> EditorMeasures {
        let line_height = 20.0;
        let viewport_height = 400.0;
        EditorMeasures {
            lines,
            line_height,
            scroll_y,
            max_scroll_y: lines as f64 * line_height - viewport_height,
            viewport_height,
        }
    }

    #[test]
    fn a_short_document_does_not_scroll_the_minimap() {
        // 100 lines are 200 px of minimap, inside its 400 px.
        let top = MinimapLayout::new(measures(100, 0.0));
        assert_eq!(top.scroll, 0.0);
        assert_eq!(top.slider_top, 0.0);
        assert_eq!(top.slider_height, 40.0);
        let bottom = MinimapLayout::new(measures(100, 1600.0));
        assert_eq!(bottom.slider_top, 160.0);
        assert_eq!(bottom.slider_top + bottom.slider_height, 200.0);
        assert_eq!(top.lines(100, 400.0), 0..100);
    }

    #[test]
    fn a_long_document_scrolls_the_minimap_with_the_editor() {
        // 10 000 lines are 20 000 px of minimap.
        let max = 10_000.0 * 20.0 - 400.0;
        let top = MinimapLayout::new(measures(10_000, 0.0));
        assert_eq!((top.scroll, top.slider_top), (0.0, 0.0));
        let bottom = MinimapLayout::new(measures(10_000, max));
        assert_eq!(bottom.scroll, 19_600.0);
        // At the bottom the slider ends at the minimap's bottom.
        assert_eq!(bottom.slider_top + bottom.slider_height, 400.0);
        assert_eq!(bottom.lines(10_000, 400.0), 9_800..10_000);
        let middle = MinimapLayout::new(measures(10_000, max / 2.0));
        assert_eq!(middle.fraction_for_slider(middle.slider_top), 0.5);
        assert_eq!(middle.line_at(0.0), 4_900);
    }

    #[test]
    fn blocks_skip_spaces_and_follow_the_classes() {
        let spans = [
            HighlightSpan {
                range: 1..4,
                highlight: Highlight::Keyword,
            },
            HighlightSpan {
                range: 9..12,
                highlight: Highlight::String,
            },
        ];
        // "\tlet x = \"s\";" : the tab advances to column 4.
        let blocks = line_blocks("\tlet x = \"s\";", &spans);
        assert_eq!(
            blocks,
            vec![
                (4..7, Some(Highlight::Keyword)),
                (8..9, None),
                (10..11, None),
                (12..15, Some(Highlight::String)),
                (15..16, None),
            ]
        );
        assert!(line_blocks("   ", &[]).is_empty());
        let long = "x".repeat(MAX_COLUMNS * 2);
        assert_eq!(line_blocks(&long, &[]), vec![(0..MAX_COLUMNS, None)]);
    }
}

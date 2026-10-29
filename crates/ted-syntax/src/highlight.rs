//! The capture classes and the spans the editor paints.

use std::ops::Range;

/// A capture class. Each one maps to a role of the syntax palette in `docs/design-system.md`
/// (`crates/ted-app/src/theme/syntax.rs`); anything without a span is painted with the plain
/// text color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Highlight {
    Comment,
    Keyword,
    Function,
    Property,
    Attribute,
    String,
    Number,
    Type,
    Boolean,
    Constant,
    Tag,
    LogError,
    LogWarn,
    LogInfo,
    LogDebug,
    LogTrace,
    LogDate,
}

impl Highlight {
    /// The class of a tree-sitter capture name (`function.macro`, `string.special.key`, ...), or
    /// `None` for captures ted paints as plain text (punctuation, variables, operators).
    /// The most specific names are checked first; a dotted name with no entry of its own takes
    /// its first part's class.
    pub(crate) fn from_capture(name: &str) -> Option<Highlight> {
        match name {
            "string.special.key" | "text.uri" | "text.reference" => {
                return Some(Highlight::Property);
            }
            "constant.builtin" => return Some(Highlight::Constant),
            "text.title" => return Some(Highlight::Keyword),
            "text.literal" | "escape" => return Some(Highlight::String),
            "tag.error" | "none" | "embedded" => return None,
            _ => {}
        }
        let head = name.split('.').next().unwrap_or(name);
        Some(match head {
            "comment" => Highlight::Comment,
            "keyword" => Highlight::Keyword,
            "function" => Highlight::Function,
            "property" => Highlight::Property,
            "attribute" => Highlight::Attribute,
            "string" => Highlight::String,
            "number" => Highlight::Number,
            "type" | "constructor" => Highlight::Type,
            "boolean" => Highlight::Boolean,
            "constant" => Highlight::Constant,
            "tag" => Highlight::Tag,
            _ => return None,
        })
    }
}

/// A class over a byte range. For a document the range is in the buffer; for a log line it is
/// relative to the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub highlight: Highlight,
}

/// Turns captures into sorted, non-overlapping spans where the innermost capture wins.
///
/// `captures` is `(range, order, highlight)`; for two captures of the same range the higher
/// `order` (a later pattern of the query) wins. The result is clipped to `clip`, and neighbours
/// of the same class are merged.
pub(crate) fn resolve(
    mut captures: Vec<(Range<usize>, usize, Highlight)>,
    clip: Range<usize>,
) -> Vec<HighlightSpan> {
    // Outer before inner (same start: longer first), then by query order.
    captures.sort_by(|a, b| {
        a.0.start
            .cmp(&b.0.start)
            .then(b.0.end.cmp(&a.0.end))
            .then(a.1.cmp(&b.1))
    });

    let mut out: Vec<HighlightSpan> = Vec::new();
    let mut stack: Vec<(usize, Highlight)> = Vec::new();
    let mut pos = clip.start;

    // Emits what the stack paints from `pos` up to `target`, popping what ends on the way.
    let flush = |stack: &mut Vec<(usize, Highlight)>,
                 pos: &mut usize,
                 target: usize,
                 out: &mut Vec<HighlightSpan>| {
        while let Some(&(end, highlight)) = stack.last() {
            let upto = end.min(target).min(clip.end);
            if upto > *pos {
                push(out, *pos..upto, highlight);
                *pos = upto;
            }
            if end <= target {
                stack.pop();
            } else {
                break;
            }
        }
    };

    for (range, _, highlight) in captures {
        let start = range.start.max(clip.start);
        let end = range.end.min(clip.end);
        if start >= end {
            continue;
        }
        flush(&mut stack, &mut pos, start, &mut out);
        if stack.is_empty() {
            pos = start;
        }
        stack.push((end, highlight));
    }
    flush(&mut stack, &mut pos, usize::MAX, &mut out);
    out
}

fn push(out: &mut Vec<HighlightSpan>, range: Range<usize>, highlight: Highlight) {
    if let Some(last) = out.last_mut()
        && last.highlight == highlight
        && last.range.end == range.start
    {
        last.range.end = range.end;
        return;
    }
    out.push(HighlightSpan { range, highlight });
}

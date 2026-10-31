//! The highlighting of one document, outside the gpui element: its language, the syntax tree
//! that follows the buffer and when to parse again, and the colored runs of a painted line.
//!
//! The tree is kept in step with the buffer cheaply: after edits, the buffer's changes move the
//! tree ([`SyntaxTree::edited`]), so the editor paints the new text at once, and a parse in the
//! background (incremental, from that tree) replaces it. One parse runs at a time; edits made
//! while it runs make its result stale, and the next parse starts from the moved tree.
//! [`Syntax::sync`] and [`Syntax::parsed`] hand back a [`ParseRequest`] when the view should
//! start one.

use std::fmt;

use ted_core::Buffer;
use ted_syntax::{Highlight, HighlightSpan, Language, SyntaxTree, highlight_log_line};

use super::editor::advance;

/// How much of a log line the log highlighter reads: dates and levels are at its start, and a
/// long line must cost no more than a short one.
const LOG_PREFIX: usize = 512;

/// A parse for the view to run on the background executor.
pub struct ParseRequest {
    pub language: Language,
    /// A snapshot of the buffer (cloning is O(1)).
    pub buffer: Buffer,
    /// The tree moved by the edits since the last parse, for an incremental parse.
    pub previous: Option<SyntaxTree>,
    pub max_bytes: usize,
}

impl ParseRequest {
    /// Parses, blocking. `None` when the buffer is above `max_bytes`.
    pub fn run(self) -> Option<SyntaxTree> {
        SyntaxTree::parse(
            self.language,
            &self.buffer,
            self.previous.as_ref(),
            self.max_bytes,
        )
    }
}

/// The highlighting state of a document.
#[derive(Clone)]
pub struct Syntax {
    pub language: Language,
    /// Buffers above this many bytes are not parsed and stay plain.
    max_bytes: usize,
    /// The tree that follows the buffer: a parse result, or one moved by the edits since.
    tree: Option<SyntaxTree>,
    /// The tree is missing or was moved by edits: a parse should replace it.
    stale: bool,
    /// A parse is running.
    parsing: bool,
}

impl Default for Syntax {
    fn default() -> Self {
        Self::new(Language::PlainText, 0)
    }
}

impl fmt::Debug for Syntax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Syntax")
            .field("language", &self.language)
            .field("has_tree", &self.tree.is_some())
            .field("stale", &self.stale)
            .field("parsing", &self.parsing)
            .finish()
    }
}

impl Syntax {
    pub fn new(language: Language, max_bytes: usize) -> Self {
        Self {
            language,
            max_bytes,
            tree: None,
            stale: true,
            parsing: false,
        }
    }

    /// Whether this document is parsed at all: a language with a grammar and a buffer that is
    /// not too big.
    fn parses(&self, buffer: &Buffer) -> bool {
        self.language.has_grammar() && buffer.len() <= self.max_bytes
    }

    /// Brings the tree to the buffer's version, cheaply, and returns a parse to start when one is
    /// due and none is running. Called before painting; costs nothing when nothing changed.
    pub fn sync(&mut self, buffer: &mut Buffer) -> Option<ParseRequest> {
        if !self.parses(buffer) {
            self.tree = None;
            return None;
        }
        // Not tracked until now (a new buffer): its changes since the tree are unknown.
        let tracked = buffer.track_changes();
        let current = self
            .tree
            .as_ref()
            .is_some_and(|tree| tree.version() == buffer.version());
        if !current {
            // Changes lost (or a new buffer): drop the tree, the next parse starts over.
            self.tree = match (self.tree.take(), buffer.take_changes()) {
                (Some(tree), Some(changes)) if tracked => {
                    Some(tree.edited(&changes, buffer.version()))
                }
                _ => None,
            };
            self.stale = true;
        }
        if !self.stale || self.parsing {
            return None;
        }
        self.stale = false;
        self.parsing = true;
        Some(ParseRequest {
            language: self.language,
            buffer: buffer.clone(),
            previous: self.tree.clone(),
            max_bytes: self.max_bytes,
        })
    }

    /// A parse finished. Its tree replaces the current one when the buffer did not change
    /// meanwhile; otherwise it is dropped and the next parse is returned.
    pub fn parsed(
        &mut self,
        tree: Option<SyntaxTree>,
        buffer: &mut Buffer,
    ) -> Option<ParseRequest> {
        self.parsing = false;
        match tree {
            Some(tree) if tree.version() == buffer.version() => {
                // The changes made it into this tree already.
                let _ = buffer.take_changes();
                self.tree = Some(tree);
                self.stale = false;
            }
            _ => self.stale = true,
        }
        self.sync(buffer)
    }

    /// The spans of the start of every line in `lines` (at most `max_bytes` of each), each
    /// relative to its line's start, for the minimap. One tree query covers all of them when they
    /// are short; when the range is bigger than `max_range` (very long lines), each line is queried
    /// on its own start instead.
    pub fn lines_spans(
        &self,
        buffer: &Buffer,
        lines: std::ops::Range<usize>,
        max_bytes: usize,
        max_range: usize,
    ) -> Vec<Vec<HighlightSpan>> {
        let line_window = |line: usize| 0..buffer.line_len(line).min(max_bytes);
        let Some(tree) = self
            .tree
            .as_ref()
            .filter(|_| self.language != Language::Log)
        else {
            return lines
                .map(|line| self.line_spans(buffer, line, line_window(line)))
                .collect();
        };
        let start = buffer.line_to_byte(lines.start);
        let end = buffer.line_to_byte(lines.end.min(buffer.line_count()));
        if end.saturating_sub(start) > max_range {
            return lines
                .map(|line| self.line_spans(buffer, line, line_window(line)))
                .collect();
        }
        let spans = tree.highlights(buffer, start..end);
        let mut spans = spans.into_iter().peekable();
        lines
            .map(|line| {
                let line_start = buffer.line_to_byte(line);
                let line_end = line_start + line_window(line).end;
                let mut out = Vec::new();
                while let Some(span) = spans.peek() {
                    if span.range.start >= line_end {
                        break;
                    }
                    let span = spans.next_if(|span| span.range.end <= line_end);
                    let span = match span {
                        Some(span) => span,
                        // Crosses the end of this line's window: keep it for the next line.
                        None => {
                            let span = spans.peek().cloned();
                            let Some(span) = span else { break };
                            out.push(HighlightSpan {
                                range: span.range.start.max(line_start) - line_start
                                    ..line_end - line_start,
                                highlight: span.highlight,
                            });
                            break;
                        }
                    };
                    let from = span.range.start.max(line_start);
                    if from < span.range.end {
                        out.push(HighlightSpan {
                            range: from - line_start..span.range.end - line_start,
                            highlight: span.highlight,
                        });
                    }
                }
                // Spans of this line past its window are not drawn.
                while spans
                    .next_if(|span| span.range.end <= buffer.line_to_byte(line + 1))
                    .is_some()
                {}
                out
            })
            .collect()
    }

    /// The spans of the part of `line` that is painted, `window` (byte columns of the line),
    /// relative to `window.start`.
    pub fn line_spans(
        &self,
        buffer: &Buffer,
        line: usize,
        window: std::ops::Range<usize>,
    ) -> Vec<HighlightSpan> {
        if self.language == Language::Log {
            let slice = buffer.line(line);
            let prefix_end = slice.floor_char_boundary(slice.len().min(LOG_PREFIX));
            let prefix = slice.slice(..prefix_end).to_string();
            return clip(highlight_log_line(&prefix), window);
        }
        if let Some(highlight_line) = self.language.line_highlighter() {
            // From the start of the line, so a string or a comment opened before the window
            // still colors it.
            let slice = buffer.line(line);
            let end = slice.floor_char_boundary(slice.len().min(window.end));
            let text = slice.slice(..end).to_string();
            return clip(highlight_line(&text), window);
        }
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        let start = buffer.line_to_byte(line);
        let spans = tree.highlights(buffer, start + window.start..start + window.end);
        let offset = start + window.start;
        spans
            .into_iter()
            .map(|span| HighlightSpan {
                range: span.range.start - offset..span.range.end - offset,
                highlight: span.highlight,
            })
            .collect()
    }
}

/// `spans` of a whole line, cut to `window` and made relative to its start.
fn clip(spans: Vec<HighlightSpan>, window: std::ops::Range<usize>) -> Vec<HighlightSpan> {
    spans
        .into_iter()
        .filter_map(|span| {
            let start = span.range.start.max(window.start);
            let end = span.range.end.min(window.end);
            (start < end).then(|| HighlightSpan {
                range: start - window.start..end - window.start,
                highlight: span.highlight,
            })
        })
        .collect()
}

/// The runs of a painted line: `(length, class)` over its text with tabs expanded (what the
/// element shapes), from `spans` over `text` before expansion. Neighbours of the same class are
/// merged; `None` is plain text. Linear in the length of the line.
pub fn line_runs(text: &str, spans: &[HighlightSpan]) -> Vec<(usize, Option<Highlight>)> {
    let mut runs: Vec<(usize, Option<Highlight>)> = Vec::new();
    let mut spans = spans.iter().peekable();
    let mut display = 0;
    for (index, ch) in text.char_indices() {
        while spans.next_if(|span| span.range.end <= index).is_some() {}
        let highlight = spans
            .peek()
            .filter(|span| span.range.start <= index)
            .map(|span| span.highlight);
        let next = advance(display, ch);
        let width = if ch == '\t' {
            next - display
        } else {
            ch.len_utf8()
        };
        display = next;
        match runs.last_mut() {
            Some(last) if last.1 == highlight => last.0 += width,
            _ => runs.push((width, highlight)),
        }
    }
    runs
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const MAX: usize = 1 << 20;

    fn span(range: std::ops::Range<usize>, highlight: Highlight) -> HighlightSpan {
        HighlightSpan { range, highlight }
    }

    #[test]
    fn runs_cover_the_expanded_text() {
        let text = "\tlet x = \"é\";";
        let spans = [
            span(1..4, Highlight::Keyword),
            span(9..13, Highlight::String),
        ];
        let runs = line_runs(text, &spans);
        assert_eq!(
            runs,
            vec![
                (4, None),
                (3, Some(Highlight::Keyword)),
                (5, None),
                (4, Some(Highlight::String)),
                (1, None),
            ]
        );
        let total: usize = runs.iter().map(|run| run.0).sum();
        assert_eq!(total, crate::state::editor::expand_tabs(text).len());
    }

    #[test]
    fn a_line_without_spans_is_one_plain_run() {
        assert_eq!(line_runs("abc", &[]), vec![(3, None)]);
        assert_eq!(line_runs("", &[]), vec![]);
    }

    /// Runs one parse request the way the view does, synchronously.
    fn run(syntax: &mut Syntax, buffer: &mut Buffer, request: ParseRequest) {
        let tree = request.run();
        let next = syntax.parsed(tree, buffer);
        assert!(next.is_none(), "no edits meanwhile: no second parse");
    }

    #[test]
    fn edits_move_the_tree_and_ask_for_one_parse_at_a_time() {
        let mut buffer = Buffer::from_text("fn main() {}\n");
        let mut syntax = Syntax::new(Language::Rust, MAX);
        let first = syntax.sync(&mut buffer).expect("a first parse");
        assert!(syntax.sync(&mut buffer).is_none(), "nothing changed");
        run(&mut syntax, &mut buffer, first);
        assert!(!syntax.line_spans(&buffer, 0, 0..12).is_empty());

        // An edit moves the tree at once (still painted) and asks for an incremental parse.
        buffer.insert(0, "// hi\n");
        let request = syntax.sync(&mut buffer).expect("a parse after the edit");
        assert!(request.previous.is_some());
        let spans = syntax.line_spans(&buffer, 1, 0..12);
        assert_eq!(spans[0].highlight, Highlight::Keyword);

        // An edit while that parse runs: its result is stale and a new parse follows.
        buffer.insert(0, "\n");
        assert!(syntax.sync(&mut buffer).is_none(), "one parse at a time");
        let stale = request.run();
        let next = syntax.parsed(stale, &mut buffer).expect("parse again");
        run(&mut syntax, &mut buffer, next);
        let spans = syntax.line_spans(&buffer, 1, 0..5);
        assert_eq!(spans, vec![span(0..5, Highlight::Comment)]);
    }

    #[test]
    fn lines_spans_matches_one_query_per_line() {
        let text = "/* a\nb */ fn x() {}\nlet s = \"long string\";\n\nfn y() {}\n";
        let mut buffer = Buffer::from_text(text);
        let mut syntax = Syntax::new(Language::Rust, MAX);
        let request = syntax.sync(&mut buffer).expect("parse");
        run(&mut syntax, &mut buffer, request);
        let lines = 0..buffer.line_count();
        for max_bytes in [4, 8, 100] {
            let one_query = syntax.lines_spans(&buffer, lines.clone(), max_bytes, usize::MAX);
            let per_line = syntax.lines_spans(&buffer, lines.clone(), max_bytes, 0);
            assert_eq!(one_query, per_line, "max_bytes {max_bytes}");
        }
    }

    #[test]
    fn a_new_buffer_starts_over() {
        let mut buffer = Buffer::from_text("fn a() {}");
        let mut syntax = Syntax::new(Language::Rust, MAX);
        let request = syntax.sync(&mut buffer).expect("parse");
        run(&mut syntax, &mut buffer, request);

        let mut reloaded = Buffer::from_text("// b");
        let request = syntax.sync(&mut reloaded).expect("parse the new text");
        assert!(request.previous.is_none());
    }

    #[test]
    fn plain_text_logs_and_big_buffers_never_parse() {
        let mut buffer = Buffer::from_text("fn a() {}");
        assert!(
            Syntax::new(Language::PlainText, MAX)
                .sync(&mut buffer)
                .is_none()
        );
        assert!(Syntax::new(Language::Log, MAX).sync(&mut buffer).is_none());
        assert!(Syntax::new(Language::Rust, 4).sync(&mut buffer).is_none());
    }

    #[test]
    fn log_lines_are_highlighted_inside_the_window() {
        let mut buffer = Buffer::from_text("2024-01-15 10:30:45 ERROR boom\n");
        let syntax = Syntax::new(Language::Log, MAX);
        let _ = syntax.clone().sync(&mut buffer);
        let spans = syntax.line_spans(&buffer, 0, 0..30);
        assert_eq!(spans[0].highlight, Highlight::LogDate);
        assert_eq!(spans[1], span(20..25, Highlight::LogError));
        // A window starting inside the level word.
        let spans = syntax.line_spans(&buffer, 0, 22..30);
        assert_eq!(spans, vec![span(0..3, Highlight::LogError)]);
    }

    #[test]
    fn batch_lines_are_highlighted_inside_the_window() {
        let mut buffer = Buffer::from_text("rem a long note\r\necho %OUT%\r\n");
        let syntax = Syntax::new(Language::Batch, MAX);
        assert!(syntax.clone().sync(&mut buffer).is_none());
        let spans = syntax.line_spans(&buffer, 1, 0..10);
        assert_eq!(
            spans,
            vec![
                span(0..4, Highlight::Keyword),
                span(5..10, Highlight::Property)
            ]
        );
        // A window inside the comment still sees the `rem` before it.
        let spans = syntax.line_spans(&buffer, 0, 6..10);
        assert_eq!(spans, vec![span(0..4, Highlight::Comment)]);
    }
}

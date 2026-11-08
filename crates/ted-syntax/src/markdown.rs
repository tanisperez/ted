//! The model of the Markdown preview: a document parsed into the blocks the preview paints
//! (headings, paragraphs, lists, quotes, code, tables, rules), each with the source line it starts
//! on, so the preview can follow the editor. No gpui: `ted-app` turns the blocks into elements.
//!
//! The parser is `pulldown-cmark`, with GitHub's tables, task lists and strikethrough: linear in
//! the size of the text and light on memory. (`markdown`, which gpui-component already builds,
//! took 6.5 s and 1.5 GB for a 1.8 MB file.) Code blocks are highlighted here with ted's own
//! grammars, by the language of their fence.

use std::iter::Peekable;
use std::ops::Range;
use std::path::Path;

use pulldown_cmark::{CodeBlockKind, Event, OffsetIter, Options, Parser, Tag, TagEnd};
use ted_core::Buffer;

use crate::{HighlightSpan, Language, SyntaxTree};

/// Code blocks bigger than this are not highlighted, as big files are not.
const MAX_HIGHLIGHTED_CODE: usize = 256 * 1024;

/// A parsed document: its top level blocks, in order, and its words.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkdownDocument {
    pub blocks: Vec<Block>,
    /// The words of the text, code blocks left out, for the status bar.
    pub words: usize,
}

impl MarkdownDocument {
    /// The top level block that holds `line` (zero based), or the last one that starts before
    /// it, and how far into that block's lines `line` is, from 0 to 1.
    pub fn block_at_line(&self, line: usize) -> Option<(usize, f32)> {
        let index = self
            .blocks
            .partition_point(|block| block.lines.start <= line)
            .checked_sub(1)?;
        let lines = &self.blocks[index].lines;
        let span = lines.end.saturating_sub(lines.start).max(1);
        let into = (line.saturating_sub(lines.start) as f32 / span as f32).min(1.0);
        Some((index, into))
    }
}

/// One block and the source lines it spans (zero based, end excluded).
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub lines: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    /// `#` to `######`.
    Heading {
        level: u8,
        text: Inline,
    },
    Paragraph(Inline),
    List(List),
    /// The blocks inside `>`.
    Quote(Vec<Block>),
    /// A fenced or indented code block, highlighted when its language is known.
    Code {
        language: Option<Language>,
        text: String,
        spans: Vec<HighlightSpan>,
    },
    Table(Table),
    /// `---`.
    Rule,
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub ordered: bool,
    /// The number of the first item of an ordered list.
    pub start: u32,
    pub items: Vec<ListItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    /// `Some` for a task (`- [ ]`, `- [x]`): whether it is done.
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub head: Vec<Inline>,
    pub rows: Vec<Vec<Inline>>,
}

/// Text with its styled parts: what a paragraph, a heading or a table cell shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inline {
    pub text: String,
    /// The styled ranges of `text` (bytes), in order and not overlapping. The rest is plain.
    pub spans: Vec<InlineSpan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InlineSpan {
    pub range: Range<usize>,
    pub style: InlineStyle,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InlineStyle {
    pub strong: bool,
    pub emphasis: bool,
    pub strikethrough: bool,
    /// `code`.
    pub code: bool,
    /// The target of a link.
    pub link: Option<String>,
    /// The alternative text of an image, which the preview does not load.
    pub image: bool,
}

impl InlineStyle {
    fn is_plain(&self) -> bool {
        *self == InlineStyle::default()
    }
}

/// Parses `text` into the blocks of the preview. Linear in its length, plus the highlighting of
/// its code blocks; meant for the background executor.
pub fn parse_markdown(text: &str) -> MarkdownDocument {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut parser = Builder {
        events: Parser::new_ext(text, options).into_offset_iter().peekable(),
        lines: LineIndex::new(text),
    };
    MarkdownDocument {
        blocks: parser.blocks(None),
        words: word_count(text),
    }
}

/// The line of a byte offset, by binary search over the line starts.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        Self { starts }
    }

    fn line(&self, offset: usize) -> usize {
        self.starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
    }

    /// The lines a byte range covers, end excluded.
    fn lines(&self, range: &Range<usize>) -> Range<usize> {
        let start = self.line(range.start);
        let end = self.line(range.end.saturating_sub(1).max(range.start));
        start..end + 1
    }
}

/// Turns the events of the parser into blocks, one container at a time.
struct Builder<'a> {
    events: Peekable<OffsetIter<'a>>,
    lines: LineIndex,
}

impl Builder<'_> {
    /// The blocks up to the end of `container` (`None`: the whole document).
    fn blocks(&mut self, container: Option<TagEnd>) -> Vec<Block> {
        let mut blocks = Vec::new();
        while let Some((event, range)) = self.events.next() {
            match event {
                Event::End(end) if Some(end) == container => break,
                Event::Start(tag) if !is_inline_tag(&tag) => {
                    if let Some(block) = self.block(tag, range) {
                        blocks.push(block);
                    }
                }
                Event::Rule => blocks.push(Block {
                    kind: BlockKind::Rule,
                    lines: self.lines.lines(&range),
                }),
                // Text right inside a list item: the items of a tight list.
                event if is_inline(&event) || matches!(event, Event::Start(_)) => {
                    let mut inline = Inline::default();
                    let mut styles = Vec::new();
                    self.inline_event(event, &mut styles, &mut inline);
                    let end = self.inline_run(&mut styles, &mut inline, range.end);
                    linkify(&mut inline);
                    blocks.push(Block {
                        kind: BlockKind::Paragraph(inline),
                        lines: self.lines.lines(&(range.start..end)),
                    });
                }
                _ => {}
            }
        }
        blocks
    }

    /// The block a start tag opens, `None` for what the preview leaves out.
    fn block(&mut self, tag: Tag, range: Range<usize>) -> Option<Block> {
        let lines = self.lines.lines(&range);
        let kind = match tag {
            Tag::Paragraph => BlockKind::Paragraph(self.inline(TagEnd::Paragraph)),
            Tag::Heading { level, .. } => BlockKind::Heading {
                level: level as u8,
                text: self.inline(TagEnd::Heading(level)),
            },
            Tag::BlockQuote(kind) => BlockKind::Quote(self.blocks(Some(TagEnd::BlockQuote(kind)))),
            Tag::CodeBlock(kind) => {
                let language = match &kind {
                    CodeBlockKind::Fenced(info) => Language::from_fence(info),
                    CodeBlockKind::Indented => None,
                };
                let mut code = String::new();
                for (event, _) in self.events.by_ref() {
                    match event {
                        Event::Text(text) => code.push_str(&text),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                // The closing line break is not part of the code.
                if code.ends_with('\n') {
                    code.pop();
                }
                BlockKind::Code {
                    spans: highlight_code(language, &code),
                    language,
                    text: code,
                }
            }
            Tag::List(start) => {
                let ordered = start.is_some();
                let mut items = Vec::new();
                while let Some((event, _)) = self.events.next() {
                    match event {
                        Event::Start(Tag::Item) => {
                            let checked = match self.events.peek() {
                                Some((Event::TaskListMarker(done), _)) => {
                                    let done = *done;
                                    self.events.next();
                                    Some(done)
                                }
                                _ => None,
                            };
                            items.push(ListItem {
                                checked,
                                blocks: self.blocks(Some(TagEnd::Item)),
                            });
                        }
                        Event::End(TagEnd::List(_)) => break,
                        _ => {}
                    }
                }
                BlockKind::List(List {
                    ordered,
                    start: start.unwrap_or(1).min(u64::from(u32::MAX)) as u32,
                    items,
                })
            }
            Tag::Table(_) => {
                let mut head = Vec::new();
                let mut rows = Vec::new();
                let mut row = Vec::new();
                while let Some((event, _)) = self.events.next() {
                    match event {
                        Event::Start(Tag::TableCell) => row.push(self.inline(TagEnd::TableCell)),
                        Event::End(TagEnd::TableHead) => head = std::mem::take(&mut row),
                        Event::End(TagEnd::TableRow) => rows.push(std::mem::take(&mut row)),
                        Event::End(TagEnd::Table) => break,
                        _ => {}
                    }
                }
                BlockKind::Table(Table { head, rows })
            }
            // Raw HTML, front matter and footnote definitions show nothing.
            other => {
                self.skip(other.to_end());
                return None;
            }
        };
        Some(Block { kind, lines })
    }

    /// Skips everything up to `end`, nested tags included.
    fn skip(&mut self, end: TagEnd) {
        let mut depth = 0;
        for (event, _) in self.events.by_ref() {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(found) if depth == 0 && found == end => break,
                Event::End(_) => depth -= 1,
                _ => {}
            }
        }
    }

    /// The inline content up to `end`.
    fn inline(&mut self, end: TagEnd) -> Inline {
        let mut inline = Inline::default();
        let mut styles = Vec::new();
        while let Some((event, _)) = self.events.next() {
            if event == Event::End(end) {
                break;
            }
            self.inline_event(event, &mut styles, &mut inline);
        }
        linkify(&mut inline);
        inline
    }

    /// More inline events right after the first one of a tight list item, up to the next block
    /// or the end of the item. Returns the end of the last one.
    fn inline_run(
        &mut self,
        styles: &mut Vec<InlineStyle>,
        inline: &mut Inline,
        mut end: usize,
    ) -> usize {
        while let Some((event, _)) = self.events.peek() {
            let opens_inline = matches!(event, Event::Start(tag) if is_inline_tag(tag));
            let closes_inline = matches!(event, Event::End(tag) if is_inline_tag_end(tag));
            if !(is_inline(event) || opens_inline || (closes_inline && !styles.is_empty())) {
                break;
            }
            let Some((event, range)) = self.events.next() else {
                break;
            };
            end = range.end;
            self.inline_event(event, styles, inline);
        }
        end
    }

    /// Adds one inline event to `inline`. `styles` is the stack of open styles.
    fn inline_event(&mut self, event: Event, styles: &mut Vec<InlineStyle>, inline: &mut Inline) {
        let current = styles.last().cloned().unwrap_or_default();
        match event {
            Event::Text(text) => push(inline, &text, &current),
            Event::Code(code) => push(
                inline,
                &code,
                &InlineStyle {
                    code: true,
                    ..current
                },
            ),
            Event::SoftBreak => push(inline, " ", &current),
            Event::HardBreak => push(inline, "\n", &InlineStyle::default()),
            Event::FootnoteReference(name) => push(inline, &format!("[{name}]"), &current),
            Event::Start(Tag::Emphasis) => styles.push(InlineStyle {
                emphasis: true,
                ..current
            }),
            Event::Start(Tag::Strong) => styles.push(InlineStyle {
                strong: true,
                ..current
            }),
            Event::Start(Tag::Strikethrough) => styles.push(InlineStyle {
                strikethrough: true,
                ..current
            }),
            Event::Start(Tag::Link { dest_url, .. }) => styles.push(InlineStyle {
                link: Some(dest_url.to_string()),
                ..current
            }),
            Event::Start(Tag::Image { .. }) => styles.push(InlineStyle {
                image: true,
                ..current
            }),
            Event::End(
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Link
                | TagEnd::Image,
            ) => {
                styles.pop();
            }
            // Inline HTML, math and the rest show nothing.
            _ => {}
        }
    }
}

/// Turns the bare URLs of `inline` into links, except inside code, images and links. It works on
/// the finished text because the parser may cut a URL into several text events (at a `&` or a
/// `_`, for example).
fn linkify(inline: &mut Inline) {
    let urls: Vec<(Range<usize>, String)> = bare_urls(&inline.text)
        .into_iter()
        .filter(|(range, _)| {
            !inline.spans.iter().any(|span| {
                span.range.start < range.end
                    && range.start < span.range.end
                    && (span.style.code || span.style.image || span.style.link.is_some())
            })
        })
        .collect();
    if urls.is_empty() {
        return;
    }
    // The text is cut at every edge of a span or a URL: each piece has one style.
    let mut edges: Vec<usize> = inline
        .spans
        .iter()
        .flat_map(|span| [span.range.start, span.range.end])
        .chain(urls.iter().flat_map(|(range, _)| [range.start, range.end]))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let mut spans: Vec<InlineSpan> = Vec::new();
    for piece in edges.windows(2) {
        let (start, end) = (piece[0], piece[1]);
        let mut style = inline
            .spans
            .iter()
            .find(|span| span.range.contains(&start))
            .map(|span| span.style.clone())
            .unwrap_or_default();
        if let Some((_, url)) = urls.iter().find(|(range, _)| range.contains(&start)) {
            style.link = Some(url.clone());
        }
        if style.is_plain() {
            continue;
        }
        match spans.last_mut() {
            Some(last) if last.range.end == start && last.style == style => last.range.end = end,
            _ => spans.push(InlineSpan {
                range: start..end,
                style,
            }),
        }
    }
    inline.spans = spans;
}

/// The bare URLs of `text`, as in GitHub's autolinks: each one's range and where it points. A URL
/// starts with `http://`, `https://` or `www.`, after anything but a letter or a digit, and ends
/// at a space or a `<`. Closing punctuation is not part of it.
fn bare_urls(text: &str) -> Vec<(Range<usize>, String)> {
    let mut urls = Vec::new();
    let mut at = 0;
    while let Some(found) = text[at..].find(['h', 'H', 'w', 'W']) {
        let start = at + found;
        let rest = &text[start..];
        let prefix = ["https://", "http://", "www."].into_iter().find(|prefix| {
            rest.get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        });
        let boundary = !text[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric);
        let url = prefix.filter(|_| boundary).and_then(|prefix| {
            let len = rest
                .find(|ch: char| ch.is_whitespace() || ch.is_control() || ch == '<')
                .unwrap_or(rest.len());
            let url = trim_url_end(&rest[..len]);
            let host = url.get(prefix.len()..)?;
            host.starts_with(char::is_alphanumeric)
                .then_some((prefix, url))
        });
        match url {
            Some((prefix, url)) => {
                let target = match prefix {
                    "www." => format!("https://{url}"),
                    _ => url.to_string(),
                };
                urls.push((start..start + url.len(), target));
                at = start + url.len();
            }
            // Past this letter, always one byte long.
            None => at = start + 1,
        }
    }
    urls
}

/// `url` without what closes the sentence around it: final punctuation and brackets opened
/// before the URL. `https://en.wikipedia.org/wiki/Rust_(language)` keeps its parenthesis.
fn trim_url_end(mut url: &str) -> &str {
    loop {
        let Some(last) = url.chars().next_back() else {
            return url;
        };
        let surplus = match last {
            '?' | '!' | '.' | ',' | ':' | ';' | '*' | '_' | '~' | '\'' | '"' => true,
            ')' => url.matches(')').count() > url.matches('(').count(),
            ']' => url.matches(']').count() > url.matches('[').count(),
            '}' => url.matches('}').count() > url.matches('{').count(),
            _ => false,
        };
        if !surplus {
            return url;
        }
        url = &url[..url.len() - last.len_utf8()];
    }
}

/// An event that is text in a paragraph.
fn is_inline(event: &Event) -> bool {
    matches!(
        event,
        Event::Text(_)
            | Event::Code(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::FootnoteReference(_)
            | Event::InlineHtml(_)
            | Event::InlineMath(_)
    )
}

fn is_inline_tag(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }
    )
}

fn is_inline_tag_end(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link | TagEnd::Image
    )
}
fn push(out: &mut Inline, text: &str, style: &InlineStyle) {
    if text.is_empty() {
        return;
    }
    let start = out.text.len();
    out.text.push_str(text);
    if style.is_plain() {
        return;
    }
    // Merge with the previous span when it has the same style and touches this one.
    if let Some(last) = out.spans.last_mut()
        && last.range.end == start
        && last.style == *style
    {
        last.range.end = out.text.len();
        return;
    }
    out.spans.push(InlineSpan {
        range: start..out.text.len(),
        style: style.clone(),
    });
}

/// The highlighted ranges of a code block, empty for an unknown language or a huge block.
fn highlight_code(language: Option<Language>, code: &str) -> Vec<HighlightSpan> {
    let Some(language) = language else {
        return Vec::new();
    };
    if code.len() > MAX_HIGHLIGHTED_CODE {
        return Vec::new();
    }
    if let Some(line_highlighter) = language.line_highlighter() {
        let mut spans = Vec::new();
        let mut offset = 0;
        for line in code.split('\n') {
            spans.extend(
                line_highlighter(line)
                    .into_iter()
                    .map(|span| HighlightSpan {
                        range: span.range.start + offset..span.range.end + offset,
                        highlight: span.highlight,
                    }),
            );
            offset += line.len() + 1;
        }
        return spans;
    }
    if !language.has_grammar() {
        return Vec::new();
    }
    let buffer = Buffer::from_text(code);
    match SyntaxTree::parse(language, &buffer, None, MAX_HIGHLIGHTED_CODE) {
        Some(tree) => tree.highlights(&buffer, 0..buffer.len()),
        None => Vec::new(),
    }
}

/// The words of a Markdown text, fenced code blocks left out: the runs of letters and digits.
pub fn word_count(text: &str) -> usize {
    let mut words = 0;
    let mut fence: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, Some(marker)) => {
                fence = Some(marker);
                continue;
            }
            (Some(open), Some(marker)) if open == marker => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            _ => {}
        }
        let mut in_word = false;
        for ch in line.chars() {
            let word_char = ch.is_alphanumeric();
            if word_char && !in_word {
                words += 1;
            }
            in_word = word_char;
        }
    }
    words
}

impl Language {
    /// The language a code fence names (` ```rust `, ` ```ts `, ` ```shell `), by its usual
    /// names and by the extensions [`Language::detect`] knows. `None` for anything else.
    pub fn from_fence(info: &str) -> Option<Language> {
        let name = info
            .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == '{')
            .next()?
            .trim()
            .to_ascii_lowercase();
        let by_name = match name.as_str() {
            "" => return None,
            "rust" => Language::Rust,
            "javascript" | "node" => Language::JavaScript,
            "typescript" => Language::TypeScript,
            "shell" | "bash" | "zsh" | "console" | "shellscript" => Language::Shell,
            "python" => Language::Python,
            "powershell" | "pwsh" => Language::PowerShell,
            "batch" => Language::Batch,
            "dockerfile" | "docker" => Language::Dockerfile,
            "makefile" | "make" => Language::Make,
            "markdown" => Language::Markdown,
            "kotlin" => Language::Kotlin,
            "gitignore" => Language::GitIgnore,
            "editorconfig" => Language::EditorConfig,
            "env" | "dotenv" => Language::Env,
            "text" | "txt" | "plain" | "plaintext" => return None,
            _ => {
                let detected = Language::detect(Path::new(&format!("file.{name}")));
                return (detected != Language::PlainText).then_some(detected);
            }
        };
        Some(by_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Highlight;
    use pretty_assertions::assert_eq;

    fn kinds(text: &str) -> Vec<BlockKind> {
        parse_markdown(text)
            .blocks
            .into_iter()
            .map(|block| block.kind)
            .collect()
    }

    fn plain(text: &str) -> Inline {
        Inline {
            text: text.to_string(),
            spans: Vec::new(),
        }
    }

    #[test]
    fn headings_paragraphs_and_rules() {
        let blocks = kinds("# Title\n\nOne\ntwo lines.\n\n---\n\n### Small\n");
        assert_eq!(
            blocks,
            vec![
                BlockKind::Heading {
                    level: 1,
                    text: plain("Title")
                },
                BlockKind::Paragraph(plain("One two lines.")),
                BlockKind::Rule,
                BlockKind::Heading {
                    level: 3,
                    text: plain("Small")
                },
            ]
        );
    }

    /// The links of the first paragraph: each one's text and target.
    fn links(source: &str) -> Vec<(String, String)> {
        let document = parse_markdown(source);
        let Some(BlockKind::Paragraph(inline)) = document.blocks.first().map(|b| &b.kind) else {
            panic!("no paragraph in {source}");
        };
        inline
            .spans
            .iter()
            .filter_map(|span| {
                let url = span.style.link.clone()?;
                Some((inline.text[span.range.clone()].to_string(), url))
            })
            .collect()
    }

    fn link(text: &str, url: &str) -> (String, String) {
        (text.to_string(), url.to_string())
    }

    #[test]
    fn bare_urls_become_links() {
        assert_eq!(
            links("See https://x.dev/a?b=1&c=2_d and http://y.dev too"),
            [
                link("https://x.dev/a?b=1&c=2_d", "https://x.dev/a?b=1&c=2_d"),
                link("http://y.dev", "http://y.dev"),
            ]
        );
        assert_eq!(
            links("Go to www.x.dev."),
            [link("www.x.dev", "https://www.x.dev")]
        );
    }

    #[test]
    fn bare_urls_leave_closing_punctuation_out() {
        for source in [
            "(https://x.dev)",
            "https://x.dev.",
            "https://x.dev, then",
            "\"https://x.dev\"",
            "[https://x.dev]",
            "https://x.dev?!",
        ] {
            assert_eq!(
                links(source),
                [link("https://x.dev", "https://x.dev")],
                "{source}"
            );
        }
        let url = "https://x.dev/Rust_(language)";
        assert_eq!(links(url), [link(url, url)]);
    }

    #[test]
    fn what_is_not_a_bare_url_stays_text() {
        for source in [
            "`https://x.dev` is code",
            "xhttps://x.dev",
            "https:// nothing",
            "www. nothing",
            "where and what",
        ] {
            assert_eq!(links(source), [], "{source}");
        }
        // A real link keeps its own target.
        assert_eq!(
            links("[https://shown.dev](https://real.dev)"),
            [link("https://shown.dev", "https://real.dev")]
        );
        assert_eq!(links("![https://x.dev](a.png)"), []);
    }

    #[test]
    fn bare_urls_keep_the_styles_around_them() {
        assert_eq!(
            links("**bold https://x.dev bold**"),
            [link("https://x.dev", "https://x.dev")]
        );
        let document = parse_markdown("**a https://x.dev b** ñ https://y.dev");
        let Some(BlockKind::Paragraph(inline)) = document.blocks.first().map(|b| &b.kind) else {
            panic!("no paragraph");
        };
        let strong = inline.spans.iter().filter(|span| span.style.strong).count();
        assert_eq!(strong, 3);
        assert_eq!(
            inline
                .spans
                .iter()
                .filter(|s| s.style.link.is_some())
                .count(),
            2
        );
    }

    #[test]
    fn bare_urls_in_tight_lists_headings_and_tables() {
        let urls = |source: &str| {
            format!("{:?}", parse_markdown(source).blocks)
                .matches("link: Some")
                .count()
        };
        assert_eq!(urls("- https://x.dev\n- other"), 1);
        assert_eq!(urls("# https://x.dev"), 1);
        assert_eq!(urls("| a |\n| - |\n| https://x.dev |"), 1);
    }

    #[test]
    fn tight_list_items_can_start_with_a_style() {
        let document = parse_markdown("- [a](b.md) c\n- **d** e\n- `f`");
        let Some(BlockKind::List(list)) = document.blocks.first().map(|b| &b.kind) else {
            panic!("no list");
        };
        let texts: Vec<&str> = list
            .items
            .iter()
            .filter_map(|item| match &item.blocks.first()?.kind {
                BlockKind::Paragraph(inline) => Some(inline.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["a c", "d e", "f"]);
    }

    #[test]
    fn inline_styles_and_links() {
        let [BlockKind::Paragraph(inline)] =
            &kinds("Some **bold _both_**, `code`, ~~old~~ and [a link](https://x.dev).")[..]
        else {
            panic!("one paragraph");
        };
        assert_eq!(inline.text, "Some bold both, code, old and a link.");
        let styled: Vec<(&str, InlineStyle)> = inline
            .spans
            .iter()
            .map(|span| (&inline.text[span.range.clone()], span.style.clone()))
            .collect();
        assert_eq!(
            styled,
            vec![
                (
                    "bold ",
                    InlineStyle {
                        strong: true,
                        ..Default::default()
                    }
                ),
                (
                    "both",
                    InlineStyle {
                        strong: true,
                        emphasis: true,
                        ..Default::default()
                    }
                ),
                (
                    "code",
                    InlineStyle {
                        code: true,
                        ..Default::default()
                    }
                ),
                (
                    "old",
                    InlineStyle {
                        strikethrough: true,
                        ..Default::default()
                    }
                ),
                (
                    "a link",
                    InlineStyle {
                        link: Some("https://x.dev".to_string()),
                        ..Default::default()
                    }
                ),
            ]
        );
    }

    #[test]
    fn lists_tasks_and_quotes() {
        let blocks =
            kinds("3. three\n4. four\n   - nested\n\n- [x] done\n- [ ] todo\n\n> quoted\n");
        let BlockKind::List(ordered) = &blocks[0] else {
            panic!("an ordered list: {blocks:?}");
        };
        assert!(ordered.ordered);
        assert_eq!(ordered.start, 3);
        assert_eq!(ordered.items.len(), 2);
        // The nested list lives inside the second item.
        assert!(matches!(
            ordered.items[1].blocks[1].kind,
            BlockKind::List(List { ordered: false, .. })
        ));
        let BlockKind::List(tasks) = &blocks[1] else {
            panic!("a task list");
        };
        let checked: Vec<_> = tasks.items.iter().map(|item| item.checked).collect();
        assert_eq!(checked, [Some(true), Some(false)]);
        assert!(matches!(&blocks[2], BlockKind::Quote(inner) if inner.len() == 1));
    }

    #[test]
    fn code_blocks_are_highlighted_by_their_fence() {
        let blocks = kinds("```rust\nfn main() {}\n```\n\n```\nplain\n```\n");
        let BlockKind::Code {
            language,
            text,
            spans,
        } = &blocks[0]
        else {
            panic!("code");
        };
        assert_eq!(*language, Some(Language::Rust));
        assert_eq!(text, "fn main() {}");
        assert!(
            spans
                .iter()
                .any(|span| &text[span.range.clone()] == "fn"
                    && span.highlight == Highlight::Keyword),
            "{spans:?}"
        );
        assert!(
            matches!(&blocks[1], BlockKind::Code { language: None, spans, .. } if spans.is_empty())
        );
    }

    #[test]
    fn tables() {
        let blocks = kinds("| Key | Value |\n| --- | --- |\n| `a` | 1 |\n| b | 2 |\n");
        let BlockKind::Table(table) = &blocks[0] else {
            panic!("a table");
        };
        assert_eq!(table.head, vec![plain("Key"), plain("Value")]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[1], vec![plain("b"), plain("2")]);
    }

    #[test]
    fn html_shows_nothing_and_images_show_their_text() {
        let blocks = kinds("<p align=\"center\">x</p>\n\n![The logo](logo.png)\n");
        let [BlockKind::Paragraph(inline)] = &blocks[..] else {
            panic!("only the image paragraph: {blocks:?}");
        };
        assert_eq!(inline.text, "The logo");
        assert!(inline.spans[0].style.image);
    }

    #[test]
    fn blocks_know_their_lines() {
        let document = parse_markdown("# A\n\npara\ngraph\n\n- x\n- y\n");
        let lines: Vec<_> = document.blocks.iter().map(|b| b.lines.clone()).collect();
        assert_eq!(lines, vec![0..1, 2..4, 5..7]);
        assert_eq!(document.block_at_line(0), Some((0, 0.0)));
        assert_eq!(document.block_at_line(3), Some((1, 0.5)));
        // A blank line between blocks belongs to the one above.
        assert_eq!(document.block_at_line(4).map(|(i, _)| i), Some(1));
        assert_eq!(document.block_at_line(99).map(|(i, _)| i), Some(2));
        assert_eq!(MarkdownDocument::default().block_at_line(0), None);
    }

    #[test]
    fn words_leave_code_out() {
        assert_eq!(word_count("# Hello, world\n\nÁrbol de 3 ramas."), 6);
        assert_eq!(word_count("one\n```rust\nfn a() {}\n```\ntwo"), 2);
        assert_eq!(parse_markdown("a b c").words, 3);
    }

    #[test]
    fn fence_languages() {
        assert_eq!(Language::from_fence("rust"), Some(Language::Rust));
        assert_eq!(Language::from_fence("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_fence("tsx"), Some(Language::Tsx));
        assert_eq!(Language::from_fence("Shell"), Some(Language::Shell));
        assert_eq!(Language::from_fence("json,ignore"), Some(Language::Json));
        assert_eq!(Language::from_fence("yml"), Some(Language::Yaml));
        assert_eq!(Language::from_fence("text"), None);
        assert_eq!(Language::from_fence("cobol"), None);
    }
}

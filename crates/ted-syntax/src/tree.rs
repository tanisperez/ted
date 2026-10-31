//! The parse tree of one buffer version, its incremental edits and its highlights.

use std::ops::Range;
use std::sync::Arc;

use tree_sitter::{InputEdit, Parser, Point, QueryCursor, StreamingIterator, TextProvider};

use crate::grammar::grammar;
use crate::highlight::resolve;
use crate::{HighlightSpan, Language};
use ted_core::{Buffer, Rope, TextChange};

/// A [`TextChange`] in tree-sitter's terms.
fn input_edit(change: &TextChange) -> InputEdit {
    let p = |(row, column): (usize, usize)| Point { row, column };
    InputEdit {
        start_byte: change.start_byte,
        old_end_byte: change.old_end_byte,
        new_end_byte: change.new_end_byte,
        start_position: p(change.start_position),
        old_end_position: p(change.old_end_position),
        new_end_position: p(change.new_end_position),
    }
}

struct Inner {
    language: Language,
    tree: tree_sitter::Tree,
    version: u64,
    /// True when edits were applied after the parse, so it is a valid base for an incremental
    /// parse and not a finished result.
    edited: bool,
}

/// The parse result for one buffer version. Cheap to clone and `Send + Sync`.
#[derive(Clone)]
pub struct SyntaxTree {
    inner: Arc<Inner>,
}

impl SyntaxTree {
    /// Parses `buffer`, blocking: run it on a background thread with a clone of the buffer.
    ///
    /// With `previous` it is incremental, but only if `previous` came from [`SyntaxTree::edited`]
    /// (a tree that still lacks the edits is ignored and the parse is full) and has the same
    /// language. Returns `None` for languages without a grammar and for buffers above
    /// `max_bytes`.
    pub fn parse(
        language: Language,
        buffer: &Buffer,
        previous: Option<&SyntaxTree>,
        max_bytes: usize,
    ) -> Option<SyntaxTree> {
        let grammar = grammar(language)?;
        if buffer.len() > max_bytes {
            return None;
        }
        let mut parser = Parser::new();
        parser.set_language(&grammar.language).ok()?;
        let old = previous
            .filter(|p| p.inner.edited && p.inner.language == language)
            .map(|p| &p.inner.tree);

        let rope: &Rope = buffer.rope();
        let len = rope.len();
        let tree = parser.parse_with_options(
            &mut |byte, _| -> &[u8] {
                if byte >= len {
                    return &[];
                }
                let (chunk, start) = rope.chunk(byte);
                &chunk.as_bytes()[byte - start..]
            },
            old,
            None,
        )?;
        Some(SyntaxTree {
            inner: Arc::new(Inner {
                language,
                tree,
                version: buffer.version(),
                edited: false,
            }),
        })
    }

    /// This tree with `edits` applied, as the tree of the buffer at `new_version`. Cheap (it
    /// shifts the nodes after the edits, no parsing).
    ///
    /// Two uses: it is the `previous` of the next incremental [`SyntaxTree::parse`], and it can be
    /// painted at once while that parse runs, because its ranges already follow the new text
    /// (only what is inside the edits is out of date). Edits must be given in the order they were
    /// applied to the buffer.
    pub fn edited(&self, edits: &[TextChange], new_version: u64) -> SyntaxTree {
        let mut tree = self.inner.tree.clone();
        for edit in edits {
            tree.edit(&input_edit(edit));
        }
        SyntaxTree {
            inner: Arc::new(Inner {
                language: self.inner.language,
                tree,
                version: new_version,
                edited: true,
            }),
        }
    }

    /// The language this tree was parsed with.
    pub fn language(&self) -> Language {
        self.inner.language
    }

    /// The buffer version it was parsed from (or moved to by [`SyntaxTree::edited`]).
    pub fn version(&self) -> u64 {
        self.inner.version
    }

    /// The spans that intersect `range` (a byte range, typically the visible lines), sorted,
    /// non-overlapping, clipped to `range`. Where captures nest the innermost wins. Cost is in
    /// proportion to the range, not to the document.
    ///
    /// Empty when `buffer` is not the version of this tree.
    pub fn highlights(&self, buffer: &Buffer, range: Range<usize>) -> Vec<HighlightSpan> {
        let Some(grammar) = grammar(self.inner.language) else {
            return Vec::new();
        };
        let end = range.end.min(buffer.len());
        if buffer.version() != self.inner.version || range.start >= end {
            return Vec::new();
        }
        let range = range.start..end;

        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(range.clone());
        let mut captures = cursor.captures(
            &grammar.query,
            self.inner.tree.root_node(),
            RopeText(buffer.rope()),
        );
        let mut found = Vec::new();
        while let Some((m, index)) = captures.next() {
            let capture = m.captures[*index];
            if let Some(highlight) = grammar.classes[capture.index as usize] {
                found.push((capture.node.byte_range(), m.pattern_index, highlight));
            }
        }
        resolve(found, range)
    }
}

/// Feeds the text predicates (`#match?`, `#any-of?`) from the rope chunks.
struct RopeText<'a>(&'a Rope);

impl<'a> TextProvider<&'a [u8]> for RopeText<'a> {
    type I = std::iter::Map<ropey::iter::Chunks<'a>, fn(&'a str) -> &'a [u8]>;

    fn text(&mut self, node: tree_sitter::Node) -> Self::I {
        let slice = self.0.slice(node.byte_range());
        slice.chunks().map(str::as_bytes)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::Highlight;
    use crate::Highlight::*;
    use pretty_assertions::assert_eq;
    use std::path::Path;

    const MAX: usize = 100 * 1024 * 1024;

    fn spans(language: Language, text: &str) -> Vec<(std::string::String, Highlight)> {
        let buffer = Buffer::from_text(text);
        let tree = SyntaxTree::parse(language, &buffer, None, MAX).expect("tree");
        tree.highlights(&buffer, 0..buffer.len())
            .into_iter()
            .map(|s| (text[s.range].to_string(), s.highlight))
            .collect()
    }

    fn has(language: Language, text: &str, needle: &str, class: Highlight) {
        let got = spans(language, text);
        assert!(
            got.iter().any(|(t, h)| t == needle && *h == class),
            "{needle:?} as {class:?} not in {got:?}"
        );
    }

    #[test]
    fn every_grammar_compiles() {
        for l in [
            Language::Rust,
            Language::JavaScript,
            Language::Json,
            Language::Toml,
            Language::Markdown,
            Language::Xml,
            Language::Html,
            Language::Make,
            Language::Shell,
            Language::Python,
            Language::PowerShell,
            Language::Css,
        ] {
            assert!(grammar(l).is_some(), "{l:?}");
        }
        assert!(grammar(Language::Log).is_none());
    }

    #[test]
    fn detects_languages() {
        let d = |p: &str| Language::detect(Path::new(p));
        assert_eq!(d("src/main.rs"), Language::Rust);
        assert_eq!(d("a/app.MJS"), Language::JavaScript);
        assert_eq!(d("x.json"), Language::Json);
        assert_eq!(d("Cargo.toml"), Language::Toml);
        assert_eq!(d("Cargo.lock"), Language::Toml);
        assert_eq!(d("README.md"), Language::Markdown);
        assert_eq!(d("icon.svg"), Language::Xml);
        assert_eq!(d("index.html"), Language::Html);
        assert_eq!(d("server.log"), Language::Log);
        assert_eq!(d("Makefile"), Language::Make);
        assert_eq!(d("GNUmakefile"), Language::Make);
        assert_eq!(d("rules.mk"), Language::Make);
        assert_eq!(d("notes.txt"), Language::PlainText);
        assert_eq!(d("run.sh"), Language::Shell);
        assert_eq!(d("home/.bashrc"), Language::Shell);
        assert_eq!(d("tool.PY"), Language::Python);
        assert_eq!(d("deploy"), Language::PlainText);
        assert_eq!(d("setup.PS1"), Language::PowerShell);
        assert_eq!(d("Tools.psm1"), Language::PowerShell);
        assert_eq!(d("build.bat"), Language::Batch);
        assert_eq!(d("run.CMD"), Language::Batch);
        assert_eq!(d("site/style.css"), Language::Css);
        assert_eq!(d("repo/.editorconfig"), Language::EditorConfig);
        assert_eq!(d("repo/.gitignore"), Language::GitIgnore);
        assert_eq!(Language::Rust.name(), "Rust");
        let s = Language::from_shebang;
        assert_eq!(s("#!/bin/sh"), Some(Language::Shell));
        assert_eq!(s("#!/usr/bin/env -S bash -e"), Some(Language::Shell));
        assert_eq!(s("#! /usr/bin/python3.12"), Some(Language::Python));
        assert_eq!(s("#!/usr/bin/perl"), None);
        assert_eq!(s("echo hi"), None);
    }

    #[test]
    fn rust_highlights() {
        let src = "// hi\nfn main() { let s = \"x\"; let n = 42; let b = true; Some(n); }\n#[derive(Debug)]\nstruct Foo;";
        has(Language::Rust, src, "// hi", Comment);
        has(Language::Rust, src, "fn", Keyword);
        has(Language::Rust, src, "main", Function);
        has(Language::Rust, src, "\"x\"", String);
        has(Language::Rust, src, "42", Number);
        has(Language::Rust, src, "true", Boolean);
        has(Language::Rust, src, "Some", Constant);
        has(Language::Rust, src, "Foo", Type);
    }

    #[test]
    fn javascript_highlights() {
        let src = "// c\nconst a = \"s\"; function f() { return 1 + true; }";
        has(Language::JavaScript, src, "// c", Comment);
        has(Language::JavaScript, src, "const", Keyword);
        has(Language::JavaScript, src, "\"s\"", String);
        has(Language::JavaScript, src, "1", Number);
        has(Language::JavaScript, src, "true", Boolean);
        has(Language::JavaScript, src, "f", Function);
    }

    #[test]
    fn json_highlights() {
        let src = "{\"key\": \"v\", \"n\": 1.5, \"b\": true, \"z\": null}";
        has(Language::Json, src, "\"key\"", Property);
        has(Language::Json, src, "\"v\"", String);
        has(Language::Json, src, "1.5", Number);
        has(Language::Json, src, "true", Boolean);
        has(Language::Json, src, "null", Constant);
    }

    #[test]
    fn toml_highlights() {
        let src = "# c\n[package]\nname = \"ted\"\nn = 3\nok = true\n";
        has(Language::Toml, src, "# c", Comment);
        has(Language::Toml, src, "package", Keyword);
        has(Language::Toml, src, "name", Property);
        has(Language::Toml, src, "\"ted\"", String);
        has(Language::Toml, src, "3", Number);
        has(Language::Toml, src, "true", Boolean);
    }

    #[test]
    fn markdown_highlights() {
        let src = "# Title\n\n- item\n\n```\ncode\n```\n";
        has(Language::Markdown, src, "#", Keyword);
        has(Language::Markdown, src, "Title", Keyword);
        has(Language::Markdown, src, "- ", Number);
    }

    #[test]
    fn html_and_xml_highlights() {
        let src = "<!-- c --><div class=\"a\">x</div>";
        for l in [Language::Html, Language::Xml] {
            has(l, src, "<!-- c -->", Comment);
            has(l, src, "div", Tag);
            has(l, src, "class", Attribute);
            has(l, src, "\"a\"", String);
        }
    }

    #[test]
    fn make_highlights() {
        let src = "# build\nCC := gcc\n.PHONY: all\nifdef DEBUG\nFLAGS = -g\nendif\nall: $(OBJS)\n\t$(CC) -o $@ $(wildcard *.c)\n";
        has(Language::Make, src, "# build", Comment);
        has(Language::Make, src, "CC", Property);
        has(Language::Make, src, ".PHONY", Constant);
        has(Language::Make, src, "ifdef", Keyword);
        has(Language::Make, src, "endif", Keyword);
        has(Language::Make, src, "all", Function);
        has(Language::Make, src, "OBJS", Property);
        has(Language::Make, src, "$@", Constant);
        has(Language::Make, src, "wildcard", Function);
    }

    #[test]
    fn shell_highlights() {
        let src = "#!/bin/sh\n# setup\nlocal n=3\nif [ -f \"$HOME/x\" ]; then\n  echo $? 'done' >&2\nfi\ngreet() { printf hi; }\n";
        has(Language::Shell, src, "# setup", Comment);
        has(Language::Shell, src, "local", Keyword);
        has(Language::Shell, src, "n", Property);
        has(Language::Shell, src, "if", Keyword);
        has(Language::Shell, src, "-f", Constant);
        has(Language::Shell, src, "echo", Function);
        has(Language::Shell, src, "?", Constant);
        has(Language::Shell, src, "'done'", String);
        has(Language::Shell, src, "fi", Keyword);
        has(Language::Shell, src, "greet", Function);
    }

    #[test]
    fn python_highlights() {
        let src = "# hi\n@cached\ndef area(r: float) -> float:\n    \"\"\"Doc.\"\"\"\n    if r is None or False:\n        return 0\n    return 3.14 * r ** 2\n\nclass Shape:\n    pass\n";
        has(Language::Python, src, "# hi", Comment);
        has(Language::Python, src, "@cached", Attribute);
        has(Language::Python, src, "def", Keyword);
        has(Language::Python, src, "area", Function);
        has(Language::Python, src, "float", Type);
        has(Language::Python, src, "\"\"\"Doc.\"\"\"", String);
        has(Language::Python, src, "None", Constant);
        has(Language::Python, src, "False", Boolean);
        has(Language::Python, src, "3.14", Number);
        has(Language::Python, src, "Shape", Type);
    }

    #[test]
    fn powershell_highlights() {
        let src = "# hi\n<# block #>\nfunction Get-Area([int]$r) {\n    if ($r -eq $null) { return $False }\n    Write-Host \"area\" 3.14\n    $items | ForEach-Object { $_.Name }\n}\n";
        has(Language::PowerShell, src, "# hi", Comment);
        has(Language::PowerShell, src, "<# block #>", Comment);
        has(Language::PowerShell, src, "function", Keyword);
        has(Language::PowerShell, src, "Get-Area", Function);
        has(Language::PowerShell, src, "int", Type);
        has(Language::PowerShell, src, "$r", Property);
        has(Language::PowerShell, src, "if", Keyword);
        has(Language::PowerShell, src, "$null", Constant);
        has(Language::PowerShell, src, "$False", Boolean);
        has(Language::PowerShell, src, "Write-Host", Function);
        has(Language::PowerShell, src, "\"area\"", String);
        has(Language::PowerShell, src, "3.14", Number);
        has(Language::PowerShell, src, "ForEach-Object", Function);
        has(Language::PowerShell, src, "Name", Property);
    }

    #[test]
    fn css_highlights() {
        let src = "/* hi */\n@media screen {\n  .card > p:hover { color: #fff; margin: 4px auto !important; display: block; }\n}\n";
        has(Language::Css, src, "/* hi */", Comment);
        has(Language::Css, src, "@media", Keyword);
        has(Language::Css, src, "card", Property);
        has(Language::Css, src, "p", Tag);
        has(Language::Css, src, "hover", Attribute);
        has(Language::Css, src, "color", Property);
        has(Language::Css, src, "#fff", String);
        has(Language::Css, src, "4", Number);
        has(Language::Css, src, "px", Type);
        has(Language::Css, src, "auto", Constant);
        has(Language::Css, src, "!important", Keyword);
    }

    #[test]
    fn highlights_are_limited_to_the_range() {
        let src = "fn a() {}\nfn b() {}\nfn c() {}\n";
        let buffer = Buffer::from_text(src);
        let tree = SyntaxTree::parse(Language::Rust, &buffer, None, MAX).expect("tree");
        let spans = tree.highlights(&buffer, 10..19);
        assert!(!spans.is_empty());
        for s in &spans {
            assert!(s.range.start >= 10 && s.range.end <= 19);
        }
        assert!(spans.iter().any(|s| &src[s.range.clone()] == "b"));
        assert!(!spans.iter().any(|s| &src[s.range.clone()] == "a"));
        for w in spans.windows(2) {
            assert!(w[0].range.end <= w[1].range.start);
        }
    }

    #[test]
    fn skips_plain_languages_and_big_buffers() {
        let buffer = Buffer::from_text("fn main() {}");
        assert!(SyntaxTree::parse(Language::Log, &buffer, None, MAX).is_none());
        assert!(SyntaxTree::parse(Language::PlainText, &buffer, None, MAX).is_none());
        assert!(SyntaxTree::parse(Language::Rust, &buffer, None, 4).is_none());
    }

    #[test]
    fn stale_tree_gives_nothing() {
        let mut buffer = Buffer::from_text("fn a() {}");
        let tree = SyntaxTree::parse(Language::Rust, &buffer, None, MAX).expect("tree");
        buffer.insert(0, " ");
        assert!(tree.highlights(&buffer, 0..buffer.len()).is_empty());
    }

    #[test]
    fn incremental_equals_full() {
        let mut buffer = Buffer::from_text(
            "fn a() {\n    let x = 1;\n}\n\nfn b() -> &'static str {\n    \"hi\"\n}\n",
        );
        let mut tree = SyntaxTree::parse(Language::Rust, &buffer, None, MAX).expect("tree");
        let edits: [(usize, usize, &str); 5] = [
            (13, 13, "mut "),
            (0, 0, "// head\nuse std::fmt;\n"),
            (30, 35, ""),
            (40, 40, "\n    let y = \"s\";\n"),
            (5, 6, "zz"),
        ];
        for (start, end, text) in edits {
            let end = end.min(buffer.len());
            let start = start.min(end);
            let _ = buffer.track_changes();
            buffer.replace(start..end, text);
            let changes = buffer.take_changes().expect("tracked");
            let edited = tree.edited(&changes, buffer.version());
            let inc = SyntaxTree::parse(Language::Rust, &buffer, Some(&edited), MAX).expect("inc");
            let full = SyntaxTree::parse(Language::Rust, &buffer, None, MAX).expect("full");
            assert_eq!(inc.version(), buffer.version());
            let all = 0..buffer.len();
            assert_eq!(
                inc.highlights(&buffer, all.clone()),
                full.highlights(&buffer, all)
            );
            assert_eq!(
                inc.inner.tree.root_node().to_sexp(),
                full.inner.tree.root_node().to_sexp()
            );
            tree = inc;
        }
    }

    #[test]
    #[ignore = "timing, run with --release -- --ignored --nocapture"]
    fn timing() {
        use std::time::Instant;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench-files");
        let json = std::fs::read_to_string(root.join("pretty.json")).expect("pretty.json");
        let buffer = Buffer::from_text(&json);
        let t = Instant::now();
        let tree = SyntaxTree::parse(Language::Json, &buffer, None, usize::MAX).expect("tree");
        println!(
            "full parse of {} MB: {:?}",
            buffer.len() / 1_000_000,
            t.elapsed()
        );

        let mid = buffer.line_to_byte(buffer.line_count() / 2);
        let end = buffer.line_to_byte(buffer.line_count() / 2 + 60);
        let t = Instant::now();
        let n = (0..100)
            .map(|_| tree.highlights(&buffer, mid..end).len())
            .next_back();
        println!(
            "highlights of 60 lines ({n:?} spans): {:?} avg",
            t.elapsed() / 100
        );

        let log = std::fs::read_to_string(root.join("big.log")).expect("big.log");
        let lines: Vec<&str> = log.lines().take(60).collect();
        let t = Instant::now();
        let mut total = 0;
        for _ in 0..1000 {
            for l in &lines {
                total += crate::highlight_log_line(l).len();
            }
        }
        println!(
            "log highlight of 60 lines ({total} spans/1000): {:?} avg",
            t.elapsed() / 1000
        );
    }
}

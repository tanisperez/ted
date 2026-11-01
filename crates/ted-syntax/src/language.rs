//! The languages ted highlights and how a path picks one.

use std::path::Path;

use crate::HighlightSpan;

/// The comment syntax of a language, for Ctrl+/.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comment {
    /// A token that comments the rest of the line (`//`, `#`).
    Line(&'static str),
    /// A start and an end token around the commented text (`<!--` and `-->`).
    Block(&'static str, &'static str),
}

/// A language of the editor. `Log` and `PlainText` have no tree-sitter grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    JavaScript,
    Json,
    Toml,
    Markdown,
    Xml,
    Html,
    Make,
    Shell,
    Python,
    PowerShell,
    Batch,
    Css,
    EditorConfig,
    GitIgnore,
    Log,
    PlainText,
}

impl Language {
    /// The language of a file, from its name first and then its extension (case insensitive).
    /// Anything unknown is [`Language::PlainText`].
    pub fn detect(path: &Path) -> Language {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return Language::PlainText;
        };
        match name {
            "Cargo.lock" | "Pipfile" | "poetry.lock" | "uv.lock" => return Language::Toml,
            "Makefile" | "makefile" | "GNUmakefile" => return Language::Make,
            ".bashrc" | ".bash_profile" | ".bash_logout" | ".profile" | ".zshrc" | ".zshenv"
            | ".zprofile" | "PKGBUILD" => return Language::Shell,
            ".babelrc" | ".prettierrc" | ".eslintrc" | "composer.lock" => return Language::Json,
            ".editorconfig" => return Language::EditorConfig,
            ".gitignore" => return Language::GitIgnore,
            _ => {}
        }
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            return Language::PlainText;
        };
        match extension.to_ascii_lowercase().as_str() {
            "rs" => Language::Rust,
            "js" | "mjs" | "cjs" | "jsx" => Language::JavaScript,
            "json" | "jsonc" | "geojson" | "webmanifest" => Language::Json,
            "toml" => Language::Toml,
            "md" | "markdown" | "mdown" => Language::Markdown,
            "xml" | "svg" | "xsd" | "xsl" | "xslt" | "plist" | "csproj" | "xaml" | "rss"
            | "atom" => Language::Xml,
            "html" | "htm" | "xhtml" => Language::Html,
            "mk" | "mak" => Language::Make,
            "sh" | "bash" | "zsh" | "ksh" => Language::Shell,
            "py" | "pyw" | "pyi" => Language::Python,
            "ps1" | "psm1" | "psd1" => Language::PowerShell,
            "bat" | "cmd" => Language::Batch,
            "css" => Language::Css,
            "log" => Language::Log,
            _ => Language::PlainText,
        }
    }

    /// The language a script names in its first line (`#!/bin/sh`, `#!/usr/bin/env python3`),
    /// for a file whose name says nothing. Reads only that line.
    pub fn from_shebang(first_line: &str) -> Option<Language> {
        let command = first_line.strip_prefix("#!")?.trim();
        let mut words = command.split_whitespace();
        let mut program = words.next()?.rsplit('/').next()?;
        if program == "env" {
            program = words.find(|word| !word.starts_with('-'))?;
        }
        let name = program.trim_end_matches(|ch: char| ch.is_ascii_digit() || ch == '.');
        match name {
            "sh" | "bash" | "zsh" | "ksh" | "dash" | "ash" => Some(Language::Shell),
            "python" => Some(Language::Python),
            _ => None,
        }
    }

    /// Whether a tree-sitter grammar highlights it (not logs, the languages of
    /// [`Language::line_highlighter`] nor plain text).
    pub fn has_grammar(self) -> bool {
        !matches!(self, Language::Log | Language::PlainText) && self.line_highlighter().is_none()
    }

    /// The highlighter of a language colored one line at a time, with no tree: batch files and
    /// the line based config files. Logs have their own, bounded to the start of the line.
    pub fn line_highlighter(self) -> Option<fn(&str) -> Vec<HighlightSpan>> {
        match self {
            Language::Batch => Some(crate::highlight_batch_line),
            Language::EditorConfig => Some(crate::highlight_editorconfig_line),
            Language::GitIgnore => Some(crate::highlight_gitignore_line),
            _ => None,
        }
    }

    /// How Ctrl+/ comments a line out, `None` for the languages without comments (JSON, logs,
    /// plain text).
    pub fn comment(self) -> Option<Comment> {
        match self {
            Language::Rust | Language::JavaScript => Some(Comment::Line("//")),
            Language::Toml
            | Language::Make
            | Language::Shell
            | Language::Python
            | Language::PowerShell
            | Language::EditorConfig
            | Language::GitIgnore => Some(Comment::Line("#")),
            Language::Batch => Some(Comment::Line("REM")),
            Language::Markdown | Language::Xml | Language::Html => {
                Some(Comment::Block("<!--", "-->"))
            }
            Language::Css => Some(Comment::Block("/*", "*/")),
            Language::Json | Language::Log | Language::PlainText => None,
        }
    }

    /// The label of the status bar.
    pub fn name(self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::JavaScript => "JavaScript",
            Language::Json => "JSON",
            Language::Toml => "TOML",
            Language::Markdown => "Markdown",
            Language::Xml => "XML",
            Language::Html => "HTML",
            Language::Make => "Makefile",
            Language::Shell => "Shell",
            Language::Python => "Python",
            Language::PowerShell => "PowerShell",
            Language::Batch => "Batch",
            Language::Css => "CSS",
            Language::EditorConfig => "EditorConfig",
            Language::GitIgnore => "Gitignore",
            Language::Log => "Log",
            Language::PlainText => "Plain text",
        }
    }
}

//! The languages ted highlights and how a path picks one.

use std::path::Path;

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
            ".babelrc" | ".prettierrc" | ".eslintrc" | "composer.lock" => return Language::Json,
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
            "log" => Language::Log,
            _ => Language::PlainText,
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
            Language::Log => "Log",
            Language::PlainText => "Plain text",
        }
    }
}

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
    Yaml,
    Dockerfile,
    Java,
    Kotlin,
    TypeScript,
    Tsx,
    Sql,
    Properties,
    Env,
    EditorConfig,
    GitIgnore,
    Log,
    PlainText,
}

impl Language {
    /// Every language, in the order the language menu of the status bar lists them: plain text
    /// first, then by name.
    pub const ALL: [Language; 26] = [
        Language::PlainText,
        Language::Batch,
        Language::Css,
        Language::Dockerfile,
        Language::EditorConfig,
        Language::Env,
        Language::GitIgnore,
        Language::Html,
        Language::Java,
        Language::JavaScript,
        Language::Json,
        Language::Kotlin,
        Language::Log,
        Language::Make,
        Language::Markdown,
        Language::PowerShell,
        Language::Properties,
        Language::Python,
        Language::Rust,
        Language::Shell,
        Language::Sql,
        Language::Toml,
        Language::Tsx,
        Language::TypeScript,
        Language::Xml,
        Language::Yaml,
    ];

    /// The language of a file, from its name first and then its extension (case insensitive).
    /// Anything unknown is [`Language::PlainText`].
    pub fn detect(path: &Path) -> Language {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return Language::PlainText;
        };
        match name {
            "Cargo.lock" | "Pipfile" | "poetry.lock" | "uv.lock" => return Language::Toml,
            "Makefile" | "makefile" | "GNUmakefile" => return Language::Make,
            "PKGBUILD" => return Language::Shell,
            ".babelrc" | ".prettierrc" | ".eslintrc" | "composer.lock" => return Language::Json,
            ".editorconfig" => return Language::EditorConfig,
            // The other ignore files use the `.gitignore` syntax.
            ".gitignore" | ".dockerignore" | ".npmignore" | ".prettierignore" | ".eslintignore"
            | ".ignore" => return Language::GitIgnore,
            _ => {}
        }
        if is_env_name(name) {
            return Language::Env;
        }
        if is_shell_config_name(name) {
            return Language::Shell;
        }
        if is_dockerfile_name(name) {
            return Language::Dockerfile;
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
            "yaml" | "yml" => Language::Yaml,
            "java" => Language::Java,
            "kt" | "kts" => Language::Kotlin,
            "ts" | "mts" | "cts" => Language::TypeScript,
            "tsx" => Language::Tsx,
            "sql" => Language::Sql,
            "properties" => Language::Properties,
            "dockerfile" | "containerfile" => Language::Dockerfile,
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
            Language::Properties => Some(crate::highlight_properties_line),
            Language::Env => Some(crate::highlight_env_line),
            _ => None,
        }
    }

    /// How Ctrl+/ comments a line out, `None` for the languages without comments (JSON, logs,
    /// plain text).
    pub fn comment(self) -> Option<Comment> {
        match self {
            Language::Rust
            | Language::JavaScript
            | Language::Java
            | Language::Kotlin
            | Language::TypeScript
            | Language::Tsx => Some(Comment::Line("//")),
            Language::Sql => Some(Comment::Line("--")),
            Language::Toml
            | Language::Make
            | Language::Shell
            | Language::Python
            | Language::PowerShell
            | Language::Yaml
            | Language::Dockerfile
            | Language::EditorConfig
            | Language::GitIgnore
            | Language::Properties
            | Language::Env => Some(Comment::Line("#")),
            Language::Batch => Some(Comment::Line("REM")),
            Language::Markdown | Language::Xml | Language::Html => {
                Some(Comment::Block("<!--", "-->"))
            }
            Language::Css => Some(Comment::Block("/*", "*/")),
            Language::Json | Language::Log | Language::PlainText => None,
        }
    }

    /// The usual extensions (`.rs`) or file names (`Dockerfile`) of a language, as the language
    /// menu shows them. A short list, not every one [`Language::detect`] knows.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Language::Rust => &[".rs"],
            Language::JavaScript => &[".js", ".mjs", ".cjs", ".jsx"],
            Language::Json => &[".json"],
            Language::Toml => &[".toml"],
            Language::Markdown => &[".md"],
            Language::Xml => &[".xml", ".svg"],
            Language::Html => &[".html", ".htm"],
            Language::Make => &["Makefile", ".mk"],
            Language::Shell => &[".sh", ".bash", ".zsh"],
            Language::Python => &[".py"],
            Language::PowerShell => &[".ps1"],
            Language::Batch => &[".bat", ".cmd"],
            Language::Css => &[".css"],
            Language::Yaml => &[".yml", ".yaml"],
            Language::Dockerfile => &["Dockerfile"],
            Language::Java => &[".java"],
            Language::Kotlin => &[".kt", ".kts"],
            Language::TypeScript => &[".ts", ".mts", ".cts"],
            Language::Tsx => &[".tsx"],
            Language::Sql => &[".sql"],
            Language::Properties => &[".properties"],
            Language::Env => &[".env", ".env.local"],
            Language::EditorConfig => &[".editorconfig"],
            Language::GitIgnore => &[".gitignore"],
            Language::Log => &[".log"],
            Language::PlainText => &[".txt"],
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
            Language::Yaml => "YAML",
            Language::Dockerfile => "Dockerfile",
            Language::Java => "Java",
            Language::Kotlin => "Kotlin",
            Language::TypeScript => "TypeScript",
            Language::Tsx => "TSX",
            Language::Sql => "SQL",
            Language::Properties => "Properties",
            Language::Env => "Env",
            Language::EditorConfig => "EditorConfig",
            Language::GitIgnore => "Gitignore",
            Language::Log => "Log",
            Language::PlainText => "Plain text",
        }
    }
}

/// The startup files of the shells, with their dot (`.zshrc`) or without it, as a dotfiles repository
/// keeps them (`zshrc`).
fn is_shell_config_name(name: &str) -> bool {
    matches!(
        name.strip_prefix('.').unwrap_or(name),
        "bashrc"
            | "bash_profile"
            | "bash_logout"
            | "bash_aliases"
            | "zshrc"
            | "zshenv"
            | "zprofile"
            | "zlogin"
            | "zlogout"
    ) || name == ".profile"
}

/// An env file: `.env`, `.env.local` and the other `.env.<variant>`, or `<name>.env`.
fn is_env_name(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.") || (name.ends_with(".env") && name.len() > 4)
}

/// `Dockerfile` or `Containerfile`, alone or followed by a variant (`Dockerfile.dev`), in any case.
fn is_dockerfile_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    stem.eq_ignore_ascii_case("dockerfile") || stem.eq_ignore_ascii_case("containerfile")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_extension_is_detected_as_its_language() {
        for language in Language::ALL {
            assert!(!language.extensions().is_empty(), "{language:?}");
            for listed in language.extensions() {
                // `.rs` is an extension, `.gitignore` and `Dockerfile` whole names.
                let as_name = Language::detect(Path::new(listed));
                let as_extension = Language::detect(Path::new(&format!("file{listed}")));
                assert!(
                    as_name == language || as_extension == language,
                    "{listed} is not detected as {language:?}"
                );
            }
        }
    }

    #[test]
    fn all_lists_every_language_once() {
        let mut names: Vec<&str> = Language::ALL
            .iter()
            .map(|language| language.name())
            .collect();
        assert_eq!(names[0], "Plain text");
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Language::ALL.len());
        // A new variant must be added to ALL too: this match stops compiling until it is
        // handled here, which is the reminder.
        for language in Language::ALL {
            match language {
                Language::Rust
                | Language::JavaScript
                | Language::Json
                | Language::Toml
                | Language::Markdown
                | Language::Xml
                | Language::Html
                | Language::Make
                | Language::Shell
                | Language::Python
                | Language::PowerShell
                | Language::Batch
                | Language::Css
                | Language::Yaml
                | Language::Dockerfile
                | Language::Java
                | Language::Kotlin
                | Language::TypeScript
                | Language::Tsx
                | Language::Sql
                | Language::Properties
                | Language::Env
                | Language::EditorConfig
                | Language::GitIgnore
                | Language::Log
                | Language::PlainText => {}
            }
        }
    }
}

//! The tree-sitter grammars and their highlight queries, built once per process.
//!
//! The queries are the ones bundled with each grammar crate (MIT), followed by a few patterns of
//! ted's own that fill what they leave out (numbers and booleans in Rust, the pieces of a
//! TOML file, markdown markers). A later pattern wins over an earlier one for the same range.

use std::sync::OnceLock;

use tree_sitter::Query;

use crate::{Highlight, Language};

/// A grammar with its compiled query and the class of every capture of that query.
pub(crate) struct Grammar {
    pub language: tree_sitter::Language,
    pub query: Query,
    pub classes: Vec<Option<Highlight>>,
}

const RUST_EXTRA: &str = r#"
(integer_literal) @number
(float_literal) @number
(boolean_literal) @boolean
(string_literal) @string
(raw_string_literal) @string
(char_literal) @string
((identifier) @constant (#any-of? @constant "None" "Some" "Ok" "Err"))
"#;

const JAVASCRIPT_EXTRA: &str = r#"
[(true) (false)] @boolean
"#;

const JSON_EXTRA: &str = r#"
(pair key: (string) @string.special.key)
[(true) (false)] @boolean
"#;

// The bundled TOML query paints the whole `key = value` pair as a property and the table names as
// types, so ted uses its own.
const TOML_QUERY: &str = r#"
(comment) @comment
(string) @string
(integer) @number
(float) @number
(boolean) @boolean
[(offset_date_time) (local_date_time) (local_date) (local_time)] @string
(pair (bare_key) @property)
(pair (quoted_key) @property)
(pair (dotted_key (bare_key) @property))
(table [(bare_key) (quoted_key) (dotted_key)] @keyword)
(table_array_element [(bare_key) (quoted_key) (dotted_key)] @keyword)
"#;

const MARKDOWN_EXTRA: &str = r#"
[
  (atx_h1_marker) (atx_h2_marker) (atx_h3_marker)
  (atx_h4_marker) (atx_h5_marker) (atx_h6_marker)
  (setext_h1_underline) (setext_h2_underline)
] @keyword
[
  (list_marker_plus) (list_marker_minus) (list_marker_star)
  (list_marker_dot) (list_marker_parenthesis) (thematic_break)
] @number
(block_quote_marker) @comment
(fenced_code_block_delimiter) @string
"#;

const HTML_EXTRA: &str = r#"
(doctype) @keyword
(quoted_attribute_value) @string
(entity) @constant
(attribute_name) @attribute
"#;

/// The grammar of a language, `None` for those without one (or if its query failed to compile,
/// which the tests rule out).
pub(crate) fn grammar(language: Language) -> Option<&'static Grammar> {
    static RUST: OnceLock<Option<Grammar>> = OnceLock::new();
    static JAVASCRIPT: OnceLock<Option<Grammar>> = OnceLock::new();
    static JSON: OnceLock<Option<Grammar>> = OnceLock::new();
    static TOML: OnceLock<Option<Grammar>> = OnceLock::new();
    static MARKDOWN: OnceLock<Option<Grammar>> = OnceLock::new();
    static HTML: OnceLock<Option<Grammar>> = OnceLock::new();

    let (cell, ts_language, query) = match language {
        Language::Rust => (
            &RUST,
            tree_sitter_rust::LANGUAGE,
            format!("{}\n{RUST_EXTRA}", tree_sitter_rust::HIGHLIGHTS_QUERY),
        ),
        Language::JavaScript => (
            &JAVASCRIPT,
            tree_sitter_javascript::LANGUAGE,
            format!(
                "{}\n{JAVASCRIPT_EXTRA}",
                tree_sitter_javascript::HIGHLIGHT_QUERY
            ),
        ),
        Language::Json => (
            &JSON,
            tree_sitter_json::LANGUAGE,
            format!("{}\n{JSON_EXTRA}", tree_sitter_json::HIGHLIGHTS_QUERY),
        ),
        Language::Toml => (&TOML, tree_sitter_toml_ng::LANGUAGE, TOML_QUERY.to_string()),
        Language::Markdown => (
            &MARKDOWN,
            tree_sitter_md::LANGUAGE,
            format!(
                "{}\n{MARKDOWN_EXTRA}",
                tree_sitter_md::HIGHLIGHT_QUERY_BLOCK
            ),
        ),
        // XML has no grammar in the versions gpui-component pins, so it uses HTML's, as Postino.
        Language::Html | Language::Xml => (
            &HTML,
            tree_sitter_html::LANGUAGE,
            format!("{}\n{HTML_EXTRA}", tree_sitter_html::HIGHLIGHTS_QUERY),
        ),
        Language::Log | Language::PlainText => return None,
    };

    cell.get_or_init(|| {
        let language: tree_sitter::Language = ts_language.into();
        let query = Query::new(&language, &query).ok()?;
        let classes = query
            .capture_names()
            .iter()
            .map(|name| Highlight::from_capture(name))
            .collect();
        Some(Grammar {
            language,
            query,
            classes,
        })
    })
    .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_grammar_matches_the_grammars() {
        use Language::*;
        for language in [
            Rust, JavaScript, Json, Toml, Markdown, Xml, Html, Log, PlainText,
        ] {
            assert_eq!(
                language.has_grammar(),
                grammar(language).is_some(),
                "{language:?}"
            );
        }
    }
}

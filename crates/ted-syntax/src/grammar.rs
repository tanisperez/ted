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
["if" "else" "match" "for" "while" "loop" "in" "break" "continue" "return" "await"] @keyword.control
"#;

// The bundled rust query has no general pattern for identifiers. It goes before the bundled query,
// so every more specific pattern wins.
const RUST_FIRST: &str = "(identifier) @variable";

const JAVASCRIPT_EXTRA: &str = r#"
[(true) (false)] @boolean
[
  "if" "else" "switch" "case" "default" "for" "while" "do" "break" "continue" "return"
  "try" "catch" "finally" "throw" "await" "import" "from"
] @keyword.control
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

// The bundled make query paints every recipe line as a string and uses captures ted has no class
// for (`@conditional`, `@include`, `@repeat`), so ted uses its own: directives as keywords, rule
// targets and make functions as functions, variables as properties, automatic variables and the
// special targets (`.PHONY`) as constants. Recipes stay plain: they are shell.
const MAKE_QUERY: &str = r#"
(comment) @comment
[
  "ifeq" "ifneq" "ifdef" "ifndef" "else" "endif"
  "define" "endef" "include" "sinclude" "-include"
  "export" "unexport" "override" "private" "undefine" "vpath"
] @keyword
(rule (targets (word) @function))
((targets (word) @constant)
  (#match? @constant "^\.[A-Z_]+$"))
(variable_assignment name: (word) @property)
(variable_reference (word) @property)
(automatic_variable) @constant
(function_call function: _ @function)
(shell_function function: _ @function)
(string) @string
"#;

// The bundled bash query leaves out the declaration keywords, numbers, the special variables
// (`$?`, `$@`) and the test operators (`-f`, `-eq`).
const BASH_EXTRA: &str = r#"
["declare" "local" "readonly" "typeset"] @keyword
(number) @number
(special_variable_name) @constant
(test_operator) @constant
[(ansi_c_string) (translated_string)] @string
["if" "then" "else" "elif" "fi" "case" "esac" "for" "while" "in" "do" "done"] @keyword.control
"#;

// The bundled python query paints `True`, `False` and `None` as builtin constants: the booleans
// get their own class, and decorators are attributes.
const PYTHON_EXTRA: &str = r#"
[(true) (false)] @boolean
(decorator) @attribute
(decorator (identifier) @attribute)
[
  "if" "elif" "else" "for" "while" "in" "break" "continue" "return" "try" "except" "finally"
  "raise" "with" "match" "case" "await" "import" "from"
] @keyword.control
"#;

// The bundled powershell query captures variables as `@variable`, which has no class, and
// paints `$true`, `$false` and `$null` as any other variable.
const POWERSHELL_EXTRA: &str = r#"
(variable) @property
((variable) @boolean (#match? @boolean "^\\$(?i:true|false)$"))
((variable) @constant (#match? @constant "^\\$(?i:null)$"))
[
  "if" "elseif" "else" "switch" "foreach" "for" "while" "do" "break" "continue" "throw" "return"
  "try" "catch" "finally"
] @keyword.control
"#;

// The bundled css query leaves the plain values of a declaration (`block`, `red`) uncolored,
// and its later `class_name` pattern paints pseudo classes (`:hover`) as classes.
const CSS_EXTRA: &str = r#"
(declaration (plain_value) @constant)
(pseudo_class_selector (class_name) @attribute)
"#;

// The bundled yaml query captures anchors and aliases as `@label`, which has no class.
const YAML_EXTRA: &str = r#"
[(anchor_name) (alias_name)] @constant
"#;

// The bundled query paints only the variables in capitals, and captures the markers of a heredoc as
// `@label`, which has no class. The commands of `RUN` stay plain, as the recipes of a Makefile.
const DOCKERFILE_EXTRA: &str = r#"
(variable) @constant
[(heredoc_marker) (heredoc_end)] @keyword
"#;

// The bundled java query paints `true`, `false` and `null` alike, and `super` as a function.
const JAVA_EXTRA: &str = r#"
[(true) (false)] @boolean
(super) @variable.builtin
(record_declaration name: (identifier) @type)
(annotation_type_declaration name: (identifier) @type)
(field_declaration declarator: (variable_declarator name: (identifier) @property))
(field_access field: (identifier) @property)
((identifier) @constant (#match? @constant "^_*[A-Z][A-Z\\d_]+$"))
[
  "if" "else" "switch" "case" "default" "for" "while" "do" "break" "continue" "return" "yield"
  "try" "catch" "finally" "throw" "import"
] @keyword.control
"#;

// The bundled kotlin query uses captures ted has no class for (`@conditional`, `@repeat`, `@float`)
// and paints whole nodes (`return x`, the primary constructor) in one color, so ted uses its own.
const KOTLIN_QUERY: &str = r#"
(simple_identifier) @variable
[(this_expression) (super_expression)] @variable.builtin
(class_parameter (simple_identifier) @property)
(class_body (property_declaration (variable_declaration (simple_identifier) @property)))
(navigation_suffix (simple_identifier) @property)
(enum_entry (simple_identifier) @constant)
(type_identifier) @type
(function_declaration . (simple_identifier) @function)
(call_expression . (simple_identifier) @function)
(call_expression (navigation_expression (navigation_suffix (simple_identifier) @function) .))
[(line_comment) (multiline_comment) (shebang_line)] @comment
[
  (integer_literal) (long_literal) (hex_literal) (bin_literal) (unsigned_literal) (real_literal)
] @number
(boolean_literal) @boolean
(null_literal) @constant
[(character_literal) (string_literal)] @string
(character_escape_seq) @string.escape
(interpolated_identifier) @property
[
  (class_modifier) (member_modifier) (function_modifier) (property_modifier) (platform_modifier)
  (variance_modifier) (parameter_modifier) (visibility_modifier) (reification_modifier)
  (inheritance_modifier)
] @keyword
[
  "val" "var" "fun" "class" "object" "interface" "enum" "typealias" "package" "constructor"
  "init" "get" "set" "is" "as" "as?"
] @keyword
["if" "else" "when" "for" "while" "do" "try" "catch" "finally" "in" "import"] @keyword.control
(jump_expression ["return" "return@" "break" "break@" "continue" "continue@" "throw"] @keyword.control)
(annotation "@" @attribute)
(annotation (user_type (type_identifier) @attribute))
(annotation (constructor_invocation (user_type (type_identifier) @attribute)))
"#;

// TypeScript reuses the javascript query, as the grammar extends it, then its own. The bundled
// typescript query paints every capitalized identifier as a type, so `MAX_SIZE` is a constant again.
const TYPESCRIPT_EXTRA: &str = r#"
((identifier) @constant (#match? @constant "^_*[A-Z][A-Z\\d_]+$"))
(type_identifier) @type
"#;

// The bundled SQL query uses captures ted has no class for (`@conditional`, `@float`, `@field`,
// `@parameter`) and its number patterns never match, so these take their place.
const SQL_EXTRA: &str = r#"
[(keyword_case) (keyword_when) (keyword_then) (keyword_else)] @keyword.control
(field name: (identifier) @property)
(parameter) @variable
((literal) @number (#match? @number "^[-+]?[0-9]+(\\.[0-9]*)?$"))
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
    static MAKE: OnceLock<Option<Grammar>> = OnceLock::new();
    static BASH: OnceLock<Option<Grammar>> = OnceLock::new();
    static PYTHON: OnceLock<Option<Grammar>> = OnceLock::new();
    static POWERSHELL: OnceLock<Option<Grammar>> = OnceLock::new();
    static CSS: OnceLock<Option<Grammar>> = OnceLock::new();
    static YAML: OnceLock<Option<Grammar>> = OnceLock::new();
    static DOCKERFILE: OnceLock<Option<Grammar>> = OnceLock::new();
    static JAVA: OnceLock<Option<Grammar>> = OnceLock::new();
    static KOTLIN: OnceLock<Option<Grammar>> = OnceLock::new();
    static TYPESCRIPT: OnceLock<Option<Grammar>> = OnceLock::new();
    static TSX: OnceLock<Option<Grammar>> = OnceLock::new();
    static SQL: OnceLock<Option<Grammar>> = OnceLock::new();

    let (cell, ts_language, query) = match language {
        Language::Rust => (
            &RUST,
            tree_sitter_rust::LANGUAGE,
            format!(
                "{RUST_FIRST}\n{}\n{RUST_EXTRA}",
                tree_sitter_rust::HIGHLIGHTS_QUERY
            ),
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
        Language::Make => (&MAKE, tree_sitter_make::LANGUAGE, MAKE_QUERY.to_string()),
        Language::Shell => (
            &BASH,
            tree_sitter_bash::LANGUAGE,
            format!("{}\n{BASH_EXTRA}", tree_sitter_bash::HIGHLIGHT_QUERY),
        ),
        Language::Python => (
            &PYTHON,
            tree_sitter_python::LANGUAGE,
            format!("{}\n{PYTHON_EXTRA}", tree_sitter_python::HIGHLIGHTS_QUERY),
        ),
        Language::PowerShell => (
            &POWERSHELL,
            tree_sitter_powershell::LANGUAGE,
            format!(
                "{}\n{POWERSHELL_EXTRA}",
                tree_sitter_powershell::HIGHLIGHTS_QUERY
            ),
        ),
        Language::Css => (
            &CSS,
            tree_sitter_css::LANGUAGE,
            format!("{}\n{CSS_EXTRA}", tree_sitter_css::HIGHLIGHTS_QUERY),
        ),
        Language::Yaml => (
            &YAML,
            tree_sitter_yaml::LANGUAGE,
            format!("{}\n{YAML_EXTRA}", tree_sitter_yaml::HIGHLIGHTS_QUERY),
        ),
        Language::Dockerfile => (
            &DOCKERFILE,
            tree_sitter_containerfile::LANGUAGE,
            format!(
                "{}\n{DOCKERFILE_EXTRA}",
                tree_sitter_containerfile::HIGHLIGHTS_QUERY
            ),
        ),
        Language::Java => (
            &JAVA,
            tree_sitter_java::LANGUAGE,
            format!("{}\n{JAVA_EXTRA}", tree_sitter_java::HIGHLIGHTS_QUERY),
        ),
        Language::Kotlin => (
            &KOTLIN,
            tree_sitter_kotlin_sg::LANGUAGE,
            KOTLIN_QUERY.to_string(),
        ),
        Language::TypeScript => (
            &TYPESCRIPT,
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            format!(
                "{}\n{}\n{JAVASCRIPT_EXTRA}\n{TYPESCRIPT_EXTRA}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            ),
        ),
        Language::Tsx => (
            &TSX,
            tree_sitter_typescript::LANGUAGE_TSX,
            format!(
                "{}\n{}\n{}\n{JAVASCRIPT_EXTRA}\n{TYPESCRIPT_EXTRA}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            ),
        ),
        Language::Sql => (
            &SQL,
            tree_sitter_sequel::LANGUAGE,
            format!("{}\n{SQL_EXTRA}", tree_sitter_sequel::HIGHLIGHTS_QUERY),
        ),
        Language::Log
        | Language::Batch
        | Language::EditorConfig
        | Language::GitIgnore
        | Language::Properties
        | Language::Env
        | Language::PlainText => return None,
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
        ] {
            assert_eq!(
                language.has_grammar(),
                grammar(language).is_some(),
                "{language:?}"
            );
        }
    }
}

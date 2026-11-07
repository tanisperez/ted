//! The highlighters of Java `.properties` files and of env files (`.env`): no tree-sitter, one
//! line at a time, as the `.editorconfig` one. Both are `key = value` lines with `#` comments.

use crate::{Highlight, HighlightSpan};

/// The spans of one line of a `.properties` file (its text without the line ending), relative to
/// the line: `#` and `!` comments, the key and the value. The key ends at the first `=`, `:` or
/// blank that is not escaped with `\`. A line that continues the value of the line above (after a
/// trailing `\`) is read as a key, since each line is colored on its own. Linear in the length of
/// the line.
pub fn highlight_properties_line(line: &str) -> Vec<HighlightSpan> {
    let start = line.len() - line.trim_start().len();
    let bytes = line.as_bytes();
    if matches!(bytes.get(start), Some(b'#' | b'!')) {
        return vec![span(start..line.len(), Highlight::Comment)];
    }
    if start == line.len() {
        return Vec::new();
    }
    let mut key_end = start;
    while key_end < bytes.len() {
        match bytes[key_end] {
            b'\\' => key_end += 2,
            b'=' | b':' | b' ' | b'\t' => break,
            _ => key_end += 1,
        }
    }
    let key_end = key_end.min(line.len());
    let mut spans = vec![span(start..key_end, Highlight::Property)];
    // The separator: blanks, then at most one `=` or `:`, then blanks.
    let rest = &line[key_end..];
    let rest = rest.trim_start_matches([' ', '\t']);
    let rest = rest
        .strip_prefix(['=', ':'])
        .unwrap_or(rest)
        .trim_start_matches([' ', '\t']);
    let value_start = line.len() - rest.len();
    let value = rest.trim_end();
    if !value.is_empty() {
        spans.push(span(
            value_start..value_start + value.len(),
            value_class(value),
        ));
    }
    spans
}

/// The spans of one line of an env file (`.env`, `.env.local`), relative to the line: `#`
/// comments, `export`, the key, and the value, quoted or not. An unquoted value ends at a ` #`
/// comment. Linear in the length of the line.
pub fn highlight_env_line(line: &str) -> Vec<HighlightSpan> {
    let start = line.len() - line.trim_start().len();
    let rest = &line[start..];
    if rest.starts_with('#') {
        return vec![span(start..line.len(), Highlight::Comment)];
    }
    let mut spans = Vec::new();
    let mut key_start = start;
    if let Some(after) = rest.strip_prefix("export")
        && after.starts_with([' ', '\t'])
    {
        spans.push(span(start..start + "export".len(), Highlight::Keyword));
        key_start = line.len() - after.trim_start().len();
    }
    let Some(equals) = line[key_start..].find('=').map(|at| key_start + at) else {
        return spans;
    };
    let key_end = key_start + line[key_start..equals].trim_end().len();
    if key_end > key_start {
        spans.push(span(key_start..key_end, Highlight::Property));
    }
    let value = &line[equals + 1..];
    let value_start = line.len() - value.trim_start().len();
    let value = value.trim_start();
    if let Some(quote) = value.chars().next().filter(|c| *c == '"' || *c == '\'') {
        // Up to the closing quote, or to the end of the line if it never closes.
        let end = value[1..]
            .find(quote)
            .map(|at| 1 + at + 1)
            .unwrap_or(value.trim_end().len());
        spans.push(span(value_start..value_start + end, Highlight::String));
        if let Some(comment) = value[end..].find('#') {
            let at = value_start + end + comment;
            spans.push(span(at..line.len(), Highlight::Comment));
        }
        return spans;
    }
    let (text, comment) = match value.find(" #") {
        Some(at) => (&value[..at], Some(value_start + at + 1)),
        None => (value, None),
    };
    let text = text.trim_end();
    if !text.is_empty() {
        spans.push(span(
            value_start..value_start + text.len(),
            value_class(text),
        ));
    }
    if let Some(at) = comment {
        spans.push(span(at..line.len(), Highlight::Comment));
    }
    spans
}

/// `true` and `false` are booleans, plain decimal numbers numbers, anything else a string.
fn value_class(value: &str) -> Highlight {
    let digits = value.strip_prefix(['-', '+']).unwrap_or(value);
    if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false") {
        Highlight::Boolean
    } else if !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && digits.bytes().filter(|&b| b == b'.').count() <= 1
        && digits.bytes().any(|b| b.is_ascii_digit())
    {
        Highlight::Number
    } else {
        Highlight::String
    }
}

fn span(range: std::ops::Range<usize>, highlight: Highlight) -> HighlightSpan {
    HighlightSpan { range, highlight }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Highlight::*;
    use pretty_assertions::assert_eq;

    fn properties(line: &str) -> Vec<(&str, Highlight)> {
        highlight_properties_line(line)
            .into_iter()
            .map(|span| (&line[span.range], span.highlight))
            .collect()
    }

    fn env(line: &str) -> Vec<(&str, Highlight)> {
        highlight_env_line(line)
            .into_iter()
            .map(|span| (&line[span.range], span.highlight))
            .collect()
    }

    #[test]
    fn properties_comments_and_blank_lines() {
        assert_eq!(properties("# Gradle"), vec![("# Gradle", Comment)]);
        assert_eq!(properties("  ! old style"), vec![("! old style", Comment)]);
        assert_eq!(properties(""), vec![]);
        assert_eq!(properties("   "), vec![]);
    }

    #[test]
    fn properties_keys_and_values() {
        assert_eq!(
            properties("org.gradle.jvmargs=-Xmx2048m -XX:MaxMetaspaceSize=512m"),
            vec![
                ("org.gradle.jvmargs", Property),
                ("-Xmx2048m -XX:MaxMetaspaceSize=512m", String)
            ]
        );
        assert_eq!(
            properties("android.useAndroidX = true"),
            vec![("android.useAndroidX", Property), ("true", Boolean)]
        );
        assert_eq!(
            properties("retries: 3"),
            vec![("retries", Property), ("3", Number)]
        );
        // A blank also ends the key.
        assert_eq!(
            properties("name value"),
            vec![("name", Property), ("value", String)]
        );
        // An escaped separator is part of the key.
        assert_eq!(
            properties("a\\=b=c"),
            vec![("a\\=b", Property), ("c", String)]
        );
        assert_eq!(properties("lonely"), vec![("lonely", Property)]);
    }

    #[test]
    fn env_comments_and_export() {
        assert_eq!(env("# secrets"), vec![("# secrets", Comment)]);
        assert_eq!(
            env("export API_URL=https://x.dev"),
            vec![
                ("export", Keyword),
                ("API_URL", Property),
                ("https://x.dev", String)
            ]
        );
        // `exported` is a key, not the keyword.
        assert_eq!(
            env("exported=1"),
            vec![("exported", Property), ("1", Number)]
        );
        assert_eq!(env("no pair"), vec![]);
    }

    #[test]
    fn env_values() {
        assert_eq!(
            env("NAME=\"two words\" # note"),
            vec![
                ("NAME", Property),
                ("\"two words\"", String),
                ("# note", Comment)
            ]
        );
        assert_eq!(
            env("TOKEN='a#b'"),
            vec![("TOKEN", Property), ("'a#b'", String)]
        );
        assert_eq!(
            env("PORT = 8080 # dev"),
            vec![("PORT", Property), ("8080", Number), ("# dev", Comment)]
        );
        assert_eq!(
            env("DEBUG=false"),
            vec![("DEBUG", Property), ("false", Boolean)]
        );
        // A `#` inside an unquoted value is part of it.
        assert_eq!(
            env("COLOR=#fff"),
            vec![("COLOR", Property), ("#fff", String)]
        );
        assert_eq!(env("EMPTY="), vec![("EMPTY", Property)]);
        assert_eq!(
            env("OPEN=\"never closed"),
            vec![("OPEN", Property), ("\"never closed", String)]
        );
    }
}

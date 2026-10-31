//! The `.editorconfig` highlighter: no tree-sitter, one line at a time, as the batch one. The
//! format is INI, which has no grammar in gpui-component.

use crate::{Highlight, HighlightSpan};

/// The spans of one line of an `.editorconfig` (its text without the line ending), relative to
/// the line: `#` and `;` comments, `[section]` headers, and `key = value` pairs. Comments only
/// start a line, as the format says. Linear in the length of the line.
pub fn highlight_editorconfig_line(line: &str) -> Vec<HighlightSpan> {
    let start = line.len() - line.trim_start().len();
    let rest = line.trim();
    let end = start + rest.len();
    if rest.starts_with('#') || rest.starts_with(';') {
        return vec![span(start..line.len(), Highlight::Comment)];
    }
    if rest.starts_with('[') {
        return vec![span(start..end, Highlight::Keyword)];
    }
    let Some(equals) = line.find('=') else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    let key = line[..equals].trim_end();
    if key.len() > start {
        spans.push(span(start..key.len(), Highlight::Property));
    }
    let value = line[equals + 1..].trim_start();
    let value_start = line.len() - value.len();
    let value = value.trim_end();
    if !value.is_empty() {
        let highlight = if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false")
        {
            Highlight::Boolean
        } else if value.bytes().all(|b| b.is_ascii_digit()) {
            Highlight::Number
        } else {
            Highlight::Constant
        };
        spans.push(span(value_start..value_start + value.len(), highlight));
    }
    spans
}

fn span(range: std::ops::Range<usize>, highlight: Highlight) -> HighlightSpan {
    HighlightSpan { range, highlight }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Highlight::*;
    use pretty_assertions::assert_eq;

    fn spans(line: &str) -> Vec<(&str, Highlight)> {
        highlight_editorconfig_line(line)
            .into_iter()
            .map(|span| (&line[span.range], span.highlight))
            .collect()
    }

    #[test]
    fn comments_and_sections() {
        assert_eq!(spans("# top-most"), vec![("# top-most", Comment)]);
        assert_eq!(spans("  ; note"), vec![("; note", Comment)]);
        assert_eq!(spans("[*.{rs,toml}]"), vec![("[*.{rs,toml}]", Keyword)]);
        assert_eq!(spans(""), vec![]);
        assert_eq!(spans("no pair here"), vec![]);
    }

    #[test]
    fn keys_and_values() {
        assert_eq!(
            spans("root = true"),
            vec![("root", Property), ("true", Boolean)]
        );
        assert_eq!(
            spans("  indent_size=4  "),
            vec![("indent_size", Property), ("4", Number)]
        );
        assert_eq!(
            spans("end_of_line = lf"),
            vec![("end_of_line", Property), ("lf", Constant)]
        );
        assert_eq!(spans("charset ="), vec![("charset", Property)]);
    }
}

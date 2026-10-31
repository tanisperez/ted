//! The `.gitignore` highlighter: no tree-sitter, one line at a time, as the batch one. Each line
//! is one pattern, so there is nothing to parse across lines.

use crate::{Highlight, HighlightSpan};

/// The spans of one line of a `.gitignore` (its text without the line ending), relative to the
/// line: `#` comments, the `!` that negates a pattern, and the wildcards (`*`, `**`, `?` and
/// `[...]`). A character escaped with `\` is left alone. Linear in the length of the line.
pub fn highlight_gitignore_line(line: &str) -> Vec<HighlightSpan> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut push = |range: std::ops::Range<usize>, highlight| {
        spans.push(HighlightSpan { range, highlight });
    };
    // Only at the very start: `\#` and `\!` are the escaped characters.
    if bytes.first() == Some(&b'#') {
        push(0..line.len(), Highlight::Comment);
        return spans;
    }
    let mut i = 0;
    if bytes.first() == Some(&b'!') {
        push(0..1, Highlight::Keyword);
        i = 1;
    }
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'*' => {
                let end = if bytes.get(i + 1) == Some(&b'*') {
                    i + 2
                } else {
                    i + 1
                };
                push(i..end, Highlight::Constant);
                i = end;
            }
            b'?' => {
                push(i..i + 1, Highlight::Constant);
                i += 1;
            }
            b'[' => match bytes[i + 1..].iter().position(|&b| b == b']') {
                Some(close) => {
                    let end = i + 1 + close + 1;
                    push(i..end, Highlight::Constant);
                    i = end;
                }
                None => i += 1,
            },
            _ => i += 1,
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use Highlight::*;
    use pretty_assertions::assert_eq;

    fn spans(line: &str) -> Vec<(&str, Highlight)> {
        highlight_gitignore_line(line)
            .into_iter()
            .map(|span| (&line[span.range], span.highlight))
            .collect()
    }

    #[test]
    fn comments_and_negation() {
        assert_eq!(spans("# build output"), vec![("# build output", Comment)]);
        assert_eq!(spans("!keep.log"), vec![("!", Keyword)]);
        assert_eq!(spans("\\#not-a-comment"), vec![]);
        assert_eq!(spans("/target/"), vec![]);
    }

    #[test]
    fn wildcards() {
        assert_eq!(
            spans("!**/logs/*.lo?"),
            vec![
                ("!", Keyword),
                ("**", Constant),
                ("*", Constant),
                ("?", Constant)
            ]
        );
        assert_eq!(spans("*.[oa]"), vec![("*", Constant), ("[oa]", Constant)]);
        // Escaped, or a bracket never closed: plain text.
        assert_eq!(spans("file\\*name["), vec![]);
    }
}

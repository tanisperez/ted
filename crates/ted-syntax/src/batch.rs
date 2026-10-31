//! The batch file highlighter (`.bat`, `.cmd`): no tree-sitter, one line at a time, as the log
//! one. Batch has no grammar in gpui-component, and its syntax is line based anyway.

use crate::{Highlight, HighlightSpan};

/// The words painted as keywords, compared case insensitively: the control flow, the built in
/// commands a script is made of, and the operators of `if`.
const KEYWORDS: [&str; 34] = [
    "if",
    "else",
    "for",
    "in",
    "do",
    "goto",
    "call",
    "exit",
    "not",
    "exist",
    "defined",
    "errorlevel",
    "equ",
    "neq",
    "lss",
    "leq",
    "gtr",
    "geq",
    "set",
    "setlocal",
    "endlocal",
    "echo",
    "shift",
    "pause",
    "start",
    "cd",
    "chdir",
    "pushd",
    "popd",
    "cls",
    "title",
    "on",
    "off",
    "nul",
];

/// The spans of one line of a batch file (its text without the line ending), relative to the
/// line: `REM` and `::` comments, `:labels`, strings, variables (`%name%`, `%%i`, `%~dp0`,
/// `!name!`), numbers and keywords. Linear in the length of the line.
pub fn highlight_batch_line(line: &str) -> Vec<HighlightSpan> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut push = |range: std::ops::Range<usize>, highlight| {
        spans.push(HighlightSpan { range, highlight });
    };

    let mut start = 0;
    while start < bytes.len() && (bytes[start] == b' ' || bytes[start] == b'\t') {
        start += 1;
    }
    let rest = &line[start..];
    if rest.starts_with("::") || is_rem(rest.trim_start_matches('@')) {
        push(start..line.len(), Highlight::Comment);
        return spans;
    }
    if rest.starts_with(':') {
        let end = word_end(bytes, start + 1);
        push(start..end, Highlight::Function);
        return spans;
    }

    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' => {
                let end = bytes[i + 1..]
                    .iter()
                    .position(|&c| c == b'"')
                    .map_or(bytes.len(), |p| i + 1 + p + 1);
                push(i..end, Highlight::String);
                i = end;
            }
            b'%' | b'!' => match variable_end(bytes, i) {
                Some(end) => {
                    push(i..end, Highlight::Property);
                    i = end;
                }
                None => i += 1,
            },
            _ if is_word(b) && (i == 0 || !is_word(bytes[i - 1])) => {
                let end = word_end(bytes, i);
                let word = &line[i..end];
                if word.bytes().all(|c| c.is_ascii_digit()) {
                    push(i..end, Highlight::Number);
                } else if KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(word)) {
                    push(i..end, Highlight::Keyword);
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    spans
}

/// Whether `text` is a `REM` comment: the word alone, or followed by a space or a tab.
fn is_rem(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 3
        && bytes[..3].eq_ignore_ascii_case(b"rem")
        && bytes.get(3).is_none_or(|&c| c == b' ' || c == b'\t')
}

/// The end of the variable that starts at `start` (a `%` or a `!`): `%%i` and `%~dp0` (a loop
/// variable or an argument, with its modifiers), or a name closed by the same sign.
fn variable_end(bytes: &[u8], start: usize) -> Option<usize> {
    let sign = bytes[start];
    let next = *bytes.get(start + 1)?;
    if sign == b'%' && next == b'%' {
        let letter = *bytes.get(start + 2)?;
        return letter.is_ascii_alphabetic().then_some(start + 3);
    }
    if sign == b'%' && (next == b'~' || next.is_ascii_digit() || next == b'*') {
        let mut end = start + 1;
        if bytes[end] == b'~' {
            end += 1;
            while end < bytes.len() && bytes[end].is_ascii_alphabetic() && bytes[end] != b'$' {
                end += 1;
            }
            // The last letter of `%~dp0`'s modifiers may be the argument's digit instead.
        }
        if end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'*') {
            return Some(end + 1);
        }
        return None;
    }
    let close = bytes[start + 1..].iter().position(|&c| c == sign)?;
    let name = &bytes[start + 1..start + 1 + close];
    let valid = !name.is_empty()
        && name
            .iter()
            .all(|&c| c != b' ' && c != b'"' && c != b'%' && c != b'!');
    valid.then_some(start + 1 + close + 1)
}

fn word_end(bytes: &[u8], mut end: usize) -> usize {
    while end < bytes.len() && is_word(bytes[end]) {
        end += 1;
    }
    end
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' || b >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;
    use Highlight::*;
    use pretty_assertions::assert_eq;

    fn spans(line: &str) -> Vec<(&str, Highlight)> {
        highlight_batch_line(line)
            .into_iter()
            .map(|span| (&line[span.range], span.highlight))
            .collect()
    }

    #[test]
    fn comments_and_labels_take_the_line() {
        assert_eq!(spans("REM build it"), vec![("REM build it", Comment)]);
        assert_eq!(spans("  @rem"), vec![("@rem", Comment)]);
        assert_eq!(spans(":: note"), vec![(":: note", Comment)]);
        assert_eq!(spans("remove x"), vec![]);
        assert_eq!(spans(":build"), vec![(":build", Function)]);
    }

    #[test]
    fn keywords_variables_strings_and_numbers() {
        assert_eq!(
            spans("@ECHO OFF"),
            vec![("ECHO", Keyword), ("OFF", Keyword)]
        );
        assert_eq!(
            spans("if \"%1\"==\"\" goto end"),
            vec![
                ("if", Keyword),
                ("\"%1\"", String),
                ("\"\"", String),
                ("goto", Keyword),
            ]
        );
        assert_eq!(
            spans("for %%f in (*.txt) do copy %%f %OUT%\\ & exit /b 1"),
            vec![
                ("for", Keyword),
                ("%%f", Property),
                ("in", Keyword),
                ("do", Keyword),
                ("%%f", Property),
                ("%OUT%", Property),
                ("exit", Keyword),
                ("1", Number),
            ]
        );
        assert_eq!(
            spans("cd /d %~dp0 && echo !count!"),
            vec![
                ("cd", Keyword),
                ("%~dp0", Property),
                ("echo", Keyword),
                ("!count!", Property),
            ]
        );
        // A lone percent sign or an unclosed string do not run away.
        assert_eq!(
            spans("echo 50% done \"open"),
            vec![("echo", Keyword), ("50", Number), ("\"open", String)]
        );
    }
}

//! Go to line (`Ctrl+G`): what the field accepts. `120` goes to line 120, `120:8` to its eighth
//! column. Numbers start at 1, as the status bar shows them.

/// A place typed in the Go to line field, 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GotoTarget {
    pub line: usize,
    pub column: Option<usize>,
}

/// Parses `line` or `line:column` (spaces around are fine, `,` works as `:`). `None` for
/// anything else, or for zeros.
pub fn parse_target(text: &str) -> Option<GotoTarget> {
    let text = text.trim();
    let (line, column) = match text.split_once([':', ',']) {
        Some((line, column)) => (line.trim(), Some(column.trim())),
        None => (text, None),
    };
    let line: usize = line.parse().ok().filter(|line| *line > 0)?;
    let column = match column {
        Some(column) => Some(column.parse().ok().filter(|column: &usize| *column > 0)?),
        None => None,
    };
    Some(GotoTarget { line, column })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_columns_parse() {
        assert_eq!(
            parse_target(" 120 "),
            Some(GotoTarget {
                line: 120,
                column: None
            })
        );
        assert_eq!(
            parse_target("12:8"),
            Some(GotoTarget {
                line: 12,
                column: Some(8)
            })
        );
        assert_eq!(
            parse_target("12, 3"),
            Some(GotoTarget {
                line: 12,
                column: Some(3)
            })
        );
    }

    #[test]
    fn anything_else_is_rejected() {
        for text in ["", "0", "abc", "12:", "12:0", "-3", "1:2:3"] {
            assert_eq!(parse_target(text), None, "{text:?}");
        }
    }
}

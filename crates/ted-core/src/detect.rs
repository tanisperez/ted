//! Line ending and indentation detection. Both read a sample from the start of the text, never
//! the whole document, so they cost the same for a 1 KB file and a 100 MB one.

/// How much text, in bytes, the detectors read at most.
pub const SAMPLE_LEN: usize = 64 * 1024;

/// The line ending of a document, as shown in the status bar and used for new lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    /// `\n`, Linux and macOS.
    #[default]
    Lf,
    /// `\r\n`, Windows.
    CrLf,
}

impl LineEnding {
    /// The characters of the line ending.
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }

    /// The status bar label.
    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::CrLf => "CRLF",
        }
    }
}

/// The indentation of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    /// One tab per level.
    Tabs,
    /// This many spaces per level.
    Spaces(u8),
}

impl Default for Indent {
    fn default() -> Self {
        Indent::Spaces(4)
    }
}

/// The most frequent line ending in the first [`SAMPLE_LEN`] bytes of `sample`, LF when there is
/// no line break or on a tie.
pub fn detect_line_ending(sample: &str) -> LineEnding {
    let sample = &sample.as_bytes()[..sample.len().min(SAMPLE_LEN)];
    let mut lf = 0usize;
    let mut crlf = 0usize;
    for (index, byte) in sample.iter().enumerate() {
        if *byte == b'\n' {
            if index > 0 && sample[index - 1] == b'\r' {
                crlf += 1;
            } else {
                lf += 1;
            }
        }
    }
    if crlf > lf {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    }
}

/// The indentation used by the first [`SAMPLE_LEN`] bytes of `sample`, `None` when no line is
/// indented. Tabs win when more lines start with a tab than with spaces; otherwise the width is
/// the most common increase of leading spaces between consecutive indented lines (2, 4 or 8, or
/// whatever the text uses between 1 and 8).
pub fn detect_indent(sample: &str) -> Option<Indent> {
    let end = floor_boundary(sample, sample.len().min(SAMPLE_LEN));
    let mut tab_lines = 0usize;
    let mut space_lines = 0usize;
    // How often each increase of leading spaces (1..=8) appears between consecutive lines.
    let mut steps = [0usize; 9];
    let mut previous = 0usize;
    for line in sample[..end].lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('\t') {
            tab_lines += 1;
            continue;
        }
        let spaces = line.len() - line.trim_start_matches(' ').len();
        if spaces > 0 {
            space_lines += 1;
        }
        if spaces > previous && spaces - previous <= 8 {
            steps[spaces - previous] += 1;
        }
        previous = spaces;
    }
    if tab_lines == 0 && space_lines == 0 {
        return None;
    }
    if tab_lines > space_lines {
        return Some(Indent::Tabs);
    }
    // The most frequent step; on a tie the smaller one, since a 4 step often hides two 2 steps.
    let (width, count) = steps
        .iter()
        .enumerate()
        .skip(1)
        .fold((0, 0), |best, (width, count)| {
            if *count > best.1 {
                (width, *count)
            } else {
                best
            }
        });
    (count > 0).then_some(Indent::Spaces(width as u8))
}

/// The largest char boundary of `text` at or below `index`.
fn floor_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn line_endings() {
        assert_eq!(detect_line_ending(""), LineEnding::Lf);
        assert_eq!(detect_line_ending("one line"), LineEnding::Lf);
        assert_eq!(detect_line_ending("a\nb\n"), LineEnding::Lf);
        assert_eq!(detect_line_ending("a\r\nb\r\n"), LineEnding::CrLf);
        assert_eq!(detect_line_ending("a\r\nb\r\nc\n"), LineEnding::CrLf);
        assert_eq!(detect_line_ending("a\r\nb\n"), LineEnding::Lf);
    }

    #[test]
    fn no_indentation() {
        assert_eq!(detect_indent(""), None);
        assert_eq!(detect_indent("a\nb\n\n  \n"), None);
    }

    #[test]
    fn tabs() {
        assert_eq!(
            detect_indent("fn a() {\n\tb();\n\tc();\n}\n"),
            Some(Indent::Tabs)
        );
    }

    #[test]
    fn spaces() {
        let two = "a:\n  b:\n    c: 1\n  d: 2\n";
        assert_eq!(detect_indent(two), Some(Indent::Spaces(2)));
        let four = "fn a() {\n    if b {\n        c();\n    }\n}\nfn d() {\n    e();\n}\n";
        assert_eq!(detect_indent(four), Some(Indent::Spaces(4)));
    }

    #[test]
    fn the_sample_never_splits_a_character() {
        let text = format!("{}ñ\n  a\n", "x".repeat(SAMPLE_LEN - 1));
        assert_eq!(detect_indent(&text), None);
    }
}

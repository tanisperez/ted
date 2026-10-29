//! The log highlighter: no tree-sitter, one line at a time, no allocation besides the result.

use crate::{Highlight, HighlightSpan};

const LEVELS: [(&str, Highlight); 8] = [
    ("ERROR", Highlight::LogError),
    ("ERR", Highlight::LogError),
    ("FATAL", Highlight::LogError),
    ("WARN", Highlight::LogWarn),
    ("WARNING", Highlight::LogWarn),
    ("INFO", Highlight::LogInfo),
    ("DEBUG", Highlight::LogDebug),
    ("TRACE", Highlight::LogTrace),
];

/// The spans of one log line (its text without the line ending), relative to the line: a leading
/// date-time (`2024-01-15T10:30:45.123Z`, `2024-01-15 10:30:45,123`, `Jan 15 10:30:45`,
/// `10:30:45`), optionally inside brackets, as [`Highlight::LogDate`], and the first level word
/// (`ERROR`, `ERR`, `FATAL`, `WARN`, `WARNING`, `INFO`, `DEBUG`, `TRACE`, upper case, as a whole
/// word) as its class. Linear in the length of the line.
pub fn highlight_log_line(line: &str) -> Vec<HighlightSpan> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();

    let mut start = 0;
    while start < bytes.len() && (bytes[start] == b' ' || bytes[start] == b'[') {
        start += 1;
    }
    let date_end = date_time(bytes, start);
    if let Some(end) = date_end {
        spans.push(HighlightSpan {
            range: start..end,
            highlight: Highlight::LogDate,
        });
    }

    let mut i = date_end.unwrap_or(0);
    while i < bytes.len() {
        if !bytes[i].is_ascii_uppercase() || (i > 0 && is_word(bytes[i - 1])) {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < bytes.len() && is_word(bytes[end]) {
            end += 1;
        }
        let word = &line[i..end];
        if let Some((_, highlight)) = LEVELS.iter().find(|(name, _)| *name == word) {
            spans.push(HighlightSpan {
                range: i..end,
                highlight: *highlight,
            });
            break;
        }
        i = end;
    }
    spans
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// The end of a date-time starting at `at`, if one does.
fn date_time(b: &[u8], at: usize) -> Option<usize> {
    let digits = |from: usize, n: usize| {
        b.get(from..from + n)
            .is_some_and(|s| s.iter().all(u8::is_ascii_digit))
    };
    let byte = |i: usize| b.get(i).copied();

    let mut i;
    if digits(at, 4) && matches!(byte(at + 4), Some(b'-' | b'/')) && digits(at + 5, 2) {
        // 2024-01-15
        if byte(at + 7) != byte(at + 4) || !digits(at + 8, 2) {
            return None;
        }
        i = at + 10;
        if matches!(byte(i), Some(b'T' | b' ')) && time(b, i + 1).is_some() {
            i = time(b, i + 1)?;
        } else {
            return Some(i);
        }
    } else if let Some(month) = b.get(at..at + 3)
        && MONTHS.contains(&month)
        && byte(at + 3) == Some(b' ')
    {
        // Jan 15 10:30:45 (the day takes one or two digits, space padded)
        let mut j = at + 4;
        if byte(j) == Some(b' ') {
            j += 1;
        }
        let day = if digits(j, 2) {
            2
        } else if digits(j, 1) {
            1
        } else {
            return None;
        };
        j += day;
        if byte(j) != Some(b' ') {
            return None;
        }
        return time(b, j + 1);
    } else {
        return time(b, at);
    }

    // Optional zone after the time: Z, +01:00, -0500.
    match byte(i) {
        Some(b'Z') => i += 1,
        Some(b'+' | b'-') if digits(i + 1, 2) => {
            i += 3;
            if byte(i) == Some(b':') && digits(i + 1, 2) {
                i += 3;
            } else if digits(i, 2) {
                i += 2;
            }
        }
        _ => {}
    }
    Some(i)
}

/// `HH:MM:SS` with optional `.fff` or `,fff`; returns its end.
fn time(b: &[u8], at: usize) -> Option<usize> {
    let d = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    if !(d(at) && d(at + 1) && b.get(at + 2) == Some(&b':') && d(at + 3) && d(at + 4)) {
        return None;
    }
    if b.get(at + 5) != Some(&b':') || !(d(at + 6) && d(at + 7)) {
        return None;
    }
    let mut i = at + 8;
    if matches!(b.get(i), Some(b'.' | b',')) && d(i + 1) {
        i += 1;
        while d(i) {
            i += 1;
        }
    }
    Some(i)
}

const MONTHS: [&[u8]; 12] = [
    b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov", b"Dec",
];

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn s(line: &str) -> Vec<(&str, Highlight)> {
        highlight_log_line(line)
            .into_iter()
            .map(|s| (&line[s.range], s.highlight))
            .collect()
    }

    #[test]
    fn iso_date_and_level() {
        assert_eq!(
            s("2024-01-15T10:30:45.123Z ERROR boom"),
            vec![
                ("2024-01-15T10:30:45.123Z", Highlight::LogDate),
                ("ERROR", Highlight::LogError)
            ]
        );
        assert_eq!(
            s("2024-01-15 10:30:45,123 [main] WARN x"),
            vec![
                ("2024-01-15 10:30:45,123", Highlight::LogDate),
                ("WARN", Highlight::LogWarn)
            ]
        );
        assert_eq!(
            s("[2024-01-15 10:30:45+01:00] INFO up"),
            vec![
                ("2024-01-15 10:30:45+01:00", Highlight::LogDate),
                ("INFO", Highlight::LogInfo)
            ]
        );
    }

    #[test]
    fn other_dates_and_levels() {
        assert_eq!(s("Jan  5 10:30:45 host DEBUG a")[0].0, "Jan  5 10:30:45");
        assert_eq!(s("10:30:45.5 TRACE a")[1], ("TRACE", Highlight::LogTrace));
        assert_eq!(s("x FATAL y")[0], ("FATAL", Highlight::LogError));
        assert_eq!(s("x WARNING y")[0], ("WARNING", Highlight::LogWarn));
        assert_eq!(s("x ERR y")[0], ("ERR", Highlight::LogError));
        assert_eq!(
            s("2024-01-15 only date"),
            vec![("2024-01-15", Highlight::LogDate)]
        );
    }

    #[test]
    fn no_false_positives() {
        assert!(s("INFORMATION about ERRORS and MY_INFO and INFO2 and info").is_empty());
        assert!(s("plain text line").is_empty());
        assert!(s("2024-13 not a date").is_empty());
        assert!(s("").is_empty());
        // Only the first level of a line.
        assert_eq!(s("INFO then ERROR").len(), 1);
    }
}

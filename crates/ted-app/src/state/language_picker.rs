//! Plain data behind the language menu of the status bar (`views/language_menu.rs`): which
//! languages a query finds, in what order, and how the keys move the highlight.

use std::ops::Range;

use ted_syntax::Language;

/// Rows that Page Up and Page Down move, as in the design.
pub const PAGE_ROWS: usize = 7;

/// One language of the menu, with its label in the UI language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub language: Language,
    pub label: String,
    /// The extensions as the row shows them, `.js .mjs`.
    pub extensions: String,
}

impl Choice {
    pub fn new(language: Language, label: String) -> Self {
        Self {
            language,
            label,
            extensions: language.extensions().join(" "),
        }
    }
}

/// A language the query found: its index in the choices and the matched part of its label, a
/// byte range, when the label matched and not only its extensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub index: usize,
    pub matched: Option<Range<usize>>,
}

/// The choices a query finds. With an empty query, all of them in their order. Otherwise the
/// labels that start with the query, then the labels that contain it, then the languages whose
/// extensions contain it, each group in the choices' order. Case and accents are ignored.
pub fn filter(choices: &[Choice], query: &str) -> Vec<Found> {
    let query = fold(query.trim());
    if query.is_empty() {
        return (0..choices.len())
            .map(|index| Found {
                index,
                matched: None,
            })
            .collect();
    }
    let mut ranked: Vec<(u8, Found)> = choices
        .iter()
        .enumerate()
        .filter_map(|(index, choice)| {
            if let Some(matched) = find_folded(&choice.label, &query) {
                let rank = if matched.start == 0 { 0 } else { 1 };
                return Some((
                    rank,
                    Found {
                        index,
                        matched: Some(matched),
                    },
                ));
            }
            fold(&choice.extensions).contains(&query).then_some((
                2,
                Found {
                    index,
                    matched: None,
                },
            ))
        })
        .collect();
    // Stable: each group keeps the choices' order.
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().map(|(_, found)| found).collect()
}

/// A move of the highlight over a list of `len` rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Up or Down: one row, wrapping around at the ends.
    Step(isize),
    /// Page Up or Page Down: [`PAGE_ROWS`] rows, stopping at the ends.
    Page(isize),
    First,
    Last,
}

/// Where `highlight` goes with `step`. `None` for an empty list.
pub fn moved(highlight: usize, step: Move, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let current = highlight.min(last) as isize;
    Some(match step {
        Move::Step(delta) => (current + delta).rem_euclid(len as isize) as usize,
        Move::Page(pages) => {
            (current + pages * PAGE_ROWS as isize).clamp(0, last as isize) as usize
        }
        Move::First => 0,
        Move::Last => last,
    })
}

/// Lowercase without accents, one char for each char of `text`, so a position in it is a
/// position in `text`.
fn fold(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

fn fold_char(ch: char) -> char {
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    match lower {
        'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }
}

/// Where the folded `query` is in `text`, as a byte range of `text`.
fn find_folded(text: &str, query: &str) -> Option<Range<usize>> {
    let folded = fold(text);
    let start_byte = folded.find(query)?;
    let start_char = folded[..start_byte].chars().count();
    let len_chars = query.chars().count();
    let mut bounds = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(text.len()));
    let start = bounds.nth(start_char)?;
    let end = if len_chars == 0 {
        start
    } else {
        bounds.nth(len_chars - 1)?
    };
    Some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn choices() -> Vec<Choice> {
        Language::ALL
            .into_iter()
            .map(|language| Choice::new(language, language.name().to_string()))
            .collect()
    }

    fn labels(choices: &[Choice], found: &[Found]) -> Vec<String> {
        found
            .iter()
            .map(|found| choices[found.index].label.clone())
            .collect()
    }

    #[test]
    fn an_empty_query_lists_every_language_in_order() {
        let choices = choices();
        let found = filter(&choices, "  ");
        assert_eq!(found.len(), choices.len());
        assert!(
            found
                .iter()
                .enumerate()
                .all(|(i, f)| f.index == i && f.matched.is_none())
        );
    }

    #[test]
    fn names_that_start_with_the_query_come_first() {
        let choices = choices();
        // "Shell" starts with "s"; the others only contain it.
        let found = filter(&choices, "sh");
        assert_eq!(labels(&choices, &found), ["Shell", "PowerShell"]);
        assert_eq!(found[0].matched, Some(0..2));
        assert_eq!(found[1].matched, Some(5..7));
    }

    #[test]
    fn the_extensions_match_after_the_names() {
        let choices = choices();
        let found = filter(&choices, "yml");
        assert_eq!(labels(&choices, &found), ["YAML"]);
        assert_eq!(found[0].matched, None);
        // "kt" is in no name, only in Kotlin's extensions.
        assert_eq!(labels(&choices, &filter(&choices, ".kt")), ["Kotlin"]);
    }

    #[test]
    fn case_and_accents_are_ignored() {
        let choices = vec![
            Choice::new(Language::PlainText, "Texto plano".to_string()),
            Choice::new(Language::Rust, "Rust".to_string()),
        ];
        let found = filter(&choices, "PLÁNO");
        assert_eq!(labels(&choices, &found), ["Texto plano"]);
        assert_eq!(found[0].matched, Some(6..11));
    }

    #[test]
    fn the_matched_range_is_in_bytes_of_the_label() {
        let choices = vec![Choice::new(Language::PlainText, "Énfasis".to_string())];
        // "É" is two bytes.
        assert_eq!(filter(&choices, "nf")[0].matched, Some(2..4));
        assert_eq!(filter(&choices, "en")[0].matched, Some(0..3));
    }

    #[test]
    fn nothing_found() {
        assert!(filter(&choices(), "zzz").is_empty());
    }

    #[test]
    fn up_and_down_wrap_around() {
        assert_eq!(moved(0, Move::Step(-1), 5), Some(4));
        assert_eq!(moved(4, Move::Step(1), 5), Some(0));
        assert_eq!(moved(2, Move::Step(1), 5), Some(3));
    }

    #[test]
    fn pages_stop_at_the_ends() {
        assert_eq!(moved(2, Move::Page(1), 20), Some(9));
        assert_eq!(moved(18, Move::Page(1), 20), Some(19));
        assert_eq!(moved(3, Move::Page(-1), 20), Some(0));
    }

    #[test]
    fn first_last_and_an_empty_list() {
        assert_eq!(moved(5, Move::First, 9), Some(0));
        assert_eq!(moved(0, Move::Last, 9), Some(8));
        assert_eq!(moved(0, Move::Step(1), 0), None);
        // A highlight past a shorter list starts from its last row.
        assert_eq!(moved(30, Move::Step(-1), 3), Some(1));
    }
}

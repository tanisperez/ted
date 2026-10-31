//! The UI language. `telaio-app` knows four languages; ted is translated into two, English (the
//! source and fallback) and Spanish, so this module narrows the `language` setting and the system
//! detection to those. The translations live in `crates/ted-app/locales/` and are looked up with
//! `t!`.
//!
//! The locale is process wide (`rust_i18n::set_locale`), so no test may call [`apply`]: tests
//! that need a language pass `locale = "xx"` to `t!` instead.

use serde::Serialize;

pub use telaio_app::language::{Language, apply, match_language, system_locales};

/// The translation of the plural keys `<key>.one` and `<key>.other` for `count`, with `count`
/// available in the text as `%{count}`, formatted for the current language (`1,234`).
pub fn plural(key: &str, count: usize) -> String {
    let key = telaio_app::language::plural_key(key, count);
    let count_text = telaio_app::number::format_integer(count as u64);
    rust_i18n::t!(key.as_str(), count = count_text).to_string()
}

/// The languages ted is translated into, in the order the Settings dropdown lists them.
pub const LANGUAGES: [Language; 2] = [Language::English, Language::Spanish];

/// The `language` setting: follow the system or force one language. Stored in `settings.toml`
/// as `language = "auto" | "en" | "es"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageChoice {
    /// Use the first system language ted supports, English if none.
    #[default]
    Auto,
    /// Always English.
    En,
    /// Always Spanish.
    Es,
}

impl LanguageChoice {
    /// Parses a value as read from the settings file, case-insensitive. Returns `None` for
    /// anything else, so the caller can fall back to the default.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(LanguageChoice::Auto),
            "en" => Some(LanguageChoice::En),
            "es" => Some(LanguageChoice::Es),
            _ => None,
        }
    }

    /// The forced language, `None` for [`LanguageChoice::Auto`].
    pub fn language(self) -> Option<Language> {
        match self {
            LanguageChoice::Auto => None,
            LanguageChoice::En => Some(Language::English),
            LanguageChoice::Es => Some(Language::Spanish),
        }
    }
}

/// The first language of `preferred` (most wanted first) that ted is translated into, English
/// when there is none.
pub fn detect(preferred: impl IntoIterator<Item = String>) -> Language {
    preferred
        .into_iter()
        .filter_map(|tag| match_language(&tag))
        .find(|language| LANGUAGES.contains(language))
        .unwrap_or(Language::English)
}

/// The language ted speaks for `choice`: the forced one, or the system's.
pub fn resolve(choice: LanguageChoice) -> Language {
    choice
        .language()
        .unwrap_or_else(|| detect(system_locales().iter().cloned()))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::Path;

    /// Every message of `locale` defined by ted's own tables (not gpui-component's).
    fn messages(locale: &str) -> BTreeMap<String, String> {
        crate::_rust_i18n_backend()
            .messages_for_locale(locale)
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    /// The `%{name}` placeholders of `text`, sorted.
    fn placeholders(text: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let mut rest = text;
        while let Some(start) = rest.find("%{") {
            let after = &rest[start + 2..];
            let Some(end) = after.find('}') else { break };
            found.insert(after[..end].to_string());
            rest = &after[end + 1..];
        }
        found
    }

    #[test]
    fn detection_only_picks_a_translated_language() {
        let tags = |list: &[&str]| list.iter().map(|tag| tag.to_string()).collect::<Vec<_>>();
        assert_eq!(detect(tags(&["es_ES.UTF-8"])), Language::Spanish);
        assert_eq!(detect(tags(&["gl-ES", "es-ES"])), Language::Spanish);
        assert_eq!(detect(tags(&["it-IT"])), Language::English);
        assert_eq!(detect(tags(&[])), Language::English);
    }

    #[test]
    fn language_choices_parse_case_insensitively() {
        assert_eq!(LanguageChoice::parse("ES"), Some(LanguageChoice::Es));
        assert_eq!(LanguageChoice::parse("auto"), Some(LanguageChoice::Auto));
        assert_eq!(LanguageChoice::parse("gl"), None);
    }

    #[test]
    fn every_key_is_translated_in_every_language() {
        let english = messages("en");
        assert!(!english.is_empty(), "no English messages were loaded");
        for language in LANGUAGES {
            let table = messages(language.code());
            for (key, source) in &english {
                let text = table
                    .get(key)
                    .unwrap_or_else(|| panic!("{key} is missing in {}", language.code()));
                assert!(
                    !text.trim().is_empty(),
                    "{key} is empty in {}",
                    language.code()
                );
                assert_eq!(
                    placeholders(text),
                    placeholders(source),
                    "{key} has different placeholders in {}",
                    language.code()
                );
            }
            for key in table.keys() {
                assert!(
                    english.contains_key(key),
                    "{key} exists in {} but not in English",
                    language.code()
                );
            }
        }
    }

    #[test]
    fn no_translation_contains_the_em_dash() {
        for language in LANGUAGES {
            for (key, text) in messages(language.code()) {
                assert!(!text.contains('\u{2014}'), "{key} in {}", language.code());
            }
        }
    }

    #[test]
    fn plural_keys_come_in_pairs() {
        let english = messages("en");
        for key in english.keys() {
            if let Some(base) = key.strip_suffix(".one") {
                assert!(english.contains_key(&format!("{base}.other")), "{key}");
            }
            if let Some(base) = key.strip_suffix(".other") {
                assert!(english.contains_key(&format!("{base}.one")), "{key}");
            }
        }
    }

    /// Every `.rs` file under `dir`, recursively.
    fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(dir).expect("read dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    /// The literal keys of every `t!` call in `source` whose first argument is a string literal.
    fn literal_keys(source: &str) -> Vec<String> {
        let mut keys = Vec::new();
        let mut from = 0;
        while let Some(found) = source[from..].find("t!(\"") {
            let start = from + found;
            let body = start + 4;
            from = body;
            // `format!(`, `assert!(` ... also end in `t!(`: only a standalone `t!` counts.
            let previous = source[..start].chars().next_back();
            if previous.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            if let Some(end) = source[body..].find('"') {
                keys.push(source[body..body + end].to_string());
            }
        }
        keys
    }

    #[test]
    fn every_key_used_in_the_sources_exists() {
        let english = messages("en");
        let mut files = Vec::new();
        rust_files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        assert!(!files.is_empty());
        let mut missing = Vec::new();
        for file in files {
            let source = fs::read_to_string(&file).expect("read source");
            for key in literal_keys(&source) {
                if !english.contains_key(&key) {
                    missing.push(format!("{}: {key}", file.display()));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "keys missing from locales/: {missing:#?}"
        );
    }
}

//! ted's syntax colors, the [`SyntaxTheme`] of each mode that [`super::install`] passes to
//! telaio. Every color is one of the palette's (keep them in sync with
//! `docs/design-system.md`): comments in `fg_muted`, keywords in the PATCH purple, functions in
//! `info`, keys and attributes in `accent_text`, strings in green, numbers and types in
//! `warning`, booleans and constants in pink, XML tags in `danger`. Captures left out (variables,
//! operators, punctuation) take the text color.
//!
//! Comments ask for italics, but the bundled Geist Mono has no italic face, so they show
//! upright until it gets one.
//!
//! The editor paints `ted_syntax`'s classes with [`SyntaxColors`]: each class takes the color of
//! one capture of the theme, and the log classes borrow theirs as the design says (dates and
//! `DEBUG`/`TRACE` as comments, `ERROR` as tags, `WARN` as numbers, `INFO` as functions).

use gpui_kit::Hsla;
use gpui_kit::component::highlighter::HighlightTheme;
use ted_syntax::Highlight;
use telaio_ui::{SyntaxStyle, SyntaxTheme};

/// The captures the editor paints with, in the order of [`capture_index`].
const CAPTURES: [&str; 11] = [
    "comment",
    "keyword",
    "function",
    "property",
    "attribute",
    "string",
    "number",
    "type",
    "boolean",
    "constant",
    "tag",
];

/// The position in [`CAPTURES`] of the capture that colors `highlight`.
fn capture_index(highlight: Highlight) -> usize {
    match highlight {
        Highlight::Comment | Highlight::LogDate | Highlight::LogDebug | Highlight::LogTrace => 0,
        Highlight::Keyword => 1,
        Highlight::Function | Highlight::LogInfo => 2,
        Highlight::Property => 3,
        Highlight::Attribute => 4,
        Highlight::String => 5,
        Highlight::Number | Highlight::LogWarn => 6,
        Highlight::Type => 7,
        Highlight::Boolean => 8,
        Highlight::Constant => 9,
        Highlight::Tag | Highlight::LogError => 10,
    }
}

/// The color of every class in the current theme, looked up once per frame.
pub struct SyntaxColors([Option<Hsla>; CAPTURES.len()]);

impl SyntaxColors {
    pub fn new(theme: &HighlightTheme) -> Self {
        Self(CAPTURES.map(|name| theme.style(name).and_then(|style| style.color)))
    }

    /// The color of `highlight`, `None` for the plain text color.
    pub fn get(&self, highlight: Highlight) -> Option<Hsla> {
        self.0[capture_index(highlight)]
    }
}

/// The light mode syntax colors.
pub const LIGHT: SyntaxTheme = SyntaxTheme(&[
    ("comment", SyntaxStyle::color("#5c606e").italic()),
    ("keyword", SyntaxStyle::color("#7a4fd6")),
    ("function", SyntaxStyle::color("#1c6fb0")),
    ("property", SyntaxStyle::color("#3f47c4")),
    ("attribute", SyntaxStyle::color("#3f47c4")),
    ("string", SyntaxStyle::color("#1d7a4f")),
    ("number", SyntaxStyle::color("#9a6500")),
    ("type", SyntaxStyle::color("#9a6500")),
    ("boolean", SyntaxStyle::color("#b03a78")),
    ("constant", SyntaxStyle::color("#b03a78")),
    ("tag", SyntaxStyle::color("#cf3535")),
]);

/// The dark mode syntax colors, the dark values of the same palette roles as [`LIGHT`].
pub const DARK: SyntaxTheme = SyntaxTheme(&[
    ("comment", SyntaxStyle::color("#a0a3ae").italic()),
    ("keyword", SyntaxStyle::color("#b597f7")),
    ("function", SyntaxStyle::color("#62b3f0")),
    ("property", SyntaxStyle::color("#a3a9ff")),
    ("attribute", SyntaxStyle::color("#a3a9ff")),
    ("string", SyntaxStyle::color("#86d7a5")),
    ("number", SyntaxStyle::color("#e8b75a")),
    ("type", SyntaxStyle::color("#e8b75a")),
    ("boolean", SyntaxStyle::color("#f3a2c6")),
    ("constant", SyntaxStyle::color("#f3a2c6")),
    ("tag", SyntaxStyle::color("#f27171")),
]);

#[cfg(test)]
mod tests {
    use super::{DARK, LIGHT};
    use pretty_assertions::assert_eq;

    #[test]
    fn every_capture_the_editor_paints_has_a_color() {
        for name in super::CAPTURES {
            assert!(LIGHT.get(name).is_some(), "{name}");
            assert!(DARK.get(name).is_some(), "{name}");
        }
    }

    #[test]
    fn both_modes_color_the_same_captures() {
        let names = |theme: telaio_ui::SyntaxTheme| {
            theme.0.iter().map(|(name, _)| *name).collect::<Vec<_>>()
        };
        assert_eq!(names(LIGHT), names(DARK));
    }

    #[test]
    fn json_keeps_the_colors_it_had_before_the_syntax_theme() {
        let color = |theme: telaio_ui::SyntaxTheme, capture| theme.get(capture).map(|s| s.color);
        assert_eq!(color(LIGHT, "property"), Some("#3f47c4"));
        assert_eq!(color(LIGHT, "string"), Some("#1d7a4f"));
        assert_eq!(color(LIGHT, "number"), Some("#9a6500"));
        assert_eq!(color(LIGHT, "boolean"), Some("#b03a78"));
        assert_eq!(color(DARK, "property"), Some("#a3a9ff"));
        assert_eq!(color(DARK, "string"), Some("#86d7a5"));
        assert_eq!(color(DARK, "number"), Some("#e8b75a"));
        assert_eq!(color(DARK, "boolean"), Some("#f3a2c6"));
    }
}

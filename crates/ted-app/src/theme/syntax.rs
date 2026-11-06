//! The syntax themes, a [`SyntaxTheme`] per mode that [`super::install`] passes to telaio, as the
//! setting `syntax_theme` chooses ([`themes`]).
//!
//! In ted's own theme every color is one of the palette's (keep them in sync with
//! `docs/design-system.md`): comments in `fg_muted`, keywords in the PATCH purple, functions in
//! `info`, keys and attributes in `accent_text`, strings in green, numbers and types in
//! `warning`, booleans and constants in pink, XML tags in `danger`. Captures left out (variables,
//! operators, punctuation) take the text color.
//!
//! Comments ask for italics, but the bundled Geist Mono has no italic face, so they show
//! upright until it gets one.
//!
//! The editor paints `ted_syntax`'s classes with [`SyntaxColors`]: each class takes the color of
//! one capture of the theme. The log classes mean a severity, not a kind of code, so they take
//! the palette's colors in every theme (dates and `DEBUG`/`TRACE` `fg_muted`, `ERROR` `danger`,
//! `WARN` `warning`, `INFO` `info`).

use gpui_kit::Hsla;
use gpui_kit::component::highlighter::HighlightTheme;
use ted_syntax::Highlight;
use telaio_ui::{Palette, SyntaxStyle, SyntaxTheme};

use crate::state::settings::SyntaxThemeChoice;

/// The capture of [`Highlight::Control`]. gpui-kit's themes have a fixed list of names without a
/// control keyword, and this one is what it uses for `keyword.control.import`.
const CONTROL_CAPTURE: &str = "preproc";

/// The captures the editor paints with, in the order of [`capture_index`].
const CAPTURES: [&str; 14] = [
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
    CONTROL_CAPTURE,
    "variable",
    "variable.special",
];

/// The position of the keyword capture, the color of [`Highlight::Control`] in a theme without one.
const KEYWORD: usize = 1;

/// The position in [`CAPTURES`] of the capture that colors `highlight`, `None` for the log classes.
fn capture_index(highlight: Highlight) -> Option<usize> {
    Some(match highlight {
        Highlight::Comment => 0,
        Highlight::Keyword => KEYWORD,
        Highlight::Function => 2,
        Highlight::Property => 3,
        Highlight::Attribute => 4,
        Highlight::String => 5,
        Highlight::Number => 6,
        Highlight::Type => 7,
        Highlight::Boolean => 8,
        Highlight::Constant => 9,
        Highlight::Tag => 10,
        Highlight::Control => 11,
        Highlight::Variable => 12,
        Highlight::BuiltinVariable => 13,
        Highlight::LogError
        | Highlight::LogWarn
        | Highlight::LogInfo
        | Highlight::LogDebug
        | Highlight::LogTrace
        | Highlight::LogDate => return None,
    })
}

/// The color of every class in the current theme, looked up once per frame.
pub struct SyntaxColors {
    captures: [Option<Hsla>; CAPTURES.len()],
    log_error: Hsla,
    log_warn: Hsla,
    log_info: Hsla,
    log_muted: Hsla,
}

impl SyntaxColors {
    pub fn new(theme: &HighlightTheme, palette: &Palette) -> Self {
        Self {
            captures: CAPTURES.map(|name| theme.style(name).and_then(|style| style.color)),
            log_error: palette.danger,
            log_warn: palette.warning,
            log_info: palette.info,
            log_muted: palette.fg_muted,
        }
    }

    /// The color of `highlight`, `None` for the plain text color.
    pub fn get(&self, highlight: Highlight) -> Option<Hsla> {
        match highlight {
            Highlight::LogError => Some(self.log_error),
            Highlight::LogWarn => Some(self.log_warn),
            Highlight::LogInfo => Some(self.log_info),
            Highlight::LogDebug | Highlight::LogTrace | Highlight::LogDate => Some(self.log_muted),
            Highlight::Control => {
                self.captures[capture_index(highlight)?].or(self.captures[KEYWORD])
            }
            _ => self.captures[capture_index(highlight)?],
        }
    }
}

/// The light and the dark colors of a syntax theme.
pub fn themes(choice: SyntaxThemeChoice) -> (SyntaxTheme, SyntaxTheme) {
    match choice {
        SyntaxThemeChoice::Ted => (LIGHT, DARK),
        SyntaxThemeChoice::VsCode => (VSCODE_LIGHT, VSCODE_DARK),
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

/// Visual Studio Code's Light+: keywords in pure blue, flow keywords in magenta.
const VSCODE_LIGHT: SyntaxTheme = SyntaxTheme(&[
    ("comment", SyntaxStyle::color("#008000")),
    ("keyword", SyntaxStyle::color("#0000ff")),
    (CONTROL_CAPTURE, SyntaxStyle::color("#af00db")),
    ("function", SyntaxStyle::color("#795e26")),
    ("property", SyntaxStyle::color("#001080")),
    ("attribute", SyntaxStyle::color("#e50000")),
    ("string", SyntaxStyle::color("#a31515")),
    ("number", SyntaxStyle::color("#098658")),
    ("type", SyntaxStyle::color("#267f99")),
    ("boolean", SyntaxStyle::color("#0000ff")),
    ("constant", SyntaxStyle::color("#0070c1")),
    ("tag", SyntaxStyle::color("#800000")),
    ("variable", SyntaxStyle::color("#001080")),
    ("variable.special", SyntaxStyle::color("#0000ff")),
]);

/// Visual Studio Code's Dark+: keywords in its blue, flow keywords in magenta, variables in light
/// blue, functions in yellow, types in teal and strings in orange.
const VSCODE_DARK: SyntaxTheme = SyntaxTheme(&[
    ("comment", SyntaxStyle::color("#6a9955")),
    ("keyword", SyntaxStyle::color("#569cd6")),
    (CONTROL_CAPTURE, SyntaxStyle::color("#c586c0")),
    ("function", SyntaxStyle::color("#dcdcaa")),
    ("property", SyntaxStyle::color("#9cdcfe")),
    ("attribute", SyntaxStyle::color("#9cdcfe")),
    ("string", SyntaxStyle::color("#ce9178")),
    ("number", SyntaxStyle::color("#b5cea8")),
    ("type", SyntaxStyle::color("#4ec9b0")),
    ("boolean", SyntaxStyle::color("#569cd6")),
    ("constant", SyntaxStyle::color("#4fc1ff")),
    ("tag", SyntaxStyle::color("#569cd6")),
    ("variable", SyntaxStyle::color("#9cdcfe")),
    ("variable.special", SyntaxStyle::color("#569cd6")),
]);

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{CONTROL_CAPTURE, DARK, LIGHT, SyntaxThemeChoice, themes};
    use pretty_assertions::assert_eq;

    /// The captures a theme may leave out: they show as the keyword or in the text color.
    const OPTIONAL: [&str; 3] = [CONTROL_CAPTURE, "variable", "variable.special"];

    #[test]
    fn every_theme_colors_what_the_editor_paints() {
        for choice in SyntaxThemeChoice::ALL {
            let (light, dark) = themes(choice);
            for name in super::CAPTURES.iter().filter(|n| !OPTIONAL.contains(n)) {
                assert!(light.get(name).is_some(), "{choice:?} {name}");
                assert!(dark.get(name).is_some(), "{choice:?} {name}");
            }
        }
    }

    #[test]
    fn both_modes_of_a_theme_color_the_same_captures() {
        let names = |theme: telaio_ui::SyntaxTheme| {
            theme.0.iter().map(|(name, _)| *name).collect::<Vec<_>>()
        };
        for choice in SyntaxThemeChoice::ALL {
            let (light, dark) = themes(choice);
            assert_eq!(names(light), names(dark), "{choice:?}");
        }
    }

    #[test]
    fn the_capture_names_are_the_ones_gpui_kit_knows() {
        // A name gpui-kit does not know would be dropped from the theme without an error.
        for choice in SyntaxThemeChoice::ALL {
            let (_, dark) = themes(choice);
            let entries = dark.0.iter().map(|(name, style)| {
                let value = serde_json::json!({ "color": style.color });
                ((*name).to_string(), value)
            });
            let json = serde_json::Value::Object(entries.collect());
            let parsed: gpui_kit::component::highlighter::SyntaxColors =
                serde_json::from_value(json).unwrap();
            for (name, _) in dark.0 {
                assert!(parsed.style(name).is_some(), "{choice:?} {name}");
            }
        }
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

//! The syntax themes, a [`SyntaxTheme`] per mode that [`super::install`] passes to telaio, as the
//! setting `syntax_theme` chooses ([`themes`]). The editor paints with them through
//! `telaio_editor::SyntaxColors`.
//!
//! In ted's own theme every color is one of the palette's (keep them in sync with
//! `docs/design-system.md`): comments in `fg_muted`, keywords in the PATCH purple, functions in
//! `info`, keys and attributes in `accent_text`, strings in green, numbers and types in
//! `warning`, booleans and constants in pink, XML tags in `danger`. Captures left out (variables,
//! operators, punctuation) take the text color.
//!
//! Comments ask for italics, but the bundled Geist Mono has no italic face, so they show
//! upright until it gets one.

use telaio_ui::{SyntaxStyle, SyntaxTheme};

use crate::state::settings::SyntaxThemeChoice;
use telaio_editor::syntax_colors::CONTROL_CAPTURE;

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
    use telaio_editor::syntax_colors::CAPTURES;

    /// The captures a theme may leave out: they show as the keyword or in the text color.
    const OPTIONAL: [&str; 3] = [CONTROL_CAPTURE, "variable", "variable.special"];

    #[test]
    fn every_theme_colors_what_the_editor_paints() {
        for choice in SyntaxThemeChoice::ALL {
            let (light, dark) = themes(choice);
            for name in CAPTURES.iter().filter(|n| !OPTIONAL.contains(n)) {
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

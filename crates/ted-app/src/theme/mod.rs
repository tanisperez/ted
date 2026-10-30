//! ted's visual foundations, on top of telaio: the palette (telaio's default tokens), layout
//! metrics, the syntax colors and the "ted Light"/"ted Dark" theme family built from the
//! settings' fonts.
//!
//! [`install`] does the startup work through `telaio_ui::install`: register the bundled fonts, load
//! the theme family into gpui-kit's `ThemeRegistry` and install the palettes `cx.palette()`
//! reads.

pub mod metrics;
pub mod syntax;

// Re-exported for views (`cx.palette()`).
pub use telaio_ui::{Palette, PaletteExt};

use gpui_kit::App;

use crate::state::settings::Settings;

/// Loads the ted Light/Dark theme family built from `settings`'s font choices and makes it the
/// registry's default light and dark themes. Call once, after `gpui_kit::init`, before opening
/// any window; the window's own `ThemeChoice::apply` call is what makes it visible.
pub fn install(cx: &mut App, settings: &Settings) {
    telaio_ui::install(cx, &theme_family(settings));
}

/// Rebuilds the themes from `settings` (their font sizes) and makes them the active ones, live.
/// The caller applies the theme choice afterwards to repaint the window.
pub fn apply_settings(cx: &mut App, settings: &Settings) {
    telaio_ui::apply(cx, &theme_family(settings));
}

/// The theme family: telaio's default tokens, ted's [`syntax`] colors and the settings' fonts.
fn theme_family(settings: &Settings) -> telaio_ui::ThemeFamily {
    telaio_ui::ThemeFamily::new("ted")
        .syntax(syntax::LIGHT, syntax::DARK)
        .fonts(telaio_ui::Fonts {
            ui_family: settings.ui_font.clone().into(),
            ui_size: settings.ui_font_size,
            mono_family: settings.mono_font.clone().into(),
            mono_size: settings.mono_font_size,
        })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use gpui_kit::component::ThemeSet;
    use pretty_assertions::assert_eq;

    #[test]
    fn family_json_parses_as_a_theme_set_with_both_modes() {
        let json = theme_family(&Settings::default()).to_json().to_string();

        let theme_set: ThemeSet =
            serde_json::from_str(&json).expect("the theme family must parse as a ThemeSet");

        assert_eq!(theme_set.themes.len(), 2);
        assert_eq!(theme_set.themes[0].name.as_ref(), "ted Light");
        assert!(!theme_set.themes[0].mode.is_dark());
        assert_eq!(theme_set.themes[1].name.as_ref(), "ted Dark");
        assert!(theme_set.themes[1].mode.is_dark());
    }

    #[test]
    fn family_json_colors_match_the_design_tokens() {
        let json = theme_family(&Settings::default()).to_json();

        assert_eq!(json["themes"][0]["font.family"], "Geist");
        assert_eq!(json["themes"][0]["mono_font.family"], "Geist Mono");
        assert_eq!(json["themes"][0]["colors"]["background"], "#fcfcfd");
        assert_eq!(json["themes"][0]["colors"]["primary.background"], "#4f57d8");
        assert_eq!(json["themes"][1]["colors"]["background"], "#15161a");
        assert_eq!(json["themes"][1]["colors"]["primary.background"], "#5b63e6");
    }
}

//! The colors the editor paints `ted_syntax`'s classes with, read from the active gpui-kit
//! highlight theme (the app's `telaio_ui::SyntaxTheme`) and the palette.
//!
//! The editor paints `ted_syntax`'s classes with [`SyntaxColors`]: each class takes the color of
//! one capture of the theme. The log classes mean a severity, not a kind of code, so they take
//! the palette's colors in every theme (dates and `DEBUG`/`TRACE` `fg_muted`, `ERROR` `danger`,
//! `WARN` `warning`, `INFO` `info`).

use gpui_kit::Hsla;
use gpui_kit::component::highlighter::HighlightTheme;
use ted_syntax::Highlight;
use telaio_ui::Palette;

/// The capture of [`Highlight::Control`]. gpui-kit's themes have a fixed list of names without a
/// control keyword, and this one is what it uses for `keyword.control.import`.
pub const CONTROL_CAPTURE: &str = "preproc";

/// The captures the editor paints with, in the order of [`capture_index`].
pub const CAPTURES: [&str; 14] = [
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

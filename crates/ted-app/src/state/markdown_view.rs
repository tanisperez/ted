//! What a Markdown document shows: the editor, the editor with the preview at its right, or the
//! preview alone. Chosen with the floating toolbar or `Ctrl+Shift+V`, which cycles them; a new
//! document starts with the view of the `markdown_view` setting.

use rust_i18n::t;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkdownView {
    Editor,
    /// The editor and the preview side by side.
    #[default]
    Split,
    Preview,
}

impl MarkdownView {
    /// Every view, in the order of the toolbar and of `Ctrl+Shift+V`.
    pub const ALL: [MarkdownView; 3] = [
        MarkdownView::Editor,
        MarkdownView::Split,
        MarkdownView::Preview,
    ];

    /// The view after this one, as `Ctrl+Shift+V` cycles them.
    pub fn next(self) -> Self {
        match self {
            MarkdownView::Editor => MarkdownView::Split,
            MarkdownView::Split => MarkdownView::Preview,
            MarkdownView::Preview => MarkdownView::Editor,
        }
    }

    pub fn shows_editor(self) -> bool {
        self != MarkdownView::Preview
    }

    pub fn shows_preview(self) -> bool {
        self != MarkdownView::Editor
    }

    /// Parses a name as read from the settings file, case-insensitive.
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "editor" => Some(MarkdownView::Editor),
            "split" => Some(MarkdownView::Split),
            "preview" => Some(MarkdownView::Preview),
            _ => None,
        }
    }

    /// The name in Settings.
    pub fn label(self) -> String {
        match self {
            MarkdownView::Editor => t!("markdown.view_editor"),
            MarkdownView::Split => t!("markdown.view_split"),
            MarkdownView::Preview => t!("markdown.view_preview"),
        }
        .into_owned()
    }

    /// The tooltip of its toolbar button.
    pub fn tooltip(self) -> String {
        match self {
            MarkdownView::Editor => t!("markdown.only_editor"),
            MarkdownView::Split => t!("markdown.editor_and_preview"),
            MarkdownView::Preview => t!("markdown.only_preview"),
        }
        .into_owned()
    }
}

/// The least width of each side of the Markdown split view, in logical pixels (#102).
pub const SPLIT_MIN_WIDTH: f32 = 240.0;

/// The editor's share of a split row `width` wide with the divider dragged to `x` (from the
/// row's left edge): each side keeps [`SPLIT_MIN_WIDTH`], and a row too narrow for both is halved.
pub fn split_share(x: f32, width: f32) -> f32 {
    if !width.is_finite() || width <= SPLIT_MIN_WIDTH * 2.0 {
        return 0.5;
    }
    x.clamp(SPLIT_MIN_WIDTH, width - SPLIT_MIN_WIDTH) / width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_split_keeps_both_sides_wide_enough() {
        assert_eq!(split_share(500.0, 1000.0), 0.5);
        assert_eq!(split_share(600.0, 1000.0), 0.6);
        assert_eq!(split_share(10.0, 1000.0), 0.24);
        assert_eq!(split_share(990.0, 1000.0), 0.76);
        assert_eq!(split_share(100.0, 400.0), 0.5);
        assert_eq!(split_share(100.0, f32::NAN), 0.5);
    }

    #[test]
    fn ctrl_shift_v_cycles_the_three_views() {
        let mut view = MarkdownView::Editor;
        let mut seen = Vec::new();
        for _ in 0..3 {
            view = view.next();
            seen.push(view);
        }
        assert_eq!(
            seen,
            [
                MarkdownView::Split,
                MarkdownView::Preview,
                MarkdownView::Editor
            ]
        );
    }

    #[test]
    fn what_each_view_shows() {
        assert!(MarkdownView::Editor.shows_editor() && !MarkdownView::Editor.shows_preview());
        assert!(MarkdownView::Split.shows_editor() && MarkdownView::Split.shows_preview());
        assert!(!MarkdownView::Preview.shows_editor() && MarkdownView::Preview.shows_preview());
    }

    #[test]
    fn parsed_from_the_settings_file() {
        assert_eq!(MarkdownView::parse("Preview"), Some(MarkdownView::Preview));
        assert_eq!(MarkdownView::parse("split"), Some(MarkdownView::Split));
        assert_eq!(MarkdownView::parse("both"), None);
        assert_eq!(MarkdownView::default(), MarkdownView::Split);
    }
}

//! Named layout constants (heights, widths, listed in `docs/design-system.md`). Views read sizes
//! only from here, never as a literal, so every size in the app traces back to one place.
//!
//! Some constants are kept for sizes the design defines even when no view reads them yet, so
//! this file allows dead code for itself.
#![allow(dead_code)]

// The sizes of the standard components and the radius scale, shared with the other apps.
#[allow(unused_imports)]
pub use telaio_ui::metrics::*;

/// Height of the title bar.
pub const TITLE_BAR_HEIGHT: f32 = 40.0;
/// Height of the status bar.
pub const STATUS_BAR_HEIGHT: f32 = 30.0;
/// Height of a panel toggle (Files, Git) at the left of the status bar.
pub const STATUS_TOGGLE_HEIGHT: f32 = 24.0;
/// Narrowest a panel toggle can be (the icon alone, without a count).
pub const STATUS_TOGGLE_MIN_WIDTH: f32 = 28.0;
/// Side length of the "t" logo in the title bar.
pub const LOGO_SIZE: f32 = 18.0;
/// Width of the line number gutter beyond its digits: the left padding plus
/// [`EDITOR_GUTTER_RIGHT`].
pub const EDITOR_GUTTER_PADDING: f32 = 28.0;
/// Space between the line numbers and the gutter's right edge.
pub const EDITOR_GUTTER_RIGHT: f32 = 16.0;
/// Space between the gutter and the text.
pub const EDITOR_TEXT_PADDING: f32 = 4.0;
/// Width of the editor's vertical scrollbar.
pub const EDITOR_SCROLLBAR_WIDTH: f32 = 10.0;
/// Width of the empty state's column.
pub const WELCOME_WIDTH: f32 = 440.0;
/// Side of the large logo of the empty state.
pub const WELCOME_LOGO_SIZE: f32 = 40.0;
/// Height of a row of the empty state (actions and recents), as the design's menu rows.
pub const WELCOME_ROW_HEIGHT: f32 = 30.0;
/// Space between the sections of the empty state.
pub const WELCOME_SECTION_GAP: f32 = 24.0;
/// Default width of the explorer panel.
pub const EXPLORER_WIDTH: f32 = 260.0;
/// Narrowest the explorer panel can be dragged to.
pub const EXPLORER_WIDTH_MIN: f32 = 180.0;
/// Widest the explorer panel can be dragged to.
pub const EXPLORER_WIDTH_MAX: f32 = 520.0;
/// Width of the invisible strip on the explorer's edge that resizes it.
pub const EXPLORER_RESIZE_HANDLE: f32 = 4.0;
/// Height of the explorer's header (the folder name and its buttons).
pub const EXPLORER_HEADER_HEIGHT: f32 = 36.0;
/// Side padding of the explorer's list.
pub const EXPLORER_LIST_PADDING: f32 = 6.0;

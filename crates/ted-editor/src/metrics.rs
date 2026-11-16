//! The editor's layout metrics: the gutter, the scrollbars, the find bar, the Go to line
//! dialog and their fields.

/// Width of the field of the find bar.
pub const FIND_BAR_FIELD_WIDTH: f32 = 420.0;

/// Height of the fields of the find bar.
pub const FIND_BAR_FIELD_HEIGHT: f32 = 28.0;

/// Size of the buttons of the find bar.
pub const FIND_BAR_BUTTON: f32 = 26.0;

/// Width of the match counter of the find bar (`120 of 4,500`).
pub const FIND_COUNT_WIDTH: f32 = 64.0;

/// Width of the Go to line dialog.
pub const GOTO_WIDTH: f32 = 380.0;

/// Distance from the top of the editor to the Go to line dialog.
pub const GOTO_TOP: f32 = 20.0;

/// Height of the Go to line field.
pub const GOTO_FIELD_HEIGHT: f32 = 32.0;

/// Height of the Cancel and Go buttons of the dialog.
pub const GOTO_BUTTON_HEIGHT: f32 = 28.0;

/// Radius of the search fields (find bar, Go to line, Find in files).
pub const FIELD_RADIUS: f32 = 7.0;

/// Width of the focus ring around a search field.
pub const FIELD_RING: f32 = 3.0;

/// Width of the line number gutter beyond its digits: the left padding plus
/// [`EDITOR_GUTTER_RIGHT`].
pub const EDITOR_GUTTER_PADDING: f32 = 28.0;

/// Space between the line numbers and the gutter's right edge.
pub const EDITOR_GUTTER_RIGHT: f32 = 16.0;

/// Space between the gutter and the text.
pub const EDITOR_TEXT_PADDING: f32 = 4.0;

/// Width of the editor's vertical scrollbar.
pub const EDITOR_SCROLLBAR_WIDTH: f32 = 10.0;

/// Radius of the editor's floating dialogs (Go to line).
pub const RADIUS_DIALOG: f32 = 12.0;

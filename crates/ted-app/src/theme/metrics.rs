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
pub const STATUS_BAR_HEIGHT: f32 = 24.0;
/// Side length of the "t" logo in the title bar.
pub const LOGO_SIZE: f32 = 18.0;

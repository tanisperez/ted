//! Filesystem access: reads files in the background, lists one folder at a time honoring
//! `.gitignore`, watches for changes and saves atomically. No `gpui`: every function here blocks,
//! the app calls them on its background executor.

mod load;

pub use load::{FileContent, LoadError, LoadedFile, load_file, load_head};

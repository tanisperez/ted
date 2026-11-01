//! Filesystem access: reads files in the background, lists one folder at a time honoring
//! `.gitignore`, watches for changes and saves atomically. No `gpui`: every function here blocks,
//! the app calls them on its background executor.

mod list;
mod load;
mod paths;
mod save;
mod watch;

pub use list::{DirEntry, EntryKind, ExplorerFilter, ListError, list_dir, natural_cmp};
pub use load::{FileContent, LoadError, LoadedFile, load_file, load_head};
pub use paths::plain_path;
pub use save::{SaveError, modified_time, save_file};
pub use watch::{FolderWatcher, WatchError};

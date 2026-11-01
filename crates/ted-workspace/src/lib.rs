//! Filesystem access: reads files in the background, lists one folder at a time honoring
//! `.gitignore` (or a whole folder for the global search), watches for changes, saves atomically
//! and keeps ted's document icons for the file types it opens by default (freedesktop systems).
//! No `gpui`: every function here blocks, the app calls them on its background executor.

mod index;
mod list;
mod load;
mod mime_icons;
mod paths;
mod save;
mod watch;

pub use index::{FileIndex, index_files};
pub use list::{DirEntry, EntryKind, ExplorerFilter, ListError, list_dir, natural_cmp};
pub use load::{FileContent, LoadError, LoadedFile, load_file, load_head};
pub use mime_icons::{MimeIconsSync, XdgDirs, sync_mime_icons};
pub use paths::plain_path;
pub use save::{SaveError, modified_time, save_file};
pub use watch::{FolderWatcher, WatchError};

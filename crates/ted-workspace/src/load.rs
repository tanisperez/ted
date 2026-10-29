//! Loading a file into a [`Buffer`]: one read, binary detection, UTF-8 validation and BOM.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use ted_core::Buffer;

/// How much of the start of the file is searched for NUL bytes to call it binary.
const BINARY_SAMPLE_LEN: usize = 8 * 1024;

/// The UTF-8 byte order mark.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// What a file turned out to hold.
#[derive(Debug)]
pub enum FileContent {
    /// UTF-8 text, or lossily decoded text (then `lossy` is true and the app opens it read only,
    /// because saving would replace the bytes that were not valid).
    Text {
        buffer: Buffer,
        lossy: bool,
        /// The file started with a UTF-8 BOM. It is not in the buffer; saving restores it.
        bom: bool,
    },
    /// NUL bytes in the first block: not shown as text.
    Binary,
}

/// A file read from disk.
#[derive(Debug)]
pub struct LoadedFile {
    pub path: PathBuf,
    /// The size of the whole file on disk, also for [`load_head`], which reads only the start.
    pub size: u64,
    /// When the file was last modified, to notice changes made by other programs.
    pub modified: Option<SystemTime>,
    pub content: FileContent,
}

/// Why a file could not be loaded. Every variant carries the path, for the error message.
#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("{} does not exist", .0.display())]
    NotFound(PathBuf),
    #[error("{} is a directory", .0.display())]
    IsDirectory(PathBuf),
    #[error("permission denied reading {}", .0.display())]
    PermissionDenied(PathBuf),
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl LoadError {
    fn new(path: &Path, source: io::Error) -> Self {
        match source.kind() {
            io::ErrorKind::NotFound => Self::NotFound(path.to_path_buf()),
            io::ErrorKind::PermissionDenied => Self::PermissionDenied(path.to_path_buf()),
            _ => Self::Io {
                path: path.to_path_buf(),
                source,
            },
        }
    }
}

/// Reads the whole file. Blocking: call it off the UI thread.
pub fn load_file(path: &Path) -> Result<LoadedFile, LoadError> {
    let start = Instant::now();
    let (mut file, size, modified) = open(path)?;
    // One allocation sized from the metadata and one read, no per-line work.
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0).saturating_add(1));
    file.read_to_end(&mut bytes)
        .map_err(|e| LoadError::new(path, e))?;
    let loaded = decode(path, size, modified, bytes);
    log::info!("loaded a file of {size} bytes in {:?}", start.elapsed());
    Ok(loaded)
}

/// The first `max_bytes` of the file, cut after a line end (and so at a char boundary), to show
/// the first screen of a huge file before [`load_file`] finishes. When the file is smaller it is
/// the whole file. Same binary detection as [`load_file`].
pub fn load_head(path: &Path, max_bytes: usize) -> Result<LoadedFile, LoadError> {
    let start = Instant::now();
    let (file, size, modified) = open(path)?;
    let mut bytes = Vec::with_capacity(max_bytes.min(usize::try_from(size).unwrap_or(usize::MAX)));
    file.take(max_bytes as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| LoadError::new(path, e))?;
    if (bytes.len() as u64) < size {
        cut_head(&mut bytes);
    }
    let loaded = decode(path, size, modified, bytes);
    log::info!(
        "loaded the head of a file of {size} bytes in {:?}",
        start.elapsed()
    );
    Ok(loaded)
}

fn open(path: &Path) -> Result<(File, u64, Option<SystemTime>), LoadError> {
    let file = File::open(path).map_err(|e| LoadError::new(path, e))?;
    let metadata = file.metadata().map_err(|e| LoadError::new(path, e))?;
    if metadata.is_dir() {
        return Err(LoadError::IsDirectory(path.to_path_buf()));
    }
    Ok((file, metadata.len(), metadata.modified().ok()))
}

/// Shortens a truncated read to end after its last `\n`. With no line end in the block, falls back
/// to the last whole char, so a multi byte char is never split.
fn cut_head(bytes: &mut Vec<u8>) {
    if let Some(newline) = bytes.iter().rposition(|&b| b == b'\n') {
        bytes.truncate(newline + 1);
    } else if let Err(e) = std::str::from_utf8(bytes)
        && e.error_len().is_none()
    {
        bytes.truncate(e.valid_up_to());
    }
}

fn decode(path: &Path, size: u64, modified: Option<SystemTime>, mut bytes: Vec<u8>) -> LoadedFile {
    let content = if bytes[..bytes.len().min(BINARY_SAMPLE_LEN)].contains(&0) {
        FileContent::Binary
    } else {
        let bom = bytes.starts_with(BOM);
        if bom {
            bytes.drain(..BOM.len());
        }
        let (text, lossy) = match String::from_utf8(bytes) {
            Ok(text) => (text, false),
            Err(e) => (String::from_utf8_lossy(e.as_bytes()).into_owned(), true),
        };
        FileContent::Text {
            buffer: Buffer::from_text(&text),
            lossy,
            bom,
        }
    };
    LoadedFile {
        path: path.to_path_buf(),
        size,
        modified,
        content,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &tempfile::TempDir, bytes: &[u8]) -> PathBuf {
        let path = dir.path().join("file.txt");
        fs::write(&path, bytes).unwrap();
        path
    }

    fn text(loaded: LoadedFile) -> (Buffer, bool, bool) {
        match loaded.content {
            FileContent::Text { buffer, lossy, bom } => (buffer, lossy, bom),
            FileContent::Binary => panic!("expected text"),
        }
    }

    #[test]
    fn plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_file(&write(&dir, b"one\ntwo\n")).unwrap();
        assert_eq!(loaded.size, 8);
        let (buffer, lossy, bom) = text(loaded);
        assert!(!lossy && !bom);
        assert_eq!(buffer.line(1), "two");
    }

    #[test]
    fn crlf() {
        let dir = tempfile::tempdir().unwrap();
        let (buffer, lossy, _) = text(load_file(&write(&dir, b"a\r\nb\r\n")).unwrap());
        assert!(!lossy);
        assert_eq!(buffer.line(0), "a");
        assert_eq!(buffer.line_ending(), ted_core::LineEnding::CrLf);
    }

    #[test]
    fn empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_file(&write(&dir, b"")).unwrap();
        assert_eq!(loaded.size, 0);
        let (buffer, lossy, bom) = text(loaded);
        assert!(!lossy && !bom);
        assert_eq!(buffer.line_count(), 1);
    }

    #[test]
    fn bom_is_stripped_and_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let (buffer, lossy, bom) = text(load_file(&write(&dir, b"\xEF\xBB\xBFhi")).unwrap());
        assert!(bom && !lossy);
        assert_eq!(buffer.line(0), "hi");
    }

    #[test]
    fn invalid_utf8_is_lossy() {
        let dir = tempfile::tempdir().unwrap();
        let (buffer, lossy, _) = text(load_file(&write(&dir, b"caf\xE9\n")).unwrap());
        assert!(lossy);
        assert_eq!(buffer.line(0), "caf\u{FFFD}");
    }

    #[test]
    fn nul_bytes_are_binary() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_file(&write(&dir, b"MZ\0\0data")).unwrap();
        assert!(matches!(loaded.content, FileContent::Binary));
        let loaded = load_head(&write(&dir, b"MZ\0\0data"), 4).unwrap();
        assert!(matches!(loaded.content, FileContent::Binary));
    }

    #[test]
    fn not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_file(&dir.path().join("missing")).unwrap_err();
        assert!(matches!(err, LoadError::NotFound(_)));
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn directory() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_file(dir.path()).unwrap_err();
        assert!(matches!(err, LoadError::IsDirectory(_)));
    }

    #[test]
    fn head_cuts_at_a_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, b"aaaa\nbbbb\ncccc\n");
        let loaded = load_head(&path, 12).unwrap();
        assert_eq!(loaded.size, 15);
        let (buffer, _, _) = text(loaded);
        assert_eq!(buffer.line_count(), 3);
        assert_eq!(buffer.line(1), "bbbb");
        assert_eq!(buffer.line(2), "");
    }

    #[test]
    fn head_cuts_at_a_char() {
        let dir = tempfile::tempdir().unwrap();
        // "é" is two bytes; the limit falls in the middle of the second one and there is no
        // newline to fall back to.
        let path = write(&dir, "aéé".as_bytes());
        let (buffer, lossy, _) = text(load_head(&path, 4).unwrap());
        assert!(!lossy);
        assert_eq!(buffer.line(0), "aé");
    }

    #[test]
    fn head_of_a_small_file_is_all_of_it() {
        let dir = tempfile::tempdir().unwrap();
        let (buffer, _, _) = text(load_head(&write(&dir, b"no newline"), 1000).unwrap());
        assert_eq!(buffer.line(0), "no newline");
    }

    /// Loading the 100 MB log of `make bench-files`. Run with
    /// `cargo test --release -p ted-workspace -- --ignored --nocapture big_log` to see the time.
    #[test]
    #[ignore = "needs `make bench-files`, a timing to read by hand in release"]
    fn big_log() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench-files/big.log");
        if !path.exists() {
            println!("{} not found, run `make bench-files`", path.display());
            return;
        }
        let start = Instant::now();
        let loaded = load_file(&path).unwrap();
        let full = start.elapsed();
        let start = Instant::now();
        let head = load_head(&path, 256 * 1024).unwrap();
        let head_time = start.elapsed();
        let (buffer, lossy, _) = text(loaded);
        println!(
            "{} MB, {} lines, lossy {lossy}: load_file in {full:?}, load_head(256 KB) in {head_time:?}",
            fs::metadata(&path).unwrap().len() / 1_000_000,
            buffer.line_count()
        );
        assert!(matches!(head.content, FileContent::Text { .. }));
    }
}

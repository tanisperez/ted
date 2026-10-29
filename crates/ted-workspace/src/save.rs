//! Saving a buffer to disk atomically: the text goes to a temporary file in the same folder, which
//! then replaces the original with a rename, so a crash or a full disk never leaves a half written
//! file. The original's permissions are kept, a symbolic link keeps pointing at the file it points
//! at, and the rope is written chunk by chunk, never copied whole.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use ted_core::Rope;

/// The UTF-8 byte order mark, written first when the file had one.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Why a file could not be saved. Carries the path, for the error message.
#[derive(thiserror::Error, Debug)]
#[error("cannot save {}: {source}", path.display())]
pub struct SaveError {
    pub path: PathBuf,
    #[source]
    pub source: io::Error,
}

/// Writes `rope` to `path`, with a UTF-8 BOM first when `bom`. Blocking: call it off the UI
/// thread. Returns the file's new modification time.
pub fn save_file(path: &Path, rope: &Rope, bom: bool) -> Result<Option<SystemTime>, SaveError> {
    let started = Instant::now();
    let error = |source| SaveError {
        path: path.to_path_buf(),
        source,
    };
    // Through a symbolic link: replace the file it points at, not the link.
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = dir.join(format!(".{name}.ted-{}.tmp", std::process::id()));
    let permissions = fs::metadata(&target).ok().map(|meta| meta.permissions());

    let written = write_temp(&temp, rope, bom).and_then(|file| {
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        drop(file);
        fs::rename(&temp, &target)
    });
    if let Err(source) = written {
        let _ = fs::remove_file(&temp);
        return Err(error(source));
    }
    let modified = fs::metadata(&target).and_then(|meta| meta.modified()).ok();
    log::info!(
        "saved a file of {} bytes in {:?}",
        rope.len(),
        started.elapsed()
    );
    Ok(modified)
}

/// Writes the temporary file and flushes it to the disk.
fn write_temp(temp: &Path, rope: &Rope, bom: bool) -> io::Result<File> {
    let file = OpenOptions::new().write(true).create_new(true).open(temp)?;
    let mut writer = BufWriter::with_capacity(256 * 1024, file);
    if bom {
        writer.write_all(BOM)?;
    }
    for chunk in rope.chunks() {
        writer.write_all(chunk.as_bytes())?;
    }
    let file = writer.into_inner().map_err(|error| error.into_error())?;
    file.sync_all()?;
    Ok(file)
}

/// The modification time of `path`, `None` when it cannot be read (removed, no permission).
pub fn modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{FileContent, load_file};
    use pretty_assertions::assert_eq;

    fn round_trip(bytes: &[u8]) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        fs::write(&path, bytes).unwrap();
        let loaded = load_file(&path).unwrap();
        let FileContent::Text { buffer, bom, .. } = loaded.content else {
            panic!("not text");
        };
        save_file(&path, buffer.rope(), bom).unwrap();
        fs::read(&path).unwrap()
    }

    #[test]
    fn saving_an_unedited_file_keeps_it_byte_for_byte() {
        for bytes in [
            &b"one\ntwo\n"[..],
            b"one\r\ntwo\r\n",
            b"mixed\r\nends\nhere",
            b"\xEF\xBB\xBFwith a bom\n",
            "ñandú 😀\n".as_bytes(),
            b"",
        ] {
            assert_eq!(round_trip(bytes), bytes);
        }
    }

    #[test]
    fn a_big_text_is_written_whole() {
        let text = "line of text\n".repeat(200_000);
        assert_eq!(round_trip(text.as_bytes()), text.as_bytes());
    }

    #[test]
    fn permissions_are_kept_and_no_temporary_file_stays() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.sh");
        fs::write(&path, "old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
        }
        save_file(&path, &Rope::from_str("new"), false).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o750);
        }
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert_eq!(names.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_keeps_pointing_at_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        let link = dir.path().join("link.txt");
        fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        save_file(&link, &Rope::from_str("new"), false).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no/such/dir/file.txt");
        let error = save_file(&path, &Rope::from_str("x"), false).unwrap_err();
        assert_eq!(error.path, path);
    }
}

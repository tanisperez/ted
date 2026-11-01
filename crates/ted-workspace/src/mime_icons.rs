//! ted's document icon for the MIME types ted opens by default, on Linux and the other
//! freedesktop systems.
//!
//! File managers (Dolphin, Nautilus) take a file's icon from its MIME type and the icon theme,
//! not from its default app. The user's shared-mime-info database can change the icon of a MIME
//! type for that user alone, so ted declares one, only for the types whose default app is ted:
//!
//! 1. `<data home>/mime/packages/codes.tanis.ted.xml` gives each of those types
//!    `<icon name="codes.tanis.ted-text-markdown"/>`.
//! 2. The icons go to `<data home>/icons/hicolor/*/mimetypes/` with those names. No theme has an
//!    icon with that name, so the lookup falls back to ted's.
//! 3. `update-mime-database <data home>/mime` rebuilds the user's database, only when the XML
//!    changed.
//!
//! Nothing outside the user's home is touched. Every function here blocks: the app calls them on
//! its background executor.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// The desktop file id of ted, as `mimeapps.list` names it.
pub const DESKTOP_ID: &str = "codes.tanis.ted.desktop";

/// The prefix of every icon ted installs, and the name of its MIME package.
const ICON_PREFIX: &str = "codes.tanis.ted-";
const PACKAGE_FILE: &str = "codes.tanis.ted.xml";

/// The MIME types of `packaging/linux/codes.tanis.ted.desktop` that have a document icon, and
/// the icon of each. C sources have no icon of their own and `inode/directory` must never change.
const MIME_ICONS: &[(&str, &str)] = &[
    ("text/plain", "txt"),
    ("text/markdown", "md"),
    ("text/x-markdown", "md"),
    ("text/x-log", "log"),
    ("text/x-rust", "rs"),
    ("application/json", "json"),
    ("application/toml", "toml"),
    ("text/x-toml", "toml"),
    ("application/xml", "xml"),
    ("text/xml", "xml"),
    ("text/html", "html"),
    ("text/javascript", "js"),
    ("application/javascript", "js"),
    ("text/x-makefile", "mk"),
    ("text/x-python", "py"),
    ("text/x-shellscript", "sh"),
    ("application/x-yaml", "yaml"),
    ("text/csv", "csv"),
];

/// The SVG masters of `packaging/icons/documents`, built into the binary so the AppImage and the
/// tarball work too.
const MASTERS: &[(&str, &str)] = &[
    (
        "csv",
        include_str!("../../../packaging/icons/documents/csv.svg"),
    ),
    (
        "html",
        include_str!("../../../packaging/icons/documents/html.svg"),
    ),
    (
        "js",
        include_str!("../../../packaging/icons/documents/js.svg"),
    ),
    (
        "json",
        include_str!("../../../packaging/icons/documents/json.svg"),
    ),
    (
        "log",
        include_str!("../../../packaging/icons/documents/log.svg"),
    ),
    (
        "md",
        include_str!("../../../packaging/icons/documents/md.svg"),
    ),
    (
        "mk",
        include_str!("../../../packaging/icons/documents/mk.svg"),
    ),
    (
        "py",
        include_str!("../../../packaging/icons/documents/py.svg"),
    ),
    (
        "rs",
        include_str!("../../../packaging/icons/documents/rs.svg"),
    ),
    (
        "sh",
        include_str!("../../../packaging/icons/documents/sh.svg"),
    ),
    (
        "toml",
        include_str!("../../../packaging/icons/documents/toml.svg"),
    ),
    (
        "txt",
        include_str!("../../../packaging/icons/documents/txt.svg"),
    ),
    (
        "xml",
        include_str!("../../../packaging/icons/documents/xml.svg"),
    ),
    (
        "yaml",
        include_str!("../../../packaging/icons/documents/yaml.svg"),
    ),
];

/// The hicolor folders the icons go to. The small sizes get the icon without the type written
/// on it, as the design draws it below 40 px.
const SCALABLE_DIR: &str = "scalable/mimetypes";
const SMALL_DIRS: [&str; 4] = [
    "16x16/mimetypes",
    "22x22/mimetypes",
    "24x24/mimetypes",
    "32x32/mimetypes",
];

/// The XDG folders the sync reads and writes, resolved once from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdgDirs {
    /// `$XDG_CONFIG_HOME`, `~/.config` by default: where file managers write `mimeapps.list`.
    pub config_home: PathBuf,
    /// `$XDG_CONFIG_DIRS`, `/etc/xdg` by default.
    pub config_dirs: Vec<PathBuf>,
    /// `$XDG_DATA_HOME`, `~/.local/share` by default.
    pub data_home: PathBuf,
    /// `$XDG_CURRENT_DESKTOP` in lower case (`kde`, `gnome`), for `kde-mimeapps.list`.
    pub desktops: Vec<String>,
}

impl XdgDirs {
    /// The folders of this session, `None` when the home folder is unknown.
    pub fn from_env() -> Option<Self> {
        Self::from_vars(|name| std::env::var(name).ok(), std::env::home_dir())
    }

    fn from_vars(var: impl Fn(&str) -> Option<String>, home: Option<PathBuf>) -> Option<Self> {
        // The spec ignores relative paths and empty values.
        let absolute = |name: &str| {
            var(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let home = home.filter(|path| path.is_absolute());
        let config_home =
            absolute("XDG_CONFIG_HOME").or_else(|| Some(home.as_ref()?.join(".config")))?;
        let data_home = absolute("XDG_DATA_HOME")
            .or_else(|| Some(home.as_ref()?.join(".local").join("share")))?;
        let config_dirs = match var("XDG_CONFIG_DIRS").filter(|value| !value.is_empty()) {
            Some(value) => value
                .split(':')
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .collect(),
            None => vec![PathBuf::from("/etc/xdg")],
        };
        let desktops = var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .filter(|name| !name.is_empty())
            .map(str::to_lowercase)
            .collect();
        Some(XdgDirs {
            config_home,
            config_dirs,
            data_home,
            desktops,
        })
    }

    /// Every `mimeapps.list`, in the order the spec gives them precedence: per folder, the
    /// desktop specific ones first.
    pub fn mimeapps_lists(&self) -> Vec<PathBuf> {
        let applications = self.data_home.join("applications");
        let folders = std::iter::once(&self.config_home)
            .chain(&self.config_dirs)
            .chain(std::iter::once(&applications));
        let mut lists = Vec::new();
        for folder in folders {
            for desktop in &self.desktops {
                lists.push(folder.join(format!("{desktop}-mimeapps.list")));
            }
            lists.push(folder.join("mimeapps.list"));
        }
        lists
    }

    /// The user's shared-mime-info folder, `<data home>/mime`.
    pub fn mime_dir(&self) -> PathBuf {
        self.data_home.join("mime")
    }

    fn package_path(&self) -> PathBuf {
        self.mime_dir().join("packages").join(PACKAGE_FILE)
    }

    fn hicolor(&self) -> PathBuf {
        self.data_home.join("icons").join("hicolor")
    }
}

/// The `[Default Applications]` of one `mimeapps.list`: each MIME type with the first desktop id
/// of its list.
fn parse_defaults(text: &str) -> Vec<(String, String)> {
    let mut defaults = Vec::new();
    let mut in_defaults = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_defaults = line == "[Default Applications]";
            continue;
        }
        if !in_defaults || line.starts_with('#') {
            continue;
        }
        let Some((mime, apps)) = line.split_once('=') else {
            continue;
        };
        let first = apps.split(';').map(str::trim).find(|app| !app.is_empty());
        if let Some(app) = first {
            defaults.push((mime.trim().to_string(), app.to_string()));
        }
    }
    defaults
}

/// The MIME types whose default app is ted and that have a document icon. `lists` are the
/// contents of the `mimeapps.list` files in precedence order: the first one that names a type
/// decides its default.
pub fn ted_default_types(lists: &[String]) -> BTreeSet<&'static str> {
    let mut decided: BTreeMap<String, String> = BTreeMap::new();
    for text in lists {
        for (mime, app) in parse_defaults(text) {
            decided.entry(mime).or_insert(app);
        }
    }
    MIME_ICONS
        .iter()
        .map(|(mime, _)| *mime)
        .filter(|mime| decided.get(*mime).is_some_and(|app| app == DESKTOP_ID))
        .collect()
}

/// `codes.tanis.ted-text-markdown` for `text/markdown`.
fn icon_name(mime: &str) -> String {
    format!("{ICON_PREFIX}{}", mime.replace('/', "-"))
}

/// The SVG master of `mime`'s icon.
fn master(mime: &str) -> Option<&'static str> {
    let (_, icon) = MIME_ICONS
        .iter()
        .find(|(candidate, _)| *candidate == mime)?;
    MASTERS
        .iter()
        .find(|(name, _)| name == icon)
        .map(|(_, svg)| *svg)
}

/// `svg` without the type written under the icon, for the small sizes.
fn without_type(svg: &str) -> String {
    match (svg.find("<text"), svg.find("</text>")) {
        (Some(start), Some(end)) if start < end => {
            format!("{}{}", &svg[..start], &svg[end + "</text>".len()..])
        }
        _ => svg.to_string(),
    }
}

/// The MIME package that gives each of `types` ted's icon.
pub fn mime_package(types: &BTreeSet<&str>) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!-- Written by ted for the file types it opens by default. Turn off \"Show ted's icon \
         on the files it opens by default\" in ted's Settings to remove it. -->\n\
         <mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n",
    );
    for mime in types {
        xml.push_str(&format!(
            "  <mime-type type=\"{mime}\">\n    <icon name=\"{}\"/>\n  </mime-type>\n",
            icon_name(mime)
        ));
    }
    xml.push_str("</mime-info>\n");
    xml
}

/// What a sync did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MimeIconsSync {
    /// The MIME types that show ted's icon now.
    pub types: BTreeSet<&'static str>,
    /// Whether the MIME package changed, so the database must be rebuilt.
    pub package_changed: bool,
}

/// Only one sync at a time: the app may start one while another is still writing.
static SYNC_LOCK: Mutex<()> = Mutex::new(());

/// Makes the user's MIME package and icons match `enabled` and the current defaults: with
/// `enabled`, ted's icon for the types whose default is ted; without it, nothing of ted's. Files
/// are only written when their content changes, and the MIME database is rebuilt only when the
/// package changed. Missing `mimeapps.list` files are normal and skipped.
pub fn sync_mime_icons(dirs: &XdgDirs, enabled: bool) -> io::Result<MimeIconsSync> {
    let _guard = SYNC_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let types = if enabled {
        let lists: Vec<String> = dirs
            .mimeapps_lists()
            .iter()
            .filter_map(|path| fs::read_to_string(path).ok())
            .collect();
        ted_default_types(&lists)
    } else {
        BTreeSet::new()
    };
    write_icons(dirs, &types)?;
    let package_changed = write_package(dirs, &types)?;
    if package_changed {
        update_mime_database(&dirs.mime_dir())?;
    }
    Ok(MimeIconsSync {
        types,
        package_changed,
    })
}

/// Writes the icons of `types` and removes the other icons of ted's.
fn write_icons(dirs: &XdgDirs, types: &BTreeSet<&str>) -> io::Result<()> {
    let hicolor = dirs.hicolor();
    let folders =
        std::iter::once((SCALABLE_DIR, false)).chain(SMALL_DIRS.iter().map(|dir| (*dir, true)));
    for (folder, small) in folders {
        let folder = hicolor.join(folder);
        let mut wanted = BTreeSet::new();
        for mime in types {
            let Some(svg) = master(mime) else {
                continue;
            };
            let name = format!("{}.svg", icon_name(mime));
            let content = if small {
                without_type(svg)
            } else {
                svg.to_string()
            };
            write_if_changed(&folder.join(&name), &content)?;
            wanted.insert(name);
        }
        remove_stale(&folder, &wanted)?;
    }
    Ok(())
}

/// Removes ted's icons in `folder` that are not `wanted`. A missing folder has none.
fn remove_stale(folder: &Path, wanted: &BTreeSet<String>) -> io::Result<()> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(ICON_PREFIX) && name.ends_with(".svg") && !wanted.contains(&name) {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Writes the package for `types`, or removes it when there are none. Returns whether it
/// changed.
fn write_package(dirs: &XdgDirs, types: &BTreeSet<&str>) -> io::Result<bool> {
    let path = dirs.package_path();
    if types.is_empty() {
        return match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        };
    }
    write_if_changed(&path, &mime_package(types))
}

/// Writes `content` to `path`, creating its folders, unless it already holds it. Returns whether
/// it wrote.
fn write_if_changed(path: &Path, content: &str) -> io::Result<bool> {
    if fs::read_to_string(path).is_ok_and(|current| current == content) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(true)
}

/// Runs `update-mime-database` on `mime_dir`. In the tests it does nothing: they never touch
/// the real database.
fn update_mime_database(mime_dir: &Path) -> io::Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    let output = Command::new("update-mime-database")
        .arg(mime_dir)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "update-mime-database failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn dirs(root: &Path) -> XdgDirs {
        XdgDirs {
            config_home: root.join("config"),
            config_dirs: vec![root.join("etc")],
            data_home: root.join("data"),
            desktops: vec!["kde".to_string()],
        }
    }

    #[test]
    fn the_folders_come_from_the_environment_with_the_spec_defaults() {
        let home = Some(PathBuf::from("/home/ana"));
        let none = XdgDirs::from_vars(|_| None, home.clone()).unwrap();
        assert_eq!(none.config_home, PathBuf::from("/home/ana/.config"));
        assert_eq!(none.data_home, PathBuf::from("/home/ana/.local/share"));
        assert_eq!(none.config_dirs, vec![PathBuf::from("/etc/xdg")]);
        assert!(none.desktops.is_empty());

        let set = XdgDirs::from_vars(
            |name| match name {
                "XDG_CONFIG_HOME" => Some("/cfg".into()),
                "XDG_DATA_HOME" => Some("relative/is/ignored".into()),
                "XDG_CONFIG_DIRS" => Some("/a:rel:/b".into()),
                "XDG_CURRENT_DESKTOP" => Some("ubuntu:GNOME".into()),
                _ => None,
            },
            home,
        )
        .unwrap();
        assert_eq!(set.config_home, PathBuf::from("/cfg"));
        assert_eq!(set.data_home, PathBuf::from("/home/ana/.local/share"));
        assert_eq!(
            set.config_dirs,
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
        assert_eq!(set.desktops, vec!["ubuntu", "gnome"]);

        assert_eq!(XdgDirs::from_vars(|_| None, None), None);
    }

    #[test]
    fn the_lists_are_in_precedence_order() {
        let dirs = dirs(Path::new("/r"));
        let lists: Vec<String> = dirs
            .mimeapps_lists()
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        assert_eq!(
            lists,
            vec![
                "/r/config/kde-mimeapps.list",
                "/r/config/mimeapps.list",
                "/r/etc/kde-mimeapps.list",
                "/r/etc/mimeapps.list",
                "/r/data/applications/kde-mimeapps.list",
                "/r/data/applications/mimeapps.list",
            ]
        );
    }

    #[test]
    fn only_the_default_applications_count() {
        let text = "[Added Associations]\ntext/x-rust=codes.tanis.ted.desktop;\n\n\
                    [Default Applications]\n# comment\ntext/markdown=codes.tanis.ted.desktop;zed.desktop;\n\
                    application/json= ;other.desktop;\ntext/plain=\nbroken line\n";
        assert_eq!(
            parse_defaults(text),
            vec![
                ("text/markdown".to_string(), DESKTOP_ID.to_string()),
                ("application/json".to_string(), "other.desktop".to_string()),
            ]
        );
    }

    #[test]
    fn the_first_list_naming_a_type_decides() {
        let user = "[Default Applications]\ntext/markdown=codes.tanis.ted.desktop;\n\
                    text/plain=kate.desktop;\ninode/directory=codes.tanis.ted.desktop;\n\
                    text/x-csrc=codes.tanis.ted.desktop;\n"
            .to_string();
        let system = "[Default Applications]\ntext/plain=codes.tanis.ted.desktop;\n\
                      text/x-log=codes.tanis.ted.desktop;\n"
            .to_string();
        // text/plain is Kate's in the user's list; folders and C sources never get an icon.
        assert_eq!(
            ted_default_types(&[user, system]),
            BTreeSet::from(["text/markdown", "text/x-log"])
        );
    }

    #[test]
    fn every_mime_type_has_its_master() {
        for (mime, _) in MIME_ICONS {
            let svg = master(mime).unwrap();
            assert!(svg.contains("<svg"), "{mime}");
            assert!(!without_type(svg).contains("<text"), "{mime}");
        }
    }

    #[test]
    fn the_package_names_each_icon() {
        let xml = mime_package(&BTreeSet::from(["text/markdown"]));
        assert!(xml.contains(
            "<mime-type type=\"text/markdown\">\n    <icon name=\"codes.tanis.ted-text-markdown\"/>"
        ));
        assert!(!xml.contains('\u{2014}'));
    }

    #[test]
    fn a_sync_follows_the_defaults_and_cleans_up_when_turned_off() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = dirs(tmp.path());
        let list = dirs.config_home.join("mimeapps.list");
        fs::create_dir_all(&dirs.config_home).unwrap();
        fs::write(
            &list,
            "[Default Applications]\ntext/markdown=codes.tanis.ted.desktop;\n",
        )
        .unwrap();
        let scalable = dirs.hicolor().join(SCALABLE_DIR);
        let small = dirs.hicolor().join("16x16/mimetypes");
        let markdown = "codes.tanis.ted-text-markdown.svg";

        let first = sync_mime_icons(&dirs, true).unwrap();
        assert_eq!(first.types, BTreeSet::from(["text/markdown"]));
        assert!(first.package_changed);
        assert!(
            fs::read_to_string(scalable.join(markdown))
                .unwrap()
                .contains("MD</text>")
        );
        assert!(
            !fs::read_to_string(small.join(markdown))
                .unwrap()
                .contains("<text")
        );
        assert!(
            fs::read_to_string(dirs.package_path())
                .unwrap()
                .contains("text/markdown")
        );

        // Nothing changed: the database is not rebuilt.
        assert!(!sync_mime_icons(&dirs, true).unwrap().package_changed);

        // Another app becomes the default for Markdown, ted for JSON.
        fs::write(
            &list,
            "[Default Applications]\ntext/markdown=zed.desktop;\n\
             application/json=codes.tanis.ted.desktop;\n",
        )
        .unwrap();
        // Someone else's icon in the same folder is never touched.
        fs::write(scalable.join("other-app.svg"), "<svg/>").unwrap();
        let moved = sync_mime_icons(&dirs, true).unwrap();
        assert_eq!(moved.types, BTreeSet::from(["application/json"]));
        assert!(moved.package_changed);
        assert!(!scalable.join(markdown).exists());
        assert!(
            scalable
                .join("codes.tanis.ted-application-json.svg")
                .exists()
        );

        let off = sync_mime_icons(&dirs, false).unwrap();
        assert!(off.types.is_empty());
        assert!(off.package_changed);
        assert!(!dirs.package_path().exists());
        let left: Vec<_> = fs::read_dir(&scalable)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, vec!["other-app.svg"]);
        assert!(!sync_mime_icons(&dirs, false).unwrap().package_changed);
    }

    #[test]
    fn no_default_list_means_no_icons() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = dirs(tmp.path());
        let sync = sync_mime_icons(&dirs, true).unwrap();
        assert!(sync.types.is_empty());
        assert!(!sync.package_changed);
        assert!(!dirs.data_home.exists());
    }
}

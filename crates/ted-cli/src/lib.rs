//! The command line of ted: the commands that run in the terminal and exit instead of
//! opening the window. No `gpui`, so everything is unit tested without a window.
//!
//! [`start`] reads every argument: `-h`/`--help` and `-V`/`--version` print and exit, an
//! unknown option is a usage error, and everything else is a path to open (after `--`, even
//! what starts with a dash). The app decides what the paths mean (`ted .`, `ted a.txt b.txt`).
//!
//! To add a command:
//! 1. Create a module with its type, a `parse` from one argument, a `run` returning an exit
//!    code and its help text (see [`info`]).
//! 2. Register it in one place: a variant in [`Command`], a line in [`Command::parse`] and one
//!    in [`Command::run`], and mention it in the help of [`info`].
//! 3. Add its tests next to it.

pub mod info;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::LazyLock;

use telaio_app::AppIdentity;

pub use info::Info;

/// Who ted is, for every piece of `telaio-app` the app and the command line use: the name,
/// the lowercase name of folders and files, the application id (Wayland `app_id`, X11 `WM_CLASS`,
/// Windows AppUserModelID, the same as `codes.tanis.ted.desktop` and the Windows installer
/// shortcut), the version, the environment variable prefix and the repository.
pub const IDENTITY: AppIdentity = AppIdentity {
    name: "ted",
    slug: "ted",
    app_id: "codes.tanis.ted",
    version: env!("CARGO_PKG_VERSION"),
    env_prefix: "TED",
    repository: "https://github.com/tanisperez/ted",
};

/// The short git commit this build was made from, captured by `build.rs` (or taken from the
/// `TED_COMMIT` environment variable at build time). `None` outside a repository, for example
/// a source tarball.
pub const COMMIT: Option<&str> = option_env!("TED_COMMIT");

/// The suffix of the versions between two releases (`0.4.0-dev`), whose builds show their commit.
const DEV_SUFFIX: &str = "-dev";

/// The version as shown to people: `0.4.0-dev (4e74e2d)` for a development build with a known
/// commit, the bare version otherwise. A release is named by its tag, so it never shows a commit.
/// The updater, the release notes link and anything machine readable use the bare version.
pub fn format_version_label(version: &str, commit: Option<&str>) -> String {
    match commit {
        Some(commit) if version.ends_with(DEV_SUFFIX) => format!("{version} ({commit})"),
        _ => version.to_string(),
    }
}

/// [`format_version_label`] of this build.
pub fn version_label() -> &'static str {
    static LABEL: LazyLock<&'static str> = LazyLock::new(|| {
        let label = format_version_label(IDENTITY.version, COMMIT);
        // `AppIdentity` wants a `&'static str`: leaked once, for the life of the process.
        Box::leak(label.into_boxed_str())
    });
    *LABEL
}

/// [`IDENTITY`] with the version label, for what only displays the version (About, the version
/// line). Pass [`IDENTITY`] itself to everything that needs the bare version.
pub fn display_identity() -> AppIdentity {
    AppIdentity {
        version: version_label(),
        ..IDENTITY
    }
}

/// A command line command that runs without opening the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `-h`/`--help` and `-V`/`--version`.
    Info(Info),
    /// An option ted does not know: a usage error.
    Unknown(String),
}

impl Command {
    /// The command asked by one argument, or `None` when it is a path.
    pub fn parse(arg: &str) -> Option<Self> {
        if let Some(info) = Info::parse(arg) {
            return Some(Command::Info(info));
        }
        // A lone `-` is a file name, as for most tools.
        (arg.starts_with('-') && arg != "-").then(|| Command::Unknown(arg.to_string()))
    }

    /// Runs the command and returns the process exit code.
    pub fn run(self) -> ExitCode {
        let code = match self {
            Command::Info(info) => info.run(),
            Command::Unknown(option) => {
                let _ = std::io::stderr().write_all(unknown_option(&option).as_bytes());
                telaio_app::cli::EXIT_USAGE
            }
        };
        ExitCode::from(code)
    }
}

/// The message of an unknown option, on stderr.
fn unknown_option(option: &str) -> String {
    format!("error: unknown option '{option}'\nRun 'ted --help' to see the options.\n")
}

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// Print something and exit.
    Command(Command),
    /// Open the window with these paths, in order (none for the empty state).
    Open(Vec<PathBuf>),
}

/// Reads the arguments (without the program name). The first command found wins; `--` ends the
/// options, so `ted -- -notes.txt` opens a file named `-notes.txt`.
pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Launch {
    let mut paths = Vec::new();
    let mut options = true;
    for arg in args {
        if options {
            if arg == "--" {
                options = false;
                continue;
            }
            if let Some(command) = arg.to_str().and_then(Command::parse) {
                return Launch::Command(command);
            }
        }
        paths.push(PathBuf::from(arg));
    }
    Launch::Open(paths)
}

/// Handles the command line before anything else is set up: runs a command (attaching to the
/// parent console first on Windows) and returns its exit code as the error, or returns the paths
/// the app opens.
pub fn start(args: impl IntoIterator<Item = OsString>) -> Result<Vec<PathBuf>, ExitCode> {
    match parse_args(args) {
        Launch::Open(paths) => Ok(paths),
        Launch::Command(command) => {
            telaio_app::cli::attach_parent_console();
            Err(command.run())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dev_build_shows_its_commit_next_to_the_version() {
        assert_eq!(
            format_version_label("0.4.0-dev", Some("4e74e2d")),
            "0.4.0-dev (4e74e2d)"
        );
        assert_eq!(format_version_label("0.4.0-dev", None), "0.4.0-dev");
    }

    #[test]
    fn a_release_is_named_by_its_tag_only() {
        assert_eq!(format_version_label("0.4.0", Some("4e74e2d")), "0.4.0");
        assert_eq!(
            format_version_label("0.4.0-rc.1", Some("4e74e2d")),
            "0.4.0-rc.1"
        );
        assert_eq!(format_version_label("0.4.0", None), "0.4.0");
    }

    #[test]
    fn the_display_identity_only_changes_the_version() {
        let display = display_identity();
        assert_eq!(display.version, version_label());
        assert_eq!(display.name, IDENTITY.name);
        assert_eq!(display.repository, IDENTITY.repository);
        assert!(version_label().starts_with(IDENTITY.version));
    }

    #[test]
    fn a_path_is_not_a_command_and_a_dash_option_is() {
        assert_eq!(Command::parse("/home/me/notes.txt"), None);
        assert_eq!(Command::parse("-"), None);
        assert_eq!(
            Command::parse("-x"),
            Some(Command::Unknown("-x".to_string()))
        );
    }

    #[test]
    fn the_info_flags_are_commands() {
        assert_eq!(Command::parse("--help"), Some(Command::Info(Info::Help)));
        assert_eq!(Command::parse("-V"), Some(Command::Info(Info::Version)));
    }

    #[test]
    fn start_returns_the_paths_when_there_is_no_command() {
        assert_eq!(
            start(args(&["src", "a.txt"])),
            Ok(vec!["src".into(), "a.txt".into()])
        );
    }

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn paths_in_order_and_commands_anywhere() {
        assert_eq!(parse_args(args(&[])), Launch::Open(Vec::new()));
        assert_eq!(
            parse_args(args(&[".", "a.txt", "-"])),
            Launch::Open(vec![".".into(), "a.txt".into(), "-".into()])
        );
        assert_eq!(
            parse_args(args(&["a.txt", "--version"])),
            Launch::Command(Command::Info(Info::Version))
        );
        assert_eq!(
            parse_args(args(&["-h", "--foo"])),
            Launch::Command(Command::Info(Info::Help))
        );
    }

    #[test]
    fn unknown_options_are_errors_unless_after_the_separator() {
        assert_eq!(
            parse_args(args(&["a.txt", "--foo"])),
            Launch::Command(Command::Unknown("--foo".to_string()))
        );
        assert_eq!(
            parse_args(args(&["--", "--foo", "-h"])),
            Launch::Open(vec!["--foo".into(), "-h".into()])
        );
        assert!(unknown_option("--foo").contains("ted --help"));
    }
}

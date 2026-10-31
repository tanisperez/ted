//! Application settings, persisted to `<OS config dir>/ted/settings.toml` and applied live: there
//! is no explicit "Save" action, every change made through the Settings view writes the file at
//! once.
//!
//! Only what 0.1.0 needs: language, theme, fonts, the editor's gutter and indentation, the save
//! cleanup, the explorer's exclude patterns, the highlighting limit and the log level. Each
//! feature adds its own settings as it lands.

use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use rust_i18n::t;
use serde::Serialize;

use super::config::config_dir;
use super::locale::LanguageChoice;

pub use telaio_app::logging::LogLevel;

/// The path, relative to the config directory ([`config_dir`]), of the settings file.
const SETTINGS_FILE: &str = "settings.toml";

/// Step of the UI font size stepper, in points.
pub const UI_FONT_SIZE_STEP: f32 = 1.0;
/// Step of the editor font size stepper, in points.
pub const MONO_FONT_SIZE_STEP: f32 = 0.5;

/// The categories of the Settings modal, in the order of its left column. Languages joins them
/// with its feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    #[default]
    Appearance,
    Editor,
    Files,
    Git,
    About,
}

impl SettingsCategory {
    pub const ALL: [SettingsCategory; 5] = [
        SettingsCategory::Appearance,
        SettingsCategory::Editor,
        SettingsCategory::Files,
        SettingsCategory::Git,
        SettingsCategory::About,
    ];

    /// The title in the left column and in the header.
    pub fn label(self) -> String {
        match self {
            SettingsCategory::Appearance => t!("settings.category.appearance"),
            SettingsCategory::Editor => t!("settings.category.editor"),
            SettingsCategory::Files => t!("settings.category.files"),
            SettingsCategory::Git => t!("settings.category.git"),
            SettingsCategory::About => t!("settings.category.about"),
        }
        .into_owned()
    }
}

/// The explorer's exclude patterns as typed in Settings: comma separated, trimmed, the empty ones
/// left out.
pub fn parse_excludes(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(String::from)
        .collect()
}

/// The tab sizes Settings offers. `settings.toml` accepts any size from 1 to 16.
pub const TAB_SIZES: [usize; 3] = [2, 4, 8];

/// Valid range of [`Settings::ui_font_size`], in points.
pub const UI_FONT_SIZE_RANGE: RangeInclusive<f32> = 11.0..=16.0;

/// Valid range of [`Settings::mono_font_size`], in points.
pub const MONO_FONT_SIZE_RANGE: RangeInclusive<f32> = 10.0..=18.0;

/// Which theme mode the app follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    /// Follow the OS light/dark setting.
    #[default]
    System,
    /// Always use the light theme.
    Light,
    /// Always use the dark theme.
    Dark,
}

impl From<ThemeChoice> for telaio_ui::ThemeChoice {
    fn from(choice: ThemeChoice) -> Self {
        match choice {
            ThemeChoice::System => telaio_ui::ThemeChoice::System,
            ThemeChoice::Light => telaio_ui::ThemeChoice::Light,
            ThemeChoice::Dark => telaio_ui::ThemeChoice::Dark,
        }
    }
}

impl ThemeChoice {
    /// Parses a theme name as read from the settings file, case-insensitive. Returns `None` for
    /// anything else, so the caller can fall back to the default.
    fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "system" => Some(ThemeChoice::System),
            "light" => Some(ThemeChoice::Light),
            "dark" => Some(ThemeChoice::Dark),
            _ => None,
        }
    }
}

/// What Ctrl+Enter does in the commit message, chosen in Settings, "Git". Stored in
/// `settings.toml` as `git_commit_action = "commit" | "commit_and_push"`, as in Postino.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitCommitAction {
    /// Commit only.
    #[default]
    Commit,
    /// Commit, then push.
    CommitAndPush,
}

impl GitCommitAction {
    /// Every choice, in the order the Settings view lists them.
    pub const ALL: [GitCommitAction; 2] = [GitCommitAction::Commit, GitCommitAction::CommitAndPush];

    fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "commit" => Some(GitCommitAction::Commit),
            "commit_and_push" => Some(GitCommitAction::CommitAndPush),
            _ => None,
        }
    }

    /// The label shown in the Settings view.
    pub fn label(self) -> String {
        match self {
            GitCommitAction::Commit => t!("settings.git.action_commit"),
            GitCommitAction::CommitAndPush => t!("settings.git.action_commit_and_push"),
        }
        .into_owned()
    }
}

/// User-configurable settings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Settings {
    /// The UI language: automatic (the system's) or a fixed one.
    pub language: LanguageChoice,
    /// The theme mode: system, light or dark.
    pub theme: ThemeChoice,
    /// The UI font family name.
    pub ui_font: String,
    /// The UI font size, in points, clamped to [`UI_FONT_SIZE_RANGE`].
    pub ui_font_size: f32,
    /// The editor font family name.
    pub mono_font: String,
    /// The editor font size, in points, clamped to [`MONO_FONT_SIZE_RANGE`].
    pub mono_font_size: f32,
    /// Whether the editor shows the line numbers.
    pub line_numbers: bool,
    /// Whether the editor highlights the line of the cursor.
    pub highlight_current_line: bool,
    /// Whether the editor shows the minimap at its right.
    pub minimap: bool,
    /// Columns a tab advances to, and the width of one indentation level in spaces.
    pub tab_size: usize,
    /// Whether Tab inserts spaces in a file whose indentation is not detected (a new or flat
    /// file); a file indented with tabs or spaces keeps its own style.
    pub insert_spaces: bool,
    /// Whether saving removes the spaces and tabs at the end of every line.
    pub trim_trailing_whitespace: bool,
    /// Whether saving ends the file with a line break.
    pub insert_final_newline: bool,
    /// Files bigger than this many MB are not parsed for highlighting and stay plain, so a huge
    /// generated file never costs a long parse nor the memory of its tree. Logs are highlighted
    /// at any size: they need no parse.
    pub highlight_max_size_mb: u32,
    /// What the explorer hides, in the gitignore syntax (`target`, `*.log`). `.gitignore` is
    /// honored too.
    pub explorer_exclude: Vec<String>,
    /// How much is written to the log file.
    pub log_level: LogLevel,
    /// The git executable to use, `None` to search the `PATH`. Absent from the file when
    /// auto-detecting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_executable: Option<PathBuf>,
    /// Whether the open repository is fetched every few minutes, so the status bar shows the
    /// commits to pull.
    pub git_fetch_automatically: bool,
    /// Whether discarding changes asks first.
    pub git_confirm_discard: bool,
    /// What Ctrl+Enter does in the commit message.
    pub git_commit_action: GitCommitAction,
}

impl Settings {
    /// [`Self::highlight_max_size_mb`] in bytes.
    pub fn highlight_max_bytes(&self) -> usize {
        self.highlight_max_size_mb as usize * 1024 * 1024
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: LanguageChoice::default(),
            theme: ThemeChoice::default(),
            ui_font: "Geist".to_string(),
            ui_font_size: 13.0,
            mono_font: "Geist Mono".to_string(),
            mono_font_size: 12.5,
            line_numbers: true,
            highlight_current_line: true,
            minimap: true,
            tab_size: 4,
            insert_spaces: true,
            trim_trailing_whitespace: true,
            insert_final_newline: true,
            highlight_max_size_mb: 4,
            explorer_exclude: [".git", "target", "node_modules"]
                .map(String::from)
                .to_vec(),
            log_level: LogLevel::default(),
            git_executable: None,
            git_fetch_automatically: true,
            git_confirm_discard: true,
            git_commit_action: GitCommitAction::default(),
        }
    }
}

/// Loads the settings from `<OS config dir>/ted/settings.toml`, or [`Settings::default`] if the
/// OS config directory is unknown, the file is missing, or its content is not valid TOML.
pub fn load_settings() -> Settings {
    match config_dir() {
        Some(base) => read_settings(&base),
        None => Settings::default(),
    }
}

/// Writes `settings` to `<OS config dir>/ted/settings.toml`. Failures are logged, not returned:
/// the change stays applied for this session.
pub fn save_settings(settings: &Settings) {
    if let Some(base) = config_dir() {
        write_settings(&base, settings);
    }
}

/// Writes `<base>/settings.toml`. Split out from [`save_settings`] for the tests.
fn write_settings(base: &Path, settings: &Settings) {
    if let Err(error) = fs::create_dir_all(base) {
        log::warn!("could not create {}: {error}", base.display());
        return;
    }
    let text = match toml::to_string_pretty(settings) {
        Ok(text) => text,
        Err(error) => {
            log::warn!("could not serialize the settings: {error}");
            return;
        }
    };
    let path = base.join(SETTINGS_FILE);
    if let Err(error) = fs::write(&path, text) {
        log::warn!("could not save {}: {error}", path.display());
    }
}

/// The real path of the settings file, `None` when the OS config directory is unknown.
pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|base| base.join(SETTINGS_FILE))
}

/// Reads and parses `<base>/settings.toml`. Split out from [`load_settings`] so tests can point
/// `base` at a temporary directory.
fn read_settings(base: &Path) -> Settings {
    match fs::read_to_string(base.join(SETTINGS_FILE)) {
        Ok(content) => parse_settings(&content),
        Err(_) => Settings::default(),
    }
}

/// Every field is read and validated independently: a field of the wrong type or an out of range
/// number falls back to its own default instead of discarding the whole file, and a key this
/// version does not know about is ignored.
fn parse_settings(content: &str) -> Settings {
    let mut settings = Settings::default();
    let Ok(table) = content.parse::<toml::Table>() else {
        log::warn!("settings.toml is not valid TOML, using the defaults");
        return settings;
    };
    let text = |key: &str| table.get(key).and_then(toml::Value::as_str);
    if let Some(value) = text("language").and_then(LanguageChoice::parse) {
        settings.language = value;
    }
    if let Some(value) = text("theme").and_then(ThemeChoice::parse) {
        settings.theme = value;
    }
    if let Some(value) = text("ui_font").filter(|name| !name.trim().is_empty()) {
        settings.ui_font = value.to_string();
    }
    if let Some(value) = text("mono_font").filter(|name| !name.trim().is_empty()) {
        settings.mono_font = value.to_string();
    }
    if let Some(value) = table.get("ui_font_size").and_then(as_f32) {
        settings.ui_font_size = value.clamp(*UI_FONT_SIZE_RANGE.start(), *UI_FONT_SIZE_RANGE.end());
    }
    if let Some(value) = table.get("mono_font_size").and_then(as_f32) {
        settings.mono_font_size =
            value.clamp(*MONO_FONT_SIZE_RANGE.start(), *MONO_FONT_SIZE_RANGE.end());
    }
    let flag = |key: &str| table.get(key).and_then(toml::Value::as_bool);
    if let Some(value) = flag("line_numbers") {
        settings.line_numbers = value;
    }
    if let Some(value) = flag("highlight_current_line") {
        settings.highlight_current_line = value;
    }
    if let Some(value) = flag("minimap") {
        settings.minimap = value;
    }
    if let Some(value) = table.get("tab_size").and_then(toml::Value::as_integer) {
        settings.tab_size = value.clamp(1, 16) as usize;
    }
    if let Some(value) = flag("insert_spaces") {
        settings.insert_spaces = value;
    }
    if let Some(patterns) = table
        .get("explorer_exclude")
        .and_then(toml::Value::as_array)
    {
        settings.explorer_exclude = patterns
            .iter()
            .filter_map(toml::Value::as_str)
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .map(String::from)
            .collect();
    }
    if let Some(value) = table
        .get("trim_trailing_whitespace")
        .and_then(toml::Value::as_bool)
    {
        settings.trim_trailing_whitespace = value;
    }
    if let Some(value) = table
        .get("insert_final_newline")
        .and_then(toml::Value::as_bool)
    {
        settings.insert_final_newline = value;
    }
    if let Some(value) = table
        .get("highlight_max_size_mb")
        .and_then(toml::Value::as_integer)
    {
        settings.highlight_max_size_mb = value.clamp(0, 1024) as u32;
    }
    if let Some(value) = text("log_level").and_then(LogLevel::parse) {
        settings.log_level = value;
    }
    settings.git_executable = text("git_executable")
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    if let Some(value) = flag("git_fetch_automatically") {
        settings.git_fetch_automatically = value;
    }
    if let Some(value) = flag("git_confirm_discard") {
        settings.git_confirm_discard = value;
    }
    if let Some(value) = text("git_commit_action").and_then(GitCommitAction::parse) {
        settings.git_commit_action = value;
    }
    settings
}

/// A TOML number as `f32`, integer or float.
fn as_f32(value: &toml::Value) -> Option<f32> {
    match value {
        toml::Value::Float(number) => Some(*number as f32),
        toml::Value::Integer(number) => Some(*number as f32),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(read_settings(dir.path()), Settings::default());
    }

    #[test]
    fn invalid_toml_gives_the_defaults() {
        assert_eq!(parse_settings("theme = "), Settings::default());
    }

    #[test]
    fn each_field_is_read_on_its_own() {
        let settings = parse_settings(
            "language = \"es\"\ntheme = \"DARK\"\nui_font_size = 14\nmono_font_size = \"big\"\n\
             log_level = \"debug\"\nunknown = 1\n",
        );
        assert_eq!(settings.language, LanguageChoice::Es);
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(settings.ui_font_size, 14.0);
        assert_eq!(settings.mono_font_size, Settings::default().mono_font_size);
        assert_eq!(settings.log_level, LogLevel::Debug);
    }

    #[test]
    fn font_sizes_are_clamped() {
        let settings = parse_settings("ui_font_size = 40\nmono_font_size = 2.0\n");
        assert_eq!(settings.ui_font_size, 16.0);
        assert_eq!(settings.mono_font_size, 10.0);
    }

    #[test]
    fn the_highlight_size_limit_is_clamped() {
        assert_eq!(
            parse_settings("highlight_max_size_mb = 16").highlight_max_size_mb,
            16
        );
        assert_eq!(
            parse_settings("highlight_max_size_mb = -3").highlight_max_size_mb,
            0
        );
        assert_eq!(
            parse_settings("highlight_max_size_mb = \"big\"").highlight_max_size_mb,
            Settings::default().highlight_max_size_mb
        );
    }

    #[test]
    fn saved_settings_read_back_the_same() {
        let dir = tempfile::tempdir().expect("temp dir");
        let settings = Settings {
            language: LanguageChoice::Es,
            theme: ThemeChoice::Light,
            mono_font_size: 14.5,
            line_numbers: false,
            highlight_current_line: false,
            minimap: false,
            tab_size: 2,
            insert_spaces: false,
            trim_trailing_whitespace: false,
            explorer_exclude: vec!["dist".to_string(), "*.log".to_string()],
            log_level: LogLevel::Debug,
            git_executable: Some(PathBuf::from("/opt/git/bin/git")),
            git_fetch_automatically: false,
            git_confirm_discard: false,
            git_commit_action: GitCommitAction::CommitAndPush,
            ..Settings::default()
        };
        write_settings(dir.path(), &settings);
        assert_eq!(read_settings(dir.path()), settings);
        write_settings(dir.path(), &Settings::default());
        assert_eq!(read_settings(dir.path()), Settings::default());
    }

    #[test]
    fn editor_settings_are_validated() {
        let settings = parse_settings(
            "tab_size = 40\nline_numbers = \"no\"\nexplorer_exclude = [\" build \", \"\", 3]\n",
        );
        assert_eq!(settings.tab_size, 16);
        assert!(settings.line_numbers);
        assert_eq!(settings.explorer_exclude, vec!["build".to_string()]);
    }

    #[test]
    fn exclude_patterns_are_split_on_commas() {
        assert_eq!(
            parse_excludes(" .git, target ,, *.log ,"),
            vec![".git", "target", "*.log"]
        );
        assert!(parse_excludes("  ").is_empty());
    }

    #[test]
    fn unsupported_languages_fall_back_to_auto() {
        assert_eq!(
            parse_settings("language = \"it\"").language,
            LanguageChoice::Auto
        );
    }
}

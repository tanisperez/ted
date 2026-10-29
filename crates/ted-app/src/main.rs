//! ted, the text editor. Binary entry point: initializes `gpui`, the theme and the main window.

// A GUI application on Windows: never open a console window behind the main window.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod state;
mod theme;
mod views;

rust_i18n::i18n!("locales", fallback = "en");

use std::path::Path;
use std::process::ExitCode;

use gpui_kit::component::Root;
use gpui_kit::*;
use ted_cli::IDENTITY;
use telaio_app::{logging, window};

use state::settings::{LogLevel, ThemeChoice};
use views::AppView;

fn main() -> ExitCode {
    // `--help` and `--version` print and exit before anything else: no settings, no log.
    if let Some(code) = ted_cli::run_if_cli(std::env::args_os().nth(1).as_deref()) {
        return code;
    }

    // Settings first, for the log level: the logger is installed before anything else runs, so
    // every later message (gpui's own included) reaches the log file.
    let settings = state::settings::load_settings();
    let log_env_var = logging::env_var(&IDENTITY);
    let env_level = std::env::var(&log_env_var).ok();
    let log_level = logging::startup_level(settings.log_level, env_level.as_deref());
    let log_path = logging::init(&IDENTITY, log_level.to_filter());
    let from_env = env_level.as_deref().and_then(LogLevel::parse).is_some();
    log_startup(
        log_level,
        from_env.then_some(log_env_var.as_str()),
        log_path.as_deref(),
    );
    if let Some(value) = env_level.filter(|_| !from_env) {
        log::warn!("ignoring {log_env_var}={value:?}: not a log level");
    }

    // Before the window opens, so its first frame is already in the right language.
    let language = state::locale::resolve(settings.language);
    state::locale::apply(language);
    log::info!(
        "language: {} ({}, system locales: {})",
        language.code(),
        match settings.language.language() {
            Some(_) => "fixed",
            None => "auto",
        },
        state::locale::system_locales().join(", "),
    );

    let theme_choice = settings.theme;

    window::application().run(move |cx| {
        window::init(cx, &IDENTITY);
        theme::install(cx, &settings);

        // 1440x900 logical, capped to 90% of the display (see `window::options`).
        let window_options = window::options(cx, &IDENTITY, window::WindowSizes::default());

        cx.spawn(async move |cx| {
            let opened = cx.open_window(window_options, move |window, cx| {
                // "System" keeps matching the OS light/dark setting, including if the user
                // changes it while ted is running; an explicit choice sets the mode once.
                telaio_ui::ThemeChoice::from(theme_choice).apply(Some(window), cx);

                let view = cx.new(|_| AppView::new(settings));

                // Always listening, whatever the theme choice at startup: Settings can switch to
                // "System" at any time (see `window::follow_system_appearance`).
                let weak = view.downgrade();
                window::follow_system_appearance(window, move |cx| {
                    weak.upgrade()
                        .is_some_and(|view| view.read(cx).settings.theme == ThemeChoice::System)
                });

                cx.new(|cx| Root::new(view, window, cx))
            });
            // Opening the very first window failing is not recoverable: there is nothing left
            // for the app to do.
            #[allow(clippy::expect_used)]
            opened.expect("failed to open the main window");
        })
        .detach();
    });
    ExitCode::SUCCESS
}

/// Logs where and how ted runs: the first lines of every session, and the ones a bug report
/// needs first.
fn log_startup(level: LogLevel, from_env: Option<&str>, log_path: Option<&Path>) {
    log::info!(
        "ted {} starting ({} build) on {} {}",
        ted_cli::version_label(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    log::info!(
        "log level {level:?}{}, log file {}",
        from_env.map_or_else(String::new, |variable| format!(" (from {variable})")),
        log_path.map_or_else(|| "none".to_string(), |path| path.display().to_string()),
    );
    log::info!(
        "settings file {}",
        state::settings::settings_path()
            .map_or_else(|| "unknown".to_string(), |path| path.display().to_string()),
    );
}

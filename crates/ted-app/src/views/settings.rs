//! The Settings modal: opened from the title bar gear and `Ctrl ,` / `Cmd ,`, closed with the
//! cross, Escape or a click outside, as Postino's (`views/settings.rs`). The 0.1.0 subset of the
//! design: Appearance (language, theme, interface size), Editor (font size, line numbers, current
//! line, tab size, spaces), Files (save cleanup, explorer excludes) and About.
//!
//! Every control writes `AppView::settings` and calls [`AppView::apply_settings`], which applies
//! the change at once and saves `settings.toml`: there is no Save button.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rust_i18n::t;
use telaio_ui::components::{
    GhostButton, LabeledRow, NumberStepper, SecondaryButton, SectionHeading, SegmentedControl,
    SegmentedItem, SettingsCategory as NavCategory, SettingsLayout, ThemeCard, settings_dialog,
    switch_halo, text_field,
};

use crate::state::editor::set_indentation;
use crate::state::launch::shorten_path;
use crate::state::locale::{self, LANGUAGES, LanguageChoice};
use crate::state::settings::{
    self, MONO_FONT_SIZE_RANGE, MONO_FONT_SIZE_STEP, Settings, SettingsCategory, Startup,
    TAB_SIZES, ThemeChoice, UI_FONT_SIZE_RANGE, UI_FONT_SIZE_STEP, parse_excludes,
};
use crate::theme::{self, Palette, PaletteExt};

use super::editor::EditorDisplay;
use super::logo::logo;
use super::root::AppView;
use super::settings_git::git_sections;

/// Side of the app icon in the About header.
const ABOUT_ICON_SIZE: f32 = 48.0;
/// Width of the exclude patterns field.
const EXCLUDE_FIELD_WIDTH: f32 = 260.0;

impl AppView {
    /// Opens the Settings modal, unless one is already open.
    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let excludes = self.settings.explorer_exclude.join(", ");
        let input = cx.new(|cx| InputState::new(window, cx).default_value(excludes));
        // Applied when the field is left or Enter is pressed, not on every keystroke: a pattern
        // half typed would hide and show folders while typing.
        cx.subscribe_in(
            &input,
            window,
            |view, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    let excludes = parse_excludes(&input.read(cx).value());
                    view.set_excludes(excludes, window, cx);
                }
            },
        )
        .detach();
        self.exclude_input = Some(input);
        self.create_git_select(window, cx);
        let weak = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, _cx| {
            let weak = weak.clone();
            settings_dialog(dialog, window, move |_window, cx| {
                render_settings(weak.clone(), cx)
            })
        });
    }

    /// Applies `self.settings` everywhere it is read and saves it. Every control ends here.
    pub(crate) fn apply_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = &self.settings;
        theme::apply_settings(cx, settings);
        telaio_ui::ThemeChoice::from(settings.theme).apply(Some(window), cx);
        apply_editor_settings(settings, cx);
        log::info!("settings changed: {settings:?}");
        settings::save_settings(settings);
        cx.refresh_windows();
        cx.notify();
    }

    /// Changes one setting with `change`, then applies and saves them all.
    fn change(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Settings),
    ) {
        change(&mut self.settings);
        self.apply_settings(window, cx);
    }

    fn set_language(
        &mut self,
        choice: LanguageChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.language = choice;
        apply_language(choice);
        // The one placeholder kept in an input's state: set it again in the new language.
        self.commit_input.update(cx, |input, cx| {
            input.set_placeholder(t!("git.commit.placeholder"), window, cx)
        });
        self.apply_settings(window, cx);
    }

    /// Sets the explorer's exclude patterns and lists the explorer again, when they changed.
    fn set_excludes(&mut self, excludes: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        if excludes == self.settings.explorer_exclude {
            return;
        }
        if let Some((explorer, _)) = &self.explorer {
            explorer.update(cx, |explorer, cx| explorer.set_excludes(&excludes, cx));
        }
        self.change(window, cx, |settings| settings.explorer_exclude = excludes);
    }

    /// Restores every default ("Reset values").
    fn reset_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let defaults = Settings::default();
        if let Some(input) = &self.exclude_input {
            let text = defaults.explorer_exclude.join(", ");
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
        if defaults.explorer_exclude != self.settings.explorer_exclude
            && let Some((explorer, _)) = &self.explorer
        {
            explorer.update(cx, |explorer, cx| {
                explorer.set_excludes(&defaults.explorer_exclude, cx)
            });
        }
        // Not shown in the modal: kept as the file has them.
        let kept = (self.settings.log_level, self.settings.highlight_max_size_mb);
        let git_executable = self.settings.git_executable.take();
        self.settings = Settings {
            log_level: kept.0,
            highlight_max_size_mb: kept.1,
            ..defaults
        };
        // Back to the system's git: detect it again.
        if git_executable.is_some() {
            if let Some((select, _)) = &self.git_select {
                telaio_ui::components::set_options(
                    select,
                    vec![telaio_ui::components::SelectOption::new(
                        "",
                        t!("settings.git.auto"),
                    )],
                    false,
                    window,
                    cx,
                );
                telaio_ui::components::pick(select, "", window, cx);
            }
            self.detect_git(cx);
        }
        apply_language(self.settings.language);
        self.apply_settings(window, cx);
    }

    fn set_category(&mut self, category: SettingsCategory, cx: &mut Context<Self>) {
        self.settings_category = category;
        cx.notify();
    }
}

/// Applies the settings the editors read: tab size, spaces, line numbers and current line. Also
/// called once at startup.
pub fn apply_editor_settings(settings: &Settings, cx: &mut App) {
    set_indentation(settings.tab_size, settings.insert_spaces);
    cx.set_global(EditorDisplay {
        line_numbers: settings.line_numbers,
        current_line: settings.highlight_current_line,
        minimap: settings.minimap,
    });
}

/// Switches the UI language process wide.
fn apply_language(choice: LanguageChoice) {
    let language = locale::resolve(choice);
    locale::apply(language);
    log::info!("language changed to {}", language.code());
}

/// The whole content of the modal, rebuilt on every frame from the view's current settings.
fn render_settings(weak: WeakEntity<AppView>, cx: &mut App) -> AnyElement {
    let Some(view) = weak.upgrade() else {
        return div().into_any_element();
    };
    let view = view.read(cx);
    let settings = view.settings.clone();
    let category = view.settings_category;
    let exclude_input = view.exclude_input.clone();
    let palette = cx.palette();
    let mono_font = cx.theme().mono_font_family.clone();

    let path_label = settings::settings_path()
        .map(|path| shorten_path(&path, std::env::home_dir().as_deref()))
        .unwrap_or_default();
    let categories = SettingsCategory::ALL.map(|category| {
        let icon = match category {
            SettingsCategory::Appearance => IconName::Palette,
            SettingsCategory::Editor => IconName::SquarePen,
            SettingsCategory::Files => IconName::Files,
            SettingsCategory::Git => IconName::GitBranch,
            SettingsCategory::About => IconName::Info,
        };
        NavCategory::new(icon, category.label())
    });
    let selected = SettingsCategory::ALL
        .iter()
        .position(|candidate| *candidate == category)
        .unwrap_or(0);
    let select = weak.clone();
    let sections = match category {
        SettingsCategory::Appearance => vec![
            language_section(weak.clone(), settings.language),
            theme_section(weak.clone(), settings.theme),
            interface_section(weak.clone(), &settings),
        ],
        SettingsCategory::Editor => vec![
            text_section(weak.clone(), &palette, &settings),
            indentation_section(weak.clone(), &palette, &settings),
        ],
        SettingsCategory::Files => vec![
            startup_section(weak.clone(), settings.startup),
            saving_section(weak.clone(), &palette, &settings),
            explorer_section(exclude_input.as_ref(), cx),
        ],
        SettingsCategory::Git => git_sections(
            weak.clone(),
            &palette,
            &settings,
            &view.git.availability,
            view.git_select.as_ref().map(|(select, _)| select),
        ),
        SettingsCategory::About => vec![about_header(&palette, &mono_font)],
    };

    let layout = SettingsLayout::new("settings")
        .title(t!("settings.title"))
        .categories(categories)
        .selected(selected)
        .on_select(move |index, _, cx| {
            if let Some(&category) = SettingsCategory::ALL.get(index) {
                let _ = select.update(cx, |view, cx| view.set_category(category, cx));
            }
        })
        .nav_footer(
            v_flex()
                .child(t!("settings.saved_to"))
                .child(div().font_family(mono_font).child(path_label)),
        )
        .heading(category.label())
        .sections(sections);
    if category == SettingsCategory::About {
        layout
            .footer(t!("settings.about.footer"), div())
            .into_any_element()
    } else {
        layout
            .footer(
                t!("settings.footer.applied"),
                GhostButton::new("settings-reset", t!("settings.footer.reset")).on_click(
                    move |_, window, cx| {
                        let _ = weak.update(cx, |view, cx| view.reset_settings(window, cx));
                    },
                ),
            )
            .into_any_element()
    }
}

/// The About header: telaio's (`telaio_app::about::header`), with ted's icon in place of the
/// initial in a square, as the design draws it.
fn about_header(palette: &Palette, mono_font: &SharedString) -> AnyElement {
    let identity = ted_cli::display_identity();
    let notes_url = identity.release_notes_url();
    let repository = identity.repository;
    h_flex()
        .items_center()
        .gap(px(14.0))
        .child(logo(ABOUT_ICON_SIZE))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(identity.name),
                )
                .child(
                    // Wraps instead of running under the buttons when the platform does not fit.
                    h_flex()
                        .flex_wrap()
                        .gap(px(4.0))
                        .text_color(palette.fg_muted)
                        .child(t!("settings.about.version"))
                        .child(
                            div()
                                .font_family(mono_font.clone())
                                .text_color(palette.fg)
                                .child(identity.version),
                        )
                        .child(format!("\u{b7} {}", platform_label())),
                )
                .child(
                    h_flex()
                        .gap(px(4.0))
                        .text_size(px(12.0))
                        .text_color(palette.fg_muted)
                        .child(t!("settings.about.design_system"))
                        .child(
                            div()
                                .id("about-telaio")
                                .cursor_pointer()
                                .text_color(palette.accent_text)
                                .hover(|style| style.underline())
                                .child("telaio")
                                .on_click(|_, _, cx| cx.open_url(telaio_ui::REPOSITORY)),
                        )
                        .child(
                            div()
                                .font_family(mono_font.clone())
                                .child(telaio_ui::VERSION),
                        ),
                ),
        )
        .child(
            h_flex()
                .flex_none()
                .gap(px(6.0))
                .child(
                    SecondaryButton::new("about-release-notes", t!("settings.about.release_notes"))
                        .icon(Icon::new(IconName::FileText))
                        .on_click(move |_, _, cx| cx.open_url(&notes_url)),
                )
                .child(
                    SecondaryButton::new("about-github", "GitHub")
                        .icon(Icon::new(IconName::Github))
                        .on_click(move |_, _, cx| cx.open_url(repository)),
                ),
        )
        .into_any_element()
}

/// The OS and architecture as the About header shows them, for example "Linux x86_64".
fn platform_label() -> String {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "macos" => "macOS",
        "windows" => "Windows",
        other => other,
    };
    format!("{os} {}", std::env::consts::ARCH)
}

/// "Language": Automatic (with the language it resolves to) and each language in itself.
fn language_section(weak: WeakEntity<AppView>, current: LanguageChoice) -> AnyElement {
    let auto = t!(
        "settings.language.auto",
        language = locale::resolve(LanguageChoice::Auto).native_name()
    )
    .into_owned();
    let choices = [
        (LanguageChoice::Auto, auto),
        (LanguageChoice::En, LANGUAGES[0].native_name().to_string()),
        (LanguageChoice::Es, LANGUAGES[1].native_name().to_string()),
    ];
    let mut control = SegmentedControl::new("settings-language");
    for (choice, label) in choices {
        let weak = weak.clone();
        control = control.item(
            SegmentedItem::new(label)
                .selected(choice == current)
                .on_click(move |window, cx| {
                    let _ = weak.update(cx, |view, cx| view.set_language(choice, window, cx));
                }),
        );
    }
    v_flex()
        .gap(px(14.0))
        .child(SectionHeading::new(t!("settings.language.title")))
        .child(LabeledRow::new(t!("settings.language.label"), control))
        .into_any_element()
}

/// "Theme": the three theme cards.
fn theme_section(weak: WeakEntity<AppView>, current: ThemeChoice) -> AnyElement {
    let cards = [
        (ThemeChoice::System, "system"),
        (ThemeChoice::Light, "light"),
        (ThemeChoice::Dark, "dark"),
    ]
    .map(|(choice, id)| {
        let weak = weak.clone();
        ThemeCard::new(
            SharedString::from(format!("settings-theme-{id}")),
            choice.into(),
        )
        .selected(choice == current)
        .on_click(move |_, window, cx| {
            let _ = weak.update(cx, |view, cx| {
                view.change(window, cx, |settings| settings.theme = choice)
            });
        })
    });
    v_flex()
        .gap(px(10.0))
        .child(
            SectionHeading::new(t!("settings.theme.title"))
                .description(t!("settings.theme.description")),
        )
        .child(h_flex().gap(px(12.0)).children(cards))
        .into_any_element()
}

/// "Interface": the UI font size.
fn interface_section(weak: WeakEntity<AppView>, settings: &Settings) -> AnyElement {
    let stepper = size_stepper(
        "settings-ui-size",
        settings.ui_font_size,
        UI_FONT_SIZE_STEP,
        UI_FONT_SIZE_RANGE.clone().into_inner(),
        move |size, window, cx| {
            let _ = weak.update(cx, |view, cx| {
                view.change(window, cx, |settings| settings.ui_font_size = size)
            });
        },
    );
    v_flex()
        .gap(px(14.0))
        .child(SectionHeading::new(t!("settings.interface.title")))
        .child(LabeledRow::new(t!("settings.font_size"), stepper))
        .into_any_element()
}

/// "Text": the editor font size, line numbers and the current line highlight.
fn text_section(weak: WeakEntity<AppView>, palette: &Palette, settings: &Settings) -> AnyElement {
    let size = weak.clone();
    let stepper = size_stepper(
        "settings-mono-size",
        settings.mono_font_size,
        MONO_FONT_SIZE_STEP,
        MONO_FONT_SIZE_RANGE.clone().into_inner(),
        move |value, window, cx| {
            let _ = size.update(cx, |view, cx| {
                view.change(window, cx, |settings| settings.mono_font_size = value)
            });
        },
    );
    v_flex()
        .gap(px(14.0))
        .child(SectionHeading::new(t!("settings.text.title")))
        .child(LabeledRow::new(t!("settings.font_size"), stepper))
        .child(LabeledRow::new(
            t!("settings.text.line_numbers"),
            toggle(
                "settings-line-numbers",
                weak.clone(),
                palette,
                settings.line_numbers,
                |settings, on| settings.line_numbers = on,
            ),
        ))
        .child(LabeledRow::new(
            t!("settings.text.current_line"),
            toggle(
                "settings-current-line",
                weak.clone(),
                palette,
                settings.highlight_current_line,
                |settings, on| settings.highlight_current_line = on,
            ),
        ))
        .child(LabeledRow::new(
            t!("settings.text.minimap"),
            toggle(
                "settings-minimap",
                weak,
                palette,
                settings.minimap,
                |settings, on| settings.minimap = on,
            ),
        ))
        .into_any_element()
}

/// "Indentation": the tab size and whether Tab inserts spaces.
fn indentation_section(
    weak: WeakEntity<AppView>,
    palette: &Palette,
    settings: &Settings,
) -> AnyElement {
    let mut sizes = SegmentedControl::new("settings-tab-size");
    for size in TAB_SIZES {
        let weak = weak.clone();
        sizes = sizes.item(
            SegmentedItem::new(size.to_string())
                .selected(size == settings.tab_size)
                .on_click(move |window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        view.change(window, cx, |settings| settings.tab_size = size)
                    });
                }),
        );
    }
    v_flex()
        .gap(px(14.0))
        .child(SectionHeading::new(t!("settings.indent.title")))
        .child(LabeledRow::new(t!("settings.indent.tab_size"), sizes))
        .child(
            LabeledRow::new(
                t!("settings.indent.insert_spaces"),
                toggle(
                    "settings-insert-spaces",
                    weak,
                    palette,
                    settings.insert_spaces,
                    |settings, on| settings.insert_spaces = on,
                ),
            )
            .description(t!("settings.indent.insert_spaces_description")),
        )
        .into_any_element()
}

/// "Saving": the whitespace cleanup done on save.
fn saving_section(weak: WeakEntity<AppView>, palette: &Palette, settings: &Settings) -> AnyElement {
    v_flex()
        .gap(px(14.0))
        .child(SectionHeading::new(t!("settings.saving.title")))
        .child(LabeledRow::new(
            t!("settings.saving.trim"),
            toggle(
                "settings-trim",
                weak.clone(),
                palette,
                settings.trim_trailing_whitespace,
                |settings, on| settings.trim_trailing_whitespace = on,
            ),
        ))
        .child(LabeledRow::new(
            t!("settings.saving.final_newline"),
            toggle(
                "settings-final-newline",
                weak,
                palette,
                settings.insert_final_newline,
                |settings, on| settings.insert_final_newline = on,
            ),
        ))
        .into_any_element()
}

/// "Startup": what ted shows when it starts without a path.
fn startup_section(weak: WeakEntity<AppView>, startup: Startup) -> AnyElement {
    let mut control = SegmentedControl::new("settings-startup");
    for choice in Startup::ALL {
        let weak = weak.clone();
        control = control.item(
            SegmentedItem::new(choice.label())
                .selected(choice == startup)
                .on_click(move |window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        view.settings.startup = choice;
                        view.apply_settings(window, cx);
                    });
                }),
        );
    }
    v_flex()
        .gap(px(14.0))
        .child(SectionHeading::new(t!("settings.startup.title")))
        .child(
            LabeledRow::new(t!("settings.startup.label"), control)
                .description(t!("settings.startup.description")),
        )
        .into_any_element()
}

/// "Explorer": the exclude patterns, one text field.
fn explorer_section(input: Option<&Entity<InputState>>, cx: &App) -> AnyElement {
    let field = match input {
        Some(input) => div()
            .w(px(EXCLUDE_FIELD_WIDTH))
            .child(text_field(input, cx))
            .into_any_element(),
        None => div().into_any_element(),
    };
    v_flex()
        .gap(px(14.0))
        .child(SectionHeading::new(t!("settings.explorer.title")))
        .child(
            LabeledRow::new(t!("settings.explorer.exclude"), field)
                .description(t!("settings.explorer.exclude_description")),
        )
        .into_any_element()
}

/// A switch in its halo that sets one boolean setting.
fn toggle(
    id: &'static str,
    weak: WeakEntity<AppView>,
    palette: &Palette,
    checked: bool,
    set: fn(&mut Settings, bool),
) -> impl IntoElement {
    switch_halo(
        SharedString::from(format!("{id}-halo")),
        Switch::new(id)
            .checked(checked)
            .on_click(move |checked, window, cx| {
                let on = *checked;
                let _ = weak.update(cx, |view, cx| {
                    view.change(window, cx, |settings| set(settings, on))
                });
            }),
        palette,
    )
}

/// A font size stepper, `-`, the size in px and `+`, within `range`.
fn size_stepper(
    id: &'static str,
    value: f32,
    step: f32,
    range: (f32, f32),
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> NumberStepper {
    NumberStepper::new(id, value)
        .step(step)
        .range(range.0, range.1)
        .format(format_size)
        .on_change(on_change)
}

/// `12.5 px`, with a decimal comma in Spanish.
fn format_size(value: f32) -> String {
    let number = value.to_string();
    if rust_i18n::locale().starts_with("es") {
        format!("{} px", number.replace('.', ","))
    } else {
        format!("{number} px")
    }
}

//! The shortcuts listed in the keyboard shortcuts cheat sheet, grouped by area, with their
//! translated labels and the keys to show as chips. It mirrors the key bindings of `actions.rs`,
//! `views/editor.rs`, `views/find.rs` and `views/goto.rs`, so a new binding gets a row here.
//! telaio's `open_shortcuts_dialog` renders it; this module is plain data, unit tested without a
//! window.

use rust_i18n::t;
use telaio_ui::components::{MODIFIER_KEY, Shortcut, ShortcutGroup};

/// The modifier of the panel toggles and of the occurrence selection: `Ctrl` on macOS, where
/// Option with a digit or a letter types a character, `Alt` elsewhere.
#[cfg(target_os = "macos")]
const ALT_OR_CTRL: &str = "Ctrl";
#[cfg(not(target_os = "macos"))]
const ALT_OR_CTRL: &str = "Alt";

/// The Alt key as macOS names it.
#[cfg(target_os = "macos")]
const ALT: &str = "Option";
#[cfg(not(target_os = "macos"))]
const ALT: &str = "Alt";

/// The keys of Find and replace: `Cmd+Option+F` on macOS, where `Cmd+H` hides the app.
fn replace_keys() -> Vec<&'static str> {
    if cfg!(target_os = "macos") {
        vec!["Cmd", "Option", "F"]
    } else {
        vec!["Ctrl", "H"]
    }
}

/// Every shortcut, grouped. Built each time the cheat sheet opens, so it follows the language.
pub fn groups() -> Vec<ShortcutGroup> {
    let modifier = MODIFIER_KEY;
    let row = |key: &str, keys: &[&str]| Shortcut::new(t!(key), keys);
    vec![
        ShortcutGroup {
            title: t!("shortcuts.group.files").to_string(),
            shortcuts: vec![
                row("shortcuts.action.new_file", &[modifier, "N"]),
                row("shortcuts.action.open_file", &[modifier, "O"]),
                row("shortcuts.action.open_folder", &[modifier, "Shift", "O"]),
                row("shortcuts.action.save", &[modifier, "S"]),
                row("shortcuts.action.close_tab", &[modifier, "W"]),
                row("shortcuts.action.next_tab", &["Ctrl", "Tab"]),
                row("shortcuts.action.previous_tab", &["Ctrl", "Shift", "Tab"]),
                row("shortcuts.action.copy_path", &[modifier, ALT, "C"]),
                row("shortcuts.action.reveal", &[modifier, ALT, "R"]),
            ],
        },
        ShortcutGroup {
            title: t!("shortcuts.group.window").to_string(),
            shortcuts: vec![
                Shortcut::open_sheet(),
                row("shortcuts.action.settings", &[modifier, ","]),
                row("shortcuts.action.toggle_explorer", &[ALT_OR_CTRL, "1"]),
                row("shortcuts.action.toggle_git", &[ALT_OR_CTRL, "2"]),
                row("shortcuts.action.zoom_in", &[modifier, "="]),
                row("shortcuts.action.zoom_out", &[modifier, "-"]),
                row("shortcuts.action.zoom_reset", &[modifier, "0"]),
            ],
        },
        ShortcutGroup {
            title: t!("shortcuts.group.editor").to_string(),
            shortcuts: vec![
                // Ctrl on macOS too: Cmd+G is Find next there.
                row("shortcuts.action.goto", &["Ctrl", "G"]),
                row("shortcuts.action.undo", &[modifier, "Z"]),
                row("shortcuts.action.redo", &[modifier, "Shift", "Z"]),
                row("shortcuts.action.select_line", &[modifier, "L"]),
                row("shortcuts.action.duplicate_line", &[modifier, "D"]),
                row("shortcuts.action.delete_line", &[modifier, ALT, "D"]),
                row("shortcuts.action.move_line_up", &[ALT, "\u{2191}"]),
                row("shortcuts.action.move_line_down", &[ALT, "\u{2193}"]),
                row("shortcuts.action.toggle_comment", &[modifier, "/"]),
                row("shortcuts.action.line_below", &[modifier, "\u{21b5}"]),
                row(
                    "shortcuts.action.line_above",
                    &[modifier, "Shift", "\u{21b5}"],
                ),
                row("shortcuts.action.indent_line", &[modifier, "]"]),
                row("shortcuts.action.outdent_line", &[modifier, "["]),
                row("shortcuts.action.toggle_wrap", &["Alt", "Z"]),
            ],
        },
        ShortcutGroup {
            title: t!("shortcuts.group.cursors").to_string(),
            shortcuts: vec![
                row(
                    "shortcuts.action.add_cursor",
                    &[ALT, &t!("shortcuts.key.click")],
                ),
                row(
                    "shortcuts.action.column_selection",
                    &[&t!("shortcuts.key.middle_drag")],
                ),
                row("shortcuts.action.next_occurrence", &[ALT_OR_CTRL, "J"]),
                row(
                    "shortcuts.action.unselect_occurrence",
                    &[ALT_OR_CTRL, "Shift", "J"],
                ),
                row("shortcuts.action.one_cursor", &["Esc"]),
            ],
        },
        ShortcutGroup {
            title: t!("shortcuts.group.find").to_string(),
            shortcuts: vec![
                row("shortcuts.action.find", &[modifier, "F"]),
                row("shortcuts.action.replace", &replace_keys()),
                row("shortcuts.action.find_next", &["F3"]),
                row("shortcuts.action.find_previous", &["Shift", "F3"]),
            ],
        },
        ShortcutGroup {
            title: t!("shortcuts.group.git").to_string(),
            shortcuts: vec![row("shortcuts.action.commit", &[modifier, "\u{21b5}"])],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_group_and_shortcut_has_text_and_keys() {
        let groups = groups();
        assert_eq!(groups.len(), 6);
        for group in &groups {
            assert!(!group.title.is_empty());
            assert!(!group.shortcuts.is_empty());
            for shortcut in &group.shortcuts {
                assert!(!shortcut.label.is_empty());
                assert!(
                    !shortcut.label.starts_with("shortcuts."),
                    "{}",
                    shortcut.label
                );
                assert!(shortcut.keys.iter().all(|key| !key.is_empty()));
            }
        }
    }

    #[test]
    fn the_git_panel_is_on_the_second_panel_key() {
        let toggle = t!("shortcuts.action.toggle_git").to_string();
        let row = groups()
            .into_iter()
            .flat_map(|group| group.shortcuts)
            .find(|shortcut| shortcut.label == toggle);
        assert_eq!(
            row.map(|shortcut| shortcut.keys),
            Some(vec![ALT_OR_CTRL.to_string(), "2".to_string()])
        );
    }
}

# ted design system

Reference for anyone (human or agent) touching the UI. It is written from the Claude Design
prototype in `design_system/` (`ted v2.dc.html`, the final one, which replaces `ted.dc.html`;
`ted-lang.js`, `ted-data.js`), as the shared rules
ask (1.3). Once the UI is implemented the code is the source of truth and this file its readable
summary: if they disagree, the code wins; fix this file.

The shared part (tokens, `Palette`, theme JSON, metrics, fonts and the standard components) lives
in [telaio](https://github.com/tanisperez/telaio). ted uses telaio's default tokens unchanged and
adds only the syntax colors and the metrics of its own layout.

## Rules

- Colors only through `cx.palette()`, never from `cx.theme()` and never as a literal.
- Sizes only from telaio's `metrics` or from ted's own `theme/metrics.rs`, never as a literal.
- Both themes ("ted Light", "ted Dark") are built from telaio's `Tokens::DEFAULT_LIGHT` and
  `DEFAULT_DARK`. Do not copy the hex values into ted; add a token in telaio, not here.
- Panels are flat (border only). Inputs and cards use `raised` plus a border. Menus, popovers and
  modals use `overlay` plus the shadow token. The explorer never has a shadow.
- The app icon is the indigo text file of the v2 design (`design_system/ted-icon.svg`, copied
  without its metadata to `crates/ted-app/assets/ted.svg`). It is painted as an image
  (`views/logo.rs`), not a gpui `svg`, which would draw it in one color.
- Icons are Lucide, through `gpui_kit::assets::IconName`. The app must keep
  `.with_assets(gpui_kit::assets::AllAssets)` or icons render empty.
- Hover is the translucent `hover` token and pressed is `hover` at 2.2x alpha (telaio derives it),
  so both read on `bg`, `surface`, `raised` and `overlay`.
- The editor text is painted by ted's own element (see `docs/architecture.md`), but it reads the
  same palette and metrics as any other view. No colors or sizes of its own outside this file.
- Nothing in a render path costs in proportion to the document size (`docs/performance.md`).

## Palette

### Tokens ted uses from telaio, unchanged

Every color token of the prototype has exactly the value of telaio's `Tokens::DEFAULT_LIGHT` and
`DEFAULT_DARK` (compared one by one against `telaio-ui/src/palette.rs`). ted overrides none.

| CSS variable | telaio token | Light | Dark | Use in ted |
| --- | --- | --- | --- | --- |
| `--bg` | `bg` | `#fcfcfd` | `#15161a` | Editor, active tab, line number gutter |
| `--surface` | `surface` | `#f4f5f7` | `#1b1c21` | Title bar, tab bar, explorer, status bar |
| `--raised` | `raised` | `#ffffff` | `#23252c` | Inputs, search trigger, code blocks in the preview |
| `--overlay` | `overlay` | `#ffffff` | `#262830` | Menus, settings modal, markdown toolbar |
| `--border` | `border` | `#e3e5ea` | `#2c2e36` | Separators, panel borders |
| `--border-strong` | `border_strong` | `#cdd0d8` | `#3b3e48` | Control borders, switch off track |
| `--fg` | `fg` | `#1b1c22` | `#e9eaee` | Text |
| `--fg-muted` | `fg_muted` | `#5c606e` | `#a0a3ae` | Inactive tabs, current line number |
| `--fg-subtle` | `fg_subtle` | `#8b8e9a` | `#6f7280` | Icons, line numbers, status bar |
| `--accent` | `accent` | `#4f57d8` | `#5b63e6` | Active tab bar line, switches, primary button |
| `--accent-fg` | `accent_fg` | `#ffffff` | `#ffffff` | Text on `accent` |
| `--accent-text` | `accent_text` | `#3f47c4` | `#a3a9ff` | Links, selected row text, branch |
| `--accent-subtle` | `accent_subtle` | `#e8e9fd` | `#272b4d` | Selected rows and categories |
| `--hover` | `hover` | `#1b1c2213` | `#ffffff0d` | Hover background |
| `--pressed` | `pressed` (derived) | `#1b1c222a` | `#ffffff1c` | Pressed background |
| `--success` | `success` | `#1d8657` | `#4fc88f` | Added or untracked files |
| `--warning` | `warning` | `#9a6500` | `#e8b75a` | Modified files, numbers and types in code |
| `--danger` | `danger` | `#cf3535` | `#f27171` | Deleted files, close button hover, tags |
| `--info` | `info` | `#1c6fb0` | `#62b3f0` | Renamed files, functions in code |
| `--scrim` | `scrim` | `#0a0b1073` | `#0a0b1073` | Backdrop behind the settings modal |
| `--shadow` | `shadow` | see below | see below | Menus and modals |

`pressed` is not a literal in telaio: it is `hover` with its alpha times 2.2 (`0x13 * 2.2 = 0x2a`
in light, `0x0d * 2.2 = 0x1c` in dark), which is what the prototype shows.

The `success_subtle`, `warning_subtle`, `danger_subtle` and `info_subtle` tokens of telaio are not
in the prototype. ted uses telaio's values if a badge ever needs them.

Shadow, two layers with a horizontal offset of 0 (the prototype's `--shadow` equals telaio's):

| Layer | Light | Dark |
| --- | --- | --- |
| 1 | y 12, blur 32, spread 0, `rgba(20,22,40,.14)` | y 12, blur 32, spread 0, `rgba(0,0,0,.45)` |
| 2 | y 0, blur 0, spread 1, `rgba(20,22,40,.06)` | y 0, blur 0, spread 1, `rgba(255,255,255,.04)` |

### What ted adds: the syntax colors

telaio ships no syntax colors, so ted passes a `SyntaxTheme` per mode through
`ThemeFamily::syntax`. Three new tokens exist in the prototype (`--s-keyword`, `--s-string`,
`--s-constant`); the rest of the highlighting reuses palette tokens. The three have exactly the
values of Postino's syntax theme, so ted and Postino look alike.

| Token | Light | Dark | Role |
| --- | --- | --- | --- |
| `--s-keyword` | `#7a4fd6` | `#b597f7` | Keywords, `=>`, markdown headings and emphasis |
| `--s-string` | `#1d7a4f` | `#86d7a5` | Strings, inline code, fenced code in markdown |
| `--s-constant` | `#b03a78` | `#f3a2c6` | `true`, `null`, `None`, `Some`, XML entities, bold |

Colors per token class (`COLORS` in `ted-lang.js`):

| Class | Color | Light | Dark | Examples |
| --- | --- | --- | --- | --- |
| `comment` | `fg_muted` | `#5c606e` | `#a0a3ae` | `// ...`, `/* */`, `<!-- -->`, log dates, `#` in TOML, quotes in markdown |
| `keyword` | `--s-keyword` | `#7a4fd6` | `#b597f7` | `fn`, `let`, `const`, `=>`, TOML tables, `<!DOCTYPE` |
| `function` | `info` | `#1c6fb0` | `#62b3f0` | `name(`, Rust macros `name!`, log `INFO` |
| `property` | `accent_text` | `#3f47c4` | `#a3a9ff` | JSON keys, TOML keys, log `[context]`, markdown links |
| `attribute` | `accent_text` | `#3f47c4` | `#a3a9ff` | `#[derive(..)]`, XML attributes |
| `string` | `--s-string` | `#1d7a4f` | `#86d7a5` | `"text"`, `'text'`, template literals |
| `number` | `warning` | `#9a6500` | `#e8b75a` | `200`, `1.5e3`, `10ms`, log `WARN`, list markers |
| `type` | `warning` | `#9a6500` | `#e8b75a` | Capitalized identifiers outside logs |
| `constant` | `--s-constant` | `#b03a78` | `#f3a2c6` | `true`, `false`, `null`, `None`, `Ok`, `Err` |
| `tag` | `danger` | `#cf3535` | `#f27171` | XML and HTML tags, log `ERROR` and `FATAL` |

Anything not listed (identifiers, operators, punctuation) is `fg`. In tree-sitter terms: `comment`,
`keyword`, `function`, `property`, `attribute`, `string`, `number`, `type`, `constant`/`boolean`,
`tag`. A dotted capture with no entry of its own takes its first part's color.

Notes:

- The prototype draws comments upright. Geist Mono has no italic face, so ted does the same and
  does not set `.italic()`.
- Markdown headings are `keyword` at weight 600; other tokens are weight 400. ted paints headings
  in `keyword` at the normal weight for now, and inline markdown (code spans, emphasis, links) is
  plain: the bundled grammar covers only the blocks.
- The log highlighter is not tree-sitter: dates are `comment`, `ERROR`/`FATAL` `tag`, `WARN`
  `number`, `INFO` `function`, `DEBUG`/`TRACE` `comment`, `[context]` `property`. ted does not
  color `[context]` yet.
- The status bar shows the language at its right: `Rust`, `JavaScript`, `JSON`, `TOML`,
  `Markdown`, `XML`, `HTML`, `Makefile`, `Log` or "Plain text".
- Makefiles (`Makefile`, `makefile`, `GNUmakefile`, `*.mk`, `*.mak`) are not in the prototype.
  ted colors directives (`ifdef`, `include`, `define`) as `keyword`, rule targets and make
  functions (`wildcard`, `shell`) as `function`, variables as `property`, automatic variables
  (`$@`) and special targets (`.PHONY`) as `constant`, and leaves the recipes plain.

### Semantic mappings

- Git status letters (explorer and Git panel): `A` and `U` `success`, `D` `danger`, `R` `info`,
  anything else (`M`) `warning`. The file name takes the same color in the explorer.
- Selected explorer row: `accent_subtle` background, `accent_text` text. Selected tab: `bg`
  background, `fg` text and a 2 px `accent` line on its top edge. Inactive tab: `fg_muted`.
- Current editor line: `hover` background, its number in `fg_muted` (others `fg_subtle`).
- Dirty tab: an 8 px circle in `fg_subtle` in place of the close cross.

## Typography

- UI font **Geist**, 13 px (user setting 11 to 16). Editor font **Geist Mono**, 12.5 px (user
  setting 10 to 18, 0.5 steps). Both are bundled by telaio (OFL) as `UI_FAMILY` and `MONO_FAMILY`.
- The prototype loads Regular 400, Medium 500, SemiBold 600 (also 700) of Geist and Regular 400,
  Medium 500 (also 600) of Geist Mono.

| Use | Font | Size / weight |
| --- | --- | --- |
| Body, tabs, tree rows, menu rows | Geist | 13 / 400 |
| Window title "ted", dialog category title | Geist | 13 / 600, 15 / 600 |
| Settings section titles, active category | Geist | 13 / 500 |
| Secondary text, hints, menu paths | Geist | 12 / 400 |
| Tab detail ("Vista previa" label) | Geist | 12 / 400 |
| Status bar, tree row detail | Geist | 11.5 / 400 |
| Section labels (`EXPLORER`, `COMMIT`, `RECENT FOLDERS`) | Geist | 11 / 600, uppercase, 0.06em tracking, `fg_subtle` |
| Git change count in its status bar toggle | Geist Mono | 11 / 500 |
| Editor text and line numbers | Geist Mono | 12.5 / 400 (setting) |
| Git letters in tree rows | Geist Mono | 11.5 / 500 |
| Shortcut hints, counts | Geist Mono | 11 / 400 |
| Path in the settings footer | Geist Mono | 11.5 / 400 |
| Markdown preview body | Geist | 14 / 400, line height 1.65 |
| Markdown preview h1, h2, h3 | Geist | 26, 19, 15 / 600, line height 1.3 |
| Markdown preview code | Geist Mono | 12.5 / 400, line height 1.6 |

The UI line height is 1.25 (the prototype's root).

## Metrics

Shared sizes come from telaio's `metrics.rs`. ted's own layout sizes go in ted's
`theme/metrics.rs` (planned) with the values below, all taken from the prototype CSS.

### Heights (px)

| Element | Value | Source |
| --- | --- | --- |
| Title bar | 40 | ted |
| Document tab bar | 36 | telaio `DOCUMENT_TABS_HEIGHT` |
| Status bar | 30 | ted |
| Tree row | 26 | telaio `TREE_ROW_HEIGHT` |
| Explorer header | 36 | ted (padding 10 10 6 14) |
| Menu row (recents) | 30 | ted |
| Settings category row | 28 | ted |
| Settings header, footer | 52 | telaio `MODAL_HEADER_HEIGHT` |
| Control (input, button, select) | 30 | telaio `CONTROL_HEIGHT` |
| Search trigger | 26 | ted |
| Workspace switcher button | 26 | ted |
| Status bar panel toggle | 24 (min width 28) | ted |
| Title bar settings gear | 26 x 26 | ted (as Postino) |
| Theme card preview | 92 | telaio `THEME_PREVIEW_HEIGHT` |
| Commit message box | 76 | ted (later milestone) |
| Switch | 20 (track 34) | ted |

### Widths (px)

| Element | Value |
| --- | --- |
| Explorer / Git panel | 260 default, resizable 200 to 480, 6 px drag handle on its right edge |
| Search trigger | 360 |
| Window control (minimize, maximize, close) | 40 |
| Recents menu | 360 |
| Settings modal | 800 x 720 (shrinks to the window with 16 px margin) |
| Settings category column | 188 |
| Select in settings | 220 |
| Switch | 34 x 20, thumb 16, 2 px inset |
| Markdown preview column | max 720, padding 36 40 80 |

### Editor

| Metric | Value |
| --- | --- |
| Font size | 12.5 px (10 to 18, step 0.5) |
| Line height | `round(font size * 1.6)`, 20 px at 12.5 |
| Padding above the first line | 10 |
| Scroll past the end | 40% of the viewport height below the last line |
| Gutter width | `digits * ceil(font size * 0.62) + 28`; 7.5 -> 8 px per digit at 12.5 |
| Gutter | right aligned, 16 px right padding, `bg` background, sticky on horizontal scroll |
| Text left padding | 4 after the gutter |
| Text right padding | 24 |
| Wrap | off by default (`white-space: pre`), a setting turns it on |
| Scrollbar | 10 px, thumb `border_strong`, radius 5, transparent track |

The gutter formula is the prototype's; in ted it is computed from the digit count of the line
count and the measured advance of Geist Mono, not from the 0.62 constant.

### Other

- Tab: padding 0 8 0 12, gap 6, icon 14, close button 14 with a 13 px cross, left of it 4 extra.
  Right border `border` between tabs. "New file" button 24 at the end of the strip.
- Tree row: padding left `8 + 16 * depth`, right 6, gap 4, chevron column 13, icon 14, radius 6,
  1 px gap between rows, list padding 0 6 8.
- Title bar: 12 px left padding, app icon 18 (`assets/ted.svg`), "ted" 600, 1x16
  separator. Settings gear at the right, before the window controls: 26 x 26, 4 px right margin,
  radius 6, icon 16 `fg_muted`, `hover` and `pressed` backgrounds.
- Status bar: padding 0 12 0 4, gap 16, 11.5 px, `fg_subtle`. Panel toggles first (gap 2 between
  them, -4 margin after the group): 24 high, min 28 wide, padding 0 7, gap 5, radius 6, icon 15;
  `fg_muted` when off, `accent_subtle` background and `accent_text` when their panel shows,
  `hover` tint and `fg` on hover. A 1 x 14 `border` separator follows. Branch button 22 high,
  radius 6, in `accent_text`.
- Icon buttons: 22 in panel headers, 24 in the tab strip, 26 x 24 in the markdown toolbar, 20 for
  the shortcuts button in the status bar.
- Radii: 4 chips and checkboxes, 5 logo and toolbar items, 6 rows and icon buttons, 7 search
  trigger and categories, 8 inputs, buttons, code blocks, 10 menus and theme cards, 14 modal.

## Layout, top to bottom

```
+--------------------------------------------------------------+
| title bar 40: logo, switcher, search, settings, window ctrls |
+-------------+------------------------------------------------+
| explorer    | tab bar 36                                     |
| or Git      +------------------------------------------------+
| panel 260   | gutter | text                                  |
|             |        |                                       |
+-------------+------------------------------------------------+
| status bar 30: Files, Git | branch, folder path ... Ln, Col  |
+--------------------------------------------------------------+
```

1. **Title bar** (`surface`, bottom border): logo, "ted", separator, workspace switcher (folder
   icon, folder name, chevron; opens the recents menu), a centered 360 px search trigger
   ("Search files and actions", `Ctrl+K` hint; folder mode only), the settings gear (`Ctrl+,`)
   and the three window controls at the right (close turns `danger` on hover). The window uses client decorations on Linux.
2. **Side panel** (`surface`, right border): Explorer or Git, one at a time, resizable.
3. **Tab bar** (folder mode only).
4. **Editor** (and, for markdown, the preview beside it or instead of it).
5. **Status bar** (`surface`, top border): left, the Files and Git panel toggles (folder mode;
   Git shows its change count), a separator, the branch with the ahead count and the folder path; right, word count (markdown), `Ln n, Col n`, indentation, `UTF-8`, line ending,
   language, shortcuts button.

## Screens

### Empty state, with recents

No argument on the command line, or nothing opened yet (`views/welcome.rs`). No tabs, no editor,
no explorer. A column of 440 px (`WELCOME_WIDTH`), centered horizontally, 64 px from the top, that
scrolls when the recents do not fit. Its sections are 24 px apart:

1. **Header**: the app icon at 40 px, "ted" 20/600
   and the tagline in `fg_muted`.
2. **Start**: Open file and Open folder rows, the shortcut at the right in Geist Mono 11
   `fg_subtle` (`Ctrl+O`, `Ctrl+Shift+O`; the macOS glyphs on macOS).
3. **Recent folders** and **Recent files** (10 each at most, hidden when empty): rows with the
   icon (folder or file-text, 14, `fg_subtle`), the name (truncated at 180 px) and the path with
   the home folder as `~` in 12 px `fg_subtle`, truncated.

Sections start with a `SectionLabel` (11/600 uppercase `fg_subtle`). Rows are 30 px high, 8 px
side padding, gap 10, radius 6, `hover` background on hover, pointer cursor; a click opens. The
recents live in `<config dir>/ted/recents.toml`. A recent that no longer exists shows the error
state and leaves the list.

**Error state**: telaio's `EmptyState` with the triangle-alert icon, "This file or folder does not
exist" (or "cannot be opened"), the path as its description, and the Open file and Open folder
secondary buttons.

### Title bar switcher

Right of the logo and the separator: a ghost button 26 px high (radius 6, `fg_muted`) with an
icon and the name of what is open: folder icon and the folder name, file-text icon and the file
name, or folder-clock and "Open recent" with nothing open. Its menu (360 px) lists the recent
folders, a separator, the recent files (each with its `~` path at the right in 12 px `fg_subtle`
and a check on the open one), a separator, and telaio's shortcut rows for Open file and Open
folder. The window title is `name · ted`, or `ted` alone.

### File mode

`ted some/file.txt`, a recent file or Open file with nothing open. The switcher shows the file;
no explorer, no tab bar: the editor fills the window. The status bar has no panel toggles and no
folder path.

### Folder mode

`ted some/folder`, a recent folder or Open folder. The full layout above; the Git panel comes later.
Until a file is opened, and after the last tab closes, the content area shows "Pick a file in the
explorer" with the `Ctrl+O` hint.

- **Files toggle**: first in the status bar (files icon 15). A click, or `Alt+1` (`Ctrl+1` on
  macOS), shows or hides the explorer; it takes the active look while the explorer shows.
- **Tabs**: telaio's `DocumentTabs`, one tab per open file (file icon, name, the shortened path as
  tooltip). Opening a file from the explorer or a dialog focuses its tab when it is already open,
  otherwise adds one right after the active tab. Each tab keeps its own editor, so its scroll and
  cursor survive switching. The cross, a middle click or `Ctrl+W` closes a tab (asking first when
  it has unsaved changes) and the neighbour to its right, or else to its left, becomes active.
  `Ctrl+Tab` and `Ctrl+Shift+Tab` cycle through the tabs on every platform. Dragging reorders.
  The "New file" button of the design joins the strip with new untitled documents.
- **Explorer**: `surface`, right border, 260 px by default, resized by dragging its right edge (a
  4 px strip with the `col-resize` cursor) between 180 and 520 px. Header 36 px: the folder name as
  a `SectionLabel`, then Collapse all (telaio `IconButton`). New file and New folder join it with
  editing. The list is telaio's `TreeRow` (26 px, 16 px per level) with a folder, open folder or
  file icon (14, `fg_subtle`) as its leading element, inside 6 px of side padding. A folder still
  being listed shows "…" after its name. The selected row is telaio's selected row (`list_active`,
  `accent_text` label).
- **Keyboard** while the explorer has the focus: Up and Down move, Left collapses or goes to the
  parent, Right expands or goes to the first child, Enter opens or toggles.
- The file opened in the editor is revealed and selected. The status bar starts with the folder
  path (hard drive icon, `~` for the home folder); a click opens it in the file manager.

### Find in file

Not in the prototype; ted builds it from the design's floating toolbar (`views/find.rs`).

- `Ctrl+F` opens a bar at the top right of the editor (top 10, right 22, `overlay`, `border`,
  radius 8, the shadow token, padding 4, gap 4), with the selected text when it is on one line:
  a 240 px field, the counter in 12 px `fg_muted` (`3 of 120`, `No results`, `120…` while
  counting), the case toggle (24 px, `accent_subtle` and `accent_text` when on), previous and
  next (`ChevronUp`, `ChevronDown`) and close.
- Typing selects the first match after the cursor. Enter and `F3` go to the next match,
  Shift+Enter and `Shift+F3` to the previous one, wrapping around. Escape closes the bar and gives
  the focus back to the text.
- Matches are tinted with `warning` at 22% alpha; the selected match at 50%, over the selection.
- The whole text is counted in the background in 8 MB steps. Only the first 100 000 matches are
  kept; past them next and previous search the text near the cursor.

### Saving and unsaved changes

- `Ctrl+S` saves. A document with unsaved changes shows `●` before its name in the window title
  (`● main.rs · ted`); its tab shows the dot in place of the cross. Undoing back to the saved text makes
  it clean again.
- Questions use telaio's modal frame (`views/choice.rs`), never the platform's plain prompt: the
  title in the header, the explanation in `fg_muted`, the answers in the footer, the suggested one
  a `PrimaryButton` (also Enter), one that loses work a `DangerButton`, the rest
  `SecondaryButton`. Escape or the close button cancels.
- Closing a tab, closing the window, or opening another folder (or another file in file mode),
  with unsaved changes asks "Save the changes to name?": Don't save (danger), Cancel, Save
  (primary), once per document with changes. Cancel stops there and nothing closes.
- Opening a path that no longer exists while documents are open shows a warning notification and
  keeps them open.
- When the window gets the focus back, a file changed by another program reloads at once if it has
  no unsaved changes; otherwise it asks: Keep my changes, Reload (primary). Saving over a file
  changed on disk asks first: Cancel, Overwrite (primary).
- Saving trims trailing whitespace and ends the file with a line break (`trim_trailing_whitespace`
  and `insert_final_newline` in `settings.toml`, both on), as one undo step.

### Settings

Modal over the window (`Ctrl+,`): 800 x 720, `overlay`, radius 14, shadow, `scrim` behind it. Left
column of 188 px with six categories: Appearance, Editor, Files, Git, Languages, About. Right side:
52 px header with the category title and a close button, scrollable content (padding 20, gap 22,
sections separated by a 1 px `border` line, each section a 500 title then rows of label at the
left and control at the right with a 14 px gap), and a 52 px footer with "Changes apply at
once." and a Reset values button. The footer of the left column says where the file lives:
`~/.config/ted/settings.toml`.

| Category | Settings |
| --- | --- |
| Appearance | Language (Auto, English, Spanish), theme cards (System, Light, Dark), UI font and size |
| Editor | Editor font and size, line numbers, highlight current line, word wrap, tab size, insert spaces, markdown view on open and synchronized scroll |
| Files | Autosave on tab or window change, trim trailing whitespace, final newline, encoding and line ending of new files, explorer exclude patterns (default `.git, target, node_modules`, `.gitignore` is honored too) |
| Git | Auto fetch, confirm before discarding |
| Languages | List of the eight supported languages and their extensions |
| About | Version and project information (not detailed in the prototype) |

Defaults of the prototype: UI size 13, editor size 12.5, tab 4, insert spaces on, wrap off, line
numbers on, current line on, autosave on, trim on, final newline on, Git fetch on, Git confirm on,
markdown sync on.

What 0.1.0 has (`views/settings.rs`, telaio's `settings_dialog` and `SettingsLayout`, as
Postino's):

- Opened with the title bar gear or `Ctrl+,`; closed with the cross, Escape, a click outside, or
  Enter. Every change applies at once and writes `settings.toml`; Reset values restores the
  defaults (except the log level and `highlight_max_size_mb`, which only the file sets).
- **Appearance**: Language as a segmented control (Automatic with the language it resolves to,
  English, Español), the three theme cards, the UI font size stepper (11 to 16).
- **Editor**: Text (editor font size stepper, 10 to 18 in 0.5 steps; line numbers; highlight
  current line), Indentation (tab size 2, 4 or 8; insert spaces, used where the file's own
  indentation is not detected). Without line numbers the gutter keeps only its 16 px padding.
- **Files**: Saving (trim trailing whitespace, final newline), Explorer (the exclude patterns, one
  comma separated field, applied on Enter or when the field is left).
- **About**: telaio's About header (version, platform, telaio, release notes and GitHub), drawn
  by ted with the app icon at 48 px in place of telaio's initial square. ted has
  no updater: the footer says new versions are published on GitHub.
- Font families, word wrap, autosave, encoding and line ending of new files, Git and Languages
  are not in 0.1.0; the families can still be set in `settings.toml`.
- `TED_OPEN=settings` or `settings-about` opens the modal at startup.

### Git panel (later milestone)

Not in 0.1.0. Header `COMMIT` with Refresh, Discard all, List/tree toggle, History and More
buttons (22 px). Groups (Changes, Unversioned) of 26 px rows with a 16 px checkbox, file icon,
name, directory detail in 11.5 px, count and status letter. At the bottom, a 10 px padded block
with a 76 px message box (`raised`, radius 8, `accent` border on focus), an Amend checkbox, the
staged count with `+n` in `success` and `-n` in `danger`, and the Commit (`accent`) and "Commit
and push..." buttons (30 px). Its toggle sits next to Files in the status bar, with the change count
in Geist Mono 11/500 beside the icon (`Alt+2`, `Ctrl+2` on macOS, as Postino).

### Markdown preview (later milestone)

Not in 0.1.0. A `.md` file gets a floating toolbar (top 10, right 22, `overlay`, radius 8) with
Editor, Split and Preview buttons and "open preview in a tab"; `Ctrl+Shift+V` cycles the views.
Split view uses synchronized scroll (setting). The preview is a 720 px column with the typography
above; code blocks use `raised`, a `border` border, radius 8 and the same syntax colors. A preview
tab shows an eye icon and the "Preview" detail in its label.

## Usage rules

- Anything that is a color is a palette token or a syntax token of the tables above.
- Line numbers and the gutter never scroll horizontally with the text; the text does.
- The current line highlight and the selection are painted under the text by the editor element,
  with `hover` and `accent_subtle`.
- A tab always shows its close cross; a modified document shows the dot in its place.
- Pointer cursor on clickable rows, text cursor on the editor, `col-resize` on the panel handle.
- Text in the editor and the preview is selectable; everything else in the window is not.
- Never animate while idle (see `docs/performance.md`). The prototype has no animation beyond
  hover, and ted keeps it that way.

## Prototype versus the decisions of `AGENTS.md`

Differences to settle, with the choice to follow in the meantime:

- The prototype strings are Spanish; ted's source language is English. Strings in this file are
  the English equivalents.
- The prototype pins telaio `v0.9.0` and `ropey = "1.6"`; ted pins telaio `v0.9.1` and uses the
  `ropey` of `gpui-component` (2.0.0-beta.1).
- The prototype's selected tree row and category use a solid `accent_subtle`; telaio's `TreeRow`
  uses `list_active` (`accent_subtle` at 20% alpha). Use telaio's component and its look.

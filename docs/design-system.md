# ted design system

Reference for anyone (human or agent) touching the UI. It is written from the Claude Design
prototype in `design_system/` (`ted.dc.html`, `ted-lang.js`, `ted-data.js`), as the shared rules
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
| `--surface` | `surface` | `#f4f5f7` | `#1b1c21` | Title bar, tab bar, explorer, rail, status bar |
| `--raised` | `raised` | `#ffffff` | `#23252c` | Inputs, search trigger, code blocks in the preview |
| `--overlay` | `overlay` | `#ffffff` | `#262830` | Menus, settings modal, markdown toolbar |
| `--border` | `border` | `#e3e5ea` | `#2c2e36` | Separators, panel borders |
| `--border-strong` | `border_strong` | `#cdd0d8` | `#3b3e48` | Control borders, switch off track |
| `--fg` | `fg` | `#1b1c22` | `#e9eaee` | Text |
| `--fg-muted` | `fg_muted` | `#5c606e` | `#a0a3ae` | Inactive tabs, current line number |
| `--fg-subtle` | `fg_subtle` | `#8b8e9a` | `#6f7280` | Icons, line numbers, status bar |
| `--accent` | `accent` | `#4f57d8` | `#5b63e6` | Active tab bar line, rail indicator, switches, primary button |
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
- Markdown headings are `keyword` at weight 600; other tokens are weight 400.
- The log highlighter is not tree-sitter: dates are `comment`, `ERROR`/`FATAL` `tag`, `WARN`
  `number`, `INFO` `function`, `DEBUG`/`TRACE` `comment`, `[context]` `property`.

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
| Rail badge | Geist | 10 / 600 |
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
| Status bar | 24 | ted (same as Postino) |
| Tree row | 26 | telaio `TREE_ROW_HEIGHT` |
| Explorer header | 36 | ted (padding 10 10 6 14) |
| Menu row (recents) | 30 | ted |
| Settings category row | 28 | ted |
| Settings header, footer | 52 | telaio `MODAL_HEADER_HEIGHT` |
| Control (input, button, select) | 30 | telaio `CONTROL_HEIGHT` |
| Search trigger | 26 | ted |
| Workspace switcher button | 26 | ted |
| Rail button | 42 | ted |
| Theme card preview | 92 | telaio `THEME_PREVIEW_HEIGHT` |
| Commit message box | 76 | ted (later milestone) |
| Switch | 20 (track 34) | ted |

### Widths (px)

| Element | Value |
| --- | --- |
| Explorer / Git panel | 260 default, resizable 200 to 480, 6 px drag handle on its right edge |
| Activity rail | 44 (the prototype; Postino's is 48) |
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
- Title bar: 12 px left padding, logo square 18 (radius 5, `accent`, white "t" 11/700), "ted" 600, 1x16
  separator. The logo block shows only in the title bar without a rail on the left.
- Rail indicator: 2 px wide `accent` bar, 9 px from top and bottom, on the left edge of the active
  button. Badge: min 15 x 15, radius 8, `accent`, 10/600.
- Status bar: padding 0 12, gap 16, 11.5 px, `fg_subtle`. Branch button 20 high in `accent_text`.
- Icon buttons: 22 in panel headers, 24 in the tab strip, 26 x 24 in the markdown toolbar, 20 for
  the shortcuts button in the status bar.
- Radii: 4 chips and checkboxes, 5 logo and toolbar items, 6 rows and icon buttons, 7 search
  trigger and categories, 8 inputs, buttons, code blocks, 10 menus and theme cards, 14 modal.

## Layout, top to bottom

```
+--------------------------------------------------------------+
| title bar 40: logo, switcher, search trigger, window controls |
+------+-------------+-----------------------------------------+
| rail | explorer    | tab bar 36                              |
| 44   | or Git      +-----------------------------------------+
|      | panel 260   | gutter | text                           |
|      |             |        |                                |
+------+-------------+-----------------------------------------+
| status bar 24                                                |
+--------------------------------------------------------------+
```

1. **Title bar** (`surface`, bottom border): logo, "ted", separator, workspace switcher (folder
   icon, folder name, chevron; opens the recents menu), a centered 360 px search trigger
   ("Search files and actions", `Ctrl+K` hint; folder mode only), and the three window controls
   at the right (close turns `danger` on hover). The window uses client decorations on Linux.
2. **Rail** (44 px, `surface`, right border): Explorer and Git buttons (icon 20), settings at the
   bottom. The Git button carries a badge with the change count.
3. **Side panel** (`surface`, right border): Explorer or Git, one at a time, resizable.
4. **Tab bar** (folder mode only).
5. **Editor** (and, for markdown, the preview beside it or instead of it).
6. **Status bar** (`surface`, top border): left, folder path (folder mode) and branch with the
   ahead count; right, word count (markdown), `Ln n, Col n`, indentation, `UTF-8`, line ending,
   language, shortcuts button.

## Screens

### Empty state, with recents

No argument on the command line. The window has no document: no tabs, no editor, no rail panel.
It lists **recent folders** and **recent files** with the actions Open file (`Ctrl+O`) and Open
folder (`Ctrl+Shift+O`).

The prototype has no dedicated empty screen. What it designs is the recents menu that opens from
the workspace switcher in the title bar, and the empty state reuses its rows: a 360 px `overlay`
card (radius 10, padding 6, shadow), a `RECENT FOLDERS` section label, rows of 30 px (icon 14,
name, path in 12 px `fg_subtle` with ellipsis, a check on the current one), a 1 px separator, a
`RECENT FILES` section, a separator, then the two Open actions with their shortcut hints in
Geist Mono 11. The empty state shows the same list centered in the content area, without the
popover chrome. Confirm this with a design pass before implementing it.

### File mode

`ted some/file.txt`. The title bar shows the file name where the workspace switcher is, no search
trigger. No rail, no explorer, no tab bar: the editor fills the window. The status bar has no
folder path.

The prototype only shows folder mode; this is taken from `AGENTS.md`.

### Folder mode

`ted some/folder` or Open folder. The full layout above. Explorer header: folder name as an
uppercase section label, then New file, New folder and Collapse all (22 px icon buttons). Tree rows
show Git status letters at the right. `Ctrl+B` toggles the explorer, `Ctrl+Shift+G` the Git panel,
`Esc` closes a floating panel.

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

### Git panel (later milestone)

Not in 0.1.0. Header `COMMIT` with Refresh, Discard all, List/tree toggle, History and More
buttons (22 px). Groups (Changes, Unversioned) of 26 px rows with a 16 px checkbox, file icon,
name, directory detail in 11.5 px, count and status letter. At the bottom, a 10 px padded block
with a 76 px message box (`raised`, radius 8, `accent` border on focus), an Amend checkbox, the
staged count with `+n` in `success` and `-n` in `danger`, and the Commit (`accent`) and "Commit
and push..." buttons (30 px). The rail already reserves the button and badge.

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
- The prototype's rail is 44 px wide; Postino's is 48. Decide when the rail is built.

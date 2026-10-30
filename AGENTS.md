# AGENTS.md

Guidance for AI agents working on ted (Tanis editor).

The rules shared by every Rust desktop app (design first, stack, telaio, performance,
translations, logging, CLI, docs, releases, packaging, CI, testing) are imported below. This file
only adds what is specific to ted, and wins where the two differ.

@~/.claude/rust/RULES.md

## Project

ted is a native, lightweight text editor for Linux, Windows and macOS, written in Rust with
`gpui` (through `gpui-kit`) and telaio. Postino (`~/dev/postino`) is the reference
implementation: copy its skeleton, conventions and patterns, and rename.

**Speed is the product.** Opening a file, scrolling, typing and browsing a folder must feel
instant, with big files too. Every decision trades features for speed when the two collide.

Two modes, from the design:

- **File mode**: `ted some/file.txt` opens that file alone, with no explorer and no tabs.
- **Folder mode**: `ted some/folder` (or Open folder) shows the explorer, the document tabs and,
  later, the Git panel.
- With no argument, the empty state lists recent folders and recent files.

It is a desktop project, unrelated to the Expo mobile apps in `~/dev`. Do not reuse the
Expo/React Native conventions here.

## Decisions taken

- Name: **ted** (Tanis editor). Binary `ted`, app id `codes.tanis.ted`, env prefix `TED_`,
  repository `tanisperez/ted`.
- License: Apache-2.0, as Postino.
- Stack: Rust (edition 2024, stable toolchain) + `gpui-kit` 0.8.0 + telaio (`telaio-ui`,
  `telaio-app`) pinned to one tag in `[workspace.dependencies]`, as in Postino (`v0.9.1` today).
  `cargo tree -d --depth 0 | grep gpui` must stay empty.
- **Text engine: our own.** No `gpui-kit` `InputState` for the document editor: it costs about
  460 MB of RSS for a 15 MB file in Postino and we do not control its render cost. ted has its
  own rope buffer and its own gpui element that lays out and paints only the visible lines.
  `gpui-kit` inputs are still fine for small fields (find box, settings).
- Rope: `ropey` at the version `gpui-component` already builds (2.0.0-beta.1 with gpui-kit
  0.8.0), so it is not compiled twice. Same for `tree-sitter` and its grammars: reuse the
  versions and, where it fits, the highlighter of `gpui-component` (`src/highlighter/`, Apache
  licensed) before adding new crates.
- **Zed is an inspiration, not a source.** Its editor crates (`editor`, `text`, `rope`,
  `sum_tree`, `language`, ...) are GPL-3.0 or AGPL-3.0, so their code must not be copied into
  ted. Reading them for ideas (display maps, how they virtualize lines, how they batch glyph
  runs) is fine; write our own code. Only `gpui` itself is Apache-2.0.
- Languages of the UI: **English** (source and fallback) and **Spanish**. Not Galician nor
  Italian.
- No website, no `site/`, no package repositories and no updater for now. The user docs live
  as Markdown under `docs/user/` until a website is decided. Sections 13, 15 and 16 of the shared
  rules apply only where they do not need a website or the updater.
- Releases (decided on 2026-10-30): the `release.yml` workflow and `packaging/` of Postino, with
  the same packages minus `latest.json` (deb, rpm, Arch, AppImage, tarball, macOS dmg and app
  tarball, Windows installer and portable zip, `SHA256SUMS`). The installers put `ted` on the
  `PATH` where the platform allows it. See `docs/releasing.md`.
- Settings: `settings.toml` under the config dir (`~/.config/ted/settings.toml` on Linux), plain
  text, applied at once, as in the design.
- Design: the Claude Design prototype is in `design_system/` (`ted.dc.html`, open it in a
  browser; `github.md` maps each screen to the Postino and telaio files it came from). It is the
  source of truth until `docs/design-system.md` is written from it (rule 1.3).

## Pending decisions (ask before choosing)

- Website, packaging channels and the in-app updater (not before the user asks).
- Language of the user docs (English for now, as everything in the repository).
- Word wrap, multiple cursors, split views: not in 0.1.0; ask before designing them. The
  minimap was decided on 2026-10-30: it is in 0.1.0 (#16), built directly in the style of VS Code
  and Zed from the design tokens.
- Files bigger than the 100 MB target (memory mapping, lazy loading): only if the user asks.
- Encodings other than UTF-8 (UTF-16, Latin-1): 0.1.0 opens UTF-8 and shows anything else as
  lossy or read only; ask before adding real support.

## Architecture

A Cargo workspace of small crates, UI kept thin. Planned layout (update this diagram when a
crate is added). Dependency direction, no cycles, no crate depends on `ted-app`:

```
ted-app ──> ted-workspace ──> ted-core
   ├──────> ted-syntax ─────> ted-core
   ├──────> ted-core
   └──────> ted-cli (standalone, depends on no other ted crate)
```

- `ted-core`: the text buffer (rope), positions, selections, edits, undo history, line endings,
  indentation detection. No IO, no gpui.
- `ted-syntax`: language detection by extension and name, tree-sitter highlighting computed
  incrementally and off the main thread, the log highlighter (levels and dates, no tree-sitter).
  No gpui.
- `ted-workspace`: filesystem access. Reads files in the background, lists one folder at a time
  (lazily, honoring `.gitignore` and the exclude patterns), watches for changes, saves
  atomically (write a temporary file, then rename). No gpui.
- `ted-cli`: `-h/--help`, `-V/--version` and the path argument, `IDENTITY` (the
  `telaio_app::AppIdentity` of ted), `build.rs` with the commit, as `postino-cli`.
- `ted-app`: the gpui binary (`ted`). Views, the editor element and glue. UI state that can be
  tested without a window lives in plain Rust under `ted-app/src/state`, as in Postino.

Everything except `ted-app` is testable with `cargo test`, with no window and no GPU.

`ted-app` must call `.with_assets(gpui_kit::assets::AllAssets)` and open the window with
`WindowDecorations::Client` on Linux, as Postino (see its `AGENTS.md` and `docs/gpui-notes.md`).

## Performance

The shared rules (section 8) and Postino's "Performance" section apply in full: an idle window
does no work, `render` never costs in proportion to the data size, big texts are shared as
`Arc`/`SharedString`, derived data is cached under a cheap key. On top of that, for ted:

- **Targets** (release build, measured on Linux and noted in `docs/performance.md` with the
  machine and the numbers):
  - a 100 MB log file shows its first screen in under 100 ms after the window has opened, and the
    rest loads in the background without blocking the UI;
  - scrolling (wheel, page down, dragging the scrollbar, jumping to the end) stays at the display
    refresh rate with that file open;
  - a keystroke is painted in the next frame, also in that file;
  - a folder with 100 000 files opens its explorer at once (only the expanded folders are read)
    and scrolls without stutter;
  - an idle window, with any file open, does no work (no cursor blink notifies while unfocused).
- **Never on the main thread:** reading or writing a file, listing a folder, parsing or
  highlighting more than the visible range, searching a whole file. Use gpui's background
  executor and send results back with a generation counter, dropping stale ones.
- **Per frame, only the visible lines:** layout, shaping and highlighting are done for the
  viewport plus a small margin, never for the whole document. Shaped lines are cached by
  (line, buffer version, font settings) and evicted.
- **No per-line allocations at load.** Building the rope and the line index of a 100 MB file must
  not allocate one object per line. Line lengths for the horizontal scroll width are estimated
  and refined lazily.
- `make bench-files` generates the big fixtures (100 MB log, deep and wide folder trees) under
  `target/bench-files/`, never committed. Every change to the buffer, the editor element, the
  loader or the explorer is checked against them before it is done.

## Translations

As in Postino (`docs/i18n.md`), with two languages: `en` and `es`. Files in
`crates/ted-app/locales/*.yml`, `_version: 2`, one file per area, keys `area.key`. The tests of
`state/locale.rs` check that every key exists in both languages with the same placeholders and no
em dash.

## Logging

`telaio_app::logging` with `IDENTITY`, as Postino. Never log file contents or text the user
typed. Paths are fine at Debug. One Info line per file or folder opened, with its size and the
load time.

## Testing and debug hooks

- Unit tests in every library crate, with fixtures under `tests/fixtures/`. Buffer edits and undo
  get property style tests against a plain `String` model.
- Visual checks in the real app as in the shared rules (section 17): XWayland, `xdotool`,
  `spectacle`, isolated `XDG_CONFIG_HOME` and `XDG_STATE_HOME`, killing only our own PID.
- Debug hooks, read once at startup: `TED_LOG=<level>`, `TED_OPEN=<state>` (`settings`,
  `settings-about`, `components`).

## Working rules

- Git remote `origin` is `tanisperez/ted`. Commits are fine; never `git push` unless the user says
  so explicitly. Commit messages, issues and pull requests in English.
- Work is tracked in the GitHub milestone of each version (`gh issue list --milestone 0.1.0`).
  Complex issues go on a branch.
- Keep changes small and traceable to the request. Do not refactor unrelated code.
- Run `make format`, `make lint` and `make test` before considering a task done.
- GPUI evolves quickly. Read the pinned `gpui-kit` source in `~/.cargo/registry/src/` instead of
  relying on memory of its API.
- Writing style: never use the em dash in code, comments or docs. Use a comma or a period.

## Commands

See `make help`. Main targets: `run`, `build`, `release`, `check`, `lint`, `format`, `test`, and
`bench-files` once #4 adds it.

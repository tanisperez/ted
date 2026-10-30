# Architecture

How ted is put together and why. For the project rules read `AGENTS.md`; for the look of the UI,
`docs/design-system.md`; for the speed targets and how they are measured, `docs/performance.md`.

## Principles

- **Speed is the product.** Opening a file, scrolling, typing and browsing a folder must feel
  instant, also with big files. When a feature and speed collide, speed wins.
- **A thin UI.** Everything except `ted-app` is plain Rust, testable with `cargo test` with no
  window and no GPU. UI state that can be tested without a window lives under
  `crates/ted-app/src/state/`; views only render and forward input.
- **Readable code over clever code.** Explicit types at public boundaries, small modules, doc
  comments on every public item, no `unsafe` (forbidden by the workspace lints).
- **Lightweight.** No async runtime of our own: background work uses gpui's background executor.
  An idle window does no work.
- **Postino is the reference.** Skeleton, conventions and patterns are copied from it and renamed.

## Crates

A Cargo workspace of small crates. Dependency direction, no cycles, and no crate depends on
`ted-app`:

```
ted-app ──> ted-workspace ──> ted-core
   ├──────> ted-syntax ─────> ted-core
   ├──────> ted-core
   └──────> ted-cli (standalone, depends on no other ted crate)
```

| Crate | Responsibility | gpui |
| --- | --- | --- |
| `ted-core` | The text buffer (rope), positions, selections, edits, undo history, line endings, indentation detection. No IO. | no |
| `ted-syntax` | Language detection by extension and name, tree-sitter highlighting computed incrementally and off the main thread, the log highlighter (levels and dates, no tree-sitter). | no |
| `ted-workspace` | Filesystem access. Reads files in the background, lists one folder at a time (lazily, honoring `.gitignore` and the exclude patterns), watches for changes, saves atomically (write a temporary file, then rename). | no |
| `ted-cli` | `-h/--help`, `-V/--version` and the path argument, `IDENTITY` (the `telaio_app::AppIdentity` of ted), `build.rs` with the commit, as `postino-cli`. | no |
| `ted-app` | The gpui binary (`ted`). Views, the editor element and glue. | yes |

Everything except `ted-app` is testable with `cargo test`. `ted-app` calls
`.with_assets(gpui_kit::assets::AllAssets)` and opens the window with `WindowDecorations::Client`
on Linux, as Postino. Update the diagram above when a crate is added.

## Why ted has its own text engine

The document editor does not use `gpui-kit`'s `InputState`:

- **Memory.** In Postino a 15 MB body in an `InputState` costs about 460 MB of RSS. A 100 MB log
  is the target here, so that cost cannot be paid.
- **Render cost is not under our control.** `InputState` lays out and shapes text in code we do
  not own, and gpui re-renders the whole window on every `cx.notify()`. We cannot guarantee that
  its per frame work stays independent of the document size.

So ted has its own rope buffer (`ted-core`) and its own gpui element (`ted-app`) that lays out and
paints only the visible lines. `gpui-kit` inputs remain fine for small fields: the find box, the
settings controls, the search trigger.

### Viewport-only layout and paint

Per frame, layout, shaping and highlighting cover the viewport plus a small margin, never the
whole document.

- The element asks the buffer for the lines in the visible range (rope slices, no copies of the
  document) and shapes only those.
- Shaped lines are cached by (line, buffer version, font settings) and evicted when they leave
  the margin.
- The vertical scroll height is `line count * line height`, known at once from the line index.
  Line lengths for the horizontal scroll width are estimated and refined lazily as lines are
  shaped.
- Building the rope and the line index of a 100 MB file does not allocate one object per line.
- `render` never costs in proportion to the document size. Derived data is computed when its
  source changes and cached under a cheap key (a document id and a generation counter).

### Background loading with generation counters

Nothing slow runs on the main thread: reading or writing a file, listing a folder, parsing or
highlighting more than the visible range, searching a whole file.

- Work runs on gpui's background executor and sends its result back to the main thread tagged
  with a **generation counter**. Each request that supersedes another (a newer edit, a newer
  scroll position, a folder collapsed and expanded again) bumps the counter. When a result
  arrives with an older generation than the current one, it is dropped.
- Opening a file: the first chunk is read and shown (first screen in under 100 ms after the window
  has opened), and the rest loads in the background in chunks into the rope, each chunk applied
  under the document's generation.
- Highlighting (`ted-app/src/state/syntax.rs`): tree-sitter parses off the main thread, one parse
  at a time. The viewport shows plain text until the first tree arrives. After that, every edit
  is recorded by the buffer (`Buffer::track_changes`, `take_changes`) and moves the tree at once
  (`SyntaxTree::edited`), so typing keeps its colors, and an incremental parse from that moved
  tree replaces it. A result for an older buffer version is dropped and the next parse starts.
  Buffer versions are unique in the process, so a reloaded file never matches the old tree.
  Files above `highlight_max_size_mb` (`settings.toml`, 4 by default) are never parsed; logs use
  the line highlighter at any size, on the visible lines only.
- Folder listing: one directory at a time, only for expanded folders, each listing tagged with the
  folder's generation.

## Dependencies

- **Rope**: `ropey` at the version `gpui-component` already builds (2.0.0-beta.1 with
  `gpui-kit` 0.8.0), so it is not compiled twice. The same goes for `tree-sitter` and its
  grammars: reuse the versions and, where it fits, the highlighter of `gpui-component`
  (`src/highlighter/`, Apache licensed) before adding new crates. `cargo tree -d --depth 0 |
  grep gpui` must stay empty.
- **gpui-kit** 0.8.0 and **telaio** (`telaio-ui`, `telaio-app`) pinned to one tag in
  `[workspace.dependencies]`, as in Postino.
- **Settings** in `settings.toml` under the config dir, applied at once.

## Zed as inspiration only

Zed's editor crates (`editor`, `text`, `rope`, `sum_tree`, `language`, ...) are GPL-3.0 or
AGPL-3.0, so their code must not be copied into ted, which is Apache-2.0. Reading them for ideas
(display maps, how they virtualize lines, how they batch glyph runs) is fine; the code is ours.
Only `gpui` itself is Apache-2.0.

## Modes

- **File mode**: `ted some/file.txt` opens that file alone, with no explorer and no tabs.
- **Folder mode**: `ted some/folder` (or Open folder) shows the explorer, the document tabs and,
  later, the Git panel.
- **Empty state**: with no argument, the recent folders and recent files.

## Out of scope for 0.1.0

Word wrap, minimap, multiple cursors and split views (ask before designing them), files larger
than the 100 MB target (memory mapping, lazy loading), encodings other than UTF-8 (UTF-16 and
Latin-1 open as lossy or read only), the Git panel and the markdown preview (later milestones),
a website, packaging channels and an in-app updater.

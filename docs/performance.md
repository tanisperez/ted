# Performance

Speed is the product. This file lists the targets, how each one is measured and the results. The
rules behind them are in `AGENTS.md` (Performance) and the shared rules (section 8).

Every change to the buffer, the editor element, the loader or the explorer is checked against
these before it is done.

## Targets

All on a **release build**, on Linux, and recorded below with the machine and the numbers.

| # | Target | Fixture |
| --- | --- | --- |
| 1 | A 100 MB log shows its first screen in under 100 ms after the window has opened; the rest loads in the background without blocking the UI | 100 MB log |
| 2 | Scrolling (wheel, page down, dragging the scrollbar, jumping to the end) stays at the display refresh rate with that file open | 100 MB log |
| 3 | A keystroke is painted in the next frame, also in that file | 100 MB log |
| 4 | A folder with 100 000 files opens its explorer at once (only the expanded folders are read) and scrolls without stutter | wide and deep trees |
| 5 | An idle window, with any file open, does no work (no cursor blink notifies while unfocused) | any file |

Memory is tracked alongside: the RSS after opening the 100 MB log must stay close to the size of
the file plus the line index, nowhere near the 460 MB that a 15 MB `InputState` costs in Postino.

## How to measure

### Setup

- `cargo build --release` (or `make release`). A debug build is not a valid measurement. Debug
  builds optimize their dependencies, so they are good enough to judge the UI by hand, but the
  numbers here are always from release.
- `make bench-files` generates the fixtures under `target/bench-files/` (never committed): the
  100 MB log and the deep and wide folder trees (100 000 files).
- Run on native Wayland. XWayland adds about 12 ticks per 10 s of CPU noise to the idle check.
- Close other heavy applications and note the machine, the display refresh rate and the commit.

### First screen (target 1)

Start `ted target/bench-files/<log>` and read the Info log line written per opened file
(`TED_LOG=info`): the size and the load time. The first screen time is the time from the window
open to the first painted frame with text. Repeat five times, report the median. The rest of the
file must keep loading while the UI responds (scroll during the load).

### Scrolling and typing (targets 2 and 3)

With the 100 MB log open, scroll with the wheel, page down, drag the scrollbar and jump to the end
(`Ctrl+End`). Check that the frame time stays under the refresh interval (16.6 ms at 60 Hz, 6.9 ms
at 144 Hz) with a frame time overlay or a temporary 10 ms threshold log of the render path, as
Postino describes for UI stalls. For typing, hold a key and check the main thread CPU while
typing and that the key is painted in the next frame, at the start, the middle and the end of the
file.

### Explorer (target 4)

Open the 100 000 files tree with `ted target/bench-files/<tree>`. Only the root listing is read:
the window must be usable at once. Expand and collapse folders, scroll the list with the wheel and
by dragging the scrollbar. Count the folders listed (Debug log) to confirm that only the expanded
ones are read.

### Idle (target 5)

With the app idle and unfocused for 10 s, with the 100 MB log open:

1. Main thread CPU ticks: fields 14 and 15 (`utime` and `stime`) of
   `/proc/<pid>/task/<pid>/stat`, read before and after the 10 s. The difference must barely
   move.
2. GPU: the `drm-engine-render` counter in `/proc/<pid>/fdinfo/*` before and after. It must not
   move at all.

```sh
pid=$(pgrep -x ted)
read_ticks() { awk '{print $14 + $15}' /proc/$pid/task/$pid/stat; }
read_gpu() { grep -h drm-engine-render /proc/$pid/fdinfo/* 2>/dev/null; }
a=$(read_ticks); g1=$(read_gpu); sleep 10; b=$(read_ticks); g2=$(read_gpu)
echo "main thread ticks: $((b - a))"; [ "$g1" = "$g2" ] && echo "GPU: unchanged"
```

Repeat with the window focused: the cursor blink may redraw only the caret, not the whole
document, and must stop while the window is unfocused.

### Memory

Read `VmRSS` from `/proc/<pid>/status` after the 100 MB log has finished loading and after
scrolling through it.

## Results

The 0.1.0 pass (#15, 2026-10-30, commit 82c12b8) met every target; its rows carry that commit.
XWayland rows were driven with `xdotool`, so their timings include its event pacing.

Fill one row per metric and run. The date is `YYYY-MM-DD`; the machine names the CPU, GPU, RAM,
display refresh rate and compositor.

| Date | Commit | Machine | Metric | Value |
| --- | --- | --- | --- | --- |
| 2026-10-29 | after 5de851c | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | Idle 10 s, debug: main thread CPU ticks, GPU render ns | 7 ticks, 0 ns |
| 2026-10-29 | after 5de851c | i5-1235U, 32 GB | `Buffer::from_text`, 99 MB log, 1.33 M lines (`big_buffer` test, release) | 61 ms |
| 2026-10-29 | after 5de851c | i5-1235U, 32 GB | `Buffer::line` of the middle line of that buffer | 2 us |
| 2026-10-29 | after 7be16b4 | i5-1235U, 32 GB | `load_file`, 100 MB generated log (`make bench-files`), 623 k lines, warm page cache (`big_log` test, release) | 85 ms |
| 2026-10-29 | after 7be16b4 | i5-1235U, 32 GB | `load_file`, same file after `posix_fadvise(DONTNEED)` via `dd oflag=nocache` (best effort cold cache, no sudo) | 101 ms |
| 2026-10-29 | after 7be16b4 | i5-1235U, 32 GB | `load_head`, first 256 KB of that file | 0.3 ms |
| 2026-10-29 | after c7bdf72 | i5-1235U, 32 GB | `list_dir`, `wide_0` of `make bench-files` (10 000 entries, default excludes plus the tree's `.gitignore`), cold and warm filter (`list_bench` test, release) | 7.1 ms |
| 2026-10-29 | after c7bdf72 | i5-1235U, 32 GB | `list_dir`, ignored `target/` of that tree (50 top level entries, 5 000 files below it), listed directly | 0.06 ms |
| 2026-10-29 | after 6d2c4b8 | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | Explorer, release: expanding `tree/wide_0` (10 000 entries), listing to rows | 8.7 ms |
| 2026-10-29 | after 6d2c4b8 | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | Explorer, release: 300 wheel steps over that list, main thread CPU | 26 ticks (260 ms) |
| 2026-10-29 | after 6d2c4b8 | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | Explorer open with a watcher, idle 10 s: main thread CPU ticks, GPU render ns | 7 ticks (as an empty window), 0 ns |
| 2026-10-29 | after 5c1c0ad | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | 100 MB log, release: 300 wheel steps (8 ms apart), main thread CPU | 35 ticks in 2.5 s, about 1.2 ms per step |
| 2026-10-29 | after 5c1c0ad | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | 100 MB log, release: dragging the scrollbar thumb from the end, 120 moves (every visible line new on each) | 44 ticks in 2.4 s, about 3.7 ms per move |
| 2026-10-29 | after 5c1c0ad | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | 100 MB log, release: RSS with the whole file loaded | 252 MB |
| 2026-10-29 | after 5e510fe | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | 100 MB log, release: typing 200 characters 10 ms apart in the middle of the file, main thread CPU | 33 ticks, about 1.7 ms per keystroke, painted in the next frame |
| 2026-10-30 | syntax-highlighting branch | i5-1235U, 32 GB | `SyntaxTree::parse`, full parse of the 15 MB pretty JSON (`timing` test, release) | 871 ms, test process peak RSS 473 MB (why `highlight_max_size_mb` defaults to 4) |
| 2026-10-30 | syntax-highlighting branch | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | 100 MB log with the log highlighter, release: 300 wheel steps down, jump to the end, 150 up; editor prepaint per frame (41 lines, trace log) | 453 frames, avg 0.28 ms, p95 0.40 ms, max 1.2 ms |
| 2026-10-30 | syntax-highlighting branch | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | Opening the samples of every language (Rust 7 KB, TOML, JSON, Markdown, HTML, JS, XML), background parse | 0.1 to 15 ms |
| 2026-10-30 | find-in-file branch | i5-1235U, Iris Xe, 32 GB, KDE Wayland (XWayland) | 100 MB log, release: counting every match of `ERROR` in the background (`searched` debug line) | 30 to 36 ms, 18 821 matches; `queue` finds 489 383 |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | 100 MB log, release, native Wayland, median of 5: from opening the file to the first painted frame (head loaded in 15 ms) | 77 ms (target 1: under 100 ms) |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | 100 MB log, release, native Wayland, median of 5: from opening the file to the whole text loaded | 146 ms, the UI responsive meanwhile |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | RSS: empty window / small file / 100 MB log loaded | 136 MB / 139 MB / 247 MB (the log adds about 110 MB) |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | 100 MB log, release, XWayland: 300 wheel steps 8 ms apart, main thread CPU | 35 ticks in 2.5 s, about 1.2 ms per step (target 2) |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | 100 MB log, release, XWayland: 200 page downs 8 ms apart (every visible line new), main thread CPU | 60 ticks in 1.8 s, about 3 ms per page |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | 100 MB log, release, XWayland: dragging the scrollbar thumb, 120 moves | 6 ticks in 0.2 s |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | 100 MB log, release, XWayland: Ctrl+End and Ctrl+Home five times, editor prepaint | max 6 ms (a 16.6 ms frame at 60 Hz) |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | 100 MB log, release, XWayland: typing 200 characters 10 ms apart at the start and at the end | 39 and 36 ticks, about 1.9 ms per keystroke, painted in the next frame (target 3) |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | 100 000 files tree, release, XWayland: root listing / expanding `wide_0` (10 000 entries) | 6.6 ms / 8 ms, window usable at once (target 4) |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | 100 000 files tree, release, XWayland: 300 wheel steps over the 10 000 rows | 27 ticks in 2.5 s |
| 2026-10-30 | 82c12b8 | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland | Idle 10 s, focused, 100 MB log open, release, native Wayland: main thread CPU ticks, GPU render | 0 ticks, GPU unchanged (target 5) |
| 2026-10-30 | minimap branch | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland (XWayland) | Editor prepaint with the minimap, 100 wheel steps, trace log: `views/editor.rs` (1 100 lines, Rust) / 100 MB log | avg 2.8 ms, max 3.9 ms / avg 0.9 ms, max 1.1 ms (one query per line before: avg 7 ms) |
| 2026-11-01 | 7f7126f | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland (XWayland) | Soft wrap on (Alt+Z), release, trace log: 100 MB log, Ctrl+End, 15 page ups, Ctrl+Home; editor prepaint | 22 frames, median 1.2 ms, p95 5.2 ms, max 9.8 ms |
| 2026-11-01 | 7f7126f | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland (XWayland) | Soft wrap on, release: the 1 MB minified JSON on one line (wrapped by characters), Ctrl+End, 10 page ups; editor prepaint | 16 frames, median 0.6 ms, max 4.4 ms |
| 2026-11-01 | after c5548bb | i5-1235U, Iris Xe, 32 GB, 2560x1440 at 60 Hz, KDE Wayland (XWayland) | Global search on the 100 000 files tree, release: listing the folder in the background (95 003 files after its `.gitignore`) / ranking every file per keystroke, in the background | 103 ms / 15 to 20 ms, typing never waits for it |

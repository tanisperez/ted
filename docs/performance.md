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

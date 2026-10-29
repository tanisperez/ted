//! Generates the bench fixtures under `target/bench-files/` (never committed): a 100 MB log, a
//! 1 MB minified JSON one-liner, a 15 MB pretty JSON and a tree of 100 000 files. Deterministic
//! (seeded), and a fixture that already exists is skipped.
//!
//! Run with `make bench-files`.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

const LOG_SIZE: usize = 100_000_000;
const MINIFIED_SIZE: usize = 1_000_000;
const PRETTY_SIZE: usize = 15_000_000;
const TREE_FILES: usize = 100_000;

const LEVELS: [(&str, u32); 5] = [
    ("ERROR", 3),
    ("WARN ", 8),
    ("INFO ", 50),
    ("DEBUG", 25),
    ("TRACE", 14),
];
const MODULES: [&str; 12] = [
    "ted::workspace::load",
    "ted::workspace::tree",
    "ted_core::buffer",
    "ted_core::detect",
    "ted_app::editor::element",
    "ted_app::editor::scroll",
    "ted_app::explorer",
    "ted_syntax::highlight",
    "gpui::window",
    "gpui::platform::linux::wayland",
    "hyper_util::client::legacy::pool",
    "tokio::runtime::scheduler::multi_thread::worker",
];
const WORDS: [&str; 16] = [
    "opened",
    "folder",
    "request",
    "failed",
    "retrying",
    "cache",
    "miss",
    "buffer",
    "frame",
    "painted",
    "watcher",
    "event",
    "queue",
    "timeout",
    "connection",
    "reset",
];

fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench-files");
    fs::create_dir_all(&root)?;
    let mut rng = fastrand::Rng::with_seed(0x7ED);
    skip_or(&root.join("big.log"), |p| log(p, &mut rng))?;
    skip_or(&root.join("minified.json"), |p| minified(p, &mut rng))?;
    skip_or(&root.join("pretty.json"), |p| pretty(p, &mut rng))?;
    skip_or(&root.join("tree"), |p| tree(p, &mut rng))?;
    Ok(())
}

fn skip_or(
    path: &Path,
    generate: impl FnOnce(&Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if path.exists() {
        println!("{} exists, skipped", path.display());
        return Ok(());
    }
    // Written under a temporary name, so an interrupted run never leaves a half fixture behind.
    let partial = path.with_extension("partial");
    if partial.is_dir() {
        fs::remove_dir_all(&partial)?;
    }
    generate(&partial)?;
    fs::rename(&partial, path)?;
    println!("generated {}", path.display());
    Ok(())
}

fn pick<'a, T>(rng: &mut fastrand::Rng, items: &'a [T]) -> &'a T {
    &items[rng.usize(..items.len())]
}

fn level(rng: &mut fastrand::Rng) -> &'static str {
    let total: u32 = LEVELS.iter().map(|l| l.1).sum();
    let mut n = rng.u32(..total);
    for (name, weight) in LEVELS {
        if n < weight {
            return name;
        }
        n -= weight;
    }
    "INFO "
}

fn log(path: &Path, rng: &mut fastrand::Rng) -> std::io::Result<()> {
    let mut out = BufWriter::with_capacity(1 << 20, File::create(path)?);
    let (mut written, mut seconds) = (0usize, 1_793_000_000u64);
    let mut line = String::new();
    while written < LOG_SIZE {
        seconds += rng.u64(0..3);
        let (days, rest) = (seconds / 86_400, seconds % 86_400);
        let (year, month, day) = civil(days);
        line.clear();
        line.push_str(&format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}+01:00 {} {}: ",
            rest / 3600,
            rest % 3600 / 60,
            rest % 60,
            rng.u32(..1000),
            level(rng),
            pick(rng, &MODULES),
        ));
        // Mostly short messages, sometimes long ones, rarely a few KB (stack traces, payloads).
        let words = match rng.u32(..1000) {
            0..=2 => rng.usize(300..700),
            3..=40 => rng.usize(30..100),
            _ => rng.usize(3..16),
        };
        for i in 0..words {
            if i > 0 {
                line.push(' ');
            }
            line.push_str(pick(rng, &WORDS));
            if rng.u32(..8) == 0 {
                line.push_str(&format!("={}", rng.u32(..100_000)));
            }
        }
        line.push('\n');
        out.write_all(line.as_bytes())?;
        written += line.len();
    }
    out.flush()
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian (Hinnant's algorithm).
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

fn record(rng: &mut fastrand::Rng, id: u32, pretty: bool) -> String {
    let name: Vec<&str> = (0..rng.usize(1..4)).map(|_| *pick(rng, &WORDS)).collect();
    let tags: Vec<String> = (0..rng.usize(0..5))
        .map(|_| format!("\"{}\"", pick(rng, &MODULES)))
        .collect();
    let (name, tags) = (name.join(" "), tags.join(if pretty { ", " } else { "," }));
    let score = rng.f64() * 1000.0;
    if pretty {
        format!(
            "  {{\n    \"id\": {id},\n    \"name\": \"{name}\",\n    \"active\": {},\n    \"score\": {score:.3},\n    \"tags\": [{tags}],\n    \"nested\": {{\n      \"x\": {},\n      \"y\": {}\n    }}\n  }}",
            rng.bool(),
            rng.i32(-1000..1000),
            rng.i32(-1000..1000),
        )
    } else {
        format!(
            "{{\"id\":{id},\"name\":\"{name}\",\"active\":{},\"score\":{score:.3},\"tags\":[{tags}],\"nested\":{{\"x\":{},\"y\":{}}}}}",
            rng.bool(),
            rng.i32(-1000..1000),
            rng.i32(-1000..1000),
        )
    }
}

fn json(path: &Path, rng: &mut fastrand::Rng, size: usize, pretty: bool) -> std::io::Result<()> {
    let mut out = BufWriter::with_capacity(1 << 20, File::create(path)?);
    let (open, sep, close) = if pretty {
        ("[\n", ",\n", "\n]\n")
    } else {
        ("[", ",", "]")
    };
    out.write_all(open.as_bytes())?;
    let (mut written, mut id) = (open.len(), 0u32);
    while written < size {
        if id > 0 {
            out.write_all(sep.as_bytes())?;
            written += sep.len();
        }
        let item = record(rng, id, pretty);
        out.write_all(item.as_bytes())?;
        written += item.len();
        id += 1;
    }
    out.write_all(close.as_bytes())?;
    out.flush()
}

fn minified(path: &Path, rng: &mut fastrand::Rng) -> std::io::Result<()> {
    json(path, rng, MINIFIED_SIZE, false)
}

fn pretty(path: &Path, rng: &mut fastrand::Rng) -> std::io::Result<()> {
    json(path, rng, PRETTY_SIZE, true)
}

const EXTENSIONS: [&str; 8] = ["rs", "toml", "md", "json", "txt", "ts", "css", "lock"];

fn tree(root: &Path, rng: &mut fastrand::Rng) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    fs::write(root.join(".gitignore"), "target/\nnode_modules/\n*.log\n")?;
    let mut created = 0usize;
    let mut dirs: Vec<PathBuf> = vec![root.to_path_buf()];

    // Wide: a few folders with 10 000 entries.
    for i in 0..3 {
        let dir = root.join(format!("wide_{i}"));
        fs::create_dir_all(&dir)?;
        for n in 0..10_000 {
            touch(&dir, n, rng, &mut created)?;
        }
    }

    // Deep: a chain of 14 nested folders, with files at every level.
    let mut deep = root.join("deep");
    for level in 0..14 {
        deep = deep.join(format!("level_{level}"));
        fs::create_dir_all(&deep)?;
        for n in 0..20 {
            touch(&deep, n, rng, &mut created)?;
        }
        dirs.push(deep.clone());
    }

    // An ignored folder, big enough to hurt if the explorer walked into it.
    let target = root.join("target");
    for d in 0..50 {
        let dir = target.join(format!("debug_{d}"));
        fs::create_dir_all(&dir)?;
        for n in 0..100 {
            touch(&dir, n, rng, &mut created)?;
        }
    }

    // The rest: a random tree of folders of up to 60 files.
    while created < TREE_FILES {
        let parent = pick(rng, &dirs).clone();
        let dir = parent.join(format!("dir_{}", rng.u32(..1_000_000)));
        if dir.exists() {
            continue;
        }
        fs::create_dir_all(&dir)?;
        for n in 0..rng.usize(1..60) {
            touch(&dir, n, rng, &mut created)?;
        }
        if dirs.len() < 4000 {
            dirs.push(dir);
        }
    }
    println!("tree: {created} files");
    Ok(())
}

fn touch(
    dir: &Path,
    n: usize,
    rng: &mut fastrand::Rng,
    created: &mut usize,
) -> std::io::Result<()> {
    let name = format!("{}_{n}.{}", pick(rng, &WORDS), pick(rng, &EXTENSIONS));
    fs::write(dir.join(name), b"x\n")?;
    *created += 1;
    Ok(())
}

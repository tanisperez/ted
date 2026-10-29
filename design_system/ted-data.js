// Sample folder for the ted prototype: the ted repository itself.
export const ROOT = 'ted';
export const ROOT_PATH = '~/projects/ted';

export const FILES = {
  'README.md': `# ted

Un editor de texto **ligero** para Linux, macOS y Windows, escrito en Rust con [gpui](https://www.gpui.rs) y los componentes de telaio.

Abre un archivo y solo ves ese archivo. Abre una carpeta y aparecen las pestañas, el explorador y Git.

## Instalación

\`\`\`sh
cargo install --path crates/ted-app
ted README.md
\`\`\`

## Formatos

| Formato | Resaltado | Vista previa |
| --- | --- | --- |
| Markdown | Sí | Sí |
| Rust, JavaScript | Sí | No |
| JSON, TOML, XML, HTML | Sí | No |
| Logs | Niveles y fechas | No |

## Atajos

- \`Ctrl+O\` abre un archivo, \`Ctrl+Shift+O\` una carpeta
- \`Ctrl+B\` muestra el explorador y \`Ctrl+Shift+G\` el panel de Git
- \`Ctrl+Shift+V\` alterna la vista previa de markdown

> Los ajustes viven en \`~/.config/ted/settings.toml\`, en texto plano.

\`\`\`rust
fn main() {
    ted_app::run(std::env::args());
}
\`\`\`
`,
  'Cargo.toml': `[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
version = "0.1.0-dev"
license = "Apache-2.0"

[workspace.dependencies]
gpui-kit = "0.8.0"
telaio-ui = { git = "https://github.com/tanisperez/telaio", tag = "v0.9.0" }
telaio-app = { git = "https://github.com/tanisperez/telaio", tag = "v0.9.0" }
ropey = "1.6"
notify = "8.2"

[profile.release]
lto = "fat"
codegen-units = 1
strip = true
`,
  'src/main.rs': `//! ted: a light text editor. One file opens alone; a folder brings tabs.

use std::path::PathBuf;

use gpui_kit::*;
use telaio_app::window;

mod buffer;
mod syntax;

fn main() {
    let target: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);
    Application::new().run(move |cx| {
        telaio_ui::install(cx);
        let mode = match &target {
            Some(path) if path.is_dir() => Mode::Folder(path.clone()),
            Some(path) => Mode::File(path.clone()),
            None => Mode::Empty,
        };
        window::open(cx, "ted", move |window, cx| {
            cx.new(|cx| Editor::new(mode, window, cx))
        });
    });
}
`,
  'src/buffer.rs': `use ropey::Rope;

/// The text of one open file and where it lives on disk.
pub struct Buffer {
    rope: Rope,
    path: Option<std::path::PathBuf>,
    dirty: bool,
    line_ending: LineEnding,
}

#[derive(Clone, Copy, PartialEq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl Buffer {
    pub fn from_text(text: &str) -> Self {
        let line_ending = if text.contains("\\r\\n") { LineEnding::CrLf } else { LineEnding::Lf };
        Self { rope: Rope::from_str(text), path: None, dirty: false, line_ending }
    }

    pub fn insert(&mut self, char_index: usize, text: &str) {
        self.rope.insert(char_index, text);
        self.dirty = true;
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }
}
`,
  'src/syntax/mod.rs': `//! Tree-sitter grammars, one per language, loaded on first use.

pub mod markdown;

pub fn language_for(extension: &str) -> Option<&'static str> {
    match extension {
        "rs" => Some("rust"),
        "js" | "mjs" => Some("javascript"),
        "json" => Some("json"),
        "md" => Some("markdown"),
        "xml" | "svg" => Some("xml"),
        "html" | "htm" => Some("html"),
        "log" => Some("log"),
        _ => None,
    }
}
`,
  'src/syntax/markdown.rs': `//! Markdown to a preview tree, rendered with gpui elements.

use pulldown_cmark::{Event, Parser, Tag};

pub fn blocks(source: &str) -> Vec<Block> {
    let mut out = Vec::new();
    for event in Parser::new(source) {
        if let Event::Start(Tag::Heading { level, .. }) = event {
            out.push(Block::Heading(level as u8));
        }
    }
    out
}
`,
  'assets/preview.html': `<!DOCTYPE html>
<html lang="es">
  <head>
    <meta charset="utf-8">
    <title>ted preview</title>
    <link rel="stylesheet" href="preview.css">
  </head>
  <body class="ted-preview">
    <!-- The markdown preview is rendered here -->
    <main id="content" data-theme="dark"></main>
    <script src="preview.js"></script>
  </body>
</html>
`,
  'assets/preview.js': `// Keeps the preview scrolled with the editor.
const content = document.getElementById('content');

export function syncScroll(line, total) {
  const ratio = total > 0 ? line / total : 0;
  content.scrollTop = ratio * (content.scrollHeight - content.clientHeight);
  return true;
}

window.addEventListener('message', (event) => {
  const { type, line, total } = event.data;
  if (type === 'scroll') syncScroll(line, total);
});
`,
  'config/default.json': `{
  "theme": "dark",
  "font": { "family": "Geist Mono", "size": 13 },
  "editor": {
    "tabSize": 4,
    "insertSpaces": true,
    "wordWrap": false,
    "lineNumbers": true,
    "rulers": [100]
  },
  "files": { "autoSave": "onFocusChange", "trimTrailingWhitespace": true },
  "panel": { "side": "right", "width": 280 },
  "markdown": { "preview": "split", "syncScroll": true },
  "exclude": [".git", "target", "node_modules"],
  "recent": null
}
`,
  'config/keymap.xml': `<?xml version="1.0" encoding="UTF-8"?>
<!-- Default key bindings -->
<keymap platform="linux">
  <bind keys="ctrl-o" action="file::Open"/>
  <bind keys="ctrl-shift-o" action="folder::Open"/>
  <bind keys="ctrl-b" action="panel::ToggleExplorer"/>
  <bind keys="ctrl-shift-g" action="panel::ToggleGit"/>
  <bind keys="ctrl-shift-v" action="markdown::TogglePreview"/>
  <bind keys="ctrl-comma" action="settings::Open"/>
</keymap>
`,
  'logs/ted.log': `2026-10-29T16:58:02.114Z INFO  [ted::app] starting ted 0.1.0-dev on linux-x86_64
2026-10-29T16:58:02.131Z DEBUG [telaio::fonts] loaded Geist, Geist Mono (7 faces)
2026-10-29T16:58:02.140Z INFO  [ted::folder] opened ~/projects/ted (14 files)
2026-10-29T16:58:02.162Z DEBUG [ted::git] branch main, 1 ahead, 4 changes
2026-10-29T16:58:03.508Z INFO  [ted::buffer] README.md loaded in 2ms
2026-10-29T16:58:09.877Z WARN  [ted::watch] ignoring target/ (12034 entries)
2026-10-29T16:58:15.002Z ERROR [ted::syntax] no grammar for "Dockerfile", using plain text
2026-10-29T16:58:21.645Z INFO  [ted::buffer] saved src/buffer.rs (1.2KB)
2026-10-29T16:58:22.019Z DEBUG [ted::git] status refreshed in 18ms
`,
  '.gitignore': `/target
*.local.toml
logs/
`,
};

// Paths in tree order; folders first.
export const TREE = [
  { path: 'assets', dir: true }, { path: 'assets/preview.html' }, { path: 'assets/preview.js' },
  { path: 'config', dir: true }, { path: 'config/default.json' }, { path: 'config/keymap.xml' },
  { path: 'logs', dir: true }, { path: 'logs/ted.log' },
  { path: 'src', dir: true }, { path: 'src/syntax', dir: true }, { path: 'src/syntax/markdown.rs' }, { path: 'src/syntax/mod.rs' },
  { path: 'src/buffer.rs' }, { path: 'src/main.rs' },
  { path: '.gitignore' }, { path: 'Cargo.toml' }, { path: 'README.md' },
];

export const GIT = {
  branch: 'main', ahead: 1, behind: 0,
  changes: [
    { path: 'README.md', letter: 'M' },
    { path: 'src/buffer.rs', letter: 'M' },
    { path: 'src/syntax/markdown.rs', letter: 'A' },
  ],
  unversioned: [{ path: 'assets/preview.js', letter: 'U' }],
  stats: { additions: 42, deletions: 7 },
};

export const DIRTY = { 'src/buffer.rs': true };

//! The kind of a file by its name, for the icon of the explorer (#108). Only the name is read,
//! never the file: the explorer shows thousands of rows and this runs for every visible one on
//! each render, so it is a couple of string comparisons.

/// What a file holds, as far as its icon goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// Source code of a programming language.
    Code,
    /// HTML, CSS and their relatives.
    Web,
    /// Markdown, plain text, reStructuredText.
    Text,
    /// Data and configuration: JSON, TOML, YAML, XML, ini, `.editorconfig`.
    Data,
    /// Shell scripts, batch files, Makefiles, Dockerfiles.
    Script,
    Image,
    Archive,
    /// Lock files (`Cargo.lock`, `package-lock.json`).
    Lock,
    /// Anything else, logs included.
    Other,
}

/// The kind of the file named `name` (no folder).
pub fn file_kind(name: &str) -> FileKind {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "makefile" | "gnumakefile" | "dockerfile" | "containerfile" | "justfile" | "pkgbuild" => {
            return FileKind::Script;
        }
        ".editorconfig" | ".gitignore" | ".gitattributes" | ".gitmodules" | ".dockerignore"
        | ".env" | ".npmrc" | "license" | "notice" | "readme" | "authors" | "changelog" => {
            return if lower.starts_with('.') {
                FileKind::Data
            } else {
                FileKind::Text
            };
        }
        _ => {}
    }
    if lower.ends_with(".lock") || lower == "package-lock.json" || lower == "pnpm-lock.yaml" {
        return FileKind::Lock;
    }
    for archive in [".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst"] {
        if lower.ends_with(archive) {
            return FileKind::Archive;
        }
    }
    let Some((_, extension)) = lower.rsplit_once('.') else {
        return FileKind::Other;
    };
    match extension {
        "rs" | "py" | "pyi" | "pyw" | "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts"
        | "cts" | "go" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "java"
        | "kt" | "kts" | "scala" | "swift" | "m" | "mm" | "cs" | "fs" | "vb" | "rb" | "php"
        | "lua" | "dart" | "zig" | "nim" | "hs" | "ml" | "mli" | "ex" | "exs" | "erl" | "clj"
        | "cljs" | "elm" | "jl" | "r" | "pl" | "pm" | "groovy" | "gradle" | "sql" | "proto"
        | "graphql" | "gql" | "vue" | "svelte" | "astro" | "sol" | "v" | "d" | "pas" | "f90"
        | "asm" | "s" | "wgsl" | "glsl" | "hlsl" => FileKind::Code,
        "html" | "htm" | "xhtml" | "css" | "scss" | "sass" | "less" => FileKind::Web,
        "md" | "markdown" | "mdown" | "mdx" | "txt" | "rst" | "adoc" | "org" | "tex" => {
            FileKind::Text
        }
        "json" | "jsonc" | "json5" | "toml" | "yaml" | "yml" | "xml" | "xsd" | "xsl" | "xslt"
        | "plist" | "ini" | "cfg" | "conf" | "properties" | "env" | "csv" | "tsv" | "csproj"
        | "props" | "targets" | "xaml" | "webmanifest" | "geojson" | "rss" | "svg" => {
            FileKind::Data
        }
        "sh" | "bash" | "zsh" | "fish" | "ksh" | "bat" | "cmd" | "ps1" | "psm1" | "psd1" | "mk"
        | "mak" | "nu" => FileKind::Script,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "icns" | "tif" | "tiff"
        | "avif" | "heic" => FileKind::Image,
        "zip" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "zst" | "7z" | "rar" | "jar" | "war"
        | "deb" | "rpm" | "dmg" | "iso" | "apk" | "appimage" => FileKind::Archive,
        _ => FileKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn kinds_by_extension() {
        assert_eq!(file_kind("main.rs"), FileKind::Code);
        assert_eq!(file_kind("App.TSX"), FileKind::Code);
        assert_eq!(file_kind("index.html"), FileKind::Web);
        assert_eq!(file_kind("README.md"), FileKind::Text);
        assert_eq!(file_kind("Cargo.toml"), FileKind::Data);
        assert_eq!(file_kind("install.sh"), FileKind::Script);
        assert_eq!(file_kind("logo.png"), FileKind::Image);
        assert_eq!(file_kind("ted.tar.gz"), FileKind::Archive);
        assert_eq!(file_kind("server.log"), FileKind::Other);
    }

    #[test]
    fn kinds_by_whole_name() {
        assert_eq!(file_kind("Makefile"), FileKind::Script);
        assert_eq!(file_kind("Dockerfile"), FileKind::Script);
        assert_eq!(file_kind(".editorconfig"), FileKind::Data);
        assert_eq!(file_kind("LICENSE"), FileKind::Text);
        assert_eq!(file_kind("Cargo.lock"), FileKind::Lock);
        assert_eq!(file_kind("package-lock.json"), FileKind::Lock);
        assert_eq!(file_kind("noextension"), FileKind::Other);
        assert_eq!(file_kind(".hidden"), FileKind::Other);
    }
}

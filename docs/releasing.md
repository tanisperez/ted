# Releasing ted

Publishing a version means pushing a tag. The `Release` workflow
(`.github/workflows/release.yml`) builds ted for every platform, attaches the packages to the
GitHub release of that tag and publishes it. It is Postino's workflow without the in-app updater
(no `latest.json`): ted has no updater, no website and no package repositories yet.

## Versions and tags

- The version lives in one place: `version` under `[workspace.package]` in `Cargo.toml`.
  Versions follow SemVer.
- Between two releases the version is the next one with the `-dev` suffix (`0.2.0-dev`). Right
  after a release, bump `version` to the next `X.Y.Z-dev`.
- A development build shows the short commit next to the version in Settings, About, in
  `ted --version` (`ted 0.2.0-dev (4e74e2d)`) and in the startup log line. It comes from
  `ted-cli/build.rs` (`git rev-parse --short HEAD`, or `TED_COMMIT` when set at build time,
  nothing without a repository). Releases and release candidates show the bare version.
- The tag is `v` plus that version: `v0.1.0`. The workflow fails if they differ, so set the final
  version (drop `-dev`) before tagging.
- A tag with a pre-release suffix (`v0.1.0-rc.1`) is accepted when the part before the suffix
  matches the version (or when the version itself has the same suffix). It is published as a
  GitHub pre-release, never marked as the latest one.

## Steps

The work of a version is tracked by its GitHub milestone (`gh issue list --milestone X.Y.Z`).

1. Check that every issue of the milestone is closed and `make format`, `make lint` and
   `make test` pass on `main`.
2. Optionally, run the workflow by hand on `main` first (`gh workflow run release.yml`): a dry
   run builds every package and keeps them as the run's artifacts, without touching any release.
3. Set `version` in `Cargo.toml` to the release (drop `-dev`), run `cargo check` so `Cargo.lock`
   follows, and commit.
4. Tag and push (ask before pushing):

   ```
   git tag v0.1.0
   git push origin main v0.1.0
   ```

5. Create the release as a **draft**, with English Markdown notes (what changes and which
   milestone issues went in, as `#N`):

   ```
   gh release create v0.1.0 --draft --verify-tag --title v0.1.0 --notes-file notes.md
   ```

6. Wait for the `Release` workflow (`gh run watch`). It checks that every package is there,
   writes `SHA256SUMS`, uploads everything to the draft and then publishes it. If the draft does
   not exist yet when the builds finish, the workflow creates one with placeholder notes; edit
   them afterwards with `gh release edit v0.1.0 --notes-file notes.md`.
7. Bump `version` to the next `X.Y.Z-dev` and commit.

## Packages

Targets: Linux x86_64 (built on Ubuntu 22.04, glibc 2.35), Windows x86_64, macOS arm64. The
scripts live in `packaging/`, and every one can run locally.

| Asset | Content |
| --- | --- |
| `ted-X.Y.Z-linux-x86_64.tar.gz` | `bin/ted`, the desktop entry and the icons in a `/usr`-like layout |
| `ted_X.Y.Z_amd64.deb`, `ted-X.Y.Z-1.x86_64.rpm` | built with `nfpm` from `packaging/linux/nfpm.yaml` |
| `ted-X.Y.Z-1-x86_64.pkg.tar.zst` | Arch package, written by `package.sh` with `bsdtar` and `zstd` |
| `ted-X.Y.Z-x86_64.AppImage` | the same files in an AppImage; system libraries are not bundled |
| `ted-X.Y.Z-macos-arm64.dmg`, `.app.tar.gz` | `ted.app`, ad-hoc signed |
| `ted-X.Y.Z-windows-x86_64-setup.exe` | Inno Setup, **per-user** install, no administrator rights |
| `ted-X.Y.Z-windows-x86_64-portable.zip` | `ted.exe`, `LICENSE` and `NOTICE`, nothing to install |
| `SHA256SUMS` | the checksums of every asset |

Locally, on Linux, with `nfpm`, `appimagetool` (and its type2 runtime), `bsdtar` and `zstd` on
the `PATH`:

```
make release
packaging/linux/package.sh 0.1.0 target/release/ted dist
APPIMAGE_EXTRACT_AND_RUN=1 packaging/linux/appimage.sh 0.1.0 target/release/ted dist
```

`packaging/macos/bundle.sh` needs a Mac (`iconutil`, `codesign`, `hdiutil`), and
`packaging/windows/ted.iss` Inno Setup 6 (`ISCC.exe`); both are run by the workflow.

## Desktop integration

- **Icons:** one master, `packaging/icons/ted.svg` (the icon of design v2). The PNGs under
  `packaging/icons/png/` and `crates/ted-app/assets/ted.ico` are generated from it and committed
  (`packaging/icons/README.md`). The macOS `.icns` is built at bundle time.
- **Linux:** `codes.tanis.ted.desktop` (the Wayland `app_id` and X11 `WM_CLASS` of the window)
  with `Exec=ted %F`, the usual text types and `inode/directory`, so ted shows in Open With for
  text files and folders. The deb, rpm and Arch packages put `ted` in `/usr/bin`, on the `PATH`.
- **Windows:** the installer adds a Start menu entry (with the AppUserModelID `codes.tanis.ted`),
  an optional desktop icon, and two tasks checked by default: `ted` on the user `PATH` (removed
  on uninstall) and "Open with ted" in the Explorer menu of files, folders and a folder's
  background. ted also shows in Open With for the usual text extensions, without taking any of
  them over. `ted.exe` carries the icon as resource 1 (`crates/ted-app/build.rs`, `ted.rc`),
  which gpui also uses for the window and the taskbar.
- **macOS:** `Info.plist` declares ted as an alternate editor of text, source code and folders,
  so it shows in Finder's Open With without taking over any type. Finder hands the files over as
  Apple Events, which `main.rs` queues (`state/open_queue.rs`) and opens once the window exists.
  A `.app` cannot put itself on the `PATH`; the README gives the `ln -s` command.
- **Unsigned for now:** Gatekeeper and SmartScreen warn on the first launch. Signing and
  notarization are later steps, as in Postino.

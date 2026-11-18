# AUR package

`PKGBUILD` is the template of `ted-editor-bin` (the AUR `ted` is another app): it installs the
release tarball (`ted-<version>-linux-x86_64.tar.gz`). Set `pkgver` to a stable version and run
`updpkgsums` before publishing. Pre-releases do not go to the AUR.

Not published yet, as Postino's: it needs an AUR account. Until then it can be built from a
checkout with `cd packaging/aur && makepkg -si`. The release also attaches an Arch package
(`ted-editor-<version>-1-x86_64.pkg.tar.zst`) for `pacman -U`.

Once there is an account: add the SSH key as a secret and push `PKGBUILD` plus `.SRCINFO`
(`makepkg --printsrcinfo`) to `ssh://aur@aur.archlinux.org/ted-editor-bin.git` from the release
workflow (`docs/releasing.md`).

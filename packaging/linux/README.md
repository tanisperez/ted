# Linux packaging

| File | Purpose |
|---|---|
| `codes.tanis.ted.desktop` | Desktop entry (`Exec=ted %F`) |
| `nfpm.yaml` | nfpm config shared by the deb and the rpm |
| `package.sh` | Builds the tarball (with `site/install.sh` in it), the deb, the rpm and the Arch package |
| `appimage.sh` | Builds the AppImage |
| `repo.sh` | Builds the signed APT and DNF repositories published on the website (#101) |
| `test-repo.sh` | Tests `repo.sh` end to end with podman (Ubuntu and Fedora containers) |
| `test-install.sh` | Tests the install script (`site/install.sh`) against fake releases, no network |

The packages are named `ted-editor` (the AUR `ted` is another app); the binary is `/usr/bin/ted`.

## Repositories

`website.yml` runs `repo.sh` after each stable release (and when it changes) and uploads the
result with the site: `apt/`, `rpm/` and `ted.asc` (the public key, shared by both). Only the last
3 stable releases are kept. Secrets: `GPG_PRIVATE_KEY` (armored export) and `GPG_PASSPHRASE`.
Without `GPG_PRIVATE_KEY` the repository steps are skipped and the site still deploys. Check a
change with `packaging/linux/test-repo.sh <dir with a deb and an rpm>`.

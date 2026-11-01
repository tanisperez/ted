# ted icon

`ted.svg` is the master: the indigo text file of design v2 (`design_system/ted-icon.svg`,
without its metadata), the same file as `crates/ted-app/assets/ted.svg`, which the title bar,
the empty state and About paint.

The PNG sizes under `png/` and the Windows icon are generated from it and committed:

```
for s in 16 24 32 48 64 128 256 512 1024; do
  rsvg-convert -w $s -h $s ted.svg -o png/ted-$s.png
done
magick png/ted-{16,24,32,48,64,128,256}.png ../../crates/ted-app/assets/ted.ico
```

The macOS `.icns` is built from these PNGs at bundle time (`iconutil`, see `packaging/macos`).

## Document icons

`documents/` holds the icons of the files ted opens when the user makes it their default app:
variant 2a of `design_system/DocIcon.dc.html`, a white page with the ted icon and the type under
it (`MD`, `TXT`, `LOG`, ...), without the type below 40 px. One `<type>.svg` master, one `.ico`
(Windows) and one `.icns` (macOS) per type, all generated and committed:

```
python3 packaging/icons/documents/generate.py [telaio/crates/telaio-ui/assets/fonts]
```

It needs `rsvg-convert`, Pillow and telaio's Geist fonts (read through a private fontconfig).
The Windows installer installs the `.ico` files in `{app}\icons` and points one ProgID per
extension (`ted.md`, ...) at them; `packaging/macos/bundle.sh` copies the `.icns` files into the
bundle, named by `CFBundleTypeIconFile` in `Info.plist`. Neither takes over a file type: the
user chooses ted with Open with. Linux has no per-app document icon (the icon follows the MIME
type and the theme), so the Linux packages install none.

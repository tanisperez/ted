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

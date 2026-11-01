#!/usr/bin/env python3
"""Generates the document icons of ted: one per file type, variant 2a of
design_system/DocIcon.dc.html (a white page with the ted icon in the middle and the type below).

For every type it writes <name>.svg (the master), <name>.ico (Windows) and <name>.icns (macOS)
next to this script. Below 40 px the label is left out, as the design does, so the small sizes
stay readable.

Needs rsvg-convert, Pillow and the Geist fonts of telaio (found through a private fontconfig,
nothing is installed):

    python3 packaging/icons/documents/generate.py [path/to/telaio/crates/telaio-ui/assets/fonts]
"""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent

# Icon name, label: one per language ted highlights (crates/ted-syntax/src/language.rs) plus
# YAML, INI and CSV. The Windows installer and the macOS Info.plist name these files, and
# EXTENSIONS in packaging/windows/ted.iss maps every extension to one of them.
TYPES = [
    ("txt", "TXT"),
    ("md", "MD"),
    ("log", "LOG"),
    ("rs", "RS"),
    ("js", "JS"),
    ("json", "JSON"),
    ("toml", "TOML"),
    ("yaml", "YAML"),
    ("xml", "XML"),
    ("html", "HTML"),
    ("css", "CSS"),
    ("mk", "MK"),
    ("sh", "SH"),
    ("py", "PY"),
    ("ps1", "PS1"),
    ("bat", "BAT"),
    ("ini", "INI"),
    ("csv", "CSV"),
]

ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
ICNS_SIZES = [16, 32, 64, 128, 256, 512, 1024]
# Smaller than this, the label is left out (the design's `px >= 40`).
LABEL_MIN = 40


def svg(label):
    """The design's variant a, with `label` under the ted icon when it is not None."""
    text = ""
    if label is not None:
        text = (
            '<text x="33" y="55" text-anchor="middle" font-family="Geist" font-weight="600" '
            f'font-size="9" letter-spacing=".3" fill="#5b63e6">{label}</text>'
        )
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">
  <path d="M15 5h25l15 15v35a4 4 0 0 1-4 4H15a4 4 0 0 1-4-4V9a4 4 0 0 1 4-4z" fill="#ffffff" stroke="#c3c7e8" stroke-width="1.2"/>
  <path d="M40 5v11a4 4 0 0 0 4 4h11z" fill="#a3a9ff"/>
  <g transform="translate(21 21) scale(.375)">
    <path d="M15 5h25l15 15v35a4 4 0 0 1-4 4H15a4 4 0 0 1-4-4V9a4 4 0 0 1 4-4z" fill="#5b63e6"/>
    <path d="M40 5v11a4 4 0 0 0 4 4h11z" fill="#a3a9ff"/>
    <rect x="19" y="28" width="22" height="4" rx="2" fill="#fff" fill-opacity=".92"/>
    <rect x="19" y="37" width="26" height="4" rx="2" fill="#fff" fill-opacity=".92"/>
    <rect x="19" y="46" width="12" height="4" rx="2" fill="#fff" fill-opacity=".92"/>
    <rect x="34" y="44" width="3" height="8" rx="1.5" fill="#fff"/>
  </g>
  {text}
</svg>
"""


def render(source, size, out, env):
    subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size), str(source), "-o", str(out)],
        check=True,
        env=env,
    )
    return Image.open(out).convert("RGBA")


def main():
    fonts = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "../../../../telaio/crates/telaio-ui/assets/fonts"
    fonts = fonts.resolve()
    if not (fonts / "Geist-SemiBold.ttf").exists():
        sys.exit(f"Geist-SemiBold.ttf not found in {fonts}")
    with tempfile.TemporaryDirectory() as work:
        work = Path(work)
        config = work / "fonts.conf"
        config.write_text(
            '<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "fonts.dtd">'
            f"<fontconfig><dir>{fonts}</dir><cachedir>{work}/cache</cachedir></fontconfig>"
        )
        env = dict(os.environ, FONTCONFIG_FILE=str(config))
        plain = work / "plain.svg"
        plain.write_text(svg(None))
        for name, label in TYPES:
            master = HERE / f"{name}.svg"
            master.write_text(svg(label))

            def image(size):
                source = master if size >= LABEL_MIN else plain
                return render(source, size, work / f"{name}-{size}.png", env)

            ico = [image(size) for size in ICO_SIZES]
            ico[-1].save(
                HERE / f"{name}.ico",
                sizes=[(size, size) for size in ICO_SIZES],
                append_images=ico[:-1],
            )
            icns = [image(size) for size in ICNS_SIZES]
            icns[-1].save(HERE / f"{name}.icns", append_images=icns[:-1])
            print(f"{name}: svg, ico, icns")


if __name__ == "__main__":
    main()

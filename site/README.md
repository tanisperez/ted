# ted website

The website at https://ted.tanis.codes, built from Postino's: plain HTML, CSS and JavaScript, no
build step except the documentation. The `Website` workflow (`.github/workflows/website.yml`)
publishes it to GitHub Pages on every push to `main` that touches `site/`.

- `index.html` and `install.html`: the home and download pages, written by hand.
- `docs-src/`: the sources of the user documentation. `python3 site/docs-src/build.py` (or
  `make docs`) wraps every page of `docs-src/pages/` in `template.html` and writes `docs/`, which
  is generated and git ignored, plus `sitemap.xml` and `llms.txt`. It fails on a broken link or
  anchor and on an em dash. Every user-visible change updates these pages in the same commit.
- `img/`: the screenshots, light and dark, taken from the real app at 1800 x 1125 and saved as
  WebP; `logo.svg`, `icon-512.png` and `og.png`.
- `css/`, `js/` and `fonts/`: Postino's, with the parts only Postino uses removed.

To preview it, build the documentation and serve the folder:

```
make docs
python3 -m http.server -d site 8000
```

repo: tanisperez/postino
branch: main

Also reads: tanisperez/telaio (main), the shared design system (palette, metrics, components).

## Last sync
date: 2026-10-30T08:03:25Z

### Updated in this project
- ted v2: sidebar removed, Files/Git toggles moved into a taller status bar
- Settings gear moved to the title bar, following Postino's title_bar.rs
- New indigo text-file app icon (ted-icon.svg)

## Sync history
- 2026-10-29T17:30:46Z: ted prototype built on Postino's shell and telaio components

## Screen map
| Screen | Repo files |
| --- | --- |
| ted v2.dc.html, title bar + settings gear | postino: crates/postino-app/src/views/title_bar.rs |
| ted.dc.html, title bar | postino: crates/postino-app/src/views/title_bar.rs, theme/metrics.rs |
| ted.dc.html, status bar | postino: crates/postino-app/src/views/status_bar.rs |
| ted.dc.html, tabs | telaio: crates/telaio-ui/src/components/document_tabs.rs |
| ted.dc.html, explorer | telaio: crates/telaio-ui/src/components/tree_row.rs, section_label.rs |
| ted.dc.html, Git panel | postino: crates/postino-app/src/views/git/panel.rs |
| ted.dc.html, settings | telaio: crates/telaio-ui/src/components/settings.rs, modal.rs, theme_card.rs, segmented_control.rs |
| Palette and type | postino: docs/design-system.md; telaio: crates/telaio-ui/src/metrics.rs |

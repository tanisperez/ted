repo: tanisperez/postino
branch: main

Also reads: tanisperez/telaio (main), the shared design system (palette, metrics, components).

## Last sync
date: 2026-10-29T17:30:46Z

### Updated in this project
- ted prototype built on Postino's shell and telaio components
- Palette, metrics, document tabs, tree rows, settings modal and Git panel taken from source

## Screen map
| Screen | Repo files |
| --- | --- |
| ted.dc.html, title bar | postino: crates/postino-app/src/views/title_bar.rs, theme/metrics.rs |
| ted.dc.html, status bar | postino: crates/postino-app/src/views/status_bar.rs |
| ted.dc.html, tabs | telaio: crates/telaio-ui/src/components/document_tabs.rs |
| ted.dc.html, explorer | telaio: crates/telaio-ui/src/components/tree_row.rs, section_label.rs |
| ted.dc.html, Git panel | postino: crates/postino-app/src/views/git/panel.rs |
| ted.dc.html, settings | telaio: crates/telaio-ui/src/components/settings.rs, modal.rs, theme_card.rs, segmented_control.rs |
| Palette and type | postino: docs/design-system.md; telaio: crates/telaio-ui/src/metrics.rs |

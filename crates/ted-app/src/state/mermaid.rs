//! Mermaid diagrams of the Markdown preview (#113): a ```` ```mermaid ```` block drawn as an SVG
//! in ted's colors, by `mermaid-rs-renderer` (pure Rust, no browser). Blocking: the preview runs it
//! on the background executor, with the parse, and keeps the result under [`diagram_key`].

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use mermaid_rs_renderer::{RenderOptions, Theme, render_with_options};

/// The palette a diagram is drawn with, as CSS colors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiagramColors {
    pub dark: bool,
    pub fg: String,
    pub fg_muted: String,
    pub bg: String,
    pub surface: String,
    pub raised: String,
    pub border: String,
    pub border_strong: String,
    pub accent: String,
}

/// The key a drawn diagram is kept under: its source and the colors.
pub fn diagram_key(source: &str, colors: &DiagramColors) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    colors.hash(&mut hasher);
    hasher.finish()
}

/// A diagram drawn: its SVG and its size in pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub svg: String,
    pub width: f32,
    pub height: f32,
}

/// The size of an SVG from the `width` and `height` of its root, or else its `viewBox`.
pub fn svg_size(svg: &str) -> Option<(f32, f32)> {
    let start = svg.find("<svg")?;
    let root = &svg[start..start + svg[start..].find('>')?];
    let attribute = |name: &str| {
        let at = root.find(&format!(" {name}=\""))? + name.len() + 3;
        let value = &root[at..at + root[at..].find('"')?];
        Some(value)
    };
    let number = |value: &str| value.trim_end_matches("px").trim().parse::<f32>().ok();
    let size = attribute("width")
        .and_then(number)
        .zip(attribute("height").and_then(number))
        .or_else(|| {
            let parts: Vec<f32> = attribute("viewBox")?
                .split([' ', ','])
                .filter_map(|part| part.parse().ok())
                .collect();
            (parts.len() == 4).then(|| (parts[2], parts[3]))
        })?;
    (size.0 > 0.0 && size.1 > 0.0).then_some(size)
}

/// `source` drawn, or why it could not be.
pub fn render_diagram(source: &str, colors: &DiagramColors) -> Result<Drawn, String> {
    let mut theme = Theme::modern();
    theme.font_family = "Geist, ui-sans-serif, system-ui, sans-serif".to_string();
    theme.font_size = 13.0;
    theme.primary_color = colors.raised.clone();
    theme.primary_text_color = colors.fg.clone();
    theme.primary_border_color = colors.border_strong.clone();
    theme.line_color = colors.fg_muted.clone();
    theme.secondary_color = colors.surface.clone();
    theme.tertiary_color = colors.surface.clone();
    theme.edge_label_background = colors.bg.clone();
    theme.cluster_background = colors.surface.clone();
    theme.cluster_border = colors.border.clone();
    theme.background = "transparent".to_string();
    theme.sequence_actor_fill = colors.raised.clone();
    theme.sequence_actor_border = colors.border_strong.clone();
    theme.sequence_actor_line = colors.fg_muted.clone();
    theme.sequence_note_fill = colors.surface.clone();
    theme.sequence_note_border = colors.border_strong.clone();
    theme.sequence_activation_fill = colors.surface.clone();
    theme.sequence_activation_border = colors.border_strong.clone();
    theme.text_color = colors.fg.clone();
    theme.pie_title_text_color = colors.fg.clone();
    theme.pie_section_text_color = colors.fg.clone();
    theme.pie_legend_text_color = colors.fg.clone();
    theme.pie_stroke_color = colors.bg.clone();
    theme.pie_outer_stroke_color = colors.border_strong.clone();
    let options = RenderOptions {
        theme,
        ..RenderOptions::default()
    };
    let svg = render_with_options(source, options).map_err(|error| error.to_string())?;
    let (width, height) = svg_size(&svg).ok_or("the diagram has no size")?;
    Ok(Drawn { svg, width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(dark: bool) -> DiagramColors {
        let color = |hex: &str| hex.to_string();
        DiagramColors {
            dark,
            fg: color("#e9eaee"),
            fg_muted: color("#a0a3ae"),
            bg: color("#15161a"),
            surface: color("#1b1c21"),
            raised: color("#23252c"),
            border: color("#2c2e36"),
            border_strong: color("#3b3e48"),
            accent: color("#5b63e6"),
        }
    }

    #[test]
    fn draws_a_flowchart_in_the_palette() {
        let drawn = render_diagram("flowchart LR\n  A[Edit] --> B[Save]", &colors(true));
        let Ok(drawn) = drawn else {
            panic!("{drawn:?}");
        };
        assert!(drawn.width > drawn.height && drawn.height > 0.0);
        let svg = drawn.svg;
        assert!(svg.contains("<svg"), "{svg}");
        assert!(svg.contains("Edit") && svg.contains("Save"));
        assert!(svg.contains("#23252c"));
    }

    #[test]
    fn sizes_from_the_root() {
        assert_eq!(
            svg_size(r#"<svg width="120" height="40px">"#),
            Some((120.0, 40.0))
        );
        assert_eq!(
            svg_size(r#"<svg viewBox="0 0 300 90">"#),
            Some((300.0, 90.0))
        );
        assert_eq!(svg_size("<svg>"), None);
    }

    #[test]
    fn a_broken_diagram_says_why() {
        assert!(render_diagram("not a diagram at all", &colors(true)).is_err());
    }

    #[test]
    fn the_key_follows_the_source_and_the_colors() {
        let key = diagram_key("graph TD; A-->B", &colors(true));
        assert_eq!(key, diagram_key("graph TD; A-->B", &colors(true)));
        assert_ne!(key, diagram_key("graph TD; A-->C", &colors(true)));
        assert_ne!(key, diagram_key("graph TD; A-->B", &colors(false)));
    }
}

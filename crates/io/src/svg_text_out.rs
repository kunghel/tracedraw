//! Text as SVG `<text>` (#2289): the glyphs the text engine placed,
//! written as characters at the same places, so other programs can edit
//! the words while the file looks as the drawing does. With
//! [`SvgText::TextWithFonts`] the font files go into the SVG as
//! `@font-face` rules.
//!
//! Each glyph's place is its transform from the font's em space; glyphs
//! that share a direction and slant go into one `<text>` (one line, or
//! several lines of a paragraph), each run of one span and font on one
//! line a `<tspan>` with an `x` and `y` for every character, in the
//! order the characters are typed, so the spacing,
//! kerning and justification stay exactly as laid out. What has no
//! characters to show (underlines, strike lines, added hyphens, synthetic
//! bold, letters no font has, fonts that may not be embedded) is written
//! as curves beside it.

use super::{escape, fmt, SvgCtx, SvgText};
use std::fmt::Write;
use tracedraw_core::{
    document::{text_glyph_layout, text_outline::GlyphFont, Shape, ShapeKind},
    geometry::{Affine, Point},
    TextSpan,
};

/// A font file put into the SVG.
pub(super) struct EmbeddedFont {
    /// The font data's address and face index: the same font file.
    key: (usize, u32),
    /// The family name the `@font-face` rule gives it.
    family: String,
    bold: bool,
    italic: bool,
    data: std::sync::Arc<Vec<u8>>,
}

/// The `@font-face` rules of the embedded fonts, in a `<style>`.
pub(super) fn font_faces(fonts: &[EmbeddedFont]) -> String {
    use base64::Engine;
    if fonts.is_empty() {
        return String::new();
    }
    let mut out = String::from("    <style>\n");
    for f in fonts {
        let (mime, format) = if f.data.starts_with(b"OTTO") {
            ("font/otf", "opentype")
        } else {
            ("font/ttf", "truetype")
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(f.data.as_slice());
        let _ = writeln!(
            out,
            "      @font-face {{ font-family: \"{}\"; font-weight: {}; font-style: {}; src: url(data:{mime};base64,{b64}) format(\"{format}\"); }}",
            css_string(&f.family),
            if f.bold { "bold" } else { "normal" },
            if f.italic { "italic" } else { "normal" },
        );
    }
    out.push_str("    </style>\n");
    out
}

/// The family name a font file gives itself (typographic family, else
/// family), if it has one.
fn family_of(data: &[u8], index: u32) -> Option<String> {
    let face = ttf_parser::Face::parse(data, index).ok()?;
    let names = face.names();
    let by_id = |id: u16| {
        names
            .into_iter()
            .filter(|n| n.name_id == id && n.is_unicode())
            .find_map(|n| n.to_string())
            .filter(|s| !s.trim().is_empty())
    };
    by_id(ttf_parser::name_id::TYPOGRAPHIC_FAMILY).or_else(|| by_id(ttf_parser::name_id::FAMILY))
}

/// `s` safe inside a double-quoted CSS string that sits in an XML
/// attribute or element.
fn css_string(s: &str) -> String {
    escape(&s.replace('\\', "").replace('"', "'"))
}

/// The family to name for a glyph of `span` drawn from `font`: with
/// embedded fonts, the family of the `@font-face` rule holding the file
/// (registered on first use); otherwise the span's family. `None` when
/// the font may not be embedded, so the glyph goes in as a curve.
fn family_for(span: &TextSpan, font: &GlyphFont, ctx: &mut SvgCtx) -> Option<String> {
    if ctx.text != SvgText::TextWithFonts {
        return Some(span.font_family.clone());
    }
    let key = (std::sync::Arc::as_ptr(&font.data) as usize, font.index);
    if let Some(f) = ctx.fonts.iter().find(|f| f.key == key) {
        return Some(f.family.clone());
    }
    // A face of a collection cannot be named in `@font-face`, and a
    // font whose licence forbids embedding stays out.
    if font.index != 0 || font.data.starts_with(b"ttcf") {
        return None;
    }
    if crate::pdf::font_licence(&font.data, font.index) == crate::pdf::FontLicence::Restricted {
        return None;
    }
    let base = family_of(&font.data, font.index).unwrap_or_else(|| span.font_family.clone());
    // Another file already under this family and style (a fallback font
    // with the same name): a name of its own.
    let mut family = base.clone();
    let mut n = 2;
    while ctx
        .fonts
        .iter()
        .any(|f| f.family == family && f.bold == span.bold && f.italic == span.italic)
    {
        family = format!("{base} {n}");
        n += 1;
    }
    ctx.fonts.push(EmbeddedFont {
        key,
        family: family.clone(),
        bold: span.bold,
        italic: span.italic,
        data: font.data.clone(),
    });
    Some(family)
}

/// One character group placed in the output: its text, the span and
/// family it is written in, and its glyph space in the output (y down,
/// one unit an em) split into a direction (`dir`, the linear part over
/// `size`), a size and an origin.
struct Placed {
    text: String,
    /// The number of its first character in the text (typed order).
    char_idx: usize,
    span: usize,
    family: String,
    dir: [f64; 4],
    size: f64,
    at: Point,
}

fn same_dir(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

/// Write the text object `shape` (under `transform`, its object space to
/// the output) as `<text>`, painted with `paint` (fill, outline and spot
/// attributes). False when there is no text engine or nothing came out
/// as characters, so the caller writes curves instead.
pub(super) fn write_text(
    shape: &Shape,
    transform: Affine,
    paint: &str,
    out: &mut String,
    ctx: &mut SvgCtx,
    indent: usize,
) -> bool {
    let ShapeKind::Text { spans, .. } = &shape.kind else {
        return false;
    };
    let Some(layout) = text_glyph_layout(&shape.kind) else {
        return false;
    };
    // Glyph space (y up) to SVG text space (y down).
    let flip = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]);
    let mut placed: Vec<Placed> = Vec::new();
    let mut curves = layout.rest.clone();
    // Whether the last glyph with characters went in as text: the further
    // glyphs of its character follow it (the font draws them from the
    // characters), or go in as curves with it.
    let mut as_text = false;
    for g in &layout.glyphs {
        // Characters only laid out here, drawn by another part.
        if spans.get(g.span).is_some_and(|s| s.layout_only) {
            continue;
        }
        if g.text.is_empty() {
            if !as_text {
                curves.extend(g.path.iter());
            }
            continue;
        }
        let family = spans
            .get(g.span)
            .filter(|_| g.exact)
            .and_then(|sp| family_for(sp, &g.font, ctx));
        let m = (transform * g.transform * flip).as_coeffs();
        let det = m[0] * m[3] - m[1] * m[2];
        let size = det.abs().sqrt();
        let (Some(family), true) = (family, size.is_finite() && size > 1e-9) else {
            as_text = false;
            curves.extend(g.path.iter());
            continue;
        };
        as_text = true;
        // Characters a text element would drop or that XML forbids.
        let text: String = g
            .text
            .chars()
            .map(|c| if (c as u32) < 0x20 { ' ' } else { c })
            .collect();
        placed.push(Placed {
            text,
            char_idx: g.char_idx,
            span: g.span,
            family,
            dir: [m[0] / size, m[1] / size, m[2] / size, m[3] / size],
            size,
            at: Point::new(m[4], m[5]),
        });
    }
    if placed.is_empty() {
        return false;
    }
    let pad = "  ".repeat(indent);
    let opacity = if shape.opacity < 1.0 {
        format!(" opacity=\"{}\"", fmt(shape.opacity.max(0.0)))
    } else {
        String::new()
    };
    let name = shape
        .name
        .as_deref()
        .map(|n| format!(" data-name=\"{}\"", escape(n)))
        .unwrap_or_default();
    let _ = writeln!(out, "{pad}<g id=\"{}\"{name}{opacity}>", shape.id.raw());
    // Consecutive characters of one direction share a `<text>`.
    let mut start = 0;
    while start < placed.len() {
        let dir = placed[start].dir;
        let end = placed[start..]
            .iter()
            .position(|p| !same_dir(&p.dir, &dir))
            .map_or(placed.len(), |k| start + k);
        write_text_element(&placed[start..end], spans, paint, out, &pad);
        start = end;
    }
    if !curves.elements().is_empty() {
        let path = transform * curves;
        let _ = writeln!(
            out,
            "{pad}  <path d=\"{}\" {paint}/>",
            super::path_data(&path)
        );
    }
    let _ = writeln!(out, "{pad}</g>");
    true
}

/// One `<text>` for characters sharing a direction: its transform puts
/// the first character's origin at 0, 0; every run of one span, family
/// and size is a `<tspan>` placing each character.
fn write_text_element(
    group: &[Placed],
    spans: &[TextSpan],
    paint: &str,
    out: &mut String,
    pad: &str,
) {
    // The characters in the order they are typed, each at its place: the
    // glyphs come in the order they are drawn, so Hebrew and Arabic went
    // in backwards ("שלום" as "םולש") for anything that reads the words.
    let mut group: Vec<&Placed> = group.iter().collect();
    group.sort_by_key(|p| p.char_idx);
    let Some(first) = group.first().copied() else {
        return;
    };
    let d = first.dir;
    let det = d[0] * d[3] - d[1] * d[2];
    if det.abs() < 1e-12 {
        return;
    }
    // Output offsets back into the text's own space.
    let local = |p: Point| {
        let (x, y) = (p.x - first.at.x, p.y - first.at.y);
        ((d[3] * x - d[2] * y) / det, (-d[1] * x + d[0] * y) / det)
    };
    // No white space between the tags: with `xml:space="preserve"` it
    // is text, spaces before the first character.
    let _ = write!(
        out,
        "{pad}  <text transform=\"matrix({} {} {} {} {} {})\" xml:space=\"preserve\" {paint}>",
        fmt(d[0]),
        fmt(d[1]),
        fmt(d[2]),
        fmt(d[3]),
        fmt(first.at.x),
        fmt(first.at.y)
    );
    // A character on another line than the one before it (half its size
    // or more above or below) starts a `<tspan>` of its own: a reader
    // that places only the first character of each sees the lines.
    let new_line = |a: &Placed, b: &Placed| (local(b.at).1 - local(a.at).1).abs() >= b.size / 2.0;
    let mut i = 0;
    while i < group.len() {
        let p = group[i];
        let end = (i + 1..group.len())
            .find(|&k| {
                let q = group[k];
                q.span != p.span
                    || q.family != p.family
                    || (q.size - p.size).abs() > 1e-9
                    || new_line(group[k - 1], q)
            })
            .unwrap_or(group.len());
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        let mut text = String::new();
        for q in &group[i..end] {
            // Every character gets a place; the further characters of a
            // ligature sit where it starts.
            let (x, y) = local(q.at);
            for c in q.text.chars() {
                xs.push(fmt(x));
                ys.push(fmt(y));
                text.push(c);
            }
        }
        let style = spans.get(p.span);
        let weight = if style.is_some_and(|s| s.bold) {
            " font-weight=\"bold\""
        } else {
            ""
        };
        let italic = if style.is_some_and(|s| s.italic) {
            " font-style=\"italic\""
        } else {
            ""
        };
        let _ = write!(
            out,
            "<tspan x=\"{}\" y=\"{}\" font-family=\"&quot;{}&quot;\" font-size=\"{}\"{weight}{italic}>{}</tspan>",
            xs.join(" "),
            ys.join(" "),
            css_string(&p.family),
            fmt(p.size),
            escape(&text)
        );
        i = end;
    }
    let _ = writeln!(out, "</text>");
}

#[cfg(test)]
mod tests {
    use super::super::{page_to_svg, page_to_svg_with, SvgOptions, SvgText};
    use crate::pdf::text_tests::doc_with;
    use tracedraw_core::{Color, Fill, TextSpan};

    fn with(text: SvgText) -> SvgOptions {
        SvgOptions { text }
    }

    /// By default text goes in as curves; with Text it is a `<text>`
    /// whose characters carry the places the engine gave them, in the
    /// span's family and size.
    #[test]
    fn text_goes_in_as_text_when_asked() {
        let doc = doc_with(
            vec![TextSpan::new("Hi & <bye>", "Quicksand", 24.0)],
            Fill::Solid(Color::BLACK),
            None,
        );
        let curves = page_to_svg(&doc, 0);
        assert!(!curves.contains("<text"), "{curves}");
        let svg = page_to_svg_with(&doc, 0, &with(SvgText::Text));
        assert!(
            svg.contains("<text transform=\"matrix(1 0 0 1 10 30)\""),
            "{svg}"
        );
        assert!(svg.contains(">Hi &amp; &lt;bye&gt;</tspan>"), "{svg}");
        assert!(
            svg.contains("font-family=\"&quot;Quicksand&quot;\""),
            "{svg}"
        );
        // 24 pt is 8.4667 mm.
        assert!(svg.contains("font-size=\"8.4667\""), "{svg}");
        assert!(svg.contains("fill=\"#000000\""), "{svg}");
        assert!(!svg.contains("@font-face"), "{svg}");
        // One x and one y for each of the ten characters, from 0.
        let xs = svg
            .split("<tspan x=\"")
            .nth(1)
            .and_then(|t| t.split('"').next())
            .expect("x list");
        let xs: Vec<f64> = xs.split(' ').map(|v| v.parse().expect("number")).collect();
        assert_eq!(xs.len(), 10);
        assert_eq!(xs[0], 0.0);
        assert!(xs.windows(2).all(|w| w[1] >= w[0]), "{xs:?}");
        // The result reads back as text.
        let mut ids = tracedraw_core::id::IdSource::default();
        let back = crate::svg_import::parse(&svg, &mut ids).expect("reads back");
        let words: String = back.shapes.iter().flat_map(all_spans).collect();
        assert!(words.contains("Hi"), "{words}");
    }

    fn all_spans(s: &tracedraw_core::Shape) -> Vec<String> {
        match &s.kind {
            tracedraw_core::ShapeKind::Text { spans, .. } => {
                spans.iter().map(|sp| sp.text.clone()).collect()
            }
            tracedraw_core::ShapeKind::Group { children } => {
                children.iter().flat_map(all_spans).collect()
            }
            _ => Vec::new(),
        }
    }

    /// With fonts, the font file goes in as an `@font-face` under its own
    /// family name and the text names it; a font whose licence forbids
    /// embedding goes in as curves.
    #[test]
    fn fonts_go_in_when_asked_unless_their_licence_forbids() {
        let doc = doc_with(
            vec![TextSpan::new("Ab", "Quicksand", 24.0)],
            Fill::Solid(Color::BLACK),
            None,
        );
        let svg = page_to_svg_with(&doc, 0, &with(SvgText::TextWithFonts));
        assert_eq!(svg.matches("@font-face").count(), 1, "{svg}");
        assert!(svg.contains("font-family: \"Quicksand\""), "{svg}");
        assert!(svg.contains("src: url(data:font/ttf;base64,"), "{svg}");
        assert!(svg.contains(">Ab</tspan>"), "{svg}");

        let locked = doc_with(
            vec![TextSpan::new("Ab", "Locked Sans", 24.0)],
            Fill::Solid(Color::BLACK),
            None,
        );
        let svg = page_to_svg_with(&locked, 0, &with(SvgText::TextWithFonts));
        assert!(!svg.contains("@font-face"), "{svg}");
        assert!(!svg.contains("<text"), "{svg}");
        assert!(svg.contains("<path"), "{svg}");
        // Named only, it stays text.
        let svg = page_to_svg_with(&locked, 0, &with(SvgText::Text));
        assert!(svg.contains(">Ab</tspan>"), "{svg}");
    }

    /// Synthetic bold and underlines have no characters to show: curves
    /// beside the text; a gradient fill keeps the whole text as curves.
    #[test]
    fn marks_without_characters_and_gradients_are_curves() {
        let mut under = TextSpan::new("ab", "Quicksand", 24.0);
        under.underline = tracedraw_core::document::LineStyle::SingleThin;
        let doc = doc_with(vec![under], Fill::Solid(Color::BLACK), None);
        let svg = page_to_svg_with(&doc, 0, &with(SvgText::Text));
        assert!(svg.contains(">ab</tspan>"), "{svg}");
        assert!(svg.contains("<path d="), "{svg}");

        let mut bold = TextSpan::new("cd", "Quicksand", 24.0);
        bold.bold = true;
        let doc = doc_with(vec![bold], Fill::Solid(Color::BLACK), None);
        let svg = page_to_svg_with(&doc, 0, &with(SvgText::Text));
        assert!(!svg.contains("<text"), "{svg}");

        let grad = Fill::linear(Color::BLACK, Color::WHITE, 0.0);
        let doc = doc_with(vec![TextSpan::new("ef", "Quicksand", 24.0)], grad, None);
        let svg = page_to_svg_with(&doc, 0, &with(SvgText::Text));
        assert!(!svg.contains("<text"), "{svg}");
    }
}

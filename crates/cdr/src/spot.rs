//! Colours from spot colour libraries (colour models 0x01 and 0x0e): the
//! file holds only the colour's number in its library and a tint, not
//! what it looks like. They are read as spot inks of their own; when the
//! file carries a picture of its first page (versions 14 and later keep
//! one in their ZIP container), each ink takes the colour that picture
//! shows where objects painted with it lie uncovered.

use tracedraw_core::document::{Ink, Shape, ShapeKind};
use tracedraw_core::geometry::{Point, Shape as _};
use tracedraw_core::{Color, Document, Fill};

/// The library of the numbered palette colours of versions 3 and 4.
pub(crate) const V3_PALETTE: u16 = 0x0300;

/// Ink numbers of library colours: the library (palette) in the upper
/// bits, the colour's number in the lower 16.
const LIBRARY_INK: u32 = 0x4000_0000;

/// The ink of library `palette`'s colour `number`.
pub(crate) fn ink_id(palette: u16, number: u16) -> u32 {
    LIBRARY_INK | (u32::from(palette & 0x3FFF) << 16) | u32::from(number)
}

/// The colour `number` of library `palette` at `tint` (0 to 100): a spot
/// ink shown as black at the tint until its colour is known.
pub(crate) fn library_color(palette: u16, number: u16, tint: u16) -> Color {
    Color::Spot {
        ink: ink_id(palette, number),
        tint: f32::from(tint.min(100)) / 100.0,
        c: 0.0,
        m: 0.0,
        y: 0.0,
        k: 1.0,
    }
}

/// The ink entry of a library colour, for `Document::inks`.
pub(crate) fn ink_entry(palette: u16, number: u16) -> Ink {
    Ink {
        id: ink_id(palette, number),
        name: format!("Spot {number}"),
        cmyk: [0.0, 0.0, 0.0, 1.0],
    }
}

/// A picture of the first page: width, height and RGBA pixels, top row
/// first.
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// The picture shows the drawing's extent stretched to its size (the
    /// thumbnails of version 3 files, 16 dithered colours) rather than the
    /// page; colours are then averaged, not picked by median.
    pub covers_drawing: bool,
    /// The picture shows the drawing's extent fitted into it whole,
    /// centred, its proportions kept (the `DISP` picture of RIFF files).
    pub fitted: bool,
}

impl Preview {
    /// A PNG or `.bmp` picture.
    pub fn decode(bytes: &[u8]) -> Option<Preview> {
        let png = if bytes.starts_with(b"\x89PNG") {
            bytes.to_vec()
        } else {
            crate::ole::decode_bmp_file(bytes)?.2
        };
        let pm = tiny_skia::Pixmap::decode_png(&png).ok()?;
        let mut rgba = Vec::with_capacity(pm.data().len());
        for p in pm.pixels() {
            let c = p.demultiply();
            rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        Some(Preview {
            width: pm.width(),
            height: pm.height(),
            rgba,
            covers_drawing: false,
            fitted: false,
        })
    }

    /// The `DISP` chunk of a RIFF file (versions 4 to 13): a u32, then a
    /// device-independent bitmap (a 40-byte header, the palette, the
    /// pixels) of the drawing fitted into a square.
    pub fn from_disp(data: &[u8]) -> Option<Preview> {
        let dib = data.get(4..)?;
        let u32_at = |i: usize| -> Option<u32> {
            Some(u32::from_le_bytes(dib.get(i..i + 4)?.try_into().ok()?))
        };
        let header = u32_at(0)? as usize;
        if !(40..=124).contains(&header) {
            return None;
        }
        let bpp = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
        let used = u32_at(32)? as usize;
        let palette = match (bpp, used) {
            (1..=8, 0) => 1usize << bpp,
            (1..=8, n) => n.min(256),
            _ => used.min(256),
        };
        let pixels_at = 14 + header + 4 * palette;
        let mut file = Vec::with_capacity(14 + dib.len());
        file.extend_from_slice(b"BM");
        file.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(&(pixels_at as u32).to_le_bytes());
        file.extend_from_slice(dib);
        let mut p = Preview::decode(&file)?;
        p.covers_drawing = true;
        p.fitted = true;
        Some(p)
    }

    /// The colour (0 to 1) at page point `p` of the area `frame` (in page
    /// mm, origin bottom left) the picture covers.
    fn at(&self, p: Point, frame: tracedraw_core::geometry::Rect) -> Option<[f64; 3]> {
        let (w, h) = (frame.width(), frame.height());
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let x = ((p.x - frame.x0) / w * self.width as f64).floor();
        let y = ((1.0 - (p.y - frame.y0) / h) * self.height as f64).floor();
        if x < 0.0 || y < 0.0 || x >= self.width as f64 || y >= self.height as f64 {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let px = self.rgba.get(i..i + 4)?;
        if px[3] < 250 {
            return None;
        }
        Some([px[0], px[1], px[2]].map(|v| v as f64 / 255.0))
    }
}

/// The area a picture of `width` x `height` pixels covers when it shows
/// the drawing's extent `b` fitted into it whole and centred: `b` grown
/// on its shorter side to the picture's proportions.
fn fit_frame(
    b: tracedraw_core::geometry::Rect,
    width: u32,
    height: u32,
) -> tracedraw_core::geometry::Rect {
    let (pw, ph) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let scale = (b.width() / pw).max(b.height() / ph);
    if !(scale > 0.0 && scale.is_finite()) {
        return b;
    }
    let (w, h) = (pw * scale, ph * scale);
    let c = b.center();
    tracedraw_core::geometry::Rect::new(c.x - w / 2.0, c.y - h / 2.0, c.x + w / 2.0, c.y + h / 2.0)
}

/// Every top-level and nested shape of the first page, bottom to top,
/// with its page transform folded in.
fn shapes_bottom_up(doc: &Document) -> Vec<Shape> {
    fn walk(s: &Shape, above: tracedraw_core::geometry::Affine, out: &mut Vec<Shape>) {
        let t = above * s.transform;
        match &s.kind {
            ShapeKind::Group { children } => {
                for c in children {
                    walk(c, t, out);
                }
            }
            ShapeKind::ClipFrame { .. } => {
                let mut c = s.clone();
                c.transform = t;
                out.push(c);
            }
            _ => {
                let mut c = s.clone();
                c.transform = t;
                out.push(c);
            }
        }
    }
    let mut out = Vec::new();
    if let Some(page) = doc.pages.first() {
        for l in page.layers.iter().filter(|l| l.visible) {
            for s in &l.shapes {
                walk(s, tracedraw_core::geometry::Affine::IDENTITY, &mut out);
            }
        }
    }
    out
}

/// Whether `s` paints its inside: it has a fill, is not text (its outline
/// here is its whole frame, its letters cover little) and is not an open
/// curve (left unfilled unless the drawing fills them).
fn paints_inside(s: &Shape, fill_open: bool) -> bool {
    !matches!(s.fill, Fill::None)
        && !matches!(s.kind, ShapeKind::Text { .. })
        && (fill_open || !s.kind.is_open_curve())
}

/// The library ink and tint a fill paints with, when it is one.
fn fill_ink(f: &Fill) -> Option<(u32, f32)> {
    match f {
        Fill::Solid(c) => stop_ink(c),
        _ => None,
    }
}

/// The library ink and tint of a colour, when it is one.
fn stop_ink(c: &Color) -> Option<(u32, f32)> {
    match c {
        Color::Spot { ink, tint, .. } if ink & LIBRARY_INK != 0 => Some((*ink, *tint)),
        _ => None,
    }
}

/// How far along a fountain (0 to 1) a point may lie from a stop and
/// still be taken as showing that stop's colour.
const STOP_REACH: f64 = 0.04;

/// Give each library ink of `doc` the colour `preview` shows where the
/// objects filled with it lie uncovered (the median of up to a few
/// hundred spots, tints taken back to full strength). Returns how many
/// inks got a colour.
pub fn colours_from_preview(doc: &mut Document, preview: &Preview) -> usize {
    let Some(page) = doc.pages.first() else {
        return 0;
    };
    let (w, h) = (page.size.width, page.size.height);
    let fill_open = doc.metadata.fill_open_curves;
    let shapes = shapes_bottom_up(doc);
    let paths: Vec<_> = shapes.iter().map(|s| s.page_path()).collect();
    let frame = if preview.covers_drawing {
        match paths
            .iter()
            .map(|p| p.bounding_box())
            .filter(|b| b.width().is_finite() && b.height().is_finite())
            .reduce(|a, b| a.union(b))
        {
            Some(b) if preview.fitted => fit_frame(b, preview.width, preview.height),
            Some(b) => b,
            None => return 0,
        }
    } else {
        tracedraw_core::geometry::Rect::new(0.0, 0.0, w, h)
    };
    let mut samples: std::collections::BTreeMap<u32, Vec<[f64; 3]>> = Default::default();
    for (i, s) in shapes.iter().enumerate() {
        let solid = fill_ink(&s.fill);
        let fountain = match &s.fill {
            Fill::Fountain(f)
                if f.kind != tracedraw_core::FountainKind::Shaped
                    && f.stops.iter().any(|st| stop_ink(&st.color).is_some()) =>
            {
                Some(f)
            }
            _ => None,
        };
        if solid.is_none() && fountain.is_none() {
            continue;
        }
        let Some(path) = paths.get(i) else {
            continue;
        };
        let b = path.bounding_box();
        if !(b.width() > 0.0 && b.height() > 0.0) {
            continue;
        }
        // A fountain's place on the page as the renderer works it out.
        let placed = fountain.map(|f| {
            let mut f = f.clone();
            if f.handles.is_none() {
                f.handles = Some(f.default_handles(b));
            }
            f
        });
        let n = if placed.is_some() { 48 } else { 24 };
        for gy in 0..n {
            for gx in 0..n {
                let p = Point::new(
                    b.x0 + b.width() * (gx as f64 + 0.5) / n as f64,
                    b.y0 + b.height() * (gy as f64 + 0.5) / n as f64,
                );
                if !path.contains(p) {
                    continue;
                }
                // Covered by an object above it: that one shows there.
                let covered = shapes.iter().zip(&paths).skip(i + 1).any(|(o, op)| {
                    paints_inside(o, fill_open) && op.bounding_box().contains(p) && op.contains(p)
                });
                if covered {
                    continue;
                }
                // The ink that paints here: the solid fill's, or the
                // fountain stop's the point lies close to (between stops
                // the colours mix, and tell nothing of either ink).
                let (ink, tint) = match (&solid, &placed) {
                    (Some(it), _) => *it,
                    (None, Some(f)) => {
                        let Some(t) = f.param_at(b, p) else {
                            continue;
                        };
                        let u = f.stop_param(t);
                        let near = f
                            .stops
                            .iter()
                            .min_by(|x, y| (x.pos - u).abs().total_cmp(&(y.pos - u).abs()));
                        match near {
                            Some(st) if (st.pos - u).abs() <= STOP_REACH => {
                                match stop_ink(&st.color) {
                                    Some(it) => it,
                                    None => continue,
                                }
                            }
                            _ => continue,
                        }
                    }
                    _ => continue,
                };
                let Some(rgb) = preview.at(p, frame) else {
                    continue;
                };
                // A tint lightens the ink towards white.
                let t = f64::from(tint).clamp(0.05, 1.0);
                let full = rgb.map(|v| (1.0 - (1.0 - v) / t).clamp(0.0, 1.0));
                samples.entry(ink).or_default().push(full);
            }
        }
    }
    let mut found = std::collections::BTreeMap::new();
    for (ink, mut list) in samples {
        if list.len() < 3 {
            continue;
        }
        let mut median = [0.0; 3];
        for (ch, m) in median.iter_mut().enumerate() {
            if preview.covers_drawing {
                // A dithered thumbnail mixes a few colours: their mean.
                *m = list.iter().map(|c| c[ch]).sum::<f64>() / list.len() as f64;
            } else {
                list.sort_by(|a, b| a[ch].total_cmp(&b[ch]));
                *m = list[list.len() / 2][ch];
            }
        }
        let rgb = Color::Rgb {
            r: median[0] as f32,
            g: median[1] as f32,
            b: median[2] as f32,
        };
        let cmyk = match rgb.convert_to("CMYK") {
            Color::Cmyk { c, m, y, k } => [c, m, y, k],
            _ => continue,
        };
        found.insert(ink, cmyk);
    }
    if found.is_empty() {
        return 0;
    }
    for ink in doc.inks.iter_mut() {
        if let Some(c) = found.get(&ink.id) {
            ink.cmyk = *c;
        }
    }
    let recolor = |c: &mut Color| {
        if let Color::Spot {
            ink,
            c: cc,
            m,
            y,
            k,
            ..
        } = c
        {
            if let Some([nc, nm, ny, nk]) = found.get(ink) {
                (*cc, *m, *y, *k) = (*nc, *nm, *ny, *nk);
            }
        }
    };
    for page in &mut doc.pages {
        for layer in &mut page.layers {
            for s in &mut layer.shapes {
                recolor_shape(s, &recolor);
            }
        }
    }
    found.len()
}

fn recolor_shape(s: &mut Shape, f: &impl Fn(&mut Color)) {
    match &mut s.fill {
        Fill::Solid(c) => f(c),
        Fill::Fountain(fo) => fo.stops.iter_mut().for_each(|st| f(&mut st.color)),
        _ => {}
    }
    if let Some(st) = &mut s.stroke {
        f(&mut st.color);
    }
    match &mut s.kind {
        ShapeKind::Group { children } => children.iter_mut().for_each(|c| recolor_shape(c, f)),
        ShapeKind::ClipFrame { frame, contents } => {
            recolor_shape(frame, f);
            contents.iter_mut().for_each(|c| recolor_shape(c, f));
        }
        ShapeKind::Text { spans, .. } => {
            for sp in spans.iter_mut() {
                if let Some(Fill::Solid(c)) = sp.fill.as_mut() {
                    f(c);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    #[test]
    fn library_inks_take_the_colour_their_objects_show_in_the_preview() {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let ink = ink_id(0x22, 0x07db);
        doc.inks.push(ink_entry(0x22, 0x07db));
        let mut s = Shape::new(
            doc.ids_mut().shape(),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 50.0, 100.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(library_color(0x22, 0x07db, 100));
        if let Some(l) = doc.pages[0].layers.first_mut() {
            l.shapes.push(s);
        }
        // A 10 x 10 preview: the left half pale green, the right white.
        let mut rgba = Vec::new();
        for _y in 0..10 {
            for x in 0..10 {
                let c = if x < 5 {
                    [200, 230, 200, 255]
                } else {
                    [255, 255, 255, 255]
                };
                rgba.extend_from_slice(&c);
            }
        }
        let preview = Preview {
            width: 10,
            height: 10,
            rgba,
            covers_drawing: false,
            fitted: false,
        };
        assert_eq!(colours_from_preview(&mut doc, &preview), 1);
        let fill = &doc.pages[0].layers[0].shapes[0].fill;
        let Fill::Solid(c) = fill else {
            panic!("fill");
        };
        let [r, g, b] = c.to_rgb8();
        assert!(g > r && g > b && r > 150, "{r} {g} {b}");
        assert_eq!(doc.inks[0].id, ink);
        assert!(doc.inks[0].cmyk[3] < 0.5);
    }

    /// A page of 100 x 100 with one rectangle over `rect`, filled `fill`.
    fn one_shape(rect: Rect, fill: Fill) -> Document {
        let mut doc = Document::new("t", tracedraw_core::geometry::Size::new(100.0, 100.0));
        doc.inks.push(ink_entry(0x22, 1));
        doc.inks.push(ink_entry(0x22, 2));
        let mut s = Shape::new(
            doc.ids_mut().shape(),
            ShapeKind::Rect {
                rect,
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = fill;
        if let Some(l) = doc.pages[0].layers.first_mut() {
            l.shapes.push(s);
        }
        doc
    }

    #[test]
    fn a_fountain_of_library_inks_takes_each_stop_s_colour() {
        // Left to right from ink 1 to ink 2 over the whole page; the
        // preview runs from red to blue.
        let fill = Fill::Fountain(tracedraw_core::Fountain::two(
            tracedraw_core::FountainKind::Linear,
            library_color(0x22, 1, 100),
            library_color(0x22, 2, 100),
            0.0,
        ));
        let mut doc = one_shape(Rect::new(0.0, 0.0, 100.0, 100.0), fill);
        let mut rgba = Vec::new();
        for _y in 0..50 {
            for x in 0..50u32 {
                let t = x as f64 / 49.0;
                let r = (255.0 * (1.0 - t)) as u8;
                let b = (255.0 * t) as u8;
                rgba.extend_from_slice(&[r, 0, b, 255]);
            }
        }
        let preview = Preview {
            width: 50,
            height: 50,
            rgba,
            covers_drawing: false,
            fitted: false,
        };
        assert_eq!(colours_from_preview(&mut doc, &preview), 2);
        let Fill::Fountain(f) = &doc.pages[0].layers[0].shapes[0].fill else {
            panic!("fill");
        };
        let [r0, _, b0] = f.stops[0].color.to_rgb8();
        let [r1, _, b1] = f.stops[1].color.to_rgb8();
        assert!(r0 > b0 + 100, "first stop {r0} {b0}");
        assert!(b1 > r1 + 100, "last stop {r1} {b1}");
    }

    #[test]
    fn a_disp_picture_frames_the_drawing_fitted_in_a_square() {
        // A wide drawing (80 x 20 mm, ink 1) fitted into an 8 x 8 picture
        // on white: rows 3 and 4 of the picture are green, the rest white.
        let fill = Fill::Solid(library_color(0x22, 1, 100));
        let mut doc = one_shape(Rect::new(10.0, 40.0, 90.0, 60.0), fill);
        let mut dib = Vec::new();
        dib.extend_from_slice(&8u32.to_le_bytes());
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&8i32.to_le_bytes());
        dib.extend_from_slice(&8i32.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&8u16.to_le_bytes());
        dib.extend_from_slice(&[0; 4]);
        dib.extend_from_slice(&64u32.to_le_bytes());
        dib.extend_from_slice(&[0; 8]);
        dib.extend_from_slice(&2u32.to_le_bytes());
        dib.extend_from_slice(&[0; 4]);
        // Palette (B, G, R, 0): white, green.
        dib.extend_from_slice(&[255, 255, 255, 0, 40, 200, 40, 0]);
        // Bottom-up rows.
        for y in 0..8 {
            let v = u8::from(y == 3 || y == 4);
            dib.extend_from_slice(&[v; 8]);
        }
        let preview = Preview::from_disp(&dib).expect("picture");
        assert!(preview.fitted && preview.covers_drawing);
        assert_eq!(colours_from_preview(&mut doc, &preview), 1);
        let Fill::Solid(c) = &doc.pages[0].layers[0].shapes[0].fill else {
            panic!("fill");
        };
        let [r, g, b] = c.to_rgb8();
        assert!(g > 150 && r < 90 && b < 90, "{r} {g} {b}");
    }

    #[test]
    fn bad_previews_give_nothing() {
        assert!(Preview::from_disp(b"").is_none());
        assert!(Preview::from_disp(&[8, 0, 0, 0, 3, 0, 0, 0]).is_none());
        assert!(Preview::decode(b"").is_none());
        assert!(Preview::decode(b"\x89PNG broken").is_none());
        assert!(Preview::decode(b"BM").is_none());
    }
}

//! Small print redrawn with the letters of a font: after a trace, rows of
//! letters a few pixels tall (a slogan under a logo, a date) are read by
//! comparing the picture with the letters of many fonts, drawn the way
//! the picture shows them (as large, as blurred, at the same place), and
//! when one font draws a whole row over the picture almost exactly, the
//! traced curves of that row are swapped for the font's own outlines, in
//! the colour the trace gave the letters.
//!
//! No text recognition model and no network: the letters are found by
//! drawing every candidate and keeping the one that differs least from
//! the picture. A row that no font matches well is left as traced. The
//! fonts are a set of free fonts that come with TraceDraw (see
//! `assets/fonts/trace/`) and the fonts installed on the computer. See
//! docs/behavior/trace-text.md.

use crate::trace::{Cancel, Settings, Traced};
use image::RgbaImage;
use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape};
use std::sync::{Mutex, OnceLock};
use tracedraw_core::Color;

/// The letters looked for: capitals and figures, the small print of
/// logos. A row in lower case does not match and stays traced.
const CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Rows whose letters are this tall (pixels of the art, at the 0.5
/// coverage level) are read; taller letters are traced well already.
const MIN_CAP: f64 = 4.5;
const MAX_CAP: f64 = 48.0;

/// Supersampling of the drawn letters for each pixel of the art: 8 for
/// letters up to 8 pixels tall, less for taller ones (at least 2), so a
/// letter takes about as many samples whatever its size.
fn supersampling(cap: f64) -> usize {
    ((64.0 / cap.max(1.0)).round() as usize).clamp(2, 8)
}

/// Fonts compared closely after the first pass.
const TOP_FACES: usize = 8;

/// How far the letter height is searched around the measured one, and
/// the blurs tried (pixels).
const SCALES: [f64; 9] = [0.88, 0.91, 0.94, 0.97, 1.0, 1.03, 1.06, 1.09, 1.12];
const BLURS: [f64; 5] = [0.35, 0.55, 0.75, 0.95, 1.2];

/// A row is redrawn only when the font draws it this close to the
/// picture: the squared coverage left over is at most this share of the
/// letters' own, for the row and for each letter.
const MAX_ROW_ERROR: f64 = 0.06;
const MAX_LETTER_ERROR: f64 = 0.16;

/// The colours of the background round a row may differ from its median
/// by this much (RGB distance) at the 90th percentile; a row over a
/// picture or a pattern is left as traced.
const MAX_BACKGROUND_SPREAD: f64 = 28.0;

/// Letters whose traced colours are this close (RGB distance) may make
/// a row.
const SAME_COLOUR: i32 = 72;

/// The redrawn letters take the trace's colour nearest theirs when it
/// is this close (RGB distance).
const NEAR_COLOUR: i32 = 40;

/// The bundled faces: (name, weight, file).
const BUNDLED: &[(&str, u16, &[u8])] = &crate::trace_fonts::FACES;

/// One letter of a face, in font units (y up).
struct Glyph {
    ch: char,
    path: BezPath,
    bbox: Rect,
    /// Centre of its area.
    centre: Point,
}

/// A face's letters.
struct Face {
    name: String,
    cap: f64,
    glyphs: Vec<Glyph>,
}

struct Pen(BezPath);

impl ttf_parser::OutlineBuilder for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to((x1 as f64, y1 as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.curve_to(
            (x1 as f64, y1 as f64),
            (x2 as f64, y2 as f64),
            (x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        // Only an outline begun is closed: a font that closes a contour it
        // never began (or closes one twice) left a path starting with a
        // close, and measuring it stopped TraceDraw while tracing (#4119).
        if self.0.elements().last().is_some_and(|e| !matches!(e, PathEl::ClosePath)) {
            self.0.close_path();
        }
    }
}

/// The centre of the area `path` fills (non-zero winding, contours of
/// either direction), from its flattened outline.
fn area_centre(path: &BezPath) -> Option<Point> {
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    let mut first: Option<Point> = None;
    let mut last = Point::ZERO;
    let mut edge = |p: Point, q: Point| {
        let c = p.x * q.y - q.x * p.y;
        a += c;
        cx += (p.x + q.x) * c;
        cy += (p.y + q.y) * c;
    };
    kurbo::flatten(path.iter(), 0.5, |el| match el {
        PathEl::MoveTo(p) => {
            first = Some(p);
            last = p;
        }
        PathEl::LineTo(p) => {
            edge(last, p);
            last = p;
        }
        PathEl::ClosePath => {
            if let Some(f) = first {
                edge(last, f);
                last = f;
            }
        }
        _ => {}
    });
    if a.abs() < 1e-6 {
        return None;
    }
    Some(Point::new(cx / (3.0 * a), cy / (3.0 * a)))
}

fn load_face(name: &str, data: &[u8], index: u32) -> Option<Face> {
    let face = ttf_parser::Face::parse(data, index).ok()?;
    let mut glyphs = Vec::with_capacity(CHARS.len());
    for ch in CHARS.chars() {
        let id = face.glyph_index(ch)?;
        let mut pen = Pen(BezPath::new());
        face.outline_glyph(id, &mut pen)?;
        // Fonts may draw a letter with overlapping pieces (filled by the
        // nonzero rule); curves fill even-odd, so the pieces are joined.
        // A piece that crosses itself is welded with itself.
        use tracedraw_core::shaping::{filled_area, overlay_with, FillRule, Op};
        let raw = pen.0;
        let nz = filled_area(&raw, FillRule::NonZero);
        let path = if (filled_area(&raw, FillRule::EvenOdd) - nz).abs() > 1e-6 * nz.abs().max(1.0) {
            overlay_with(&raw, &BezPath::new(), Op::Weld, FillRule::NonZero)
        } else {
            raw
        };
        let bbox = path.bounding_box();
        let centre = area_centre(&path)?;
        if bbox.width() <= 0.0 || bbox.height() <= 0.0 {
            return None;
        }
        glyphs.push(Glyph {
            ch,
            path,
            bbox,
            centre,
        });
    }
    let cap = face
        .capital_height()
        .map(f64::from)
        .filter(|c| *c > 0.0)
        .or_else(|| glyphs.iter().find(|g| g.ch == 'H').map(|g| g.bbox.y1))?;
    Some(Face {
        name: name.to_string(),
        cap,
        glyphs,
    })
}

fn bundled_faces() -> &'static [Face] {
    static FACES: OnceLock<Vec<Face>> = OnceLock::new();
    FACES.get_or_init(|| {
        BUNDLED
            .iter()
            .filter_map(|(name, weight, data)| load_face(&format!("{name} {weight}"), data, 0))
            .collect()
    })
}

/// The installed fonts' faces, read once the font list is loaded.
#[cfg(not(target_arch = "wasm32"))]
fn installed_faces() -> &'static [Face] {
    static FACES: OnceLock<Vec<Face>> = OnceLock::new();
    if FACES.get().is_none() && !tracedraw_text::fonts_loaded() {
        return &[];
    }
    FACES.get_or_init(|| {
        let mut out = Vec::new();
        let bundled: Vec<&str> = BUNDLED.iter().map(|(n, _, _)| *n).collect();
        tracedraw_text::fonts().each_upright_face(|family, weight, data, index| {
            if bundled.contains(&family) {
                return;
            }
            if let Some(f) = load_face(&format!("{family} {weight}"), data, index) {
                out.push(f);
            }
        });
        out
    })
}

#[cfg(target_arch = "wasm32")]
fn installed_faces() -> &'static [Face] {
    &[]
}

// ----- the picture of a row ---------------------------------------------------

/// A row of the picture as letter coverage: 0 background, 1 the
/// letters' colour.
struct Coverage {
    w: usize,
    h: usize,
    a: Vec<f32>,
}

impl Coverage {
    fn at(&self, x: usize, y: usize) -> f32 {
        self.a.get(y * self.w + x).copied().unwrap_or(0.0)
    }
}

/// The background and letter colours of the box `(x0, y0, x1, y1)` of
/// `img` and its coverage, or `None` when the background is not plain.
fn coverage(
    img: &RgbaImage,
    b: (usize, usize, usize, usize),
) -> Option<(Coverage, [f32; 3], Vec<[f32; 3]>)> {
    let (x0, y0, x1, y1) = b;
    let (w, h) = (x1.checked_sub(x0)?, y1.checked_sub(y0)?);
    if w < 3 || h < 3 {
        return None;
    }
    let px = |x: usize, y: usize| -> [f32; 3] {
        img.get_pixel_checked(x as u32, y as u32)
            .map(|p| {
                // Transparent pixels count as white paper.
                let a = p.0[3] as f32 / 255.0;
                [
                    p.0[0] as f32 * a + 255.0 * (1.0 - a),
                    p.0[1] as f32 * a + 255.0 * (1.0 - a),
                    p.0[2] as f32 * a + 255.0 * (1.0 - a),
                ]
            })
            .unwrap_or([255.0; 3])
    };
    let mut border = Vec::new();
    for x in x0..x1 {
        border.push(px(x, y0));
        border.push(px(x, y1 - 1));
    }
    for y in y0..y1 {
        border.push(px(x0, y));
        border.push(px(x1 - 1, y));
    }
    let mut bg = [0f32; 3];
    for (c, v) in bg.iter_mut().enumerate() {
        let mut vals: Vec<f32> = border.iter().map(|p| p[c]).collect();
        vals.sort_by(f32::total_cmp);
        *v = vals.get(vals.len() / 2).copied().unwrap_or(255.0);
    }
    let dist = |p: [f32; 3]| {
        ((p[0] - bg[0]).powi(2) + (p[1] - bg[1]).powi(2) + (p[2] - bg[2]).powi(2)).sqrt()
    };
    let mut spread: Vec<f32> = border.iter().map(|p| dist(*p)).collect();
    spread.sort_by(f32::total_cmp);
    if spread.get(spread.len() * 9 / 10).copied().unwrap_or(0.0) as f64 > MAX_BACKGROUND_SPREAD {
        return None;
    }
    let mut pix = Vec::with_capacity(w * h);
    for y in y0..y1 {
        for x in x0..x1 {
            pix.push(px(x, y));
        }
    }
    let far = pix.iter().map(|p| dist(*p)).fold(0f32, f32::max);
    if far < 40.0 {
        return None;
    }
    // The letters' colour: the mean of the pixels farthest from the
    // background (the cores of the strokes).
    let mut fg = [0f32; 3];
    let mut n = 0f32;
    for p in &pix {
        if dist(*p) > 0.85 * far {
            for c in 0..3 {
                fg[c] += p[c];
            }
            n += 1.0;
        }
    }
    for v in &mut fg {
        *v /= n.max(1.0);
    }
    let v = [fg[0] - bg[0], fg[1] - bg[1], fg[2] - bg[2]];
    let vv = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    let a: Vec<f32> = pix
        .iter()
        .map(|p| {
            (((p[0] - bg[0]) * v[0] + (p[1] - bg[1]) * v[1] + (p[2] - bg[2]) * v[2]) / vv)
                .clamp(0.0, 1.0)
        })
        .collect();
    Some((Coverage { w, h, a }, bg, pix))
}

/// A letter of the row: its inked columns and the columns its error is
/// measured on (a little past its ink, never past half way to the next).
#[derive(Clone, Copy, Debug)]
struct Run {
    a: usize,
    b: usize,
    lo: usize,
    hi: usize,
}

fn runs_of(cov: &Coverage) -> Vec<Run> {
    let col = |x: usize| (0..cov.h).map(|y| cov.at(x, y)).fold(0f32, f32::max);
    let mut spans = Vec::new();
    let mut start = None;
    for x in 0..=cov.w {
        let ink = x < cov.w && col(x) > 0.25;
        match (ink, start) {
            (true, None) => start = Some(x),
            (false, Some(s)) => {
                spans.push((s, x));
                start = None;
            }
            _ => {}
        }
    }
    if spans.is_empty() {
        return Vec::new();
    }
    // Letters that touch: split a span much wider than the rest at its
    // weakest column.
    let mut widths: Vec<usize> = spans.iter().map(|(a, b)| b - a).collect();
    widths.sort_unstable();
    let typical = widths.get(widths.len() / 2).copied().unwrap_or(1) as f64;
    let mut split = Vec::new();
    for (a, b) in spans {
        let w = b - a;
        if w as f64 > 1.6 * typical && w >= 6 {
            let sum = |x: usize| (0..cov.h).map(|y| cov.at(x, y)).sum::<f32>();
            let (lo, hi) = (a + w * 3 / 10, a + w * 7 / 10);
            let mut best = lo;
            for x in lo..hi {
                if sum(x) < sum(best) {
                    best = x;
                }
            }
            split.push((a, best + 1));
            split.push((best, b));
        } else {
            split.push((a, b));
        }
    }
    let n = split.len();
    (0..n)
        .map(|i| {
            let (a, b) = split[i];
            let mut lo = a.saturating_sub(3);
            let mut hi = (b + 3).min(cov.w);
            if i > 0 {
                lo = lo.max((split[i - 1].1 + a).div_ceil(2));
            }
            if i + 1 < n {
                hi = hi.min((b + split[i + 1].0) / 2 + 1);
            }
            Run {
                a,
                b,
                lo,
                hi: hi.max(lo + 1),
            }
        })
        .collect()
}

// ----- letters drawn as the picture shows them -------------------------------

/// A glyph filled at one size, supersampled, as a summed-area table.
struct Drawn {
    /// Supersampled pixels of the table (one more than the bitmap each
    /// way).
    w: usize,
    h: usize,
    sat: Vec<u32>,
    /// Where the bitmap's top left corner sits from the glyph's origin,
    /// in supersampled pixels.
    left: i64,
    top: i64,
    /// The glyph's centre from its origin, pixels (y down).
    cx: f64,
    cy: f64,
    /// Supersampled pixels per pixel.
    ss: usize,
}

fn draw_glyph(g: &Glyph, s: f64, ss: usize) -> Option<Drawn> {
    let k = s * ss as f64;
    let left = (g.bbox.x0 * k).floor() as i64 - 1;
    let top = (-g.bbox.y1 * k).floor() as i64 - 1;
    let w = ((g.bbox.x1 * k).ceil() as i64 + 1 - left).max(1) as usize;
    let h = ((-g.bbox.y0 * k).ceil() as i64 + 1 - top).max(1) as usize;
    if w * h > 4_000_000 {
        return None;
    }
    let mut mask = tiny_skia::Mask::new(w as u32, h as u32)?;
    let mut pb = tiny_skia::PathBuilder::new();
    let tr = Affine::new([k, 0.0, 0.0, -k, -left as f64, -top as f64]);
    for el in (tr * g.path.clone()).elements() {
        match *el {
            PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(a, p) => pb.quad_to(a.x as f32, a.y as f32, p.x as f32, p.y as f32),
            PathEl::CurveTo(a, b, p) => pb.cubic_to(
                a.x as f32, a.y as f32, b.x as f32, b.y as f32, p.x as f32, p.y as f32,
            ),
            PathEl::ClosePath => pb.close(),
        }
    }
    let path = pb.finish()?;
    mask.fill_path(
        &path,
        tiny_skia::FillRule::Winding,
        false,
        tiny_skia::Transform::identity(),
    );
    let data = mask.data();
    let (sw, sh) = (w + 1, h + 1);
    let mut sat = vec![0u32; sw * sh];
    for y in 0..h {
        let mut row = 0u32;
        for x in 0..w {
            row += u32::from(data.get(y * w + x).copied().unwrap_or(0) > 127);
            sat[(y + 1) * sw + x + 1] = sat[y * sw + x + 1] + row;
        }
    }
    Some(Drawn {
        w: sw,
        h: sh,
        sat,
        left,
        top,
        cx: g.centre.x * s,
        cy: -g.centre.y * s,
        ss,
    })
}

impl Drawn {
    /// Supersampled pixels filled in the table's rectangle (bitmap
    /// coordinates, clipped).
    fn sum(&self, x0: i64, y0: i64, x1: i64, y1: i64) -> u32 {
        let cl = |v: i64, m: usize| v.clamp(0, m as i64 - 1) as usize;
        let (x0, x1) = (cl(x0, self.w), cl(x1, self.w));
        let (y0, y1) = (cl(y0, self.h), cl(y1, self.h));
        if x1 <= x0 || y1 <= y0 {
            return 0;
        }
        let at = |x: usize, y: usize| self.sat[y * self.w + x];
        at(x1, y1) + at(x0, y0) - at(x0, y1) - at(x1, y0)
    }

    /// Coverage of the columns `lo..hi` and every row of a `h` row patch
    /// with the glyph's origin at `(ox, oy)` (pixels of the patch).
    fn render(&self, ox: f64, oy: f64, lo: usize, hi: usize, h: usize) -> Vec<f32> {
        let ssf = self.ss as f64;
        let bx = (ox * ssf).round() as i64 + self.left;
        let by = (oy * ssf).round() as i64 + self.top;
        let ss = self.ss as i64;
        let norm = 1.0 / (self.ss * self.ss) as f32;
        let mut out = Vec::with_capacity((hi - lo) * h);
        for y in 0..h as i64 {
            for x in lo as i64..hi as i64 {
                let x0 = x * ss - bx;
                let y0 = y * ss - by;
                out.push(self.sum(x0, y0, x0 + ss, y0 + ss) as f32 * norm);
            }
        }
        out
    }
}

/// `a` (`w` x `h`) blurred with a Gaussian of `sigma` pixels.
fn blur(a: &[f32], w: usize, h: usize, sigma: f64) -> Vec<f32> {
    let r = (sigma * 3.0).ceil() as i64;
    let k: Vec<f32> = (-r..=r)
        .map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp() as f32)
        .collect();
    let ksum: f32 = k.iter().sum();
    let mut tmp = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut v = 0.0;
            for (j, kv) in k.iter().enumerate() {
                let xx = x as i64 + j as i64 - r;
                if xx >= 0 && (xx as usize) < w {
                    v += a[y * w + xx as usize] * kv;
                }
            }
            tmp[y * w + x] = v / ksum;
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut v = 0.0;
            for (j, kv) in k.iter().enumerate() {
                let yy = y as i64 + j as i64 - r;
                if yy >= 0 && (yy as usize) < h {
                    v += tmp[yy as usize * w + x] * kv;
                }
            }
            out[y * w + x] = v / ksum;
        }
    }
    out
}

/// The squared difference between `a` (`w` columns from column `lo` of
/// `cov`, every row) blurred as [`blur`] blurs it and the coverage,
/// summed row by row; the sum stops at the first row that takes it to
/// `limit` or over (that glyph is not taken), so only the rows needed
/// are blurred. Below `limit` it is the same number, to the last bit, as
/// summing the whole [`blur`] picture.
fn blurred_error(a: &[f32], w: usize, cov: &Coverage, lo: usize, sigma: f64, limit: f64) -> f64 {
    let h = cov.h;
    let r = (sigma * 3.0).ceil() as i64;
    let k: Vec<f32> = (-r..=r)
        .map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp() as f32)
        .collect();
    let ksum: f32 = k.iter().sum();
    let ru = r as usize;
    // The rows blurred across, made as the rows below need them.
    let mut tmp = vec![0f32; w * h];
    let mut made = 0usize;
    let mut across = |upto: usize, tmp: &mut [f32]| {
        while made < upto.min(h) {
            let y = made;
            let row = &a[y * w..(y + 1) * w];
            for x in 0..w {
                let j0 = ru.saturating_sub(x);
                let j1 = (w + ru - x).min(k.len());
                let mut v = 0.0;
                for j in j0..j1 {
                    v += row[x + j - ru] * k[j];
                }
                tmp[y * w + x] = v / ksum;
            }
            made += 1;
        }
    };
    let mut e = 0.0;
    for y in 0..h {
        across(y + ru + 1, &mut tmp);
        let j0 = ru.saturating_sub(y);
        let j1 = (h + ru - y).min(k.len());
        for x in 0..w {
            let mut v = 0.0;
            for j in j0..j1 {
                v += tmp[(y + j - ru) * w + x] * k[j];
            }
            let dv = v / ksum - cov.at(lo + x, y);
            e += (dv * dv) as f64;
        }
        if e >= limit {
            break;
        }
    }
    e
}

/// The centre of the coverage in the columns `lo..hi` (pixel centres at
/// +0.5).
fn centre(cov: &Coverage, lo: usize, hi: usize) -> (f64, f64) {
    let (mut t, mut sx, mut sy) = (0.0, 0.0, 0.0);
    for y in 0..cov.h {
        for x in lo..hi {
            let v = cov.at(x, y) as f64;
            t += v;
            sx += v * (x as f64 + 0.5);
            sy += v * (y as f64 + 0.5);
        }
    }
    if t <= 0.0 {
        return ((lo + hi) as f64 / 2.0, cov.h as f64 / 2.0);
    }
    (sx / t, sy / t)
}

/// The squared difference between the picture and the glyph drawn at
/// `(ox, oy)` and blurred, over the run's columns.
fn error_at(cov: &Coverage, run: &Run, d: &Drawn, ox: f64, oy: f64, sigma: f64) -> f64 {
    let w = run.hi - run.lo;
    let r = blur(&d.render(ox, oy, run.lo, run.hi, cov.h), w, cov.h, sigma);
    let mut e = 0.0;
    for y in 0..cov.h {
        for x in 0..w {
            let dv = r[y * w + x] - cov.at(run.lo + x, y);
            e += (dv * dv) as f64;
        }
    }
    e
}

/// The glyph placed on the run by centres; the error and origin.
fn place(cov: &Coverage, run: &Run, d: &Drawn, sigma: f64) -> (f64, f64, f64) {
    let (px, py) = centre(cov, run.lo, run.hi);
    let (ox, oy) = (px - d.cx, py - d.cy);
    (error_at(cov, run, d, ox, oy, sigma), ox, oy)
}

/// For each blur of `sigmas`: the summed error of the best glyph of
/// `face` (drawn at `s` pixels per unit) on each run, and those glyphs.
/// `None` once every blur's sum, after some runs, is over `bound()`:
/// the face can no longer be among those looked for, so the rest of the
/// row is not compared (the choice comes out as without the bound).
fn score(
    cov: &Coverage,
    runs: &[Run],
    face: &Face,
    s: f64,
    ss: usize,
    sigmas: &[f64],
    bound: &dyn Fn() -> f64,
) -> Option<Vec<(f64, Vec<usize>)>> {
    let drawn: Vec<Option<Drawn>> = face.glyphs.iter().map(|g| draw_glyph(g, s, ss)).collect();
    let mut out: Vec<(f64, Vec<usize>)> = sigmas
        .iter()
        .map(|_| (0.0, Vec::with_capacity(runs.len())))
        .collect();
    for run in runs {
        let (px, py) = centre(cov, run.lo, run.hi);
        let w = run.hi - run.lo;
        let mut best: Vec<(f64, usize)> = vec![(f64::INFINITY, 0); sigmas.len()];
        for (gi, d) in drawn.iter().enumerate() {
            let Some(d) = d else {
                continue;
            };
            let sharp = d.render(px - d.cx, py - d.cy, run.lo, run.hi, cov.h);
            for (si, &sigma) in sigmas.iter().enumerate() {
                let e = blurred_error(&sharp, w, cov, run.lo, sigma, best[si].0);
                if e < best[si].0 {
                    best[si] = (e, gi);
                }
            }
        }
        for (o, b) in out.iter_mut().zip(best) {
            o.0 += b.0;
            o.1.push(b.1);
        }
        let least = out.iter().map(|o| o.0).fold(f64::INFINITY, f64::min);
        if least > bound() {
            return None;
        }
    }
    Some(out)
}

/// `f(0..n)` on the computer's cores; `None` once `cancel` is set.
fn par_map<T: Send>(
    n: usize,
    cancel: Option<&Cancel>,
    f: impl Fn(usize) -> T + Sync,
) -> Option<Vec<T>> {
    #[cfg(not(target_arch = "wasm32"))]
    let threads = std::thread::available_parallelism()
        .map_or(1, |t| t.get())
        .min(n.max(1));
    #[cfg(target_arch = "wasm32")]
    let threads = 1;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, T)>> = Mutex::new(Vec::with_capacity(n));
    let work = || loop {
        if crate::trace::cancelled(cancel) {
            return;
        }
        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if i >= n {
            return;
        }
        let v = f(i);
        results
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((i, v));
    };
    if threads <= 1 {
        work();
    } else {
        std::thread::scope(|sc| {
            for _ in 0..threads {
                sc.spawn(work);
            }
        });
    }
    if crate::trace::cancelled(cancel) {
        return None;
    }
    let mut r = results.into_inner().unwrap_or_else(|e| e.into_inner());
    r.sort_by_key(|(i, _)| *i);
    Some(r.into_iter().map(|(_, v)| v).collect())
}

/// A row read in a face.
#[derive(Debug, Clone)]
pub(crate) struct RowFit {
    /// Face name and the letters read.
    pub face: String,
    pub text: String,
    /// The letters' outlines in pixels of the box (y down).
    pub path: BezPath,
    /// Squared coverage left over over the letters' own, for the row and
    /// the worst letter.
    pub error: f64,
    pub worst: f64,
    /// The letters' colour on the picture, from the drawn row.
    pub color: [u8; 3],
}

/// Reads the row of the picture box `b`; `None` when it is not a row of
/// capitals and figures that a face draws.
fn read_row(
    img: &RgbaImage,
    b: (usize, usize, usize, usize),
    cancel: Option<&Cancel>,
) -> Option<RowFit> {
    let (cov, bg, pix) = coverage(img, b)?;
    let runs = runs_of(&cov);
    if runs.len() < 2 {
        return None;
    }
    let mut heights: Vec<usize> = runs
        .iter()
        .filter_map(|r| {
            let rows: Vec<usize> = (0..cov.h)
                .filter(|&y| (r.a..r.b).any(|x| cov.at(x, y) > 0.5))
                .collect();
            Some(rows.last()? - rows.first()? + 1)
        })
        .collect();
    heights.sort_unstable();
    let cap = *heights.get(heights.len() / 2)? as f64;
    if !(MIN_CAP..=MAX_CAP).contains(&cap) {
        return None;
    }
    let faces: Vec<&Face> = bundled_faces().iter().chain(installed_faces()).collect();
    if faces.is_empty() {
        return None;
    }
    let ss = supersampling(cap);
    // 1. Every face at the measured height. A face whose error is
    // already over the TOP_FACES-th best of the faces done stops early.
    let done: Mutex<Vec<f64>> = Mutex::new(Vec::new());
    let top_bound = || {
        let d = done.lock().unwrap_or_else(|e| e.into_inner());
        d.get(TOP_FACES - 1).copied().unwrap_or(f64::INFINITY)
    };
    let firsts = par_map(faces.len(), cancel, |fi| {
        let f = faces[fi];
        let total = score(&cov, &runs, f, cap / f.cap, ss, &[0.55], &top_bound)
            .and_then(|r| r.into_iter().next())
            .map(|(t, _)| t)
            .unwrap_or(f64::INFINITY);
        if total.is_finite() {
            let mut d = done.lock().unwrap_or_else(|e| e.into_inner());
            let at = d.partition_point(|v| *v <= total);
            d.insert(at, total);
            d.truncate(TOP_FACES);
        }
        total
    })?;
    let mut first: Vec<(f64, usize)> = firsts.into_iter().zip(0..).collect();
    first.sort_by(|a, b| a.0.total_cmp(&b.0));
    // 2. The closest faces at heights and blurs around it.
    let jobs: Vec<(usize, f64)> = first
        .iter()
        .take(TOP_FACES)
        .flat_map(|&(_, fi)| {
            let fc = faces[fi].cap;
            SCALES.iter().map(move |k| (fi, cap * k / fc))
        })
        .collect();
    // The heights nearest the measured one first, best faces first: the
    // best error found soon lets the other jobs stop early.
    let mut order: Vec<usize> = (0..jobs.len()).collect();
    order.sort_by_key(|&j| {
        (j % SCALES.len()).abs_diff(SCALES.len() / 2) * TOP_FACES + j / SCALES.len()
    });
    let best_bits = std::sync::atomic::AtomicU64::new(f64::INFINITY.to_bits());
    let best_bound = || f64::from_bits(best_bits.load(std::sync::atomic::Ordering::Relaxed));
    let scored = par_map(jobs.len(), cancel, |k| {
        let j = order[k];
        let (fi, s) = jobs[j];
        let r = score(&cov, &runs, faces[fi], s, ss, &BLURS, &best_bound)
            .unwrap_or_else(|| vec![(f64::INFINITY, Vec::new()); BLURS.len()]);
        let least = r.iter().map(|o| o.0).fold(f64::INFINITY, f64::min);
        let _ = best_bits.fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |b| (least < f64::from_bits(b)).then_some(least.to_bits()),
        );
        (j, r)
    })?;
    let mut scored: Vec<(usize, Vec<(f64, Vec<usize>)>)> = scored;
    scored.sort_by_key(|(j, _)| *j);
    let scored: Vec<Vec<(f64, Vec<usize>)>> = scored.into_iter().map(|(_, r)| r).collect();
    let mut best: Option<(f64, usize, f64, f64, Vec<usize>)> = None;
    for (&(fi, s), per_blur) in jobs.iter().zip(scored) {
        for (sigma, (total, letters)) in BLURS.iter().zip(per_blur) {
            if best.as_ref().is_none_or(|b| total < b.0) {
                best = Some((total, fi, s, *sigma, letters));
            }
        }
    }
    let (_, fi, s, sigma, letters) = best?;
    let f = faces[fi];
    // 3. Each letter placed finely, then all on the row's baseline.
    let drawn: Vec<Option<Drawn>> = letters
        .iter()
        .map(|&gi| draw_glyph(&f.glyphs[gi], s, ss))
        .collect();
    let mut placed = Vec::with_capacity(runs.len());
    for (r, d) in runs.iter().zip(&drawn) {
        let d = d.as_ref()?;
        let (_, ox, oy) = place(&cov, r, d, sigma);
        let mut b = (f64::INFINITY, ox, oy);
        for i in -3..=3 {
            for j in -3..=3 {
                let (x, y) = (ox + i as f64 / 8.0, oy + j as f64 / 8.0);
                let e = error_at(&cov, r, d, x, y, sigma);
                if e < b.0 {
                    b = (e, x, y);
                }
            }
        }
        placed.push(b);
    }
    let mut ys: Vec<f64> = placed.iter().map(|p| p.2).collect();
    ys.sort_by(f64::total_cmp);
    let baseline = *ys.get(ys.len() / 2)?;
    let (mut err, mut ink, mut worst) = (0.0, 0.0, 0f64);
    let mut path = BezPath::new();
    let mut text = String::new();
    let mut drawn_row = vec![0f32; cov.w * cov.h];
    for ((r, d), (&gi, p)) in runs.iter().zip(&drawn).zip(letters.iter().zip(&placed)) {
        let d = d.as_ref()?;
        let mut b = (f64::INFINITY, p.1);
        for i in -8..=8 {
            let x = p.1 + i as f64 / 16.0;
            let e = error_at(&cov, r, d, x, baseline, sigma);
            if e < b.0 {
                b = (e, x);
            }
        }
        let own: f64 = (0..cov.h)
            .flat_map(|y| (r.lo..r.hi).map(move |x| (x, y)))
            .map(|(x, y)| (cov.at(x, y) as f64).powi(2))
            .sum();
        err += b.0;
        ink += own;
        worst = worst.max(b.0 / own.max(1e-9));
        let g = &f.glyphs[gi];
        text.push(g.ch);
        let tr = Affine::new([s, 0.0, 0.0, -s, b.1, baseline]);
        path.extend((tr * g.path.clone()).elements().iter().copied());
        for (v, c) in drawn_row
            .iter_mut()
            .zip(d.render(b.1, baseline, 0, cov.w, cov.h))
        {
            *v = v.max(c);
        }
    }
    if ink <= 0.0 {
        return None;
    }
    // The colour that, laid over the background as the letters cover
    // it, gives the picture best (least squares).
    let a = blur(&drawn_row, cov.w, cov.h, sigma);
    let (mut num, mut den) = ([0f64; 3], 0f64);
    for (av, p) in a.iter().zip(&pix) {
        let av = *av as f64;
        den += av * av;
        for c in 0..3 {
            num[c] += av * (p[c] - bg[c]) as f64;
        }
    }
    let den = den.max(1e-9);
    let color = [0, 1, 2].map(|c| (bg[c] as f64 + num[c] / den).round().clamp(0.0, 255.0) as u8);
    Some(RowFit {
        face: f.name.clone(),
        text,
        path,
        error: err / ink,
        worst,
        color,
    })
}

/// Rows read lately, by their pixels: a trace that only changes a
/// setting reads the same rows again.
fn read_row_cached(
    img: &RgbaImage,
    b: (usize, usize, usize, usize),
    cancel: Option<&Cancel>,
) -> Option<RowFit> {
    use std::hash::{Hash, Hasher};
    static CACHE: Mutex<Vec<(u64, Option<RowFit>)>> = Mutex::new(Vec::new());
    let mut hs = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut hs);
    for y in b.1..b.3 {
        for x in b.0..b.2 {
            img.get_pixel_checked(x as u32, y as u32)
                .map(|p| p.0)
                .hash(&mut hs);
        }
    }
    let key = hs.finish();
    if let Some((_, r)) = CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(k, _)| *k == key)
    {
        return r.clone();
    }
    let r = read_row(img, b, cancel);
    if crate::trace::cancelled(cancel) {
        return None;
    }
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    c.push((key, r.clone()));
    if c.len() > 64 {
        c.remove(0);
    }
    r
}

// ----- rows of letters among the traced curves ------------------------------

/// A closed contour of a traced curve.
struct Piece {
    traced: usize,
    bbox: Rect,
}

fn subpaths(p: &BezPath) -> Vec<BezPath> {
    let mut out: Vec<BezPath> = Vec::new();
    for el in p.elements() {
        if matches!(el, PathEl::MoveTo(_)) || out.is_empty() {
            out.push(BezPath::new());
        }
        if let Some(last) = out.last_mut() {
            last.push(*el);
        }
    }
    out
}

fn inside(a: Rect, b: Rect) -> bool {
    a.x0 >= b.x0 && a.y0 >= b.y0 && a.x1 <= b.x1 && a.y1 <= b.y1
}

fn colour_near(traced: &[Traced], a: usize, b: usize) -> bool {
    let (Some(ta), Some(tb)) = (traced.get(a), traced.get(b)) else {
        return false;
    };
    let (ca, cb) = (ta.color.to_rgb8(), tb.color.to_rgb8());
    let d2: i32 = (0..3).map(|c| (ca[c] as i32 - cb[c] as i32).pow(2)).sum();
    a == b || d2 <= SAME_COLOUR * SAME_COLOUR
}

/// The letters the pieces make: pieces of near colours that touch are
/// one letter (the trace may cut a soft letter into patches of shades),
/// kept when the letter is of a size the rows are read at. Each letter
/// is given the curve of its largest piece.
fn letters_of(traced: &[Traced], pieces: Vec<Piece>) -> Vec<Piece> {
    let n = pieces.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (pieces[i].bbox.inflate(0.75, 0.75), pieces[j].bbox);
            if a.intersect(b).area() > 0.0
                && colour_near(traced, pieces[i].traced, pieces[j].traced)
            {
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    let mut out: Vec<(usize, Piece, f64)> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        let r = root(&mut parent, i);
        match out.iter_mut().find(|o| o.0 == r) {
            Some(o) => {
                o.1.bbox = o.1.bbox.union(p.bbox);
                if p.bbox.area() > o.2 {
                    o.1.traced = p.traced;
                    o.2 = p.bbox.area();
                }
            }
            None => out.push((
                r,
                Piece {
                    traced: p.traced,
                    bbox: p.bbox,
                },
                p.bbox.area(),
            )),
        }
    }
    out.into_iter()
        .map(|o| o.1)
        .filter(|p| {
            let (bw, bh) = (p.bbox.width(), p.bbox.height());
            (MIN_CAP - 1.0..=MAX_CAP + 4.0).contains(&bh) && bw <= 2.6 * bh && bw >= 0.8
        })
        .collect()
}

/// The boxes of rows of letters among the curves (pixels of the
/// picture `scale` times smaller than the curves' own).
fn rows(traced: &[Traced], scale: f64, w: usize, h: usize) -> Vec<(usize, usize, usize, usize)> {
    let mut pieces = Vec::new();
    for (ti, t) in traced.iter().enumerate() {
        if !t.closed {
            continue;
        }
        let boxes: Vec<Rect> = subpaths(&t.path)
            .iter()
            .map(|p| {
                let b = p.bounding_box();
                Rect::new(b.x0 / scale, b.y0 / scale, b.x1 / scale, b.y1 / scale)
            })
            .collect();
        for (i, b) in boxes.iter().enumerate() {
            // Outer contours only: a hole lies inside another contour of
            // its curve.
            let held = boxes
                .iter()
                .enumerate()
                .any(|(j, o)| j != i && inside(*b, *o) && o.area() > b.area());
            if !held && b.height() <= MAX_CAP + 4.0 && b.width() <= 3.0 * (MAX_CAP + 4.0) {
                pieces.push(Piece {
                    traced: ti,
                    bbox: *b,
                });
            }
        }
    }
    let pieces = letters_of(traced, pieces);
    // Join letters of a colour standing side by side on a line.
    let n = pieces.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (pieces[i].bbox, pieces[j].bbox);
            // Letters of one colour, or of shades of it (the trace may
            // give each letter of soft small print its own shade).
            let same = colour_near(traced, pieces[i].traced, pieces[j].traced);
            let hm = a.height().max(b.height());
            let gap = (b.x0 - a.x1).max(a.x0 - b.x1);
            if same
                && a.height().min(b.height()) >= 0.6 * hm
                && (a.y1 - b.y1).abs() <= 0.25 * hm
                && (a.y0 - b.y0).abs() <= 0.3 * hm
                && gap <= 3.0 * hm
            {
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    let mut groups: Vec<(usize, Rect, usize)> = Vec::new();
    for (i, piece) in pieces.iter().enumerate() {
        let r = root(&mut parent, i);
        match groups.iter_mut().find(|g| g.0 == r) {
            Some(g) => {
                g.1 = g.1.union(piece.bbox);
                g.2 += 1;
            }
            None => groups.push((r, piece.bbox, 1)),
        }
    }
    groups
        .into_iter()
        .filter(|g| g.2 >= 3)
        .map(|(_, b, _)| {
            let m = 3.0;
            (
                (b.x0 - m).floor().max(0.0) as usize,
                (b.y0 - m).floor().max(0.0) as usize,
                ((b.x1 + m).ceil() as usize).min(w),
                ((b.y1 + m).ceil() as usize).min(h),
            )
        })
        .collect()
}

/// `img` made `f` times smaller by averaging.
fn shrink(img: &RgbaImage, f: f64) -> RgbaImage {
    let w = ((img.width() as f64 / f).round() as u32).max(1);
    let h = ((img.height() as f64 / f).round() as u32).max(1);
    image::imageops::resize(img, w, h, image::imageops::FilterType::Triangle)
}

/// Swaps the traced curves of the rows of small print in `traced` (made
/// from `img` with `s`) for the outlines of the font that draws them.
/// The rows redrawn, for the tests and the log; `None` once `cancel` is set.
pub(crate) fn redraw_text(
    img: &RgbaImage,
    s: &Settings,
    traced: &mut Vec<Traced>,
    cancel: Option<&Cancel>,
) -> Option<Vec<RowFit>> {
    if s.centerline || traced.is_empty() {
        return Some(Vec::new());
    }
    let scale = if s.pixel_scale.is_finite() {
        s.pixel_scale.clamp(1.0, 8.0)
    } else {
        1.0
    };
    let small_owned;
    let small = if scale > 1.01 {
        small_owned = shrink(img, scale);
        &small_owned
    } else {
        img
    };
    let (w, h) = (small.width() as usize, small.height() as usize);
    let mut done = Vec::new();
    let mut taken = Vec::new();
    let mut found = rows(traced, scale, w, h);
    // Longer rows first.
    found.sort_by_key(|b| std::cmp::Reverse(b.2 - b.0));
    let rows_found = found.len().max(1);
    for (k, b) in found.into_iter().enumerate() {
        if crate::trace::cancelled(cancel) {
            return None;
        }
        crate::trace_progress::report(
            cancel,
            crate::trace_progress::Step::Letters,
            k as f64 / rows_found as f64,
        );
        // A row inside one already redrawn (a word of it) is skipped.
        if taken.iter().any(|t: &(usize, usize, usize, usize)| {
            b.0 < t.2 && t.0 < b.2 && b.1 < t.3 && t.1 < b.3
        }) {
            continue;
        }
        let Some(fit) = read_row_cached(small, b, cancel) else {
            continue;
        };
        if fit.error > MAX_ROW_ERROR || fit.worst > MAX_LETTER_ERROR {
            continue;
        }
        let area = Rect::new(
            (b.0 as f64 - 0.5) * scale,
            (b.1 as f64 - 0.5) * scale,
            (b.2 as f64 + 0.5) * scale,
            (b.3 as f64 + 0.5) * scale,
        );
        // The letters' curve: the one with the most ink in the row.
        let mut ink: Vec<(usize, f64)> = Vec::new();
        for (ti, t) in traced.iter_mut().enumerate() {
            let mut kept = BezPath::new();
            let mut removed = 0.0;
            for p in subpaths(&t.path) {
                let bb = p.bounding_box();
                if inside(bb, area) {
                    removed += p.area();
                } else {
                    kept.extend(p.elements().iter().copied());
                }
            }
            if removed != 0.0 {
                ink.push((ti, removed.abs()));
                t.path = kept;
            }
        }
        let Some(&(owner, _)) = ink.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else {
            continue;
        };
        let Some(base) = traced.get(owner) else {
            continue;
        };
        let to_curves = Affine::scale(scale) * Affine::translate((b.0 as f64, b.1 as f64));
        // The trace's own colour nearest the letters' colour when it is
        // close (a colour of the trace, not one more), else theirs.
        let near = traced
            .iter()
            .map(|t| t.color)
            .min_by_key(|c| {
                let c = c.to_rgb8();
                (0..3)
                    .map(|i| (c[i] as i32 - fit.color[i] as i32).pow(2))
                    .sum::<i32>()
            })
            .filter(|c| {
                let c = c.to_rgb8();
                (0..3)
                    .map(|i| (c[i] as i32 - fit.color[i] as i32).pow(2))
                    .sum::<i32>()
                    <= NEAR_COLOUR * NEAR_COLOUR
            });
        let color = near.unwrap_or(Color::rgb8(fit.color[0], fit.color[1], fit.color[2]));
        let letters = Traced {
            path: to_curves * fit.path.clone(),
            color,
            closed: true,
            blend: base.blend,
        };
        log::info!(
            "trace: \"{}\" redrawn in {} (left over {:.3})",
            fit.text,
            fit.face,
            fit.error
        );
        traced.push(letters);
        done.push(fit);
        taken.push(b);
    }
    traced.retain(|t| !t.path.elements().is_empty());
    Some(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font that closes a contour it never began draws no close at the
    /// start of the letter's path, which could not be measured (#4119).
    #[test]
    fn a_close_without_a_contour_is_left_out() {
        use ttf_parser::OutlineBuilder;
        let mut pen = Pen(BezPath::new());
        pen.close();
        pen.move_to(0.0, 0.0);
        pen.line_to(2.0, 0.0);
        pen.line_to(2.0, 2.0);
        pen.close();
        pen.close();
        assert_eq!(pen.0.elements().len(), 4);
        assert!(matches!(pen.0.elements()[0], PathEl::MoveTo(_)));
        let b = pen.0.bounding_box();
        assert!((b.width() - 2.0).abs() < 1e-9);
    }

    /// `text` in the bundled face `face`, capitals `cap` pixels tall, in
    /// `color` on white, blurred as a small picture saved from a larger
    /// one is.
    fn picture(face: &str, text: &str, cap: f64, tracking: f64, color: [u8; 3]) -> RgbaImage {
        let f = bundled_faces().iter().find(|f| f.name == face).unwrap();
        let s = cap / f.cap;
        let mut path = BezPath::new();
        let mut x = 10.0;
        for ch in text.chars() {
            if ch == ' ' {
                x += cap * 1.2;
                continue;
            }
            let g = f.glyphs.iter().find(|g| g.ch == ch).unwrap();
            let tr = Affine::new([s, 0.0, 0.0, -s, x - g.bbox.x0 * s, 10.0 + cap]);
            path.extend((tr * g.path.clone()).elements().iter().copied());
            x += g.bbox.width() * s + tracking;
        }
        let (w, h) = ((x + 10.0) as u32, (cap + 20.0) as u32);
        // Drawn four times larger and reduced, then softened.
        let mut pm = tiny_skia::Pixmap::new(w * 4, h * 4).unwrap();
        pm.fill(tiny_skia::Color::WHITE);
        let mut pb = tiny_skia::PathBuilder::new();
        for el in (Affine::scale(4.0) * path).elements() {
            match *el {
                PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
                PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
                PathEl::QuadTo(a, p) => pb.quad_to(a.x as f32, a.y as f32, p.x as f32, p.y as f32),
                PathEl::CurveTo(a, b, p) => pb.cubic_to(
                    a.x as f32, a.y as f32, b.x as f32, b.y as f32, p.x as f32, p.y as f32,
                ),
                PathEl::ClosePath => pb.close(),
            }
        }
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], 255);
        pm.fill_path(
            &pb.finish().unwrap(),
            &paint,
            tiny_skia::FillRule::EvenOdd,
            tiny_skia::Transform::identity(),
            None,
        );
        let big = RgbaImage::from_raw(w * 4, h * 4, pm.data().to_vec()).unwrap();
        let small = image::imageops::resize(&big, w, h, image::imageops::FilterType::Triangle);
        image::imageops::blur(&small, 0.5)
    }

    fn traced_with_text(img: &RgbaImage) -> (Vec<Traced>, Vec<RowFit>) {
        let s = crate::trace::Preset::Logo.settings();
        let mut traced = crate::trace::outline_for_tests(img, &s);
        let rows = redraw_text(img, &s, &mut traced, None).unwrap();
        (traced, rows)
    }

    #[test]
    fn small_capitals_are_read_and_redrawn_in_their_font() {
        let img = picture("Montserrat 700", "FIGHT CLUB", 7.0, 4.0, [255, 140, 0]);
        let (traced, rows) = traced_with_text(&img);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].text, "FIGHTCLUB");
        assert!(rows[0].face.starts_with("Montserrat"), "{}", rows[0].face);
        assert!(rows[0].error < 0.03, "{}", rows[0].error);
        // The letters are one curve in the trace's orange, with the
        // font's own counters (the G, the B's two holes, the U...).
        let letters = traced.last().unwrap();
        let c = letters.color.to_rgb8();
        assert!(c[0] > 200 && c[1] > 100 && c[1] < 180 && c[2] < 80, "{c:?}");
        assert!(subpaths(&letters.path).len() >= 11);
        // Nothing else of the trace is left in the row.
        let row = letters.path.bounding_box().inflate(1.0, 1.0);
        for t in &traced[..traced.len() - 1] {
            for p in subpaths(&t.path) {
                assert!(
                    !inside(p.bounding_box(), row),
                    "a traced piece stayed under the letters"
                );
            }
        }
    }

    #[test]
    fn figures_and_another_face_are_read() {
        let img = picture("Poppins 600", "EST 2020", 8.0, 5.0, [40, 40, 40]);
        let (_, rows) = traced_with_text(&img);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].text, "EST2020");
        assert!(rows[0].face.starts_with("Poppins"), "{}", rows[0].face);
    }

    #[test]
    fn a_row_of_shapes_that_are_not_letters_stays_traced() {
        // Five filled dots and stars of letter size in a row.
        let mut img = RgbaImage::from_pixel(120, 30, image::Rgba([255, 255, 255, 255]));
        for k in 0..5 {
            let (cx, cy) = (14.0 + k as f64 * 22.0, 15.0);
            for y in 0..30 {
                for x in 0..120 {
                    let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                    let r = (dx * dx + dy * dy).sqrt();
                    let a = (5.0 - r).clamp(0.0, 1.0);
                    if a > 0.0 {
                        let v = (255.0 * (1.0 - a)) as u8;
                        img.put_pixel(x, y, image::Rgba([v, v, 255, 255]));
                    }
                }
            }
        }
        let (traced, rows) = traced_with_text(&img);
        assert!(rows.is_empty(), "{rows:?}");
        assert!(!traced.is_empty());
    }

    #[test]
    fn letters_fill_alike_under_either_rule() {
        // Curves fill even-odd: a letter drawn with crossing or
        // overlapping pieces must not get holes where they cross.
        use tracedraw_core::shaping::{filled_area, FillRule};
        for f in bundled_faces() {
            for g in &f.glyphs {
                let eo = filled_area(&g.path, FillRule::EvenOdd);
                let nz = filled_area(&g.path, FillRule::NonZero);
                assert!(
                    (eo - nz).abs() <= 1e-3 * nz,
                    "{} {}: {eo} {nz}",
                    f.name,
                    g.ch
                );
            }
        }
    }

    #[test]
    fn every_bundled_face_has_all_letters() {
        assert_eq!(bundled_faces().len(), BUNDLED.len());
        for f in bundled_faces() {
            assert_eq!(f.glyphs.len(), CHARS.len(), "{}", f.name);
            assert!(f.cap > 0.0);
        }
    }

    /// Traces the pictures named in `TRACE_TEXT` (comma separated; the
    /// art's scale in `TRACE_TEXT_SCALE`, 1 by default) and writes the
    /// trace drawn 4 times larger to `TRACE_TEXT_OUT`, printing the rows
    /// read: `cargo test -p tracedraw --release --lib trace_text::tests::lab -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn lab() {
        let Ok(list) = std::env::var("TRACE_TEXT") else {
            return;
        };
        let scale: f64 = std::env::var("TRACE_TEXT_SCALE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0);
        let out = std::env::var("TRACE_TEXT_OUT").unwrap_or_else(|_| ".".into());
        for file in list.split(',') {
            let img = image::open(file).unwrap().to_rgba8();
            let mut s = crate::trace::Preset::Logo.settings();
            s.pixel_scale = scale;
            let t0 = std::time::Instant::now();
            let mut traced = crate::trace::outline_for_tests(&img, &s);
            let t1 = t0.elapsed();
            let rows = redraw_text(&img, &s, &mut traced, None).unwrap();
            println!("{file}: trace {:?}, text {:?}", t1, t0.elapsed() - t1);
            for r in &rows {
                println!(
                    "  {} \"{}\" error {:.4} worst {:.4}",
                    r.face, r.text, r.error, r.worst
                );
            }
            let zoom: f64 = std::env::var("TRACE_TEXT_ZOOM")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4.0);
            let k = (zoom / scale) as f32;
            let png =
                crate::trace::render_traced_scaled(&traced, img.width(), img.height(), k, 1.0);
            let name = std::path::Path::new(file)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string();
            png.save(format!("{out}/{name}_text.png")).unwrap();
        }
    }
}

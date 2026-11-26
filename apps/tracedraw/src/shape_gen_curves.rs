//! Shape Generator curves: rolling-circle and harmonic curves, guilloche
//! rosettes, fractals and L-systems, and aperiodic and star tilings.
//!
//! - Spirograph: hypotrochoids and epitrochoids (a pen in a circle rolling
//!   inside or outside a fixed one, closed after the whole number of turns
//!   the two radii need), Lissajous figures, rose curves and mandalas
//!   (rings of petals with N-fold symmetry).
//! - Guilloche: rings whose radius follows two sine waves, each ring's
//!   phase turned a little from the last, so the lines weave.
//! - Fractal: a branching tree, and L-systems (Koch snowflake, Sierpinski
//!   arrowhead, dragon curve, plant, or the panel's own axiom and rules)
//!   drawn by a turtle.
//! - Tiling: Penrose rhombs from repeated subdivision, and star patterns
//!   drawn from rays at a contact angle out of every edge's middle on a
//!   square (eight-point stars) or hexagon (six-point stars) grid.
//!
//! Every curve is fitted into the Width x Height box. Model space: mm,
//! y up, the box's bottom left at the origin.

use crate::shape_gen::{polygon, smooth_through, Generated, Paint, Rng, ShapeGenSettings};
use tracedraw_core::geometry::{BezPath, Point, Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpiroType {
    #[default]
    Hypotrochoid,
    Epitrochoid,
    Lissajous,
    Rose,
    Mandala,
}

impl SpiroType {
    pub const ALL: [SpiroType; 5] = [
        SpiroType::Hypotrochoid,
        SpiroType::Epitrochoid,
        SpiroType::Lissajous,
        SpiroType::Rose,
        SpiroType::Mandala,
    ];
    pub fn key(self) -> &'static str {
        match self {
            SpiroType::Hypotrochoid => "shapegen.spiro_inside",
            SpiroType::Epitrochoid => "shapegen.spiro_outside",
            SpiroType::Lissajous => "shapegen.spiro_lissajous",
            SpiroType::Rose => "shapegen.spiro_rose",
            SpiroType::Mandala => "shapegen.spiro_mandala",
        }
    }
}

/// Where the guilloche lines run: rings round the middle, a band across
/// the box, or a frame round its edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuilLayout {
    #[default]
    Rosette,
    Band,
    Frame,
}

impl GuilLayout {
    pub const ALL: [GuilLayout; 3] = [GuilLayout::Rosette, GuilLayout::Band, GuilLayout::Frame];
    pub fn key(self) -> &'static str {
        match self {
            GuilLayout::Rosette => "shapegen.guil_rosette",
            GuilLayout::Band => "shapegen.guil_band_layout",
            GuilLayout::Frame => "shapegen.guil_frame_layout",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FractalType {
    #[default]
    Tree,
    Koch,
    Sierpinski,
    Dragon,
    Plant,
    Custom,
}

impl FractalType {
    pub const ALL: [FractalType; 6] = [
        FractalType::Tree,
        FractalType::Koch,
        FractalType::Sierpinski,
        FractalType::Dragon,
        FractalType::Plant,
        FractalType::Custom,
    ];
    pub fn key(self) -> &'static str {
        match self {
            FractalType::Tree => "shapegen.frac_tree",
            FractalType::Koch => "shapegen.frac_koch",
            FractalType::Sierpinski => "shapegen.frac_sierpinski",
            FractalType::Dragon => "shapegen.frac_dragon",
            FractalType::Plant => "shapegen.frac_plant",
            FractalType::Custom => "shapegen.frac_custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TilingType {
    #[default]
    Penrose,
    Star8,
    Star6,
}

impl TilingType {
    pub const ALL: [TilingType; 3] = [TilingType::Penrose, TilingType::Star8, TilingType::Star6];
    pub fn key(self) -> &'static str {
        match self {
            TilingType::Penrose => "shapegen.tile_penrose",
            TilingType::Star8 => "shapegen.tile_star8",
            TilingType::Star6 => "shapegen.tile_star6",
        }
    }
}

/// Move and scale every curve so the drawing fits the box, centred,
/// keeping its proportions.
fn fit_box(s: &ShapeGenSettings, out: &mut Generated) {
    let Some(b) = out.bounds() else {
        return;
    };
    let k = (s.width / b.width().max(1e-9)).min(s.height / b.height().max(1e-9));
    let m = tracedraw_core::geometry::Affine::translate(Vec2::new(s.width / 2.0, s.height / 2.0))
        * tracedraw_core::geometry::Affine::scale(k)
        * tracedraw_core::geometry::Affine::translate(-b.center().to_vec2());
    for it in &mut out.items {
        it.path.apply_affine(m);
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

/// A curve sampled from `f` over `0..t_end`, `n` samples, closed or not.
fn sampled(f: impl Fn(f64) -> Point, t_end: f64, n: usize, closed: bool) -> BezPath {
    let n = n.max(8);
    let last = if closed { n } else { n + 1 };
    let pts: Vec<Point> = (0..last).map(|k| f(t_end * k as f64 / n as f64)).collect();
    smooth_through(&pts, closed)
}

// ----- spirograph -----------------------------------------------------------

pub(crate) fn spirograph(s: &ShapeGenSettings, out: &mut Generated) {
    let tau = std::f64::consts::TAU;
    match s.spiro_type {
        SpiroType::Hypotrochoid | SpiroType::Epitrochoid => {
            let inside = s.spiro_type == SpiroType::Hypotrochoid;
            let (big, small, d) = (s.spiro_fixed, s.spiro_rolling, s.spiro_pen);
            // Whole turns: the radii in tenths of a millimetre.
            let (bi, si) = (
                (big * 10.0).round() as u64,
                (small * 10.0).round().max(1.0) as u64,
            );
            let turns = (si / gcd(bi, si)).clamp(1, 400);
            let k = if inside { big - small } else { big + small };
            let ratio = k / small;
            let f = |t: f64| {
                if inside {
                    Point::new(
                        k * t.cos() + d * (ratio * t).cos(),
                        k * t.sin() - d * (ratio * t).sin(),
                    )
                } else {
                    Point::new(
                        k * t.cos() - d * (ratio * t).cos(),
                        k * t.sin() - d * (ratio * t).sin(),
                    )
                }
            };
            // Samples follow how fast the pen wheels round.
            let per_turn = ((ratio.abs() + 1.0) * 48.0).clamp(48.0, 2400.0) as usize;
            let n = (per_turn * turns as usize).min(60_000);
            out.push(sampled(f, tau * turns as f64, n, true), true, Paint::Line);
        }
        SpiroType::Lissajous => {
            let (a, b) = (s.liss_a as f64, s.liss_b as f64);
            let ph = s.liss_phase.to_radians();
            let n = ((a.max(b)) * 120.0) as usize;
            let f = |t: f64| Point::new((a * t + ph).sin(), (b * t).sin());
            out.push(sampled(f, tau, n, true), true, Paint::Line);
        }
        SpiroType::Rose => {
            let (n, d) = (s.rose_n.max(1) as u64, s.rose_d.max(1) as u64);
            let g = gcd(n, d);
            let (n, d) = (n / g, d / g);
            let k = n as f64 / d as f64;
            let end = if (n * d) % 2 == 1 {
                std::f64::consts::PI * d as f64
            } else {
                tau * d as f64
            };
            let samples = ((n + d) as usize * 160).min(40_000);
            let f = |t: f64| {
                let r = (k * t).cos();
                Point::new(r * t.cos(), r * t.sin())
            };
            out.push(sampled(f, end, samples, true), true, Paint::Line);
        }
        SpiroType::Mandala => mandala(s, out),
    }
    fit_box(s, out);
}

/// Rings of petals round the middle with N-fold symmetry; each ring's
/// petal shape comes from the seed.
fn mandala(s: &ShapeGenSettings, out: &mut Generated) {
    let fold = s.mandala_fold.max(3);
    let rings = s.mandala_rings.max(1);
    let mut rng = Rng::new(s.seed, 0x3a4d);
    let step = 1.0 / rings as f64;
    let sector = std::f64::consts::TAU / fold as f64;
    for ring in 0..rings {
        let (r0, r1) = (ring as f64 * step + step * 0.08, (ring + 1) as f64 * step);
        let shape = rng.below(3);
        let width = 0.55 + 0.4 * rng.unit();
        let offset = if ring % 2 == 1 { sector / 2.0 } else { 0.0 };
        for k in 0..fold {
            if out.full() {
                out.cut = true;
                return;
            }
            let a = offset + k as f64 * sector;
            let half = sector * width / 2.0;
            let at = |r: f64, t: f64| Point::new(r * t.cos(), r * t.sin());
            let mut p = BezPath::new();
            let base_l = at(r0, a - half);
            let base_r = at(r0, a + half);
            let tip = at(r1, a);
            p.move_to(base_l);
            match shape {
                // Teardrop: bulging sides to a point.
                0 => {
                    p.curve_to(
                        at(r0 + (r1 - r0) * 0.6, a - half * 1.4),
                        at(r1, a - half * 0.2),
                        tip,
                    );
                    p.curve_to(
                        at(r1, a + half * 0.2),
                        at(r0 + (r1 - r0) * 0.6, a + half * 1.4),
                        base_r,
                    );
                }
                // Round lobe.
                1 => {
                    p.curve_to(at(r1 * 1.02, a - half), at(r1 * 1.02, a - half * 0.3), tip);
                    p.curve_to(
                        at(r1 * 1.02, a + half * 0.3),
                        at(r1 * 1.02, a + half),
                        base_r,
                    );
                }
                // Pointed leaf with straight-ish sides.
                _ => {
                    p.quad_to(at(r0 + (r1 - r0) * 0.5, a - half * 0.9), tip);
                    p.quad_to(at(r0 + (r1 - r0) * 0.5, a + half * 0.9), base_r);
                }
            }
            p.close_path();
            out.push(p, true, Paint::Line);
        }
        out.push(
            tracedraw_core::geometry::ellipse_path(Rect::new(-r0, -r0, r0, r0)),
            true,
            Paint::Line,
        );
    }
    out.push(
        tracedraw_core::geometry::ellipse_path(Rect::new(-1.0, -1.0, 1.0, 1.0)),
        true,
        Paint::Line,
    );
}

// ----- guilloche ------------------------------------------------------------

pub(crate) fn guilloche(s: &ShapeGenSettings, out: &mut Generated) {
    match s.guil_layout {
        GuilLayout::Rosette => {}
        GuilLayout::Band => return guilloche_band(s, out),
        GuilLayout::Frame => return guilloche_frame(s, out),
    }
    let c = Point::new(s.width / 2.0, s.height / 2.0);
    let n = s.guil_lines.max(1);
    let (w1, w2) = (s.guil_waves as f64, s.guil_waves2 as f64);
    let samples = ((w1.max(w2) * 24.0) as usize).clamp(180, 3000);
    let (inner, outer) = if s.guil_inner <= s.guil_outer {
        (s.guil_inner, s.guil_outer)
    } else {
        (s.guil_outer, s.guil_inner)
    };
    for k in 0..n {
        if out.full() {
            out.cut = true;
            return;
        }
        let t = if n == 1 {
            0.5
        } else {
            k as f64 / (n - 1) as f64
        };
        let base = inner + (outer - inner) * t;
        let ph = (k as f64 * s.guil_twist).to_radians();
        let f = |a: f64| {
            let r =
                base + s.guil_amp * (w1 * a + ph).sin() + s.guil_amp2 * (w2 * a - ph * 2.0).sin();
            Point::new(c.x + r * a.cos(), c.y + r * a.sin())
        };
        out.push(
            sampled(f, std::f64::consts::TAU, samples, true),
            true,
            Paint::Line,
        );
    }
}

/// The share (0 to 1) of the `k`th of `n` lines across the band.
fn line_share(k: u32, n: u32) -> f64 {
    if n <= 1 {
        0.5
    } else {
        k as f64 / (n - 1) as f64
    }
}

/// Guilloche lines across the box, left to right: each is two sine
/// waves (a whole number of each across the width) round its own height,
/// its phase turned from the last line's, so the lines weave. The waves
/// stay inside the box.
fn guilloche_band(s: &ShapeGenSettings, out: &mut Generated) {
    let (w, h) = (s.width, s.height);
    out.frame = Some(Rect::new(0.0, 0.0, w, h));
    let n = s.guil_lines.max(1);
    let (a1, a2) = (s.guil_amp.abs(), s.guil_amp2.abs());
    let reach = (a1 + a2).min(h / 2.0);
    let k = if a1 + a2 > 1e-9 { reach / (a1 + a2) } else { 1.0 };
    let (a1, a2) = (a1 * k, a2 * k);
    let w1 = (w / s.guil_period).round().max(1.0) * std::f64::consts::TAU / w;
    let w2 = (w / s.guil_period2).round().max(1.0) * std::f64::consts::TAU / w;
    let samples = ((w * w1.max(w2) / std::f64::consts::TAU * 24.0) as usize).clamp(48, 6000);
    for i in 0..n {
        if out.full() {
            out.cut = true;
            return;
        }
        // A hair inside, so the smoothed waves never poke out.
        let edge = reach + h * 0.002;
        let base = edge + (h - 2.0 * edge) * line_share(i, n);
        let ph = (i as f64 * s.guil_twist).to_radians();
        let f = |x: f64| {
            Point::new(
                x,
                base + a1 * (w1 * x + ph).sin() + a2 * (w2 * x - ph * 2.0).sin(),
            )
        };
        out.push(sampled(f, w, samples, false), false, Paint::Line);
    }
}

/// Guilloche lines round the box's edge in a band `guil_band` wide:
/// rounded rectangles one inside the other, each waved like a band's
/// line along its length, the waves keeping step from line to line.
fn guilloche_frame(s: &ShapeGenSettings, out: &mut Generated) {
    let (w, h) = (s.width, s.height);
    let outer = Rect::new(0.0, 0.0, w, h);
    out.frame = Some(outer);
    let band = s.guil_band.min(w / 2.0).min(h / 2.0).max(0.1);
    let n = s.guil_lines.max(1);
    let (a1, a2) = (s.guil_amp.abs(), s.guil_amp2.abs());
    let reach = (a1 + a2).min(band / 2.0);
    let k = if a1 + a2 > 1e-9 { reach / (a1 + a2) } else { 1.0 };
    let (a1, a2) = (a1 * k, a2 * k);
    // The waves are counted on the band's middle line.
    let mid_rad = band / 2.0;
    let mid_len = 2.0 * (w + h - 2.0 * band) + std::f64::consts::TAU * mid_rad;
    let n1 = (mid_len / s.guil_period).round().max(1.0);
    let n2 = (mid_len / s.guil_period2).round().max(1.0);
    let samples = ((n1.max(n2) * 24.0) as usize).clamp(96, 12_000);
    for i in 0..n {
        if out.full() {
            out.cut = true;
            return;
        }
        let edge = reach + band * 0.002;
        let d = edge + (band - 2.0 * edge) * line_share(i, n);
        let r = outer.inflate(-d, -d);
        let rad = (band - d).max(0.0).min(r.width() / 2.0).min(r.height() / 2.0);
        let per = 2.0 * (r.width() + r.height() - 4.0 * rad) + std::f64::consts::TAU * rad;
        let ph = (i as f64 * s.guil_twist).to_radians();
        let tau = std::f64::consts::TAU;
        let f = |u: f64| {
            let (p, nrm) = crate::frame_gen::round_rect_at(r, rad, u * per);
            let off = a1 * (tau * n1 * u + ph).sin() + a2 * (tau * n2 * u - ph * 2.0).sin();
            p + nrm * off
        };
        out.push(sampled(f, 1.0, samples, true), true, Paint::Line);
    }
}

// ----- fractals -------------------------------------------------------------

/// The axiom, rules and turning angle of an L-system preset.
fn preset(t: FractalType) -> (&'static str, &'static str, f64) {
    match t {
        FractalType::Koch => ("F--F--F", "F=F+F--F+F", 60.0),
        FractalType::Sierpinski => ("A", "A=B-A-B;B=A+B+A", 60.0),
        FractalType::Dragon => ("FX", "X=X+YF+;Y=-FX-Y", 90.0),
        FractalType::Plant => ("X", "X=F+[[X]-X]-F[-FX]+X;F=FF", 25.0),
        FractalType::Tree | FractalType::Custom => ("F", "F=F[+F]F[-F]F", 25.7),
    }
}

/// Rules written `F=F+F;X=...`: one symbol, an equals sign, its
/// replacement; anything else is skipped.
fn parse_rules(text: &str) -> Vec<(char, String)> {
    text.split([';', '\n', ','])
        .filter_map(|r| {
            let (l, rhs) = r.split_once('=')?;
            let mut l = l.trim().chars();
            let c = l.next()?;
            if l.next().is_some() {
                return None;
            }
            Some((c, rhs.trim().to_string()))
        })
        .collect()
}

/// The most symbols an L-system string may grow to.
const MAX_SYMBOLS: usize = 400_000;

fn expand(axiom: &str, rules: &[(char, String)], depth: u32) -> String {
    let mut cur: String = axiom.chars().take(200).collect();
    for _ in 0..depth {
        let mut next = String::with_capacity(cur.len() * 2);
        for ch in cur.chars() {
            match rules.iter().find(|(c, _)| *c == ch) {
                Some((_, r)) => next.push_str(r),
                None => next.push(ch),
            }
            if next.len() > MAX_SYMBOLS {
                return cur;
            }
        }
        cur = next;
    }
    cur
}

/// Draw an L-system string: letters A, B, F and G draw a step, f moves
/// without drawing, + and - turn, [ and ] keep and restore the place, |
/// turns round.
fn turtle(text: &str, angle: f64, out: &mut Generated) {
    let turn = angle.to_radians();
    let mut pos = Point::ORIGIN;
    let mut dir = std::f64::consts::FRAC_PI_2;
    let mut stack: Vec<(Point, f64)> = Vec::new();
    let mut line: Vec<Point> = vec![pos];
    let flush = |line: &mut Vec<Point>, out: &mut Generated| {
        if line.len() >= 2 {
            out.push(polygon_open(line), false, Paint::Line);
        }
        line.clear();
    };
    for ch in text.chars() {
        if out.full() {
            out.cut = true;
            return;
        }
        match ch {
            'F' | 'G' | 'A' | 'B' => {
                pos += Vec2::new(dir.cos(), dir.sin());
                line.push(pos);
            }
            'f' => {
                flush(&mut line, out);
                pos += Vec2::new(dir.cos(), dir.sin());
                line.push(pos);
            }
            '+' => dir += turn,
            '-' => dir -= turn,
            '|' => dir += std::f64::consts::PI,
            '[' => stack.push((pos, dir)),
            ']' => {
                if let Some((p, d)) = stack.pop() {
                    flush(&mut line, out);
                    pos = p;
                    dir = d;
                    line.push(pos);
                }
            }
            _ => {}
        }
    }
    flush(&mut line, out);
}

fn polygon_open(pts: &[Point]) -> BezPath {
    let mut p = BezPath::new();
    for (k, q) in pts.iter().enumerate() {
        if k == 0 {
            p.move_to(*q);
        } else {
            p.line_to(*q);
        }
    }
    p
}

/// A branching tree: each branch splits in two, turned by the angle (and
/// the seed's jitter) and shortened by the ratio.
fn tree(s: &ShapeGenSettings, out: &mut Generated) {
    let mut rng = Rng::new(s.seed, 0x7ee);
    let turn = s.frac_angle.to_radians();
    let jit = s.frac_jitter / 100.0;
    let mut todo: Vec<(Point, f64, f64, u32)> = vec![(
        Point::ORIGIN,
        std::f64::consts::FRAC_PI_2,
        1.0,
        s.frac_depth,
    )];
    while let Some((p, dir, len, depth)) = todo.pop() {
        if out.full() {
            out.cut = true;
            return;
        }
        let q = p + Vec2::new(dir.cos(), dir.sin()) * len;
        let mut seg = BezPath::new();
        seg.move_to(p);
        seg.line_to(q);
        out.push(seg, false, Paint::Line);
        if depth == 0 {
            continue;
        }
        for side in [-1.0, 1.0] {
            let a = dir + side * turn * (1.0 + jit * (rng.unit() - 0.5) * 2.0);
            let l = len * s.frac_ratio * (1.0 + jit * (rng.unit() - 0.5));
            todo.push((q, a, l, depth - 1));
        }
    }
}

pub(crate) fn fractal(s: &ShapeGenSettings, out: &mut Generated) {
    match s.frac_type {
        FractalType::Tree => tree(s, out),
        FractalType::Custom => {
            let rules = parse_rules(&s.frac_rules);
            let text = expand(&s.frac_axiom, &rules, s.frac_depth);
            turtle(&text, s.frac_angle, out);
        }
        t => {
            let (axiom, rules, angle) = preset(t);
            let text = expand(axiom, &parse_rules(rules), s.frac_depth);
            turtle(&text, angle, out);
        }
    }
    fit_box(s, out);
}

/// The axiom, rules and angle a preset uses, for filling the custom
/// fields from it.
pub fn preset_text(t: FractalType) -> (String, String, f64) {
    let (a, r, ang) = preset(t);
    (a.to_string(), r.to_string(), ang)
}

// ----- tilings --------------------------------------------------------------

pub(crate) fn tiling(s: &ShapeGenSettings, out: &mut Generated) {
    let rect = Rect::new(0.0, 0.0, s.width, s.height);
    out.frame = Some(rect);
    match s.tile_type {
        TilingType::Penrose => penrose(s, rect, out),
        TilingType::Star8 => stars(s, rect, 4, out),
        TilingType::Star6 => stars(s, rect, 6, out),
    }
}

/// Penrose rhombs: a wheel of half-rhomb triangles subdivided until the
/// edges are about the tile size; triangle pairs sharing their base are
/// joined into rhombs and cut to the box. Thin and thick rhombs take the
/// light and dark face.
fn penrose(s: &ShapeGenSettings, rect: Rect, out: &mut Generated) {
    for (color, q) in penrose_rhombs(rect, s.tile_size) {
        if out.full() {
            out.cut = true;
            return;
        }
        let pts = clip_polygon(rect, &q);
        if pts.len() < 3 {
            continue;
        }
        let paint = if s.tile_fill {
            Paint::Face(if color == 0 { 0 } else { 2 })
        } else {
            Paint::Line
        };
        out.push(polygon(&pts), true, paint);
    }
}

/// The rhombs of a Penrose tiling covering `rect` with edges about
/// `size` long: thin (0) or thick (1), corners in order. Rhombs reaching
/// past the box are whole.
pub(crate) fn penrose_rhombs(rect: Rect, size: f64) -> Vec<(u8, [Point; 4])> {
    let phi = (1.0 + 5f64.sqrt()) / 2.0;
    let c = rect.center();
    let radius = rect.width().hypot(rect.height()) / 2.0 * 1.05;
    // Each subdivision divides the edge by phi.
    let depth = ((radius / size.max(1e-9)).ln() / phi.ln())
        .ceil()
        .clamp(1.0, 11.0) as u32;
    let mut tris: Vec<(u8, Point, Point, Point)> = Vec::new();
    for i in 0..10 {
        let a0 = (2 * i as i32 - 1) as f64 * std::f64::consts::PI / 10.0;
        let a1 = (2 * i as i32 + 1) as f64 * std::f64::consts::PI / 10.0;
        let mut b = c + Vec2::new(a0.cos(), a0.sin()) * radius;
        let mut cc = c + Vec2::new(a1.cos(), a1.sin()) * radius;
        if i % 2 == 0 {
            std::mem::swap(&mut b, &mut cc);
        }
        tris.push((0, c, b, cc));
    }
    for _ in 0..depth {
        let mut next = Vec::with_capacity(tris.len() * 3);
        for (color, a, b, cc) in tris {
            if color == 0 {
                let p = a + (b - a) / phi;
                next.push((0, cc, p, b));
                next.push((1, p, cc, a));
            } else {
                let q = b + (a - b) / phi;
                let r = b + (cc - b) / phi;
                next.push((1, r, cc, a));
                next.push((1, q, r, b));
                next.push((0, r, q, a));
            }
        }
        tris = next;
        if tris.len() > 400_000 {
            break;
        }
    }
    // Join the halves: same colour, same base (b, c).
    let key = |p: Point, q: Point| {
        let r = |v: f64| (v * 1e4).round() as i64;
        let (a, b) = ((r(p.x), r(p.y)), (r(q.x), r(q.y)));
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };
    let mut halves: std::collections::HashMap<((i64, i64), (i64, i64), u8), Point> =
        std::collections::HashMap::new();
    let mut out = Vec::new();
    for (color, a, b, cc) in tris {
        let k = key(b, cc);
        match halves.remove(&(k.0, k.1, color)) {
            Some(other) => {
                if out.len() >= crate::shape_gen::MAX_ITEMS * 2 {
                    break;
                }
                out.push((color, [a, b, other, cc]));
            }
            None => {
                halves.insert((k.0, k.1, color), a);
            }
        }
    }
    out
}

/// A polygon cut to the box (Sutherland-Hodgman), so tiles on the edge
/// end at it.
fn clip_polygon(r: Rect, pts: &[Point]) -> Vec<Point> {
    let mut poly = pts.to_vec();
    // How far inside each side of the box a point is.
    let sides: [fn(Rect, Point) -> f64; 4] = [
        |r, p| p.x - r.x0,
        |r, p| r.x1 - p.x,
        |r, p| p.y - r.y0,
        |r, p| r.y1 - p.y,
    ];
    for dist in sides {
        if poly.is_empty() {
            break;
        }
        let mut next = Vec::with_capacity(poly.len() + 2);
        for k in 0..poly.len() {
            let (p, q) = (poly[k], poly[(k + 1) % poly.len()]);
            let (dp, dq) = (dist(r, p), dist(r, q));
            if dp >= 0.0 {
                next.push(p);
            }
            if (dp >= 0.0) != (dq >= 0.0) {
                next.push(p + (q - p) * (dp / (dp - dq)));
            }
        }
        poly = next;
    }
    poly
}

/// Clip the segment to the box (Liang-Barsky); None when outside.
pub(crate) fn clip_segment(r: Rect, p: Point, q: Point) -> Option<(Point, Point)> {
    let d = q - p;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (pv, qv) in [
        (-d.x, p.x - r.x0),
        (d.x, r.x1 - p.x),
        (-d.y, p.y - r.y0),
        (d.y, r.y1 - p.y),
    ] {
        if pv.abs() < 1e-12 {
            if qv < 0.0 {
                return None;
            }
        } else {
            let t = qv / pv;
            if pv < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 < t1).then(|| (p + d * t0, p + d * t1))
}

fn ray_meet(p: Point, u: Vec2, q: Point, v: Vec2) -> Option<Point> {
    let den = u.x * v.y - u.y * v.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let w = q - p;
    let t = (w.x * v.y - w.y * v.x) / den;
    Some(p + u * t)
}

/// Star patterns: on a grid of squares (`sides` 4) or hexagons (6) of the
/// tile size, two rays leave every edge's middle at the contact angle and
/// stop where they meet the rays of the neighbouring edges.
fn stars(s: &ShapeGenSettings, rect: Rect, sides: usize, out: &mut Generated) {
    let t = s.tile_size;
    let theta = s.tile_angle.to_radians();
    let turn = s.tile_rotation.to_radians();
    let c = rect.center();
    let rot = |p: Point| {
        let v = p - c;
        let (sn, cs) = turn.sin_cos();
        c + Vec2::new(v.x * cs - v.y * sn, v.x * sn + v.y * cs)
    };
    let reach = rect.width().hypot(rect.height()) / 2.0 + 2.0 * t;
    // Cell middles and corners.
    let mut cells: Vec<Vec<Point>> = Vec::new();
    if sides == 4 {
        let n = (reach / t).ceil() as i64;
        for j in -n..=n {
            for i in -n..=n {
                let m = Point::new(c.x + i as f64 * t, c.y + j as f64 * t);
                let h = t / 2.0;
                cells.push(vec![
                    Point::new(m.x - h, m.y - h),
                    Point::new(m.x + h, m.y - h),
                    Point::new(m.x + h, m.y + h),
                    Point::new(m.x - h, m.y + h),
                ]);
            }
        }
    } else {
        // Pointy-top hexagons whose edge is the tile size.
        let r = t;
        let (dx, dy) = (3f64.sqrt() * r, 1.5 * r);
        let (ni, nj) = (
            (reach / dx).ceil() as i64 + 1,
            (reach / dy).ceil() as i64 + 1,
        );
        for j in -nj..=nj {
            for i in -ni..=ni {
                let shift = if j.rem_euclid(2) == 1 { dx / 2.0 } else { 0.0 };
                let m = Point::new(c.x + i as f64 * dx + shift, c.y + j as f64 * dy);
                cells.push(
                    (0..6)
                        .map(|k| {
                            let a = std::f64::consts::FRAC_PI_6
                                + k as f64 * std::f64::consts::FRAC_PI_3;
                            Point::new(m.x + r * a.cos(), m.y + r * a.sin())
                        })
                        .collect(),
                );
            }
        }
    }
    for v in cells {
        let n = v.len();
        for k in 0..n {
            if out.full() {
                out.cut = true;
                return;
            }
            let (a, b, cc) = (v[k], v[(k + 1) % n], v[(k + 2) % n]);
            let (m1, m2) = (a.midpoint(b), b.midpoint(cc));
            // Inward normals (corners run counterclockwise).
            let e1 = (b - a).normalize();
            let e2 = (cc - b).normalize();
            let (n1, n2) = (Vec2::new(-e1.y, e1.x), Vec2::new(-e2.y, e2.x));
            // From edge k toward their shared corner b, and from edge k+1
            // toward it too.
            let u = e1 * theta.cos() + n1 * theta.sin();
            let w = -e2 * theta.cos() + n2 * theta.sin();
            let Some(x) = ray_meet(m1, u, m2, w) else {
                continue;
            };
            for (from, to) in [(m1, x), (m2, x)] {
                if let Some((p, q)) = clip_segment(rect, rot(from), rot(to)) {
                    let mut seg = BezPath::new();
                    seg.move_to(p);
                    seg.line_to(q);
                    out.push(seg, false, Paint::Line);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_gen::{generate, GenKind};

    fn fits(g: &Generated, s: &ShapeGenSettings) {
        let b = g.bounds().expect("bounds");
        assert!(b.x0 > -1e-6 && b.y0 > -1e-6, "{b:?}");
        assert!(b.x1 < s.width + 1e-6 && b.y1 < s.height + 1e-6, "{b:?}");
    }

    #[test]
    fn every_curve_kind_draws_inside_the_box() {
        for st in SpiroType::ALL {
            let s = ShapeGenSettings {
                kind: GenKind::Spirograph,
                spiro_type: st,
                ..ShapeGenSettings::default()
            };
            let g = generate(&s);
            assert!(!g.items.is_empty(), "{st:?}");
            fits(&g, &s);
        }
        for ft in FractalType::ALL {
            let s = ShapeGenSettings {
                kind: GenKind::Fractal,
                frac_type: ft,
                ..ShapeGenSettings::default()
            };
            let g = generate(&s);
            assert!(!g.items.is_empty(), "{ft:?}");
            fits(&g, &s);
        }
        for tt in TilingType::ALL {
            let s = ShapeGenSettings {
                kind: GenKind::Tiling,
                tile_type: tt,
                ..ShapeGenSettings::default()
            };
            let g = generate(&s);
            assert!(g.items.len() > 20, "{tt:?}");
            fits(&g, &s);
        }
        let s = ShapeGenSettings {
            kind: GenKind::Guilloche,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        assert_eq!(g.items.len(), s.guil_lines as usize);
    }

    #[test]
    fn guilloche_bands_and_frames_stay_in_the_box() {
        for layout in [GuilLayout::Band, GuilLayout::Frame] {
            let s = ShapeGenSettings {
                kind: GenKind::Guilloche,
                guil_layout: layout,
                width: 160.0,
                height: 60.0,
                ..ShapeGenSettings::default()
            };
            let g = generate(&s);
            assert_eq!(g.items.len(), s.guil_lines as usize, "{layout:?}");
            fits(&g, &s);
            let b = g.bounds().expect("bounds");
            // It spans the box.
            assert!(b.width() > 150.0, "{layout:?} {b:?}");
            if layout == GuilLayout::Frame {
                // The middle stays empty: every line is within the band.
                let hole = Rect::new(
                    s.guil_band + 0.5,
                    s.guil_band + 0.5,
                    s.width - s.guil_band - 0.5,
                    s.height - s.guil_band - 0.5,
                );
                for it in &g.items {
                    for el in it.path.elements() {
                        if let Some(p) = el.end_point() {
                            assert!(!hole.contains(p), "{p:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_spirograph_closes_after_its_whole_turns() {
        let s = ShapeGenSettings {
            kind: GenKind::Spirograph,
            spiro_fixed: 40.0,
            spiro_rolling: 15.0,
            spiro_pen: 10.0,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        assert_eq!(g.items.len(), 1);
        assert!(g.items[0].closed);
        // 40 and 15: three turns of the small wheel's centre, eight petals.
        let n = g.items[0].path.elements().len();
        assert!(n > 100, "{n}");
    }

    #[test]
    fn koch_grows_four_times_per_level() {
        let count = |depth: u32| {
            let (a, r, _) = preset(FractalType::Koch);
            expand(a, &parse_rules(r), depth)
                .chars()
                .filter(|c| *c == 'F')
                .count()
        };
        assert_eq!(count(0), 3);
        assert_eq!(count(1), 12);
        assert_eq!(count(3), 3 * 64);
    }

    #[test]
    fn odd_rules_and_deep_levels_never_run_away() {
        let s = ShapeGenSettings {
            kind: GenKind::Fractal,
            frac_type: FractalType::Custom,
            frac_axiom: "F[[[".into(),
            frac_rules: "FF=; =x;F=FFFFFFFF;]=".into(),
            frac_depth: 12,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        assert!(!g.items.is_empty());
        assert!(expand("F", &parse_rules("F=FFFF"), 40).len() <= MAX_SYMBOLS);
    }

    #[test]
    fn penrose_has_thick_and_thin_rhombs_in_the_golden_ratio() {
        let s = ShapeGenSettings {
            kind: GenKind::Tiling,
            tile_type: TilingType::Penrose,
            tile_fill: true,
            tile_size: 4.0,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        let thin = g.items.iter().filter(|i| i.paint == Paint::Face(0)).count() as f64;
        let thick = g.items.iter().filter(|i| i.paint == Paint::Face(2)).count() as f64;
        assert!(thin > 50.0, "{thin}");
        let phi = (1.0 + 5f64.sqrt()) / 2.0;
        assert!((thick / thin - phi).abs() < 0.2, "{thick} {thin}");
        // Every piece away from the box's edge is a rhomb: four equal sides.
        let inner = Rect::new(5.0, 5.0, 95.0, 95.0);
        for it in g
            .items
            .iter()
            .filter(|i| {
                use tracedraw_core::geometry::Shape as _;
                inner.contains_rect(i.path.bounding_box())
            })
            .take(50)
        {
            let pts: Vec<Point> = it
                .path
                .elements()
                .iter()
                .filter_map(|e| e.end_point())
                .collect();
            let side = |a: Point, b: Point| (a - b).hypot();
            let l0 = side(pts[0], pts[1]);
            for k in 1..4 {
                assert!((side(pts[k], pts[(k + 1) % 4]) - l0).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn star_rays_meet_in_pairs() {
        let s = ShapeGenSettings {
            kind: GenKind::Tiling,
            tile_type: TilingType::Star8,
            tile_size: 20.0,
            tile_angle: 67.5,
            ..ShapeGenSettings::default()
        };
        let g = generate(&s);
        // Every segment starts on a cell edge's middle (a multiple of
        // half the tile from the middle) or at the box.
        assert!(g.items.len() > 50);
        for it in &g.items {
            assert!(!it.closed);
        }
    }
}

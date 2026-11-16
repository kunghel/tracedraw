//! The letters of the glyphs of a Devanagari font a PDF embeds, read off
//! their own outlines (#1806).
//!
//! A word processor's Hindi PDF names its glyphs' characters in a
//! `ToUnicode` map that is often wrong for conjuncts, half forms and
//! joined signs, and that holds some glyphs' characters nowhere (see
//! `pdf_import::unscramble_tounicode`). The embedded font is a subset
//! without the tables that say which letters make a glyph, and its
//! `cmap` names only the glyphs of single characters. But the glyphs the
//! `cmap` does not name are drawn with the same outlines as the ones it
//! names, and a font's `glyf` table keeps those outlines point by point:
//!
//! - a glyph for a letter with a sign (कु, कू), a letter with its virama
//!   shown (न्) or signs joined over a letter (ें, ों, ीं) is the letter's
//!   contours and the sign's, each set moved as a whole: the glyph stands
//!   for those characters;
//! - a half form (श्) is the letter without its stem: all of its points
//!   are points of the letter's outline, with fewer of them and a shorter
//!   advance: the glyph stands for the letter and the virama;
//! - the short i sign is drawn in several lengths, one for each width of
//!   the cluster it reaches over: the same hook with a longer arm, the
//!   same advance and the same foot: each such glyph stands for the sign.
//!
//! Only an exact match of outlines counts. A glyph that matches nothing,
//! or that two readings fit, is left to the rules of the map.

use std::collections::{HashMap, HashSet};
use ttf_parser::{Face, GlyphId};

const VIRAMA: char = '\u{094D}';
const NUKTA: char = '\u{093C}';
const SIGN_I: char = '\u{093F}';
const SIGN_AA: char = '\u{093E}';
const SIGN_E: char = '\u{0947}';
const SIGN_AI: char = '\u{0948}';
const SIGN_O: char = '\u{094B}';
const SIGN_AU: char = '\u{094C}';

/// A consonant (with the nukta forms).
fn consonant(c: char) -> bool {
    matches!(c, '\u{0915}'..='\u{0939}' | '\u{0958}'..='\u{095F}')
}

/// An independent vowel.
fn vowel(c: char) -> bool {
    ('\u{0904}'..='\u{0914}').contains(&c)
}

/// A vowel sign (not the short i sign, which is drawn before its
/// letter).
fn vowel_sign(c: char) -> bool {
    matches!(
        c,
        '\u{093E}' | '\u{0940}'..='\u{094C}' | '\u{0962}' | '\u{0963}'
    )
}

/// Candrabindu, anusvara, visarga.
fn nasal_sign(c: char) -> bool {
    ('\u{0901}'..='\u{0903}').contains(&c)
}

/// A character a glyph of the font can be made of.
fn part(c: char) -> bool {
    consonant(c)
        || vowel(c)
        || vowel_sign(c)
        || nasal_sign(c)
        || matches!(c, SIGN_I | VIRAMA | NUKTA)
}

/// A contour's shape and where it starts (see [`signature`]).
type Shape = (Vec<(i32, i32)>, (i32, i32));
/// Contour shape to the named glyphs having it (glyph index, contour
/// index).
type ByShape<'a> = HashMap<&'a [(i32, i32)], Vec<(usize, usize)>>;

/// A glyph's outline: its contours' points (on and off the curve, as
/// the font's `glyf` table lists them, in half units) and its metrics
/// (in font units).
struct Outline {
    contours: Vec<Vec<(i32, i32)>>,
    advance: i32,
    x_min: i32,
    y_min: i32,
    y_max: i32,
}

impl Outline {
    fn points(&self) -> usize {
        self.contours.iter().map(Vec::len).sum()
    }
}

#[derive(Default)]
struct Points {
    contours: Vec<Vec<(i32, i32)>>,
    current: Vec<(i32, i32)>,
}

impl Points {
    /// Points are kept in half units: the point between two off-curve
    /// points, which the outline reader puts in, then lands on a whole
    /// number wherever the contour lies.
    fn push(&mut self, x: f32, y: f32) {
        self.current
            .push(((x * 2.0).round() as i32, (y * 2.0).round() as i32));
    }
    fn flush(&mut self) {
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
    }
}

impl ttf_parser::OutlineBuilder for Points {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        self.push(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.push(x1, y1);
        self.push(x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.push(x1, y1);
        self.push(x2, y2);
        self.push(x, y);
    }
    fn close(&mut self) {
        // The closing point repeats the first one.
        if self.current.len() > 1 && self.current.last() == self.current.first() {
            self.current.pop();
        }
        self.flush();
    }
}

fn outline(face: &Face, gid: u16) -> Option<Outline> {
    let id = GlyphId(gid);
    let mut points = Points::default();
    let bbox = face.outline_glyph(id, &mut points)?;
    points.flush();
    if points.contours.is_empty() {
        return None;
    }
    Some(Outline {
        contours: points.contours,
        advance: i32::from(face.glyph_hor_advance(id).unwrap_or(0)),
        x_min: i32::from(bbox.x_min),
        y_min: i32::from(bbox.y_min),
        y_max: i32::from(bbox.y_max),
    })
}

/// A contour's shape apart from where it lies: its point to point steps,
/// turned to start at the smallest, with the point that start is at.
fn signature(contour: &[(i32, i32)]) -> Shape {
    let n = contour.len();
    let steps: Vec<(i32, i32)> = (0..n)
        .map(|i| {
            let (a, b) = (contour[i], contour[(i + 1) % n]);
            (b.0 - a.0, b.1 - a.1)
        })
        .collect();
    let mut best = 0;
    for start in 1..n {
        let less = (0..n).find_map(|k| {
            let (a, b) = (steps[(start + k) % n], steps[(best + k) % n]);
            (a != b).then_some(a < b)
        });
        if less == Some(true) {
            best = start;
        }
    }
    let rotated: Vec<(i32, i32)> = (0..n).map(|k| steps[(best + k) % n]).collect();
    (rotated, contour[best])
}

/// The longest run of steps the two contours share, each taken round.
fn shared_run(a: &[(i32, i32)], b: &[(i32, i32)]) -> usize {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return 0;
    }
    let steps = |c: &[(i32, i32)]| -> Vec<(i32, i32)> {
        let l = c.len();
        (0..l)
            .map(|i| (c[(i + 1) % l].0 - c[i].0, c[(i + 1) % l].1 - c[i].1))
            .collect()
    };
    let (sa, sb) = (steps(a), steps(b));
    let mut best = 0;
    let mut previous = vec![0usize; 2 * m + 1];
    for i in 1..=2 * n {
        let mut current = vec![0usize; 2 * m + 1];
        for j in 1..=2 * m {
            if sa[(i - 1) % n] == sb[(j - 1) % m] {
                current[j] = previous[j - 1] + 1;
                best = best.max(current[j]);
            }
        }
        previous = current;
    }
    best.min(n).min(m)
}

/// A glyph the font's `cmap` names, with its outline.
struct Named {
    ch: char,
    outline: Outline,
    /// The signature and start of each contour.
    shapes: Vec<Shape>,
}

/// The characters the font's own `cmap` gives its glyphs, for the
/// Devanagari letters and signs.
fn named_glyphs(face: &Face) -> Vec<(u16, Named)> {
    let mut chars: HashMap<u16, char> = HashMap::new();
    if let Some(cmap) = face.tables().cmap {
        for st in cmap.subtables {
            if !st.is_unicode() {
                continue;
            }
            st.codepoints(|cp| {
                if let (Some(c), Some(g)) = (char::from_u32(cp), st.glyph_index(cp)) {
                    if g.0 != 0 && part(c) {
                        chars.entry(g.0).or_insert(c);
                    }
                }
            });
        }
    }
    let mut out: Vec<(u16, Named)> = chars
        .into_iter()
        .filter_map(|(gid, ch)| {
            let outline = outline(face, gid)?;
            let shapes = outline.contours.iter().map(|c| signature(c)).collect();
            Some((
                gid,
                Named {
                    ch,
                    outline,
                    shapes,
                },
            ))
        })
        .collect();
    out.sort_by_key(|(gid, _)| *gid);
    out
}

/// The texts of the glyphs `used` of the font `face` that its `cmap`
/// does not name, as far as their outlines tell (see the module notes).
pub(crate) fn glyph_texts(face: &Face, used: &[u16]) -> HashMap<u16, String> {
    let named = named_glyphs(face);
    let known: HashSet<u16> = named.iter().map(|(g, _)| *g).collect();
    let mut by_shape: ByShape = HashMap::new();
    for (n, (_, named)) in named.iter().enumerate() {
        for (k, (shape, _)) in named.shapes.iter().enumerate() {
            by_shape.entry(shape.as_slice()).or_default().push((n, k));
        }
    }
    let sign_i = named.iter().find(|(_, n)| n.ch == SIGN_I).map(|(_, n)| n);
    let mut out = HashMap::new();
    let mut seen = HashSet::new();
    for &gid in used {
        if gid == 0 || known.contains(&gid) || !seen.insert(gid) {
            continue;
        }
        let Some(glyph) = outline(face, gid) else {
            continue;
        };
        let text = composed(&glyph, &named, &by_shape)
            .or_else(|| half_form(&glyph, &named))
            .or_else(|| sign_i.and_then(|i| short_i(&glyph, i)));
        if let Some(text) = text {
            out.insert(gid, text);
        }
    }
    out
}

/// The glyph is named glyphs' outlines put together, each moved as a
/// whole: their characters, when they make one text.
fn composed(glyph: &Outline, named: &[(u16, Named)], by_shape: &ByShape) -> Option<String> {
    let shapes: Vec<Shape> = glyph.contours.iter().map(|c| signature(c)).collect();
    let mut covers: Vec<Vec<usize>> = Vec::new();
    let mut taken = vec![false; shapes.len()];
    cover(
        &shapes,
        named,
        by_shape,
        &mut taken,
        &mut Vec::new(),
        &mut covers,
    );
    covers.iter_mut().for_each(|c| c.sort_unstable());
    covers.dedup();
    let [parts] = covers.as_slice() else {
        return None;
    };
    let chars: Vec<char> = parts.iter().map(|&n| named[n].1.ch).collect();
    text_of(&chars)
}

/// Every way the contours not `taken` can be covered by named glyphs
/// whose contours all appear, moved by one offset; `picked` so far.
fn cover(
    shapes: &[Shape],
    named: &[(u16, Named)],
    by_shape: &ByShape,
    taken: &mut Vec<bool>,
    picked: &mut Vec<usize>,
    covers: &mut Vec<Vec<usize>>,
) {
    if covers.len() > 8 || picked.len() > 4 {
        return;
    }
    let Some(first) = taken.iter().position(|t| !t) else {
        covers.push(picked.clone());
        return;
    };
    let (shape, start) = &shapes[first];
    let Some(candidates) = by_shape.get(shape.as_slice()) else {
        return;
    };
    for &(n, k) in candidates {
        let component = &named[n].1;
        let (_, origin) = &component.shapes[k];
        let offset = (start.0 - origin.0, start.1 - origin.1);
        // Each contour of the component at the same offset, on a contour
        // not taken yet.
        let mut used: Vec<usize> = Vec::new();
        let complete = component.shapes.iter().all(|(s, o)| {
            let at = (o.0 + offset.0, o.1 + offset.1);
            match shapes.iter().enumerate().find(|(i, (gs, gstart))| {
                !taken[*i] && !used.contains(i) && gs == s && *gstart == at
            }) {
                Some((i, _)) => {
                    used.push(i);
                    true
                }
                None => false,
            }
        });
        if !complete || !used.contains(&first) {
            continue;
        }
        for &i in &used {
            taken[i] = true;
        }
        picked.push(n);
        cover(shapes, named, by_shape, taken, picked, covers);
        picked.pop();
        for &i in &used {
            taken[i] = false;
        }
    }
}

/// The text the characters of a glyph's parts make, in typed order: a
/// letter, its nukta, its virama or vowel sign, a nasal sign; or signs
/// alone (the short i sign first, as it is drawn).
fn text_of(chars: &[char]) -> Option<String> {
    let letters: Vec<char> = chars
        .iter()
        .copied()
        .filter(|c| consonant(*c) || vowel(*c))
        .collect();
    let signs: Vec<char> = chars
        .iter()
        .copied()
        .filter(|c| !consonant(*c) && !vowel(*c))
        .collect();
    if letters.len() > 1 || chars.is_empty() {
        return None;
    }
    let count = |p: fn(char) -> bool| signs.iter().filter(|c| p(**c)).count();
    let nuktas = count(|c| c == NUKTA);
    let viramas = count(|c| c == VIRAMA);
    let vowels = count(vowel_sign);
    let short_i = count(|c| c == SIGN_I);
    let nasals = count(nasal_sign);
    if nuktas > 1 || viramas > 1 || nasals > 1 || short_i > 1 {
        return None;
    }
    let mut vowel_signs: Vec<char> = signs.iter().copied().filter(|c| vowel_sign(*c)).collect();
    // The o and au signs are often the aa sign and the e or ai sign.
    if vowels == 2 {
        vowel_signs.sort_unstable();
        vowel_signs = match vowel_signs.as_slice() {
            [SIGN_AA, SIGN_E] => vec![SIGN_O],
            [SIGN_AA, SIGN_AI] => vec![SIGN_AU],
            _ => return None,
        };
    } else if vowels > 2 {
        return None;
    }
    let mut text = String::new();
    match letters.first() {
        Some(&l) => {
            if vowel(l) && (nuktas + viramas + short_i > 0) {
                return None;
            }
            if viramas > 0 && (vowel_signs.len() + short_i > 0) {
                return None;
            }
            text.push(l);
            if nuktas > 0 {
                text.push(NUKTA);
            }
            if viramas > 0 {
                text.push(VIRAMA);
            }
            text.extend(vowel_signs);
            if short_i > 0 {
                text.push(SIGN_I);
            }
        }
        None => {
            if signs.len() < 2 || nuktas + viramas > 0 || vowel_signs.len() + short_i > 1 {
                return None;
            }
            if short_i > 0 {
                text.push(SIGN_I);
            }
            text.extend(vowel_signs);
        }
    }
    text.extend(signs.iter().copied().filter(|c| nasal_sign(*c)));
    Some(text)
}

/// The glyph is a consonant's outline with points left out and a shorter
/// advance: its half form.
fn half_form(glyph: &Outline, named: &[(u16, Named)]) -> Option<String> {
    let points: HashSet<(i32, i32)> = glyph.contours.iter().flatten().copied().collect();
    if points.len() < 8 {
        return None;
    }
    let mut found: Option<char> = None;
    for (_, n) in named {
        if !consonant(n.ch)
            || n.outline.points() <= glyph.points()
            || n.outline.advance <= glyph.advance
        {
            continue;
        }
        let theirs: HashSet<(i32, i32)> = n.outline.contours.iter().flatten().copied().collect();
        let inside = points.iter().filter(|p| theirs.contains(p)).count();
        if inside * 10 >= points.len() * 9 {
            if found.is_some() {
                return None;
            }
            found = Some(n.ch);
        }
    }
    found.map(|c| format!("{c}{VIRAMA}"))
}

/// The glyph is the short i sign with a longer arm: the same advance,
/// the same foot, and a good part of the hook's outline.
fn short_i(glyph: &Outline, sign: &Named) -> Option<String> {
    let i = &sign.outline;
    if glyph.advance != i.advance
        || (glyph.y_min - i.y_min).abs() > 2
        || (glyph.x_min - i.x_min).abs() > 2
        || glyph.y_max < i.y_max - 20
    {
        return None;
    }
    let needed = (i.points() / 4).max(6);
    let run = glyph
        .contours
        .iter()
        .flat_map(|g| i.contours.iter().map(move |c| shared_run(g, c)))
        .max()
        .unwrap_or(0);
    (run >= needed).then(|| SIGN_I.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A glyph of a small TrueType font: its character, advance and
    /// contours (points on the curve).
    pub(crate) struct TestGlyph {
        pub ch: Option<char>,
        pub advance: u16,
        pub contours: Vec<Vec<(i16, i16)>>,
    }

    /// A TrueType font with the glyphs given (glyph 0 first), as far as
    /// the tables the importer reads.
    pub(crate) fn tiny_font(glyphs: &[TestGlyph]) -> Vec<u8> {
        fn be16(v: &mut Vec<u8>, x: u16) {
            v.extend_from_slice(&x.to_be_bytes());
        }
        fn be32(v: &mut Vec<u8>, x: u32) {
            v.extend_from_slice(&x.to_be_bytes());
        }
        // glyf and loca.
        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        for g in glyphs {
            be32(&mut loca, glyf.len() as u32);
            if g.contours.is_empty() {
                continue;
            }
            let points: Vec<(i16, i16)> = g.contours.iter().flatten().copied().collect();
            let (x_min, x_max) = points
                .iter()
                .fold((i16::MAX, i16::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
            let (y_min, y_max) = points
                .iter()
                .fold((i16::MAX, i16::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
            be16(&mut glyf, g.contours.len() as u16);
            for v in [x_min, y_min, x_max, y_max] {
                be16(&mut glyf, v as u16);
            }
            let mut end = 0u16;
            for c in &g.contours {
                end += c.len() as u16;
                be16(&mut glyf, end - 1);
            }
            be16(&mut glyf, 0);
            glyf.extend(std::iter::repeat_n(1u8, points.len()));
            let mut last = 0i16;
            for p in &points {
                be16(&mut glyf, (p.0 - last) as u16);
                last = p.0;
            }
            last = 0;
            for p in &points {
                be16(&mut glyf, (p.1 - last) as u16);
                last = p.1;
            }
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        }
        be32(&mut loca, glyf.len() as u32);
        let mut head = Vec::new();
        be32(&mut head, 0x0001_0000);
        be32(&mut head, 0);
        be32(&mut head, 0);
        be32(&mut head, 0x5F0F_3CF5);
        be16(&mut head, 0);
        be16(&mut head, 1000);
        head.extend_from_slice(&[0; 16]);
        for v in [0i16, -300, 1000, 1000] {
            be16(&mut head, v as u16);
        }
        be16(&mut head, 0);
        be16(&mut head, 8);
        be16(&mut head, 2);
        be16(&mut head, 1);
        be16(&mut head, 0);
        let mut hhea = Vec::new();
        be32(&mut hhea, 0x0001_0000);
        for v in [800i16, -200, 0, 1000, 0, 0, 1000, 1, 0, 0] {
            be16(&mut hhea, v as u16);
        }
        hhea.extend_from_slice(&[0; 8]);
        be16(&mut hhea, 0);
        be16(&mut hhea, glyphs.len() as u16);
        let mut maxp = Vec::new();
        be32(&mut maxp, 0x0000_5000);
        be16(&mut maxp, glyphs.len() as u16);
        let mut hmtx = Vec::new();
        for g in glyphs {
            be16(&mut hmtx, g.advance);
            be16(&mut hmtx, 0);
        }
        // cmap format 4, one segment per character.
        let mut chars: Vec<(u16, u16)> = glyphs
            .iter()
            .enumerate()
            .filter_map(|(i, g)| g.ch.map(|c| (c as u16, i as u16)))
            .collect();
        chars.sort_unstable();
        chars.push((0xFFFF, 0));
        let segs = chars.len() as u16;
        let mut sub = Vec::new();
        be16(&mut sub, 4);
        be16(&mut sub, 16 + 8 * segs);
        be16(&mut sub, 0);
        be16(&mut sub, segs * 2);
        be16(&mut sub, 0);
        be16(&mut sub, 0);
        be16(&mut sub, 0);
        for (c, _) in &chars {
            be16(&mut sub, *c);
        }
        be16(&mut sub, 0);
        for (c, _) in &chars {
            be16(&mut sub, *c);
        }
        for (c, g) in &chars {
            be16(&mut sub, g.wrapping_sub(*c));
        }
        for _ in &chars {
            be16(&mut sub, 0);
        }
        let mut cmap = Vec::new();
        be16(&mut cmap, 0);
        be16(&mut cmap, 1);
        be16(&mut cmap, 3);
        be16(&mut cmap, 1);
        be32(&mut cmap, 12);
        cmap.extend(sub);
        let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"cmap", cmap),
            (b"glyf", glyf),
            (b"head", head),
            (b"hhea", hhea),
            (b"hmtx", hmtx),
            (b"loca", loca),
            (b"maxp", maxp),
        ];
        let mut out = Vec::new();
        be32(&mut out, 0x0001_0000);
        be16(&mut out, tables.len() as u16);
        be16(&mut out, 0);
        be16(&mut out, 0);
        be16(&mut out, 0);
        let mut offset = 12 + 16 * tables.len();
        let mut body = Vec::new();
        for (tag, data) in &tables {
            out.extend_from_slice(*tag);
            be32(&mut out, 0);
            be32(&mut out, offset as u32);
            be32(&mut out, data.len() as u32);
            body.extend_from_slice(data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
            offset = 12 + 16 * tables.len() + body.len();
        }
        out.extend(body);
        out
    }

    fn shifted(c: &[(i16, i16)], dx: i16, dy: i16) -> Vec<(i16, i16)> {
        c.iter().map(|p| (p.0 + dx, p.1 + dy)).collect()
    }

    /// The glyphs of the test font: the letters क and श, the signs ू, ि
    /// and ं, and glyphs the cmap does not name made from them.
    pub(crate) fn deva_glyphs() -> Vec<TestGlyph> {
        // क: a body and a stem; the stem is the last contour.
        let ka_body = vec![
            (40, 0),
            (40, 100),
            (120, 180),
            (200, 240),
            (260, 320),
            (300, 400),
            (340, 480),
            (420, 500),
            (500, 480),
            (560, 400),
            (600, 300),
            (560, 200),
            (480, 140),
            (400, 120),
            (320, 160),
            (240, 100),
            (160, 40),
            (100, 0),
        ];
        let ka_stem = vec![(500, 0), (500, 516), (560, 516), (560, 0)];
        let sha = vec![
            (30, 0),
            (30, 120),
            (90, 200),
            (170, 260),
            (250, 300),
            (330, 360),
            (410, 420),
            (470, 516),
            (530, 516),
            (530, 0),
            (470, 0),
            (470, 380),
            (390, 320),
            (310, 260),
            (230, 220),
            (150, 180),
            (90, 100),
        ];
        let uu = vec![
            (0, -200),
            (-60, -140),
            (-180, -120),
            (-120, -60),
            (0, -40),
            (90, -80),
        ];
        let i_hook = vec![
            (0, 0),
            (0, 600),
            (60, 700),
            (160, 741),
            (300, 741),
            (400, 700),
            (380, 640),
            (300, 680),
            (180, 680),
            (100, 640),
            (60, 560),
            (60, 0),
        ];
        let dot = vec![(0, 620), (0, 680), (60, 680), (60, 620)];
        let hook_long: Vec<(i16, i16)> = {
            // The same hook, its arm a cluster longer.
            let mut v = i_hook.clone();
            v[4] = (600, 741);
            v[5] = (700, 700);
            v[6] = (680, 640);
            v[7] = (600, 680);
            v
        };
        vec![
            TestGlyph {
                ch: None,
                advance: 500,
                contours: vec![],
            },
            TestGlyph {
                ch: Some('क'),
                advance: 637,
                contours: vec![ka_body.clone(), ka_stem.clone()],
            },
            TestGlyph {
                ch: Some('श'),
                advance: 560,
                contours: vec![sha.clone()],
            },
            TestGlyph {
                ch: Some('ू'),
                advance: 0,
                contours: vec![uu.clone()],
            },
            TestGlyph {
                ch: Some('ि'),
                advance: 216,
                contours: vec![i_hook.clone()],
            },
            TestGlyph {
                ch: Some('ं'),
                advance: 0,
                contours: vec![dot.clone()],
            },
            // 6: कू, the letter and the sign under it.
            TestGlyph {
                ch: None,
                advance: 637,
                contours: vec![shifted(&uu, 300, -10), ka_body.clone(), ka_stem.clone()],
            },
            // 7: the half form of श, its stem gone.
            TestGlyph {
                ch: None,
                advance: 371,
                contours: vec![sha[..8].to_vec()],
            },
            // 8: the short i sign with a longer arm.
            TestGlyph {
                ch: None,
                advance: 216,
                contours: vec![hook_long],
            },
            // 9: ूं, two signs.
            TestGlyph {
                ch: None,
                advance: 0,
                contours: vec![shifted(&dot, -200, 0), uu.clone()],
            },
            // 10: a body of its own: nothing to read.
            TestGlyph {
                ch: None,
                advance: 420,
                contours: vec![vec![(0, 0), (0, 516), (200, 516), (300, 300), (200, 0)]],
            },
            // 11: the stem alone, which is a contour of क but not its
            // half form.
            TestGlyph {
                ch: None,
                advance: 100,
                contours: vec![ka_stem],
            },
            // 12 on: letters and signs of their own for words (12 is the
            // half form of स, drawn unlike स).
            TestGlyph {
                ch: None,
                advance: 371,
                contours: vec![box_at(0, 0, 300, 516)],
            },
            TestGlyph {
                ch: Some('ल'),
                advance: 602,
                contours: vec![box_at(0, 0, 580, 516)],
            },
            TestGlyph {
                ch: Some('म'),
                advance: 472,
                contours: vec![box_at(0, 0, 450, 516)],
            },
            TestGlyph {
                ch: Some('ु'),
                advance: 0,
                contours: vec![box_at(-300, -200, -100, -40)],
            },
            TestGlyph {
                ch: Some('व'),
                advance: 445,
                contours: vec![box_at(0, 0, 420, 516)],
            },
            TestGlyph {
                ch: Some('ा'),
                advance: 215,
                contours: vec![box_at(0, 0, 60, 516)],
            },
            TestGlyph {
                ch: Some('स'),
                advance: 588,
                contours: vec![box_at(0, 0, 560, 516)],
            },
            TestGlyph {
                ch: Some('उ'),
                advance: 541,
                contours: vec![box_at(0, 0, 520, 516)],
            },
            TestGlyph {
                ch: Some('प'),
                advance: 445,
                contours: vec![box_at(0, 0, 430, 516)],
            },
            TestGlyph {
                ch: Some('य'),
                advance: 504,
                contours: vec![box_at(0, 0, 480, 516)],
            },
            TestGlyph {
                ch: Some('त'),
                advance: 495,
                contours: vec![box_at(0, 0, 470, 516)],
            },
        ]
    }

    /// A rectangle with a notch, so that no two are alike.
    fn box_at(x0: i16, y0: i16, x1: i16, y1: i16) -> Vec<(i16, i16)> {
        vec![
            (x0, y0),
            (x0, y1),
            (x1, y1),
            (x1, (y0 + y1) / 2 + (x1 - x0) / 7),
            (x1, y0),
        ]
    }

    #[test]
    fn glyphs_made_of_named_outlines_take_their_characters() {
        let data = tiny_font(&deva_glyphs());
        let face = Face::parse(&data, 0).expect("font");
        assert_eq!(face.number_of_glyphs(), 23);
        let texts = glyph_texts(&face, &[1, 2, 6, 7, 8, 9, 10, 11, 6]);
        let mut got: Vec<(u16, &str)> = texts.iter().map(|(g, t)| (*g, t.as_str())).collect();
        got.sort_unstable();
        assert_eq!(
            got,
            [
                (6, "\u{0915}\u{0942}"),
                (7, "\u{0936}\u{094D}"),
                (8, "\u{093F}"),
                (9, "\u{0942}\u{0902}"),
            ]
        );
    }

    #[test]
    fn a_contour_shape_is_the_same_wherever_it_lies_or_starts() {
        let c = [(0, 0), (0, 10), (10, 10), (10, 0)];
        let (a, start_a) = signature(&c);
        let moved: Vec<(i32, i32)> = c.iter().map(|p| (p.0 + 5, p.1 + 7)).collect();
        let (b, start_b) = signature(&moved);
        let turned = [(10, 10), (10, 0), (0, 0), (0, 10)];
        let (d, start_d) = signature(&turned);
        assert_eq!(a, b);
        assert_eq!(a, d);
        assert_eq!((start_b.0 - start_a.0, start_b.1 - start_a.1), (5, 7));
        assert_eq!(start_d, start_a);
        let other = [(0, 0), (0, 10), (12, 10), (12, 0)];
        assert_ne!(signature(&other).0, a);
        assert_eq!(shared_run(&c, &other), 1);
        assert_eq!(shared_run(&c, &turned), 4);
    }

    #[test]
    fn parts_make_a_text_in_typed_order_or_none() {
        assert_eq!(
            text_of(&['\u{0942}', '\u{0915}']).as_deref(),
            Some("\u{0915}\u{0942}")
        );
        assert_eq!(
            text_of(&['\u{0928}', '\u{094D}']).as_deref(),
            Some("\u{0928}\u{094D}")
        );
        assert_eq!(
            text_of(&['\u{0902}', '\u{0947}']).as_deref(),
            Some("\u{0947}\u{0902}")
        );
        assert_eq!(
            text_of(&['\u{0902}', '\u{093F}']).as_deref(),
            Some("\u{093F}\u{0902}")
        );
        assert_eq!(
            text_of(&['\u{0947}', '\u{093E}']).as_deref(),
            Some("\u{094B}")
        );
        assert_eq!(
            text_of(&['\u{0915}', '\u{093C}', '\u{0947}', '\u{0901}']).as_deref(),
            Some("\u{0915}\u{093C}\u{0947}\u{0901}")
        );
        // Two letters, a sign alone, a virama with a vowel sign, two
        // vowel signs that make nothing: not one text.
        assert_eq!(text_of(&['\u{0915}', '\u{0924}']), None);
        assert_eq!(text_of(&['\u{0942}']), None);
        assert_eq!(text_of(&['\u{0915}', '\u{094D}', '\u{0942}']), None);
        assert_eq!(text_of(&['\u{0940}', '\u{0942}']), None);
    }
}

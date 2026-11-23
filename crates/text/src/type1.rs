//! PostScript Type 1 fonts (`.pfb`, `.pfa`, with the Windows metrics file
//! `.pfm` beside them), as older Windows systems and CorelDRAW installs
//! kept many script and display fonts (#193).
//!
//! The text engine reads OpenType fonts only, so a Type 1 font is turned
//! into one in memory: its glyph programs are run once to get each
//! glyph's outline and width, and those are written out as an OpenType
//! font with CFF outlines, named as Windows names the font (the face
//! name of its `.pfm` file when there is one).
//!
//! Written from Adobe's published "Adobe Type 1 Font Format"
//! specification (file layout, eexec and charstring encryption,
//! charstring operators, flex and hint replacement other subroutines,
//! StandardEncoding), Adobe Technical Note #5178 for the `.pfm` header,
//! Adobe Technical Note #5176 for CFF and Type 2 charstrings, and the
//! OpenType specification for the tables written. See CLEANROOM.md.

use std::collections::HashMap;

/// A point of a glyph outline, in font units.
type Pt = (f64, f64);

/// One piece of a glyph contour.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Seg {
    Move(Pt),
    Line(Pt),
    Curve(Pt, Pt, Pt),
}

/// A glyph of a Type 1 font, as run.
#[derive(Debug, Clone, PartialEq)]
struct Glyph {
    name: String,
    advance: f64,
    path: Vec<Seg>,
}

/// What a Type 1 font holds that the OpenType font needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Type1Font {
    font_name: String,
    family: String,
    full_name: String,
    weight: String,
    italic_angle: f64,
    fixed_pitch: bool,
    underline_position: f64,
    underline_thickness: f64,
    units_per_em: u16,
    /// Code to glyph name, from the font's own encoding (StandardEncoding
    /// when it names that).
    encoding: Vec<(u8, String)>,
    standard_encoding: bool,
    glyphs: Vec<Glyph>,
}

/// The Windows metrics of a Type 1 font (its `.pfm` file).
#[derive(Debug, Clone, PartialEq)]
pub struct PfmInfo {
    /// The name Windows lists the font under.
    pub face: String,
    pub weight: u16,
    pub italic: bool,
    /// The Windows character set: 0 ANSI, 2 symbol, others national.
    pub charset: u8,
}

/// Most bytes of a Type 1 font read.
const MAX_FONT_BYTES: usize = 16 << 20;
/// Most glyphs taken from one font.
const MAX_GLYPHS: usize = 4096;
/// Most nested subroutine calls in one glyph.
const MAX_CALL_DEPTH: usize = 16;
/// Most operators run for one glyph (a damaged program never ends).
const MAX_OPS: usize = 100_000;

// ---------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------

/// The clear text and the decrypted private part of a `.pfb` (segments
/// marked 0x80) or `.pfa` (the private part in hex after `eexec`).
fn split_font(data: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    if data.len() > MAX_FONT_BYTES {
        return None;
    }
    if data.first() == Some(&0x80) {
        let mut clear = Vec::new();
        let mut binary = Vec::new();
        let mut at = 0usize;
        while at + 2 <= data.len() && data[at] == 0x80 {
            let kind = data[at + 1];
            if kind == 3 {
                break;
            }
            let len = u32::from_le_bytes(data.get(at + 2..at + 6)?.try_into().ok()?) as usize;
            let body = data.get(at + 6..(at + 6).checked_add(len)?)?;
            match kind {
                1 => {
                    if binary.is_empty() {
                        clear.extend_from_slice(body);
                    }
                }
                2 => binary.extend_from_slice(body),
                _ => return None,
            }
            at += 6 + len;
        }
        // Some files carry the private part as hex in an ASCII segment.
        if binary.is_empty() {
            return split_ascii(&clear);
        }
        let plain = decrypt(&binary, 55665, 4);
        return Some((clear, plain));
    }
    split_ascii(data)
}

/// A `.pfa`: clear text up to `eexec`, then the private part in hex (or
/// binary).
fn split_ascii(data: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let at = find(data, b"eexec")? + 5;
    let clear = data[..at].to_vec();
    let mut rest = &data[at..];
    while let Some((c, r)) = rest.split_first() {
        if c.is_ascii_whitespace() {
            rest = r;
        } else {
            break;
        }
    }
    let hex = rest.iter().take(4).all(|c| c.is_ascii_hexdigit());
    let cipher = if hex {
        let mut out = Vec::with_capacity(rest.len() / 2);
        let mut high: Option<u8> = None;
        for &c in rest {
            let v = match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                c if c.is_ascii_whitespace() => continue,
                _ => break,
            };
            match high.take() {
                None => high = Some(v),
                Some(h) => out.push(h << 4 | v),
            }
        }
        out
    } else {
        rest.to_vec()
    };
    Some((clear, decrypt(&cipher, 55665, 4)))
}

/// Type 1 decryption (eexec with `r` 55665, charstrings with 4330),
/// leaving out the first `skip` bytes.
fn decrypt(cipher: &[u8], mut r: u16, skip: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(cipher.len());
    for &c in cipher {
        out.push(c ^ (r >> 8) as u8);
        r = (c as u16)
            .wrapping_add(r)
            .wrapping_mul(52845)
            .wrapping_add(22719);
    }
    out.split_off(skip.min(out.len()))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------
// PostScript tokens
// ---------------------------------------------------------------------

/// A token reader over PostScript text, with the binary blocks that
/// follow `RD` / `-|` read as bytes.
struct Tokens<'a> {
    data: &'a [u8],
    at: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok<'a> {
    Name(&'a [u8]),
    Word(&'a [u8]),
    Str(Vec<u8>),
    Num(f64),
    Open,
    Close,
}

impl<'a> Tokens<'a> {
    fn new(data: &'a [u8]) -> Self {
        Tokens { data, at: 0 }
    }

    fn delimiter(c: u8) -> bool {
        c.is_ascii_whitespace() || b"()<>[]{}/%".contains(&c)
    }

    fn next(&mut self) -> Option<Tok<'a>> {
        let d = self.data;
        loop {
            let c = *d.get(self.at)?;
            if c.is_ascii_whitespace() {
                self.at += 1;
            } else if c == b'%' {
                while self.at < d.len() && d[self.at] != b'\n' && d[self.at] != b'\r' {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
        let c = d[self.at];
        match c {
            b'[' | b'{' => {
                self.at += 1;
                Some(Tok::Open)
            }
            b']' | b'}' => {
                self.at += 1;
                Some(Tok::Close)
            }
            b'(' => {
                self.at += 1;
                let mut depth = 1;
                let mut s = Vec::new();
                while let Some(&c) = d.get(self.at) {
                    self.at += 1;
                    match c {
                        b'\\' => {
                            if let Some(&e) = d.get(self.at) {
                                self.at += 1;
                                s.push(match e {
                                    b'n' => b'\n',
                                    b'r' => b'\r',
                                    b't' => b'\t',
                                    other => other,
                                });
                            }
                        }
                        b'(' => {
                            depth += 1;
                            s.push(c);
                        }
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            s.push(c);
                        }
                        _ => s.push(c),
                    }
                }
                Some(Tok::Str(s))
            }
            b'<' => {
                // A hex string: skipped as an empty string.
                while let Some(&c) = d.get(self.at) {
                    self.at += 1;
                    if c == b'>' {
                        break;
                    }
                }
                Some(Tok::Str(Vec::new()))
            }
            b'/' => {
                self.at += 1;
                let start = self.at;
                while self.at < d.len() && !Self::delimiter(d[self.at]) {
                    self.at += 1;
                }
                Some(Tok::Name(&d[start..self.at]))
            }
            _ => {
                let start = self.at;
                self.at += 1;
                while self.at < d.len() && !Self::delimiter(d[self.at]) {
                    self.at += 1;
                }
                let w = &d[start..self.at];
                let num = std::str::from_utf8(w).ok().and_then(|s| {
                    let numeric = s
                        .bytes()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit() || b"+-.".contains(&c));
                    if !numeric {
                        None
                    } else if s.contains('#') {
                        let (base, digits) = s.split_once('#')?;
                        let base: u32 = base.parse().ok().filter(|b| (2..=36).contains(b))?;
                        i64::from_str_radix(digits, base).ok().map(|v| v as f64)
                    } else {
                        s.parse::<f64>().ok()
                    }
                });
                Some(match num {
                    Some(v) => Tok::Num(v),
                    None => Tok::Word(w),
                })
            }
        }
    }

    /// The `n` bytes after the single space that follows `RD`.
    fn binary(&mut self, n: usize) -> Option<&'a [u8]> {
        let start = self.at + 1;
        let bytes = self.data.get(start..start.checked_add(n)?)?;
        self.at = start + n;
        Some(bytes)
    }
}

/// The value after `/key` in `text` (the first one).
fn value_after<'a>(text: &'a [u8], key: &[u8]) -> Option<Tok<'a>> {
    let mut t = Tokens::new(text);
    while let Some(tok) = t.next() {
        if tok == Tok::Name(key) {
            return t.next();
        }
    }
    None
}

fn string_after(text: &[u8], key: &[u8]) -> Option<String> {
    match value_after(text, key)? {
        Tok::Str(s) => Some(latin1(&s)),
        Tok::Name(n) => Some(latin1(n)),
        _ => None,
    }
}

fn number_after(text: &[u8], key: &[u8]) -> Option<f64> {
    match value_after(text, key)? {
        Tok::Num(v) => Some(v),
        _ => None,
    }
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

/// The numbers of the array after `/key` (`[...]` or `{...}`).
fn array_after(text: &[u8], key: &[u8]) -> Vec<f64> {
    let mut t = Tokens::new(text);
    while let Some(tok) = t.next() {
        if tok == Tok::Name(key) {
            if t.next() != Some(Tok::Open) {
                return Vec::new();
            }
            let mut out = Vec::new();
            while let Some(tok) = t.next() {
                match tok {
                    Tok::Num(v) => out.push(v),
                    Tok::Close => break,
                    _ => {}
                }
            }
            return out;
        }
    }
    Vec::new()
}

/// The font's encoding: `StandardEncoding`, or the `dup code /name put`
/// entries of its own array.
fn encoding_of(clear: &[u8]) -> (Vec<(u8, String)>, bool) {
    let mut t = Tokens::new(clear);
    while let Some(tok) = t.next() {
        if tok != Tok::Name(b"Encoding") {
            continue;
        }
        match t.next() {
            Some(Tok::Word(b"StandardEncoding")) => {
                return (
                    STANDARD_ENCODING
                        .iter()
                        .map(|(c, n)| (*c, n.to_string()))
                        .collect(),
                    true,
                );
            }
            Some(Tok::Num(_)) => {
                let mut out = Vec::new();
                let mut prev: [Option<Tok>; 2] = [None, None];
                while let Some(tok) = t.next() {
                    match &tok {
                        Tok::Word(b"put") => {
                            if let [Some(Tok::Num(code)), Some(Tok::Name(name))] = &prev {
                                if (0.0..256.0).contains(code) {
                                    out.push((*code as u8, latin1(name)));
                                }
                            }
                        }
                        Tok::Word(b"readonly") | Tok::Word(b"def") => break,
                        _ => {}
                    }
                    prev = [prev[1].take(), Some(tok)];
                }
                return (out, false);
            }
            _ => break,
        }
    }
    (
        STANDARD_ENCODING
            .iter()
            .map(|(c, n)| (*c, n.to_string()))
            .collect(),
        true,
    )
}

// ---------------------------------------------------------------------
// Parsing a font
// ---------------------------------------------------------------------

impl Type1Font {
    /// Read a `.pfb` or `.pfa` font; `None` when it is not one or is
    /// damaged.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let head = data.get(..64.min(data.len()))?;
        let looks = data.first() == Some(&0x80)
            || find(head, b"%!PS-AdobeFont").is_some()
            || find(head, b"%!FontType1").is_some();
        if !looks {
            return None;
        }
        let (clear, private) = split_font(data)?;
        let font_name = string_after(&clear, b"FontName")?;
        let family = string_after(&clear, b"FamilyName").unwrap_or_else(|| font_name.clone());
        let full_name = string_after(&clear, b"FullName").unwrap_or_else(|| family.clone());
        let weight = string_after(&clear, b"Weight").unwrap_or_else(|| "Regular".into());
        let matrix = array_after(&clear, b"FontMatrix");
        let scale = matrix
            .first()
            .copied()
            .filter(|v| *v > 0.0)
            .unwrap_or(0.001);
        let units_per_em = (1.0 / scale).round().clamp(16.0, 16384.0) as u16;
        let (encoding, standard_encoding) = encoding_of(&clear);
        let len_iv = number_after(&private, b"lenIV").unwrap_or(4.0);
        let subrs = read_subrs(&private);
        let strings = read_charstrings(&private)?;
        let decode = |s: &[u8]| -> Vec<u8> {
            if len_iv < 0.0 {
                s.to_vec()
            } else {
                decrypt(s, 4330, len_iv as usize)
            }
        };
        let subrs: Vec<Vec<u8>> = subrs.iter().map(|s| decode(s)).collect();
        let programs: HashMap<String, Vec<u8>> = strings
            .iter()
            .map(|(n, s)| (n.clone(), decode(s)))
            .collect();
        let mut glyphs = Vec::new();
        // .notdef first, as OpenType wants it at glyph 0.
        let mut names: Vec<&String> = strings.iter().map(|(n, _)| n).collect();
        names.sort_by_key(|n| *n != ".notdef");
        for name in names.into_iter().take(MAX_GLYPHS) {
            let Some(program) = programs.get(name) else {
                continue;
            };
            let mut run = Run::new(&subrs, &programs);
            if run.glyph(program, 0).is_none() && run.path.is_empty() {
                continue;
            }
            glyphs.push(Glyph {
                name: name.clone(),
                advance: run.advance,
                path: run.path,
            });
        }
        if glyphs.first().is_none_or(|g| g.name != ".notdef") {
            glyphs.insert(
                0,
                Glyph {
                    name: ".notdef".into(),
                    advance: 0.0,
                    path: Vec::new(),
                },
            );
        }
        Some(Type1Font {
            font_name,
            family,
            full_name,
            weight,
            italic_angle: number_after(&clear, b"ItalicAngle").unwrap_or(0.0),
            fixed_pitch: matches!(
                value_after(&clear, b"isFixedPitch"),
                Some(Tok::Word(b"true"))
            ),
            underline_position: number_after(&clear, b"UnderlinePosition").unwrap_or(-100.0),
            underline_thickness: number_after(&clear, b"UnderlineThickness").unwrap_or(50.0),
            units_per_em,
            encoding,
            standard_encoding,
            glyphs,
        })
    }

    /// The PostScript name of the font.
    pub fn font_name(&self) -> &str {
        &self.font_name
    }
}

/// The subroutines of the private part: `dup i n RD <n bytes> NP`.
fn read_subrs(private: &[u8]) -> Vec<Vec<u8>> {
    let Some(at) = find(private, b"/Subrs") else {
        return Vec::new();
    };
    let mut t = Tokens::new(&private[at..]);
    t.next();
    let count = match t.next() {
        Some(Tok::Num(n)) if (0.0..65536.0).contains(&n) => n as usize,
        _ => return Vec::new(),
    };
    let mut out = vec![Vec::new(); count];
    let mut nums: Vec<f64> = Vec::new();
    while let Some(tok) = t.next() {
        match tok {
            Tok::Num(v) => nums.push(v),
            Tok::Word(b"RD") | Tok::Word(b"-|") => {
                let (Some(&len), Some(&index)) = (nums.last(), nums.iter().rev().nth(1)) else {
                    break;
                };
                let Some(bytes) = t.binary(len.max(0.0) as usize) else {
                    break;
                };
                if let Some(slot) = out.get_mut(index as usize) {
                    *slot = bytes.to_vec();
                }
                nums.clear();
            }
            Tok::Word(b"dup")
            | Tok::Word(b"NP")
            | Tok::Word(b"|")
            | Tok::Word(b"noaccess")
            | Tok::Word(b"put") => {}
            Tok::Name(b"CharStrings") => break,
            Tok::Word(b"ND") | Tok::Word(b"|-") | Tok::Word(b"def") | Tok::Word(b"readonly") => {
                break
            }
            _ => nums.clear(),
        }
    }
    out
}

/// The glyph programs of the private part: `/name n RD <n bytes> ND`.
fn read_charstrings(private: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let at = find(private, b"/CharStrings")?;
    let mut t = Tokens::new(&private[at..]);
    t.next();
    let mut out = Vec::new();
    let mut name: Option<String> = None;
    let mut len: Option<f64> = None;
    while let Some(tok) = t.next() {
        match tok {
            Tok::Name(n) => {
                name = Some(latin1(n));
                len = None;
            }
            Tok::Num(v) => len = Some(v),
            Tok::Word(b"RD") | Tok::Word(b"-|") => {
                let (Some(n), Some(l)) = (name.take(), len.take()) else {
                    continue;
                };
                let bytes = t.binary(l.max(0.0) as usize)?;
                out.push((n, bytes.to_vec()));
                if out.len() > MAX_GLYPHS * 4 {
                    break;
                }
            }
            Tok::Word(b"end") => break,
            _ => {}
        }
    }
    (!out.is_empty()).then_some(out)
}

// ---------------------------------------------------------------------
// Running glyph programs
// ---------------------------------------------------------------------

/// The state of one glyph program run.
struct Run<'a> {
    subrs: &'a [Vec<u8>],
    programs: &'a HashMap<String, Vec<u8>>,
    stack: Vec<f64>,
    ps_stack: Vec<f64>,
    x: f64,
    y: f64,
    sbx: f64,
    advance: f64,
    path: Vec<Seg>,
    open: bool,
    flex: Option<Vec<Pt>>,
    ops: usize,
    /// Offset added to every point (an accent placed by `seac`).
    shift: Pt,
    ended: bool,
}

impl<'a> Run<'a> {
    fn new(subrs: &'a [Vec<u8>], programs: &'a HashMap<String, Vec<u8>>) -> Self {
        Run {
            subrs,
            programs,
            stack: Vec::new(),
            ps_stack: Vec::new(),
            x: 0.0,
            y: 0.0,
            sbx: 0.0,
            advance: 0.0,
            path: Vec::new(),
            open: false,
            flex: None,
            ops: 0,
            shift: (0.0, 0.0),
            ended: false,
        }
    }

    fn pt(&self) -> Pt {
        (self.x + self.shift.0, self.y + self.shift.1)
    }

    fn move_by(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        if let Some(points) = &mut self.flex {
            points.push((self.x, self.y));
            return;
        }
        self.open = false;
    }

    fn start_contour(&mut self) {
        if !self.open {
            let p = self.pt();
            self.path.push(Seg::Move(p));
            self.open = true;
        }
    }

    fn line_by(&mut self, dx: f64, dy: f64) {
        self.start_contour();
        self.x += dx;
        self.y += dy;
        let p = self.pt();
        self.path.push(Seg::Line(p));
    }

    fn curve_by(&mut self, d: [f64; 6]) {
        self.start_contour();
        let p1 = (self.x + d[0], self.y + d[1]);
        let p2 = (p1.0 + d[2], p1.1 + d[3]);
        let p3 = (p2.0 + d[4], p2.1 + d[5]);
        self.x = p3.0;
        self.y = p3.1;
        let s = self.shift;
        let at = |p: Pt| (p.0 + s.0, p.1 + s.1);
        self.path.push(Seg::Curve(at(p1), at(p2), at(p3)));
    }

    fn pop(&mut self) -> f64 {
        self.stack.pop().unwrap_or(0.0)
    }

    /// Run `program`; `None` when it is damaged.
    fn glyph(&mut self, program: &[u8], depth: usize) -> Option<()> {
        if depth > MAX_CALL_DEPTH {
            return None;
        }
        let mut i = 0usize;
        while i < program.len() {
            if self.ended {
                return Some(());
            }
            self.ops += 1;
            if self.ops > MAX_OPS {
                return None;
            }
            let v = program[i];
            i += 1;
            if v >= 32 {
                let n = match v {
                    32..=246 => v as f64 - 139.0,
                    247..=250 => {
                        let w = *program.get(i)? as f64;
                        i += 1;
                        (v as f64 - 247.0) * 256.0 + w + 108.0
                    }
                    251..=254 => {
                        let w = *program.get(i)? as f64;
                        i += 1;
                        -(v as f64 - 251.0) * 256.0 - w - 108.0
                    }
                    _ => {
                        let b = program.get(i..i + 4)?;
                        i += 4;
                        i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f64
                    }
                };
                if self.stack.len() < 64 {
                    self.stack.push(n);
                }
                continue;
            }
            match v {
                // hstem, vstem: hints, not needed for outlines.
                1 | 3 => self.stack.clear(),
                4 => {
                    let dy = self.pop();
                    self.move_by(0.0, dy);
                    self.stack.clear();
                }
                5 => {
                    let dy = self.pop();
                    let dx = self.pop();
                    self.line_by(dx, dy);
                    self.stack.clear();
                }
                6 => {
                    let dx = self.pop();
                    self.line_by(dx, 0.0);
                    self.stack.clear();
                }
                7 => {
                    let dy = self.pop();
                    self.line_by(0.0, dy);
                    self.stack.clear();
                }
                8 => {
                    let mut d = [0.0; 6];
                    for k in (0..6).rev() {
                        d[k] = self.pop();
                    }
                    self.curve_by(d);
                    self.stack.clear();
                }
                9 => {
                    self.open = false;
                    self.stack.clear();
                }
                10 => {
                    let n = self.pop();
                    let sub = self.subrs.get(n.max(0.0) as usize)?;
                    self.glyph(sub, depth + 1)?;
                }
                11 => return Some(()),
                12 => {
                    let e = *program.get(i)?;
                    i += 1;
                    self.escape(e, depth)?;
                }
                13 => {
                    let wx = self.pop();
                    let sbx = self.pop();
                    self.sbx = sbx;
                    self.advance = wx;
                    self.x = sbx;
                    self.y = 0.0;
                    self.stack.clear();
                }
                14 => {
                    self.ended = true;
                    return Some(());
                }
                21 => {
                    let dy = self.pop();
                    let dx = self.pop();
                    self.move_by(dx, dy);
                    self.stack.clear();
                }
                22 => {
                    let dx = self.pop();
                    self.move_by(dx, 0.0);
                    self.stack.clear();
                }
                30 => {
                    let mut d = [0.0; 4];
                    for k in (0..4).rev() {
                        d[k] = self.pop();
                    }
                    self.curve_by([0.0, d[0], d[1], d[2], d[3], 0.0]);
                    self.stack.clear();
                }
                31 => {
                    let mut d = [0.0; 4];
                    for k in (0..4).rev() {
                        d[k] = self.pop();
                    }
                    self.curve_by([d[0], 0.0, d[1], d[2], 0.0, d[3]]);
                    self.stack.clear();
                }
                _ => self.stack.clear(),
            }
        }
        Some(())
    }

    fn escape(&mut self, e: u8, depth: usize) -> Option<()> {
        match e {
            // dotsection, vstem3, hstem3: hints.
            0..=2 => self.stack.clear(),
            6 => {
                let achar = self.pop();
                let bchar = self.pop();
                let ady = self.pop();
                let adx = self.pop();
                let asb = self.pop();
                self.stack.clear();
                self.seac(asb, adx, ady, bchar as u8, achar as u8, depth)?;
                self.ended = true;
            }
            7 => {
                let _wy = self.pop();
                let wx = self.pop();
                let sby = self.pop();
                let sbx = self.pop();
                self.sbx = sbx;
                self.advance = wx;
                self.x = sbx;
                self.y = sby;
                self.stack.clear();
            }
            12 => {
                let b = self.pop();
                let a = self.pop();
                self.stack.push(if b != 0.0 { a / b } else { 0.0 });
            }
            16 => {
                let n = self.pop() as i64;
                let count = self.pop().max(0.0) as usize;
                let mut args = Vec::new();
                for _ in 0..count.min(self.stack.len()) {
                    args.push(self.pop());
                }
                // args now holds the arguments last first.
                match n {
                    0 => {
                        // End of flex: two curves through the points
                        // gathered since othersubr 1.
                        if let Some(points) = self.flex.take() {
                            if points.len() >= 7 {
                                let s = self.shift;
                                let at = |p: Pt| (p.0 + s.0, p.1 + s.1);
                                self.start_contour();
                                self.path.push(Seg::Curve(
                                    at(points[1]),
                                    at(points[2]),
                                    at(points[3]),
                                ));
                                self.path.push(Seg::Curve(
                                    at(points[4]),
                                    at(points[5]),
                                    at(points[6]),
                                ));
                                self.x = points[6].0;
                                self.y = points[6].1;
                            }
                        }
                        self.ps_stack = vec![self.y, self.x];
                    }
                    1 => {
                        self.start_contour();
                        self.flex = Some(Vec::new());
                        self.ps_stack.clear();
                    }
                    2 => self.ps_stack.clear(),
                    _ => {
                        // Hint replacement (3) and others: the arguments
                        // come back with pop in the order given.
                        self.ps_stack = args;
                    }
                }
            }
            17 => {
                let v = self.ps_stack.pop().unwrap_or(0.0);
                self.stack.push(v);
            }
            33 => {
                // setcurrentpoint: the flex already left the point there.
                self.stack.clear();
            }
            _ => self.stack.clear(),
        }
        Some(())
    }

    /// An accented character: the base glyph, and the accent moved so its
    /// side bearing point lies `adx`, `ady` from the base's.
    fn seac(&mut self, asb: f64, adx: f64, ady: f64, b: u8, a: u8, depth: usize) -> Option<()> {
        let name = |code: u8| {
            STANDARD_ENCODING
                .iter()
                .find(|(c, _)| *c == code)
                .map(|(_, n)| *n)
        };
        let base = self.programs.get(name(b)?)?;
        let accent = self.programs.get(name(a)?)?;
        let (advance, sbx) = (self.advance, self.sbx);
        let mut run = Run::new(self.subrs, self.programs);
        run.glyph(base, depth + 1)?;
        let mut acc = Run::new(self.subrs, self.programs);
        acc.shift = (sbx + adx - asb, ady);
        acc.glyph(accent, depth + 1)?;
        self.path.extend(run.path);
        self.path.extend(acc.path);
        self.advance = if advance != 0.0 { advance } else { run.advance };
        Some(())
    }
}

// ---------------------------------------------------------------------
// The .pfm file
// ---------------------------------------------------------------------

impl PfmInfo {
    /// Read the header of a `.pfm` file.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 117 || data.len() > 1 << 20 {
            return None;
        }
        let u16_at = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]);
        let u32_at =
            |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        let version = u16_at(0);
        if version != 0x0100 && version != 0x0200 && version != 0x0300 {
            return None;
        }
        let face_at = u32_at(105) as usize;
        let face: Vec<u8> = data
            .get(face_at..)?
            .iter()
            .take_while(|c| **c != 0)
            .take(64)
            .copied()
            .collect();
        let face = latin1(&face).trim().to_string();
        if face.is_empty() {
            return None;
        }
        Some(PfmInfo {
            face,
            weight: u16_at(83).clamp(100, 900),
            italic: data[80] != 0,
            charset: data[85],
        })
    }
}

// ---------------------------------------------------------------------
// Glyph names
// ---------------------------------------------------------------------

/// The character a glyph name stands for: the Adobe Glyph List names of
/// Latin text, `uniXXXX`, `uXXXX` and one-letter names.
fn glyph_char(name: &str) -> Option<char> {
    let base = name.split('.').next().unwrap_or(name);
    if let Some(hex) = base.strip_prefix("uni") {
        if hex.len() == 4 {
            return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
        }
    }
    if let Some(hex) = base.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
        }
    }
    let mut chars = base.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphabetic() {
            return Some(c);
        }
    }
    GLYPH_NAMES
        .iter()
        .find(|(n, _)| *n == base)
        .and_then(|(_, u)| char::from_u32(*u))
}

/// Character map entries (code point, glyph) and whether the map is a
/// symbol map: fonts whose glyph names mostly stand for no character, or
/// whose `.pfm` says symbol, map their codes at U+F020 to U+F0FF as the
/// legacy script fonts do (#1937); others map by glyph name, and the
/// codes of their encoding whose names stand for no character at the
/// characters of code page 1252 there.
fn char_map(font: &Type1Font, pfm: Option<&PfmInfo>) -> (Vec<(u32, u16)>, bool) {
    let gid: HashMap<&str, u16> = font
        .glyphs
        .iter()
        .enumerate()
        .map(|(i, g)| (g.name.as_str(), i as u16))
        .collect();
    let named = font
        .glyphs
        .iter()
        .skip(1)
        .filter(|g| glyph_char(&g.name).is_some())
        .count();
    let real = font.glyphs.len().saturating_sub(1).max(1);
    let symbol = match pfm.map(|p| p.charset) {
        Some(2) => true,
        Some(_) => false,
        None => !font.standard_encoding && named * 2 < real,
    };
    let mut map: HashMap<u32, u16> = HashMap::new();
    if symbol {
        for (code, name) in &font.encoding {
            if let Some(&g) = gid.get(name.as_str()) {
                map.insert(0xF000 + *code as u32, g);
            }
        }
    } else {
        for (i, g) in font.glyphs.iter().enumerate().skip(1) {
            if let Some(c) = glyph_char(&g.name) {
                map.entry(c as u32).or_insert(i as u16);
            }
        }
        // The codes of the font's own encoding too, as code page 1252 has
        // them, where nothing else took that character: the font's own
        // keyboard layout sends them.
        for (code, name) in &font.encoding {
            let Some(&g) = gid.get(name.as_str()) else {
                continue;
            };
            if font.standard_encoding && glyph_char(name).is_some() {
                continue;
            }
            let c = match *code {
                0x80..=0x9F => crate::CP1252_HIGH[*code as usize - 0x80] as u32,
                c => c as u32,
            };
            map.entry(c).or_insert(g);
        }
    }
    let mut out: Vec<(u32, u16)> = map.into_iter().filter(|(c, _)| *c <= 0xFFFF).collect();
    out.sort_unstable();
    (out, symbol)
}

// ---------------------------------------------------------------------
// Writing the OpenType font
// ---------------------------------------------------------------------

/// The font as an OpenType font with CFF outlines, named as Windows names
/// it (`pfm`'s face name, weight and slant when given).
pub fn to_opentype(font: &Type1Font, pfm: Option<&PfmInfo>) -> Option<Vec<u8>> {
    let n = font.glyphs.len();
    if n < 2 || n > u16::MAX as usize {
        return None;
    }
    let round = |v: f64| v.round().clamp(-32000.0, 32000.0) as i32;
    // Bounds of every glyph, and of all.
    let bounds: Vec<Option<[i32; 4]>> = font
        .glyphs
        .iter()
        .map(|g| {
            let mut b: Option<[i32; 4]> = None;
            let mut add = |p: Pt| {
                let (x, y) = (round(p.0), round(p.1));
                b = Some(match b {
                    None => [x, y, x, y],
                    Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                });
            };
            for s in &g.path {
                match *s {
                    Seg::Move(p) | Seg::Line(p) => add(p),
                    Seg::Curve(a, b2, c) => {
                        add(a);
                        add(b2);
                        add(c);
                    }
                }
            }
            b
        })
        .collect();
    let all = bounds.iter().flatten().fold(None::<[i32; 4]>, |acc, b| {
        Some(match acc {
            None => *b,
            Some(a) => [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ],
        })
    });
    let [x_min, y_min, x_max, y_max] = all.unwrap_or([0, 0, 0, 0]);
    let upm = font.units_per_em as i32;
    let ascent = y_max.max(upm * 7 / 10);
    let descent = y_min.min(-(upm / 5));
    let bold = pfm.map_or_else(
        || {
            let w = font.weight.to_ascii_lowercase();
            w.contains("bold") || w.contains("black") || w.contains("heavy")
        },
        |p| p.weight >= 600,
    );
    let italic = pfm.map_or(font.italic_angle != 0.0, |p| p.italic);
    let weight_class: u16 = pfm.map_or(if bold { 700 } else { 400 }, |p| p.weight);
    let family = pfm.map_or_else(|| font.family.clone(), |p| p.face.clone());
    let style = match (bold, italic) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (true, true) => "Bold Italic",
    };
    let full = if pfm.is_some() {
        if style == "Regular" {
            family.clone()
        } else {
            format!("{family} {style}")
        }
    } else {
        font.full_name.clone()
    };
    let (cmap_entries, symbol) = char_map(font, pfm);
    let advances: Vec<u16> = font
        .glyphs
        .iter()
        .map(|g| g.advance.round().clamp(0.0, 65535.0) as u16)
        .collect();

    let cff = write_cff(font, &family, &full, round)?;

    // cmap: one format 4 subtable.
    let cmap = {
        let mut segs: Vec<(u16, u16, i16)> = Vec::new();
        for &(c, g) in &cmap_entries {
            let c = c as u16;
            let delta = g.wrapping_sub(c) as i16;
            match segs.last_mut() {
                Some((_, end, d)) if *end + 1 == c && *d == delta => *end = c,
                _ => segs.push((c, c, delta)),
            }
        }
        segs.push((0xFFFF, 0xFFFF, 1));
        let count = segs.len() as u16;
        let mut sub = Vec::new();
        let len = 16 + 8 * count as usize;
        let entry = 1u16 << (15 - count.leading_zeros());
        let search = entry * 2;
        let selector = entry.trailing_zeros() as u16;
        for v in [
            4u16,
            len as u16,
            0,
            count * 2,
            search,
            selector,
            count * 2 - search,
        ] {
            sub.extend_from_slice(&v.to_be_bytes());
        }
        for s in &segs {
            sub.extend_from_slice(&s.1.to_be_bytes());
        }
        sub.extend_from_slice(&0u16.to_be_bytes());
        for s in &segs {
            sub.extend_from_slice(&s.0.to_be_bytes());
        }
        for s in &segs {
            sub.extend_from_slice(&s.2.to_be_bytes());
        }
        for _ in &segs {
            sub.extend_from_slice(&0u16.to_be_bytes());
        }
        let mut t = Vec::new();
        for v in [0u16, 1, 3, if symbol { 0 } else { 1 }] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        t.extend_from_slice(&12u32.to_be_bytes());
        t.extend_from_slice(&sub);
        t
    };

    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    head.extend_from_slice(&0u32.to_be_bytes()); // checkSumAdjustment
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    head.extend_from_slice(&3u16.to_be_bytes());
    head.extend_from_slice(&font.units_per_em.to_be_bytes());
    head.extend_from_slice(&[0; 16]); // created, modified
    for v in [x_min, y_min, x_max, y_max] {
        head.extend_from_slice(&(v as i16).to_be_bytes());
    }
    let mac_style = (bold as u16) | ((italic as u16) << 1);
    for v in [mac_style, 8, 2, 0, 0] {
        head.extend_from_slice(&v.to_be_bytes());
    }

    let max_advance = advances.iter().copied().max().unwrap_or(0);
    let mut hhea = Vec::new();
    hhea.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    for v in [ascent as i16, descent as i16, 0] {
        hhea.extend_from_slice(&v.to_be_bytes());
    }
    hhea.extend_from_slice(&max_advance.to_be_bytes());
    let min_lsb = bounds.iter().flatten().map(|b| b[0]).min().unwrap_or(0);
    let min_rsb = bounds
        .iter()
        .zip(&advances)
        .filter_map(|(b, a)| b.map(|b| *a as i32 - b[2]))
        .min()
        .unwrap_or(0);
    for v in [
        min_lsb as i16,
        min_rsb as i16,
        x_max as i16,
        1,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ] {
        hhea.extend_from_slice(&v.to_be_bytes());
    }
    hhea.extend_from_slice(&(n as u16).to_be_bytes());

    let mut maxp = Vec::new();
    maxp.extend_from_slice(&0x0000_5000u32.to_be_bytes());
    maxp.extend_from_slice(&(n as u16).to_be_bytes());

    let mut hmtx = Vec::new();
    for (a, b) in advances.iter().zip(&bounds) {
        hmtx.extend_from_slice(&a.to_be_bytes());
        hmtx.extend_from_slice(&(b.map_or(0, |b| b[0]) as i16).to_be_bytes());
    }

    let first = cmap_entries.first().map_or(0x20, |e| e.0 as u16);
    let last = cmap_entries.last().map_or(0x20, |e| e.0 as u16);
    let avg = {
        let w: Vec<u32> = advances
            .iter()
            .filter(|a| **a > 0)
            .map(|a| *a as u32)
            .collect();
        (w.iter().sum::<u32>() / (w.len() as u32).max(1)) as i16
    };
    let mut os2 = Vec::new();
    os2.extend_from_slice(&4u16.to_be_bytes());
    os2.extend_from_slice(&avg.to_be_bytes());
    os2.extend_from_slice(&weight_class.to_be_bytes());
    os2.extend_from_slice(&5u16.to_be_bytes()); // medium width
    os2.extend_from_slice(&0u16.to_be_bytes()); // fsType: installable
    let em = upm as i16;
    for v in [
        em * 13 / 20,
        em * 7 / 10,
        0,
        em * 7 / 50,
        em * 13 / 20,
        em * 7 / 10,
        0,
        em * 12 / 25,
        em / 20,
        em * 6 / 25,
    ] {
        os2.extend_from_slice(&v.to_be_bytes());
    }
    os2.extend_from_slice(&0i16.to_be_bytes()); // sFamilyClass
    os2.extend_from_slice(&[0; 10]); // panose
    let unicode_range: [u32; 4] = if symbol { [0; 4] } else { [3, 0, 0, 0] };
    for v in unicode_range {
        os2.extend_from_slice(&v.to_be_bytes());
    }
    os2.extend_from_slice(b"T1  ");
    let fs_selection: u16 = if bold || italic {
        (italic as u16) | ((bold as u16) << 5)
    } else {
        1 << 6
    };
    for v in [fs_selection, first, last] {
        os2.extend_from_slice(&v.to_be_bytes());
    }
    for v in [ascent as i16, descent as i16, 0] {
        os2.extend_from_slice(&v.to_be_bytes());
    }
    os2.extend_from_slice(&(ascent.max(0) as u16).to_be_bytes());
    os2.extend_from_slice(&((-descent).max(0) as u16).to_be_bytes());
    let code_pages: [u32; 2] = if symbol { [1 << 31, 0] } else { [1, 0] };
    for v in code_pages {
        os2.extend_from_slice(&v.to_be_bytes());
    }
    for v in [em / 2, em * 7 / 10, 0, 0x20, 1] {
        os2.extend_from_slice(&(v as u16).to_be_bytes());
    }

    let mut post = Vec::new();
    post.extend_from_slice(&0x0003_0000u32.to_be_bytes());
    post.extend_from_slice(&((font.italic_angle * 65536.0).round() as i32).to_be_bytes());
    post.extend_from_slice(&(font.underline_position as i16).to_be_bytes());
    post.extend_from_slice(&(font.underline_thickness.max(1.0) as i16).to_be_bytes());
    post.extend_from_slice(&(font.fixed_pitch as u32).to_be_bytes());
    post.extend_from_slice(&[0; 16]);

    let name = {
        let ps: String = font
            .font_name
            .chars()
            .filter(|c| c.is_ascii_graphic() && !"[](){}<>/%".contains(*c))
            .take(63)
            .collect();
        let records: Vec<(u16, String)> = vec![
            (1, family.clone()),
            (2, style.to_string()),
            (3, format!("Type 1: {}", font.font_name)),
            (4, full),
            (6, ps),
        ];
        let mut strings = Vec::new();
        let mut recs = Vec::new();
        for (id, text) in &records {
            let bytes: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
            for v in [
                3u16,
                1,
                0x409,
                *id,
                bytes.len() as u16,
                strings.len() as u16,
            ] {
                recs.extend_from_slice(&v.to_be_bytes());
            }
            strings.extend_from_slice(&bytes);
        }
        let mut t = Vec::new();
        for v in [0u16, records.len() as u16, 6 + recs.len() as u16] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        t.extend_from_slice(&recs);
        t.extend_from_slice(&strings);
        t
    };

    let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"CFF ", cff),
        (b"OS/2", os2),
        (b"cmap", cmap),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"maxp", maxp),
        (b"name", name),
        (b"post", post),
    ];
    Some(write_sfnt(tables))
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, c| {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(w))
    })
}

/// The tables (sorted by tag) as one font file.
fn write_sfnt(tables: Vec<(&[u8; 4], Vec<u8>)>) -> Vec<u8> {
    let count = tables.len() as u16;
    let entry = 1u16 << (15 - count.leading_zeros());
    let mut out = Vec::new();
    out.extend_from_slice(b"OTTO");
    for v in [
        count,
        entry * 16,
        entry.trailing_zeros() as u16,
        count * 16 - entry * 16,
    ] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    let mut head_at = None;
    for (tag, data) in &tables {
        out.extend_from_slice(*tag);
        out.extend_from_slice(&checksum(data).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        if *tag == b"head" {
            head_at = Some(12 + 16 * tables.len() + body.len());
        }
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend_from_slice(&body);
    if let Some(at) = head_at {
        let adjust = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        out[at + 8..at + 12].copy_from_slice(&adjust.to_be_bytes());
    }
    out
}

/// A CFF INDEX of `items`.
fn cff_index(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(items.len() as u16).to_be_bytes());
    if items.is_empty() {
        return out;
    }
    let total: usize = items.iter().map(Vec::len).sum::<usize>() + 1;
    let size: u8 = match total {
        0..=0xFF => 1,
        0x100..=0xFFFF => 2,
        0x1_0000..=0xFF_FFFF => 3,
        _ => 4,
    };
    out.push(size);
    let mut at = 1usize;
    let put = |out: &mut Vec<u8>, v: usize| {
        out.extend_from_slice(&(v as u32).to_be_bytes()[4 - size as usize..]);
    };
    put(&mut out, at);
    for item in items {
        at += item.len();
        put(&mut out, at);
    }
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

/// A CFF DICT integer in its five-byte form (sizes stay fixed while the
/// offsets are worked out).
fn dict_int5(out: &mut Vec<u8>, v: i32) {
    out.push(29);
    out.extend_from_slice(&v.to_be_bytes());
}

/// A CFF DICT real number.
fn dict_real(out: &mut Vec<u8>, v: f64) {
    let s = format!("{v}");
    let mut nibbles: Vec<u8> = Vec::new();
    for c in s.chars() {
        nibbles.push(match c {
            '0'..='9' => c as u8 - b'0',
            '.' => 0xA,
            '-' => 0xE,
            _ => continue,
        });
    }
    nibbles.push(0xF);
    if nibbles.len() % 2 == 1 {
        nibbles.push(0xF);
    }
    out.push(30);
    for p in nibbles.chunks(2) {
        out.push(p[0] << 4 | p[1]);
    }
}

/// A Type 2 charstring number.
fn cs_int(out: &mut Vec<u8>, v: i32) {
    match v {
        -107..=107 => out.push((v + 139) as u8),
        108..=1131 => {
            let w = v - 108;
            out.push((w / 256 + 247) as u8);
            out.push((w % 256) as u8);
        }
        -1131..=-108 => {
            let w = -v - 108;
            out.push((w / 256 + 251) as u8);
            out.push((w % 256) as u8);
        }
        _ => {
            out.push(28);
            out.extend_from_slice(&(v.clamp(-32768, 32767) as i16).to_be_bytes());
        }
    }
}

/// The glyph's outline as a Type 2 charstring, its width first.
fn charstring(g: &Glyph, round: impl Fn(f64) -> i32) -> Vec<u8> {
    let mut out = Vec::new();
    cs_int(&mut out, round(g.advance));
    let (mut x, mut y) = (0i32, 0i32);
    let mut rel = |out: &mut Vec<u8>, p: Pt| {
        let (px, py) = (round(p.0), round(p.1));
        cs_int(out, px - x);
        cs_int(out, py - y);
        x = px;
        y = py;
    };
    for s in &g.path {
        match *s {
            Seg::Move(p) => {
                rel(&mut out, p);
                out.push(21);
            }
            Seg::Line(p) => {
                rel(&mut out, p);
                out.push(5);
            }
            Seg::Curve(a, b, c) => {
                rel(&mut out, a);
                rel(&mut out, b);
                rel(&mut out, c);
                out.push(8);
            }
        }
    }
    out.push(14);
    out
}

/// The CFF table: one font, every name a string of its own.
fn write_cff(
    font: &Type1Font,
    family: &str,
    full: &str,
    round: impl Fn(f64) -> i32 + Copy,
) -> Option<Vec<u8>> {
    let ps: Vec<u8> = font
        .font_name
        .bytes()
        .filter(|c| c.is_ascii_graphic() && !b"[](){}<>/%".contains(c))
        .take(63)
        .collect();
    let ps = if ps.is_empty() {
        b"Type1Font".to_vec()
    } else {
        ps
    };
    // Strings: full name, family, weight, then the glyph names.
    let mut strings: Vec<Vec<u8>> = vec![
        full.as_bytes().to_vec(),
        family.as_bytes().to_vec(),
        font.weight.as_bytes().to_vec(),
    ];
    let first_sid = 391 + strings.len() as i32;
    strings.extend(
        font.glyphs
            .iter()
            .skip(1)
            .map(|g| g.name.as_bytes().to_vec()),
    );
    let charstrings: Vec<Vec<u8>> = font.glyphs.iter().map(|g| charstring(g, round)).collect();

    let mut private = Vec::new();
    dict_int5(&mut private, 0);
    private.push(20); // defaultWidthX
    dict_int5(&mut private, 0);
    private.push(21); // nominalWidthX

    let mut charset = vec![0u8];
    for i in 0..font.glyphs.len().saturating_sub(1) {
        charset.extend_from_slice(&((first_sid + i as i32) as u16).to_be_bytes());
    }

    let top = |charset_at: i32, strings_at: i32, private_at: i32| {
        let mut d = Vec::new();
        dict_int5(&mut d, 391);
        d.push(2); // FullName
        dict_int5(&mut d, 392);
        d.push(3); // FamilyName
        dict_int5(&mut d, 393);
        d.push(4); // Weight
        if font.units_per_em != 1000 {
            let s = 1.0 / font.units_per_em as f64;
            for v in [s, 0.0, 0.0, s, 0.0, 0.0] {
                dict_real(&mut d, v);
            }
            d.extend_from_slice(&[12, 7]);
        }
        dict_int5(&mut d, charset_at);
        d.push(15);
        dict_int5(&mut d, strings_at);
        d.push(17);
        dict_int5(&mut d, private.len() as i32);
        dict_int5(&mut d, private_at);
        d.push(18);
        d
    };
    let header = [1u8, 0, 4, 4];
    let name_index = cff_index(&[ps]);
    let string_index = cff_index(&strings);
    let global_subrs = cff_index(&[]);
    let top_len = cff_index(&[top(0, 0, 0)]).len();
    let charset_at =
        header.len() + name_index.len() + top_len + string_index.len() + global_subrs.len();
    let charstrings_at = charset_at + charset.len();
    let cs_index = cff_index(&charstrings);
    let private_at = charstrings_at + cs_index.len();
    let top_index = cff_index(&[top(
        charset_at as i32,
        charstrings_at as i32,
        private_at as i32,
    )]);
    if top_index.len() != top_len {
        return None;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&header);
    out.extend_from_slice(&name_index);
    out.extend_from_slice(&top_index);
    out.extend_from_slice(&string_index);
    out.extend_from_slice(&global_subrs);
    out.extend_from_slice(&charset);
    out.extend_from_slice(&cs_index);
    out.extend_from_slice(&private);
    Some(out)
}

// ---------------------------------------------------------------------
// Finding the fonts
// ---------------------------------------------------------------------

/// The encoding a Type 1 program carries in its clear text (`dup <code>
/// /<name> put` entries), as codes and glyph names; `None` when it names
/// the Standard encoding or is not a Type 1 program. A PDF uses it for
/// an embedded font whose dictionary names no encoding (#3810).
pub fn own_encoding(data: &[u8]) -> Option<Vec<(u8, String)>> {
    let (clear, _) = split_font(data)?;
    let (enc, standard) = encoding_of(&clear);
    (!standard && !enc.is_empty()).then_some(enc)
}

/// The Type 1 font in `pfb` (with its `.pfm`, `pfm`, when there is one)
/// as an OpenType font the engine reads.
pub fn convert(pfb: &[u8], pfm: Option<&[u8]>) -> Option<Vec<u8>> {
    let font = Type1Font::parse(pfb)?;
    let info = pfm.and_then(PfmInfo::parse);
    to_opentype(&font, info.as_ref())
}

/// Whether `path` names a Type 1 font file (`.pfb` or `.pfa`).
pub fn is_type1_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pfb") || e.eq_ignore_ascii_case("pfa"))
}

/// Whether `path` names a file of a Type 1 font: `.pfb`, `.pfa` or the
/// `.pfm` beside one.
pub fn is_type1_file(path: &std::path::Path) -> bool {
    is_type1_path(path)
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("pfm"))
}

/// The `.pfm` beside the Type 1 font `path` (same name, any case).
pub fn pfm_beside(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let dir = path.parent()?;
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("pfm"))
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.to_ascii_lowercase() == stem)
        })
}

/// The Type 1 font file at `path` (a `.pfb`/`.pfa`, or a `.pfm` with its
/// `.pfb` beside it) converted; `None` when it does not read.
pub fn convert_file(path: &std::path::Path) -> Option<Vec<u8>> {
    let is_pfm = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pfm"));
    let (pfb_path, pfm_path) = if is_pfm {
        let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
        let pfb = std::fs::read_dir(path.parent()?)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                is_type1_path(p)
                    && p.file_stem()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.to_ascii_lowercase() == stem)
            })?;
        (pfb, Some(path.to_path_buf()))
    } else {
        (path.to_path_buf(), pfm_beside(path))
    };
    let meta = std::fs::metadata(&pfb_path).ok()?;
    if meta.len() > MAX_FONT_BYTES as u64 {
        return None;
    }
    let pfb = std::fs::read(&pfb_path).ok()?;
    let pfm = pfm_path.and_then(|p| std::fs::read(p).ok());
    convert(&pfb, pfm.as_deref())
}

/// The Type 1 fonts under `roots` (and their subfolders), converted.
pub fn fonts_under(roots: &[std::path::PathBuf]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut stack: Vec<(std::path::PathBuf, usize)> =
        roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        if !seen.insert(dir.clone()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if depth < 8 {
                    stack.push((path, depth + 1));
                }
            } else if is_type1_path(&path) {
                if let Some(font) = convert_file(&path) {
                    out.push(font);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------

/// Adobe StandardEncoding: code and glyph name.
#[rustfmt::skip]
const STANDARD_ENCODING: &[(u8, &str)] = &[
    (32, "space"), (33, "exclam"), (34, "quotedbl"), (35, "numbersign"), (36, "dollar"),
    (37, "percent"), (38, "ampersand"), (39, "quoteright"), (40, "parenleft"),
    (41, "parenright"), (42, "asterisk"), (43, "plus"), (44, "comma"), (45, "hyphen"),
    (46, "period"), (47, "slash"), (48, "zero"), (49, "one"), (50, "two"), (51, "three"),
    (52, "four"), (53, "five"), (54, "six"), (55, "seven"), (56, "eight"), (57, "nine"),
    (58, "colon"), (59, "semicolon"), (60, "less"), (61, "equal"), (62, "greater"),
    (63, "question"), (64, "at"), (65, "A"), (66, "B"), (67, "C"), (68, "D"), (69, "E"),
    (70, "F"), (71, "G"), (72, "H"), (73, "I"), (74, "J"), (75, "K"), (76, "L"), (77, "M"),
    (78, "N"), (79, "O"), (80, "P"), (81, "Q"), (82, "R"), (83, "S"), (84, "T"), (85, "U"),
    (86, "V"), (87, "W"), (88, "X"), (89, "Y"), (90, "Z"), (91, "bracketleft"),
    (92, "backslash"), (93, "bracketright"), (94, "asciicircum"), (95, "underscore"),
    (96, "quoteleft"), (97, "a"), (98, "b"), (99, "c"), (100, "d"), (101, "e"), (102, "f"),
    (103, "g"), (104, "h"), (105, "i"), (106, "j"), (107, "k"), (108, "l"), (109, "m"),
    (110, "n"), (111, "o"), (112, "p"), (113, "q"), (114, "r"), (115, "s"), (116, "t"),
    (117, "u"), (118, "v"), (119, "w"), (120, "x"), (121, "y"), (122, "z"), (123, "braceleft"),
    (124, "bar"), (125, "braceright"), (126, "asciitilde"), (161, "exclamdown"), (162, "cent"),
    (163, "sterling"), (164, "fraction"), (165, "yen"), (166, "florin"), (167, "section"),
    (168, "currency"), (169, "quotesingle"), (170, "quotedblleft"), (171, "guillemotleft"),
    (172, "guilsinglleft"), (173, "guilsinglright"), (174, "fi"), (175, "fl"), (177, "endash"),
    (178, "dagger"), (179, "daggerdbl"), (180, "periodcentered"), (182, "paragraph"),
    (183, "bullet"), (184, "quotesinglbase"), (185, "quotedblbase"), (186, "quotedblright"),
    (187, "guillemotright"), (188, "ellipsis"), (189, "perthousand"), (191, "questiondown"),
    (193, "grave"), (194, "acute"), (195, "circumflex"), (196, "tilde"), (197, "macron"),
    (198, "breve"), (199, "dotaccent"), (200, "dieresis"), (202, "ring"), (203, "cedilla"),
    (205, "hungarumlaut"), (206, "ogonek"), (207, "caron"), (208, "emdash"), (225, "AE"),
    (227, "ordfeminine"), (232, "Lslash"), (233, "Oslash"), (234, "OE"), (235, "ordmasculine"),
    (241, "ae"), (245, "dotlessi"), (248, "lslash"), (249, "oslash"), (250, "oe"),
    (251, "germandbls"),
];

/// Glyph names of the Adobe Glyph List used by Latin fonts, with the
/// character each stands for.
#[rustfmt::skip]
const GLYPH_NAMES: &[(&str, u32)] = &[
    ("space", 0x20), ("exclam", 0x21), ("quotedbl", 0x22), ("numbersign", 0x23),
    ("dollar", 0x24), ("percent", 0x25), ("ampersand", 0x26), ("quotesingle", 0x27),
    ("parenleft", 0x28), ("parenright", 0x29), ("asterisk", 0x2A), ("plus", 0x2B),
    ("comma", 0x2C), ("hyphen", 0x2D), ("period", 0x2E), ("slash", 0x2F), ("zero", 0x30),
    ("one", 0x31), ("two", 0x32), ("three", 0x33), ("four", 0x34), ("five", 0x35),
    ("six", 0x36), ("seven", 0x37), ("eight", 0x38), ("nine", 0x39), ("colon", 0x3A),
    ("semicolon", 0x3B), ("less", 0x3C), ("equal", 0x3D), ("greater", 0x3E), ("question", 0x3F),
    ("at", 0x40), ("bracketleft", 0x5B), ("backslash", 0x5C), ("bracketright", 0x5D),
    ("asciicircum", 0x5E), ("underscore", 0x5F), ("grave", 0x60), ("braceleft", 0x7B),
    ("bar", 0x7C), ("braceright", 0x7D), ("asciitilde", 0x7E), ("exclamdown", 0xA1),
    ("cent", 0xA2), ("sterling", 0xA3), ("currency", 0xA4), ("yen", 0xA5), ("brokenbar", 0xA6),
    ("section", 0xA7), ("dieresis", 0xA8), ("copyright", 0xA9), ("ordfeminine", 0xAA),
    ("guillemotleft", 0xAB), ("logicalnot", 0xAC), ("registered", 0xAE), ("macron", 0xAF),
    ("degree", 0xB0), ("plusminus", 0xB1), ("twosuperior", 0xB2), ("threesuperior", 0xB3),
    ("acute", 0xB4), ("mu", 0xB5), ("paragraph", 0xB6), ("periodcentered", 0xB7),
    ("cedilla", 0xB8), ("onesuperior", 0xB9), ("ordmasculine", 0xBA), ("guillemotright", 0xBB),
    ("onequarter", 0xBC), ("onehalf", 0xBD), ("threequarters", 0xBE), ("questiondown", 0xBF),
    ("Agrave", 0xC0), ("Aacute", 0xC1), ("Acircumflex", 0xC2), ("Atilde", 0xC3),
    ("Adieresis", 0xC4), ("Aring", 0xC5), ("AE", 0xC6), ("Ccedilla", 0xC7), ("Egrave", 0xC8),
    ("Eacute", 0xC9), ("Ecircumflex", 0xCA), ("Edieresis", 0xCB), ("Igrave", 0xCC),
    ("Iacute", 0xCD), ("Icircumflex", 0xCE), ("Idieresis", 0xCF), ("Eth", 0xD0),
    ("Ntilde", 0xD1), ("Ograve", 0xD2), ("Oacute", 0xD3), ("Ocircumflex", 0xD4),
    ("Otilde", 0xD5), ("Odieresis", 0xD6), ("multiply", 0xD7), ("Oslash", 0xD8),
    ("Ugrave", 0xD9), ("Uacute", 0xDA), ("Ucircumflex", 0xDB), ("Udieresis", 0xDC),
    ("Yacute", 0xDD), ("Thorn", 0xDE), ("germandbls", 0xDF), ("agrave", 0xE0), ("aacute", 0xE1),
    ("acircumflex", 0xE2), ("atilde", 0xE3), ("adieresis", 0xE4), ("aring", 0xE5), ("ae", 0xE6),
    ("ccedilla", 0xE7), ("egrave", 0xE8), ("eacute", 0xE9), ("ecircumflex", 0xEA),
    ("edieresis", 0xEB), ("igrave", 0xEC), ("iacute", 0xED), ("icircumflex", 0xEE),
    ("idieresis", 0xEF), ("eth", 0xF0), ("ntilde", 0xF1), ("ograve", 0xF2), ("oacute", 0xF3),
    ("ocircumflex", 0xF4), ("otilde", 0xF5), ("odieresis", 0xF6), ("divide", 0xF7),
    ("oslash", 0xF8), ("ugrave", 0xF9), ("uacute", 0xFA), ("ucircumflex", 0xFB),
    ("udieresis", 0xFC), ("yacute", 0xFD), ("thorn", 0xFE), ("ydieresis", 0xFF),
    ("dotlessi", 0x131), ("Lslash", 0x141), ("lslash", 0x142), ("OE", 0x152), ("oe", 0x153),
    ("Scaron", 0x160), ("scaron", 0x161), ("Ydieresis", 0x178), ("Zcaron", 0x17D),
    ("zcaron", 0x17E), ("florin", 0x192), ("circumflex", 0x2C6), ("caron", 0x2C7),
    ("breve", 0x2D8), ("dotaccent", 0x2D9), ("ring", 0x2DA), ("ogonek", 0x2DB),
    ("tilde", 0x2DC), ("hungarumlaut", 0x2DD), ("endash", 0x2013), ("emdash", 0x2014),
    ("quoteleft", 0x2018), ("quoteright", 0x2019), ("quotesinglbase", 0x201A),
    ("quotedblleft", 0x201C), ("quotedblright", 0x201D), ("quotedblbase", 0x201E),
    ("dagger", 0x2020), ("daggerdbl", 0x2021), ("bullet", 0x2022), ("ellipsis", 0x2026),
    ("perthousand", 0x2030), ("guilsinglleft", 0x2039), ("guilsinglright", 0x203A),
    ("fraction", 0x2044), ("Euro", 0x20AC), ("trademark", 0x2122), ("minus", 0x2212),
    ("fi", 0xFB01), ("fl", 0xFB02), ("nbspace", 0xA0), ("sfthyphen", 0xAD), ("middot", 0xB7),
    // Latin Extended-A (Central European letters, #3810).
    ("Amacron", 0x100), ("amacron", 0x101), ("Abreve", 0x102), ("abreve", 0x103),
    ("Aogonek", 0x104), ("aogonek", 0x105), ("Cacute", 0x106), ("cacute", 0x107),
    ("Ccircumflex", 0x108), ("ccircumflex", 0x109), ("Cdotaccent", 0x10A), ("cdotaccent", 0x10B),
    ("Ccaron", 0x10C), ("ccaron", 0x10D), ("Dcaron", 0x10E), ("dcaron", 0x10F), ("Dcroat", 0x110),
    ("dcroat", 0x111), ("Emacron", 0x112), ("emacron", 0x113), ("Ebreve", 0x114),
    ("ebreve", 0x115), ("Edotaccent", 0x116), ("edotaccent", 0x117), ("Eogonek", 0x118),
    ("eogonek", 0x119), ("Ecaron", 0x11A), ("ecaron", 0x11B), ("Gcircumflex", 0x11C),
    ("gcircumflex", 0x11D), ("Gbreve", 0x11E), ("gbreve", 0x11F), ("Gdotaccent", 0x120),
    ("gdotaccent", 0x121), ("Gcommaaccent", 0x122), ("gcommaaccent", 0x123),
    ("Hcircumflex", 0x124), ("hcircumflex", 0x125), ("Hbar", 0x126), ("hbar", 0x127),
    ("Itilde", 0x128), ("itilde", 0x129), ("Imacron", 0x12A), ("imacron", 0x12B),
    ("Ibreve", 0x12C), ("ibreve", 0x12D), ("Iogonek", 0x12E), ("iogonek", 0x12F),
    ("Idotaccent", 0x130), ("IJ", 0x132), ("ij", 0x133), ("Jcircumflex", 0x134),
    ("jcircumflex", 0x135), ("Kcommaaccent", 0x136), ("kcommaaccent", 0x137),
    ("kgreenlandic", 0x138), ("Lacute", 0x139), ("lacute", 0x13A), ("Lcommaaccent", 0x13B),
    ("lcommaaccent", 0x13C), ("Lcaron", 0x13D), ("lcaron", 0x13E), ("Ldot", 0x13F),
    ("ldot", 0x140), ("Nacute", 0x143), ("nacute", 0x144), ("Ncommaaccent", 0x145),
    ("ncommaaccent", 0x146), ("Ncaron", 0x147), ("ncaron", 0x148), ("napostrophe", 0x149),
    ("Eng", 0x14A), ("eng", 0x14B), ("Omacron", 0x14C), ("omacron", 0x14D), ("Obreve", 0x14E),
    ("obreve", 0x14F), ("Ohungarumlaut", 0x150), ("ohungarumlaut", 0x151), ("Racute", 0x154),
    ("racute", 0x155), ("Rcommaaccent", 0x156), ("rcommaaccent", 0x157), ("Rcaron", 0x158),
    ("rcaron", 0x159), ("Sacute", 0x15A), ("sacute", 0x15B), ("Scircumflex", 0x15C),
    ("scircumflex", 0x15D), ("Scedilla", 0x15E), ("scedilla", 0x15F), ("Tcommaaccent", 0x162),
    ("tcommaaccent", 0x163), ("Tcaron", 0x164), ("tcaron", 0x165), ("Tbar", 0x166),
    ("tbar", 0x167), ("Utilde", 0x168), ("utilde", 0x169), ("Umacron", 0x16A), ("umacron", 0x16B),
    ("Ubreve", 0x16C), ("ubreve", 0x16D), ("Uring", 0x16E), ("uring", 0x16F),
    ("Uhungarumlaut", 0x170), ("uhungarumlaut", 0x171), ("Uogonek", 0x172), ("uogonek", 0x173),
    ("Wcircumflex", 0x174), ("wcircumflex", 0x175), ("Ycircumflex", 0x176), ("ycircumflex", 0x177),
    ("Zacute", 0x179), ("zacute", 0x17A), ("Zdotaccent", 0x17B), ("zdotaccent", 0x17C),
    ("longs", 0x17F),
];

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Shape as _;

    /// A Type 1 charstring number.
    fn num(out: &mut Vec<u8>, v: i32) {
        match v {
            -107..=107 => out.push((v + 139) as u8),
            108..=1131 => {
                out.push(((v - 108) / 256 + 247) as u8);
                out.push(((v - 108) % 256) as u8);
            }
            -1131..=-108 => {
                out.push(((-v - 108) / 256 + 251) as u8);
                out.push(((-v - 108) % 256) as u8);
            }
            _ => {
                out.push(255);
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
    }

    /// A charstring from numbers and operators (`op(n)`, escapes as
    /// `12 * 256 + n`).
    enum C {
        N(i32),
        Op(u16),
    }

    fn program(items: &[C]) -> Vec<u8> {
        let mut out = Vec::new();
        for c in items {
            match c {
                C::N(v) => num(&mut out, *v),
                C::Op(op) if *op >= 256 => out.extend_from_slice(&[12, (*op - 3072) as u8]),
                C::Op(op) => out.push(*op as u8),
            }
        }
        out
    }

    fn encrypt(plain: &[u8], mut r: u16) -> Vec<u8> {
        plain
            .iter()
            .map(|&p| {
                let c = p ^ (r >> 8) as u8;
                r = (c as u16)
                    .wrapping_add(r)
                    .wrapping_mul(52845)
                    .wrapping_add(22719);
                c
            })
            .collect()
    }

    /// A `.pfb` holding `glyphs` (name, plain charstring) and `subrs`,
    /// with the encoding `encoding` (`None`: StandardEncoding).
    fn pfb(
        name: &str,
        glyphs: &[(&str, Vec<u8>)],
        subrs: &[Vec<u8>],
        encoding: Option<&[(u8, &str)]>,
    ) -> Vec<u8> {
        let mut clear = format!(
            "%!PS-AdobeFont-1.0: {name} 001.000\n11 dict begin\n/FontInfo 8 dict dup begin\n\
             /FamilyName ({name}) readonly def\n/FullName ({name} Regular) readonly def\n\
             /Weight (Medium) readonly def\n/ItalicAngle 0 def\nend readonly def\n\
             /FontName /{name} def\n/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n"
        );
        match encoding {
            None => clear.push_str("/Encoding StandardEncoding def\n"),
            Some(e) => {
                clear.push_str("/Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\n");
                for (code, n) in e {
                    clear.push_str(&format!("dup {code} /{n} put\n"));
                }
                clear.push_str("readonly def\n");
            }
        }
        clear.push_str("currentdict end\ncurrentfile eexec\n");
        let mut private = b"\x01\x02\x03\x04dup /Private 8 dict dup begin\n/RD{string currentfile exch readstring pop}executeonly def\n/lenIV 4 def\n".to_vec();
        private.extend_from_slice(format!("/Subrs {} array\n", subrs.len()).as_bytes());
        for (i, s) in subrs.iter().enumerate() {
            let mut cs = vec![0u8; 4];
            cs.extend_from_slice(s);
            let cs = encrypt(&cs, 4330);
            private.extend_from_slice(format!("dup {i} {} RD ", cs.len()).as_bytes());
            private.extend_from_slice(&cs);
            private.extend_from_slice(b" NP\n");
        }
        private.extend_from_slice(b"ND\n2 index /CharStrings ");
        private.extend_from_slice(format!("{} dict dup begin\n", glyphs.len()).as_bytes());
        for (n, g) in glyphs {
            let mut cs = vec![9u8; 4];
            cs.extend_from_slice(g);
            let cs = encrypt(&cs, 4330);
            private.extend_from_slice(format!("/{n} {} RD ", cs.len()).as_bytes());
            private.extend_from_slice(&cs);
            private.extend_from_slice(b" ND\n");
        }
        private.extend_from_slice(b"end\nend\nreadonly put\nput\ndup /FontName get exch definefont pop\nmark currentfile closefile\n");
        let binary = encrypt(&private, 55665);
        let mut out = Vec::new();
        for (kind, body) in [(1u8, clear.into_bytes()), (2, binary), (1, vec![b'0'; 64])] {
            out.extend_from_slice(&[0x80, kind]);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&body);
        }
        out.extend_from_slice(&[0x80, 3]);
        out
    }

    use C::{Op, N};

    /// A box glyph 300 x 600 from x 100, width 500.
    fn box_glyph() -> Vec<u8> {
        program(&[
            N(0),
            N(500),
            Op(13),
            N(100),
            N(0),
            Op(21),
            N(300),
            Op(6),
            N(600),
            Op(7),
            N(-300),
            Op(6),
            Op(9),
            Op(14),
        ])
    }

    fn sample() -> Vec<u8> {
        let notdef = program(&[N(0), N(250), Op(13), Op(14)]);
        let space = program(&[N(0), N(250), Op(13), Op(14)]);
        // "B": a line drawn by a subroutine, after a hint replacement,
        // then a flex.
        let b = program(&[
            N(0),
            N(600),
            Op(13),
            N(2),
            N(1),
            N(3),
            Op(3072 + 16),
            Op(3072 + 17),
            Op(10),
            N(50),
            N(0),
            Op(21),
            N(0),
            Op(10),
            N(0),
            N(1),
            Op(3072 + 16),
            N(100),
            N(0),
            Op(21),
            N(0),
            N(2),
            Op(3072 + 16),
            N(0),
            N(0),
            Op(21),
            N(0),
            N(2),
            Op(3072 + 16),
            N(50),
            N(20),
            Op(21),
            N(0),
            N(2),
            Op(3072 + 16),
            N(50),
            N(0),
            Op(21),
            N(0),
            N(2),
            Op(3072 + 16),
            N(50),
            N(0),
            Op(21),
            N(0),
            N(2),
            Op(3072 + 16),
            N(50),
            N(-20),
            Op(21),
            N(0),
            N(2),
            Op(3072 + 16),
            N(0),
            N(0),
            Op(21),
            N(0),
            N(2),
            Op(3072 + 16),
            N(50),
            N(350),
            N(0),
            N(3),
            N(0),
            Op(3072 + 16),
            Op(3072 + 17),
            Op(3072 + 17),
            Op(3072 + 33),
            N(0),
            N(300),
            Op(5),
            Op(9),
            Op(14),
        ]);
        let acute = program(&[
            N(30),
            N(300),
            Op(13),
            N(0),
            N(700),
            Op(21),
            N(60),
            N(100),
            Op(5),
            N(-20),
            N(0),
            Op(5),
            Op(9),
            Op(14),
        ]);
        // "Aacute": seac of A and acute, the accent 150 right and 0 up.
        let aacute = program(&[
            N(0),
            N(500),
            Op(13),
            N(30),
            N(150),
            N(0),
            N(65),
            N(194),
            Op(3072 + 6),
        ]);
        // Subr 0 draws a line; subr 1 returns at once; subr 2 is a hint
        // set.
        let subrs = vec![
            program(&[N(200), N(0), Op(5), Op(11)]),
            program(&[Op(11)]),
            program(&[N(10), N(20), Op(1), Op(11)]),
        ];
        pfb(
            "TestTypeOne",
            &[
                (".notdef", notdef),
                ("space", space),
                ("A", box_glyph()),
                ("B", b),
                ("acute", acute),
                ("Aacute", aacute),
            ],
            &subrs,
            None,
        )
    }

    fn glyph<'a>(f: &'a Type1Font, name: &str) -> &'a Glyph {
        f.glyphs.iter().find(|g| g.name == name).expect(name)
    }

    #[test]
    fn a_pfb_reads_with_its_names_widths_and_outlines() {
        let f = Type1Font::parse(&sample()).expect("font");
        assert_eq!(f.font_name(), "TestTypeOne");
        assert_eq!(f.family, "TestTypeOne");
        assert_eq!(f.units_per_em, 1000);
        assert_eq!(f.glyphs[0].name, ".notdef");
        let a = glyph(&f, "A");
        assert_eq!(a.advance, 500.0);
        assert_eq!(
            a.path,
            vec![
                Seg::Move((100.0, 0.0)),
                Seg::Line((400.0, 0.0)),
                Seg::Line((400.0, 600.0)),
                Seg::Line((100.0, 600.0)),
            ]
        );
        // The subroutine's line, then the flex as two curves.
        let b = glyph(&f, "B");
        assert_eq!(b.path[0], Seg::Move((50.0, 0.0)));
        assert_eq!(b.path[1], Seg::Line((250.0, 0.0)));
        assert_eq!(
            b.path[2],
            Seg::Curve((350.0, 0.0), (400.0, 20.0), (450.0, 20.0))
        );
        assert_eq!(
            b.path[3],
            Seg::Curve((500.0, 20.0), (550.0, 0.0), (550.0, 0.0))
        );
        assert_eq!(b.path[4], Seg::Line((550.0, 300.0)));
        // The accented letter: A and the accent moved 150 right.
        let aa = glyph(&f, "Aacute");
        assert_eq!(aa.advance, 500.0);
        assert_eq!(&aa.path[..4], &a.path[..]);
        assert_eq!(aa.path[4], Seg::Move((150.0, 700.0)));
    }

    #[test]
    fn the_opentype_font_draws_the_type1_glyphs() {
        let otf = convert(&sample(), None).expect("converted");
        let face = ttf_parser::Face::parse(&otf, 0).expect("an OpenType font");
        assert_eq!(face.units_per_em(), 1000);
        let a = face.glyph_index('A').expect("A");
        assert_eq!(face.glyph_hor_advance(a), Some(500));
        struct Count(usize);
        impl ttf_parser::OutlineBuilder for Count {
            fn move_to(&mut self, _: f32, _: f32) {}
            fn line_to(&mut self, _: f32, _: f32) {
                self.0 += 1;
            }
            fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {}
            fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
                self.0 += 1;
            }
            fn close(&mut self) {}
        }
        let mut c = Count(0);
        let b = face.outline_glyph(a, &mut c).expect("outline");
        assert_eq!((b.x_min, b.y_min, b.x_max, b.y_max), (100, 0, 400, 600));
        assert!(c.0 >= 3);
        assert!(face.glyph_index('\u{C1}').is_some(), "Aacute by its name");
        assert!(face.glyph_index(' ').is_some());
        let names: Vec<String> = face
            .names()
            .into_iter()
            .filter_map(|n| n.to_string())
            .collect();
        assert!(names.iter().any(|n| n == "TestTypeOne"), "{names:?}");
        // The engine lists it and draws text in it.
        let fonts = crate::tests::system_with(&[&otf]);
        assert!(fonts.families().iter().any(|f| f == "TestTypeOne"));
        let got = fonts.outline(&[crate::TextSpan::new("AB", "TestTypeOne", 100.0)]);
        assert_eq!(got.glyphs, 2);
    }

    /// A `.pfm` beside the font names it as Windows lists it, with its
    /// weight and slant; a symbol character set maps the codes as the
    /// legacy script fonts do, so the letters typed draw (#193).
    #[test]
    fn the_pfm_names_the_font_and_a_symbol_set_maps_its_codes() {
        let mut pfm = vec![0u8; 140];
        pfm[0..2].copy_from_slice(&0x0100u16.to_le_bytes());
        pfm[80] = 1; // italic
        pfm[83..85].copy_from_slice(&700u16.to_le_bytes());
        pfm[85] = 2; // symbol
        pfm[105..109].copy_from_slice(&120u32.to_le_bytes());
        pfm[120..131].copy_from_slice(b"ML-Revathi\0");
        let info = PfmInfo::parse(&pfm).expect("pfm");
        assert_eq!(
            info,
            PfmInfo {
                face: "ML-Revathi".into(),
                weight: 700,
                italic: true,
                charset: 2
            }
        );
        // Glyph names that stand for no character, at codes of its own.
        let font = pfb(
            "Revathi",
            &[
                (".notdef", program(&[N(0), N(0), Op(13), Op(14)])),
                ("ka", box_glyph()),
                ("kha", box_glyph()),
            ],
            &[],
            Some(&[(0x6B, "ka"), (0x84, "kha")]),
        );
        let otf = convert(&font, Some(&pfm)).expect("converted");
        let face = ttf_parser::Face::parse(&otf, 0).expect("font");
        let names: Vec<String> = face
            .names()
            .into_iter()
            .filter_map(|n| n.to_string())
            .collect();
        assert!(names.iter().any(|n| n == "ML-Revathi"), "{names:?}");
        assert!(face.is_bold() && face.is_italic());
        let fonts = crate::tests::system_with(&[&otf]);
        assert!(fonts.families().iter().any(|f| f == "ML-Revathi"));
        for typed in ["k", "\u{201E}"] {
            let got = fonts.outline(&[crate::TextSpan::new(typed, "ML-Revathi", 100.0)]);
            assert_eq!(got.glyphs, 1, "{typed:?}");
            assert!(got.path.bounding_box().width() > 1.0, "{typed:?} draws");
        }
        // Without a .pfm the same font is a symbol font by its names.
        let (_, symbol) = char_map(&Type1Font::parse(&font).expect("font"), None);
        assert!(symbol);
    }

    /// The Type 1 fonts of a font folder are found by their .pfb, named
    /// by the .pfm beside them (any case), also when the .pfm is the file
    /// picked; one already loaded the usual way is not added twice.
    #[test]
    fn type1_fonts_in_a_folder_load_once() {
        let dir = std::env::temp_dir().join(format!("td-type1-{}", std::process::id()));
        let sub = dir.join("old");
        std::fs::create_dir_all(&sub).expect("dir");
        std::fs::write(sub.join("TESTT1.PFB"), sample()).expect("pfb");
        let mut pfm = vec![0u8; 140];
        pfm[0..2].copy_from_slice(&0x0100u16.to_le_bytes());
        pfm[83..85].copy_from_slice(&400u16.to_le_bytes());
        pfm[105..109].copy_from_slice(&120u32.to_le_bytes());
        pfm[120..131].copy_from_slice(b"Old Script\0");
        std::fs::write(sub.join("testt1.pfm"), &pfm).expect("pfm");
        std::fs::write(sub.join("notes.txt"), b"x").expect("txt");
        let found = fonts_under(std::slice::from_ref(&dir));
        assert_eq!(found.len(), 1);
        let family = |data: &[u8]| {
            let mut db = fontdb::Database::new();
            db.load_font_data(data.to_vec());
            let name = db.faces().next().map(|f| f.families[0].0.clone());
            name
        };
        assert_eq!(family(&found[0]).as_deref(), Some("Old Script"));
        let by_pfm = convert_file(&sub.join("testt1.pfm")).expect("by the pfm");
        assert_eq!(family(&by_pfm).as_deref(), Some("Old Script"));
        assert!(is_type1_file(&sub.join("testt1.pfm")) && !is_type1_path(&sub.join("testt1.pfm")));
        let mut db = fontdb::Database::new();
        assert_eq!(crate::load_type1_in(&mut db, std::slice::from_ref(&dir)), 1);
        assert_eq!(crate::load_type1_in(&mut db, std::slice::from_ref(&dir)), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_fonts_are_refused() {
        assert!(Type1Font::parse(b"").is_none());
        assert!(Type1Font::parse(b"%!PS-AdobeFont-1.0 nothing more").is_none());
        let mut cut = sample();
        cut.truncate(cut.len() / 2);
        let _ = Type1Font::parse(&cut);
        for n in [1, 7, 100, 300] {
            let mut bad = sample();
            for b in bad.iter_mut().skip(n).step_by(97) {
                *b ^= 0x5A;
            }
            if let Some(f) = Type1Font::parse(&bad) {
                let _ = to_opentype(&f, None);
            }
        }
        assert!(PfmInfo::parse(&[0; 10]).is_none());
    }
}

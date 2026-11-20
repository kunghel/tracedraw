//! Bitmap helpers: PNG codecs, resampling, inflation, colour masks and
//! the simple colour modes. The effects of the Effects menu are in `fx`,
//! Straighten Image in `straighten`.

use image::{ImageBuffer, Rgba, RgbaImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Grayscale,
    Rgb,
    Cmyk,
    /// Indexed (8-bit): a PNG with a palette (#2607).
    Paletted,
    /// Lab Color (24-bit): marked in the PNG (#2605).
    Lab,
    /// Black and White (1-bit): a 1-bit grey PNG, or a grey one marked so
    /// when some pixels are see-through (#2922).
    BlackWhite,
}

/// Convert the colour mode (Image > Mode).
pub fn convert_mode(img: &RgbaImage, mode: ColorMode) -> RgbaImage {
    match mode {
        ColorMode::Grayscale => {
            let mut out = img.clone();
            for p in out.pixels_mut() {
                let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                *p = Rgba([l, l, l, p[3]]);
            }
            out
        }
        // RGB and CMYK keep the pixels; the mode is a document attribute.
        // Indexed and Lab keep them here too: their pixels are made by
        // `bitmap_modes::paletted` and `lab_mode`, the mode by the PNG.
        ColorMode::Rgb
        | ColorMode::Cmyk
        | ColorMode::Paletted
        | ColorMode::Lab
        | ColorMode::BlackWhite => img.clone(),
    }
}

/// Grid points per channel of the table [`through_cmyk`] samples.
const CMYK_GRID: usize = 17;

/// `img` as it prints: every pixel taken to CMYK and back to the screen
/// through the colour engine (the drawing's profiles, or the press model
/// without them), so a picture converted to CMYK shows the colours the
/// inks can make (#1254). Alpha is kept. The engine is sampled on a grid
/// of `CMYK_GRID` points per channel and the pixels read from it by
/// trilinear interpolation, so a large photo takes a few milliseconds.
pub fn through_cmyk(img: &RgbaImage) -> RgbaImage {
    let n = CMYK_GRID;
    let step = 255.0 / (n - 1) as f32;
    let mut grid: Vec<[f32; 3]> = Vec::with_capacity(n * n * n);
    for r in 0..n {
        for g in 0..n {
            for b in 0..n {
                let c = tracedraw_core::Color::rgb8(
                    (r as f32 * step).round() as u8,
                    (g as f32 * step).round() as u8,
                    (b as f32 * step).round() as u8,
                );
                let back = c.convert_to("CMYK").convert_to("RGB").to_rgb8();
                grid.push([back[0] as f32, back[1] as f32, back[2] as f32]);
            }
        }
    }
    let at = |r: usize, g: usize, b: usize| grid[(r * n + g) * n + b];
    let mut out = img.clone();
    for p in out.pixels_mut() {
        let pos = |v: u8| {
            let x = v as f32 / step;
            let i = (x.floor() as usize).min(n - 2);
            (i, x - i as f32)
        };
        let (ri, rf) = pos(p[0]);
        let (gi, gf) = pos(p[1]);
        let (bi, bf) = pos(p[2]);
        let mut acc = [0.0f32; 3];
        for (dr, wr) in [(0, 1.0 - rf), (1, rf)] {
            for (dg, wg) in [(0, 1.0 - gf), (1, gf)] {
                for (db, wb) in [(0, 1.0 - bf), (1, bf)] {
                    let w = wr * wg * wb;
                    if w > 0.0 {
                        let v = at(ri + dr, gi + dg, bi + db);
                        acc[0] += w * v[0];
                        acc[1] += w * v[1];
                        acc[2] += w * v[2];
                    }
                }
            }
        }
        *p = Rgba([
            acc[0].round().clamp(0.0, 255.0) as u8,
            acc[1].round().clamp(0.0, 255.0) as u8,
            acc[2].round().clamp(0.0, 255.0) as u8,
            p[3],
        ]);
    }
    out
}

/// Decode PNG bytes into an RGBA image.
pub fn decode(png: &[u8]) -> Option<RgbaImage> {
    image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()
        .map(|i| i.to_rgba8())
}

/// Encode an RGBA image as PNG bytes.
pub fn encode(img: &RgbaImage) -> Option<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(buf.into_inner())
}

/// `img` laid on white paper: every pixel opaque, a see-through one
/// mixed with white by its opacity.
pub fn flatten_on_white(img: &RgbaImage) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels_mut() {
        let a = u16::from(p[3]);
        for c in 0..3 {
            p[c] = ((u16::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
        }
        p[3] = 255;
    }
    out
}

/// Encode `img` as a grayscale PNG (8-bit gray, with an alpha channel
/// only when a pixel is not opaque): a grayscale picture, whose colour
/// mode the status line reads from the PNG (#1777). Each pixel's level
/// is its luminance, `0.299 R + 0.587 G + 0.114 B`.
pub fn encode_gray(img: &RgbaImage) -> Option<Vec<u8>> {
    let level = |p: &Rgba<u8>| {
        (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    if img.pixels().all(|p| p[3] == 255) {
        let gray = image::GrayImage::from_fn(img.width(), img.height(), |x, y| {
            image::Luma([level(img.get_pixel(x, y))])
        });
        gray.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    } else {
        let gray = image::GrayAlphaImage::from_fn(img.width(), img.height(), |x, y| {
            let p = img.get_pixel(x, y);
            image::LumaA([level(p), p[3]])
        });
        gray.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    }
    Some(buf.into_inner())
}

/// The colour mode a bitmap is in: CMYK when marked so, black and white
/// when its PNG is 1-bit (or marked so), grayscale when its PNG holds
/// one grey channel, indexed when it holds a palette, Lab
/// when the PNG is marked Lab, else RGB (the status bar and the Mode
/// menu, #1254, #2330, #2605, #2607).
pub fn mode_of(cmyk: bool, png: &[u8]) -> ColorMode {
    if cmyk {
        ColorMode::Cmyk
    } else if png_is_black_white(png) {
        ColorMode::BlackWhite
    } else if png_is_gray(png) {
        ColorMode::Grayscale
    } else if png_is_paletted(png) {
        ColorMode::Paletted
    } else if png_is_lab(png) {
        ColorMode::Lab
    } else {
        ColorMode::Rgb
    }
}

/// Whether `png` holds a palette (colour type 3 in its header): a bitmap
/// in the Indexed (8-bit) mode (#2607).
pub fn png_is_paletted(png: &[u8]) -> bool {
    png.get(..8) == Some(b"\x89PNG\r\n\x1a\n".as_slice())
        && png.get(12..16) == Some(b"IHDR".as_slice())
        && png.get(25) == Some(&3)
}

/// The `tEXt` keyword that marks a PNG as a Lab Color bitmap.
const LAB_KEY: &[u8] = b"TraceDraw color mode";

/// The chunks of a PNG after its signature: (type, data).
fn png_chunks(png: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut at = 8usize;
    std::iter::from_fn(move || {
        let len = u32::from_be_bytes(png.get(at..at + 4)?.try_into().ok()?) as usize;
        let ty = png.get(at + 4..at + 8)?;
        let data = png.get(at + 8..at + 8 + len)?;
        at += 12 + len;
        Some((ty, data))
    })
}

/// Whether `png` is marked as a Lab Color bitmap (Image > Mode > Lab
/// Color, #2605): a `tEXt` chunk naming the mode, which the pixels (RGB
/// for the screen, as a CMYK bitmap's are) cannot tell.
pub fn png_is_lab(png: &[u8]) -> bool {
    png_mode_mark(png) == Some(b"Lab".as_slice())
}

/// The colour mode a PNG is marked with in its `tEXt` chunk, if any.
fn png_mode_mark(png: &[u8]) -> Option<&[u8]> {
    if png.get(..8) != Some(b"\x89PNG\r\n\x1a\n".as_slice()) {
        return None;
    }
    png_chunks(png)
        .take_while(|(ty, _)| *ty != b"IDAT")
        .find_map(|(ty, data)| {
            (ty == b"tEXt")
                .then(|| data.strip_prefix(LAB_KEY)?.strip_prefix(b"\0"))
                .flatten()
        })
}

/// Whether `png` is a Black and White (1-bit) bitmap: a grey PNG of one
/// bit per pixel, or a grey one marked so (see-through pixels need an
/// alpha channel, which 1-bit PNGs lack) (#2922).
pub fn png_is_black_white(png: &[u8]) -> bool {
    let one_bit = png.get(..8) == Some(b"\x89PNG\r\n\x1a\n".as_slice())
        && png.get(12..16) == Some(b"IHDR".as_slice())
        && png.get(24) == Some(&1)
        && png.get(25) == Some(&0);
    one_bit || png_mode_mark(png) == Some(b"BlackWhite".as_slice())
}

/// Encode a black and white picture (every pixel black or white) as a
/// Black and White (1-bit) bitmap: a 1-bit grey PNG when it is opaque, a
/// grey PNG with alpha marked Black and White when some pixels are
/// see-through. Images > Mode > Black and White left the picture RGB
/// (#2922).
pub fn encode_bw(img: &RgbaImage) -> Option<Vec<u8>> {
    use std::io::Write;
    let white =
        |p: &Rgba<u8>| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32 >= 127.5;
    if !img.pixels().all(|p| p[3] == 255) {
        let bw = ImageBuffer::from_fn(img.width(), img.height(), |x, y| {
            let p = img.get_pixel(x, y);
            let v = if white(p) { 255 } else { 0 };
            Rgba([v, v, v, p[3]])
        });
        return with_mode_mark(&encode_gray(&bw)?, b"BlackWhite");
    }
    let (w, h) = img.dimensions();
    let row = (w as usize).div_ceil(8);
    let mut raw = Vec::with_capacity((row + 1) * h as usize);
    for y in 0..h {
        raw.push(0); // filter: none
        let start = raw.len();
        raw.resize(start + row, 0);
        for x in 0..w {
            if white(img.get_pixel(x, y)) {
                raw[start + x as usize / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&raw).ok()?;
    let idat = z.finish().ok()?;
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[1, 0, 0, 0, 0]);
    put_chunk(&mut out, b"IHDR", &ihdr);
    put_chunk(&mut out, b"IDAT", &idat);
    put_chunk(&mut out, b"IEND", &[]);
    Some(out)
}

/// One PNG chunk: length, type, data and the CRC of type and data.
fn put_chunk(out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(ty);
    out.extend_from_slice(data);
    let mut crc = flate2::Crc::new();
    crc.update(ty);
    crc.update(data);
    out.extend_from_slice(&crc.sum().to_be_bytes());
}

/// Encode `img` as an indexed PNG (colour type 3, 8 bits): its colours
/// in a palette, with a `tRNS` chunk when some are see-through. `None`
/// when it has more than 256 colours (with their opacity). The Indexed
/// (8-bit) mode stores its bitmaps so; they were saved as RGB (#2607).
pub fn encode_indexed(img: &RgbaImage) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut colors: Vec<[u8; 4]> = Vec::new();
    let mut index: std::collections::HashMap<[u8; 4], u8> = std::collections::HashMap::new();
    let (w, h) = img.dimensions();
    let mut raw = Vec::with_capacity((w as usize + 1) * h as usize);
    for y in 0..h {
        raw.push(0); // filter: none
        for x in 0..w {
            let mut c = img.get_pixel(x, y).0;
            if c[3] == 0 {
                c = [0, 0, 0, 0];
            }
            let i = match index.get(&c) {
                Some(i) => *i,
                None => {
                    if colors.len() == 256 {
                        return None;
                    }
                    let i = colors.len() as u8;
                    colors.push(c);
                    index.insert(c, i);
                    i
                }
            };
            raw.push(i);
        }
    }
    if colors.is_empty() {
        colors.push([0, 0, 0, 0]);
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&raw).ok()?;
    let idat = z.finish().ok()?;
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 3, 0, 0, 0]);
    put_chunk(&mut out, b"IHDR", &ihdr);
    let plte: Vec<u8> = colors.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
    put_chunk(&mut out, b"PLTE", &plte);
    if let Some(last) = colors.iter().rposition(|c| c[3] != 255) {
        let trns: Vec<u8> = colors[..=last].iter().map(|c| c[3]).collect();
        put_chunk(&mut out, b"tRNS", &trns);
    }
    put_chunk(&mut out, b"IDAT", &idat);
    put_chunk(&mut out, b"IEND", &[]);
    Some(out)
}

/// Encode `img`, a picture in the Indexed (8-bit) mode, as an indexed
/// PNG in any case (#3504, #3539). Its palette colours with the many
/// see-through levels of soft edges made more than 256 entries, and the
/// picture was stored as RGB: the see-through levels are then made clear
/// or opaque (at half), as a palette picture keeps one clear entry, and
/// when more than 255 colours are still left beside it they are taken
/// to a palette of 255. `None` only when the picture cannot be encoded.
pub fn encode_indexed_fitted(img: &RgbaImage) -> Option<Vec<u8>> {
    if let Some(png) = encode_indexed(img) {
        return Some(png);
    }
    let mut snapped = img.clone();
    for p in snapped.pixels_mut() {
        *p = if p[3] < 128 {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([p[0], p[1], p[2], 255])
        };
    }
    if let Some(png) = encode_indexed(&snapped) {
        return Some(png);
    }
    let settings = crate::bitmap_modes::PalettedSettings {
        colors: 255,
        dither: crate::bitmap_modes::Dither::None,
        ..Default::default()
    };
    let fewer = crate::bitmap_modes::paletted(&snapped, &settings);
    encode_indexed(&fewer).or_else(|| encode(&fewer))
}

/// Encode `img` as a PNG marked Lab Color (see [`png_is_lab`]).
pub fn encode_lab(img: &RgbaImage) -> Option<Vec<u8>> {
    with_mode_mark(&encode(img)?, b"Lab")
}

/// `png` with a `tEXt` chunk marking its colour mode `mode`.
fn with_mode_mark(png: &[u8], mode: &[u8]) -> Option<Vec<u8>> {
    // After the signature and the IHDR chunk (8 + 25 bytes).
    let at = 8 + 12 + u32::from_be_bytes(png.get(8..12)?.try_into().ok()?) as usize;
    let mut text = LAB_KEY.to_vec();
    text.push(0);
    text.extend_from_slice(mode);
    let mut chunk = Vec::new();
    put_chunk(&mut chunk, b"tEXt", &text);
    let mut out = png.get(..at)?.to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(png.get(at..)?);
    Some(out)
}

/// Whether `png` is a grayscale PNG (colour type 0 or 4 in its header):
/// a bitmap in the Grayscale colour mode (#1777).
pub fn png_is_gray(png: &[u8]) -> bool {
    png.get(..8) == Some(b"\x89PNG\r\n\x1a\n".as_slice())
        && png.get(12..16) == Some(b"IHDR".as_slice())
        && matches!(png.get(25), Some(0 | 4))
}

/// Resample to a new pixel size (Image > Resample).
pub fn resample(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    image::imageops::resize(
        img,
        w.max(1),
        h.max(1),
        image::imageops::FilterType::Lanczos3,
    )
}

/// Grow the canvas to `w` by `h` pixels (at least the image's size), the
/// image in the middle and transparent around it.
pub fn inflate_to(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (w, h) = (w.max(img.width()), h.max(img.height()));
    let mut out = ImageBuffer::from_pixel(w, h, Rgba([0, 0, 0, 0]));
    let (left, top) = ((w - img.width()) / 2, (h - img.height()) / 2);
    image::imageops::overlay(&mut out, img, left as i64, top as i64);
    out
}

/// Grow the canvas by `px` on every side (Bitmap Padding).
pub fn inflate(img: &RgbaImage, px: u32) -> RgbaImage {
    let mut out = ImageBuffer::from_pixel(
        img.width() + 2 * px,
        img.height() + 2 * px,
        Rgba([0, 0, 0, 0]),
    );
    image::imageops::overlay(&mut out, img, px as i64, px as i64);
    out
}

/// The slots of the Color Mask docker.
pub const MASK_SLOTS: usize = 10;

/// One colour of a bitmap mask: in use, the colour, and how far (in % of
/// the channel range) a pixel may be from it and still match.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaskSlot {
    pub on: bool,
    pub color: tracedraw_core::Color,
    pub tolerance: u8,
}

impl Default for MaskSlot {
    fn default() -> Self {
        MaskSlot {
            on: false,
            color: tracedraw_core::Color::BLACK,
            tolerance: 0,
        }
    }
}

/// The Color Mask docker's state.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskSettings {
    pub slots: [MaskSlot; MASK_SLOTS],
    pub picked: usize,
    /// Hide the selected colours (else show only them).
    pub hide: bool,
    /// The eyedropper fills the picked slot with the colour it samples.
    pub picking: bool,
    /// The sampled colour already taken, so one sample fills one slot.
    pub seen: Option<tracedraw_core::Color>,
}

impl Default for MaskSettings {
    fn default() -> Self {
        MaskSettings {
            slots: [MaskSlot::default(); MASK_SLOTS],
            picked: 0,
            hide: false,
            picking: false,
            seen: None,
        }
    }
}

impl MaskSettings {
    /// The slots in use.
    pub fn active(&self) -> Vec<MaskSlot> {
        self.slots.iter().copied().filter(|s| s.on).collect()
    }

    /// The mask as a text file: one line per slot, "on #rrggbb tolerance".
    pub fn to_text(&self) -> String {
        self.slots
            .iter()
            .map(|s| format!("{} {} {}\n", u8::from(s.on), s.color.to_hex(), s.tolerance))
            .collect()
    }

    /// Read a mask written by [`MaskSettings::to_text`]; lines that do not
    /// read are skipped.
    pub fn read_text(&mut self, text: &str) -> bool {
        let mut slots = [MaskSlot::default(); MASK_SLOTS];
        let mut n = 0;
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(on), Some(hex), Some(tol)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let (Some(color), Ok(tol)) = (tracedraw_core::Color::from_hex(hex), tol.parse::<u8>())
            else {
                continue;
            };
            if n < MASK_SLOTS {
                slots[n] = MaskSlot {
                    on: on == "1",
                    color,
                    tolerance: tol.min(100),
                };
                n += 1;
            }
        }
        if n > 0 {
            self.slots = slots;
        }
        n > 0
    }
}

/// Bitmap mask: with `hide`, pixels matching any slot become transparent;
/// without, only the matching pixels stay. A slot matches a pixel whose
/// channels are each within the slot's tolerance (% of 255).
pub fn mask_bitmap(img: &RgbaImage, slots: &[MaskSlot], hide: bool) -> RgbaImage {
    let targets: Vec<([i32; 3], i32)> = slots
        .iter()
        .map(|s| {
            let [r, g, b] = s.color.to_rgb8();
            (
                [r as i32, g as i32, b as i32],
                (s.tolerance.min(100) as i32 * 255 + 50) / 100,
            )
        })
        .collect();
    crate::fx::util::map_px(img, |_, _, p| {
        let hit = targets.iter().any(|(c, t)| {
            (p[0] as i32 - c[0]).abs() <= *t
                && (p[1] as i32 - c[1]).abs() <= *t
                && (p[2] as i32 - c[2]).abs() <= *t
        });
        if hit == hide {
            Rgba([p[0], p[1], p[2], 0])
        } else {
            *p
        }
    })
}

/// The `n` colours a bitmap uses most (opaque pixels, near colours
/// merged), most used first.
pub fn main_colors(img: &RgbaImage, n: usize) -> Vec<[u8; 3]> {
    let mut counts: std::collections::HashMap<[u8; 3], usize> = Default::default();
    for p in img.pixels() {
        if p[3] < 128 {
            continue;
        }
        // Merged by their top five bits, so noise does not split a colour.
        let key = [p[0] & 0xF8, p[1] & 0xF8, p[2] & 0xF8];
        *counts.entry(key).or_default() += 1;
    }
    let mut v: Vec<([u8; 3], usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter()
        .take(n)
        .map(|(c, _)| [c[0] | 4, c[1] | 4, c[2] | 4])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient() -> RgbaImage {
        ImageBuffer::from_fn(32, 32, |x, _| Rgba([(x * 8) as u8, 128, 64, 255]))
    }

    #[test]
    fn grayscale_mode_uses_luminance() {
        let g = convert_mode(&gradient(), ColorMode::Grayscale);
        let p = g.get_pixel(10, 3);
        assert!(p[0] == p[1] && p[1] == p[2]);
        assert_eq!(convert_mode(&gradient(), ColorMode::Rgb), gradient());
    }

    #[test]
    fn inflate_pads_with_transparency() {
        let g = gradient();
        let i = inflate(&g, 5);
        assert_eq!(i.dimensions(), (42, 42));
        assert_eq!(i.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn png_round_trip() {
        let g = gradient();
        let png = encode(&g).unwrap();
        let back = decode(&png).unwrap();
        assert_eq!(back.get_pixel(10, 10), g.get_pixel(10, 10));
    }

    #[test]
    fn a_bitmap_mask_hides_or_keeps_its_colours() {
        let img: RgbaImage = ImageBuffer::from_fn(4, 1, |x, _| match x {
            0 => Rgba([250, 10, 10, 255]),
            1 => Rgba([240, 20, 20, 255]),
            2 => Rgba([10, 10, 250, 255]),
            _ => Rgba([255, 255, 255, 255]),
        });
        let red = MaskSlot {
            on: true,
            color: tracedraw_core::Color::rgb8(255, 0, 0),
            tolerance: 10,
        };
        // 10 % is 26 levels: both reds match, blue and white do not.
        let hidden = mask_bitmap(&img, &[red], true);
        let alpha: Vec<u8> = hidden.pixels().map(|p| p[3]).collect();
        assert_eq!(alpha, vec![0, 0, 255, 255]);
        let kept = mask_bitmap(&img, &[red], false);
        let alpha: Vec<u8> = kept.pixels().map(|p| p[3]).collect();
        assert_eq!(alpha, vec![255, 255, 0, 0]);
        // The main colours, most used first.
        let many: RgbaImage = ImageBuffer::from_fn(10, 1, |x, _| {
            if x < 7 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let main = main_colors(&many, 10);
        assert_eq!(main.len(), 2);
        assert!(main[0][0] < 10 && main[1][0] > 245);
        // The mask file reads back.
        let mut m = MaskSettings::default();
        m.slots[0] = red;
        m.slots[3].tolerance = 40;
        let text = m.to_text();
        let mut back = MaskSettings::default();
        assert!(back.read_text(&text));
        assert_eq!(back.slots, m.slots);
        assert!(!back.read_text("nothing here"));
    }

    #[test]
    fn indexed_png_holds_a_palette_and_reads_back() {
        // #2607: colour type 3, the colours and their opacity read back.
        let img = RgbaImage::from_fn(5, 3, |x, y| {
            Rgba([
                x as u8 * 50,
                y as u8 * 90,
                7,
                if x == 4 { 128 } else { 255 },
            ])
        });
        let png = encode_indexed(&img).expect("16 colours fit");
        assert!(png_is_paletted(&png) && !png_is_lab(&png));
        assert_eq!(mode_of(false, &png), ColorMode::Paletted);
        assert_eq!(decode(&png).expect("reads"), img);
        // 300 colours do not fit a palette.
        let many = RgbaImage::from_fn(300, 1, |x, _| Rgba([x as u8, (x >> 8) as u8, 0, 255]));
        assert!(encode_indexed(&many).is_none());
        // Lab: marked, the pixels as they are.
        let lab = encode_lab(&img).expect("png");
        assert!(png_is_lab(&lab) && !png_is_paletted(&lab));
        assert_eq!(mode_of(false, &lab), ColorMode::Lab);
        assert_eq!(decode(&lab).expect("reads"), img);
        assert!(!png_is_lab(&encode(&img).expect("png")));
    }

    #[test]
    fn black_and_white_bitmaps_are_one_bit_pngs() {
        // #2922: opaque, a 1-bit grey PNG; with see-through pixels, a grey
        // PNG marked Black and White. Either reads back as it was.
        let img = RgbaImage::from_fn(13, 5, |x, y| {
            let v = if (x + y) % 3 == 0 { 0 } else { 255 };
            Rgba([v, v, v, 255])
        });
        let png = encode_bw(&img).expect("encodes");
        assert_eq!(png[24], 1, "one bit per pixel");
        assert_eq!(png[25], 0, "grey");
        assert_eq!(mode_of(false, &png), ColorMode::BlackWhite);
        assert_eq!(decode(&png).expect("decodes"), img);
        let mut clear = img.clone();
        clear.get_pixel_mut(0, 0)[3] = 0;
        let png = encode_bw(&clear).expect("encodes");
        assert_eq!(mode_of(false, &png), ColorMode::BlackWhite);
        assert!(!png_is_lab(&png));
        let back = decode(&png).expect("decodes");
        assert_eq!(back.get_pixel(0, 0)[3], 0);
        assert_eq!(back.get_pixel(1, 0), img.get_pixel(1, 0));
        // A grey picture is not black and white, nor is a Lab one.
        assert_eq!(
            mode_of(false, &encode_gray(&img).expect("gray")),
            ColorMode::Grayscale
        );
        assert_eq!(
            mode_of(false, &encode_lab(&img).expect("lab")),
            ColorMode::Lab
        );
    }
}

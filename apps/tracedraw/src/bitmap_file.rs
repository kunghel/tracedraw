//! The file of one picture (#1834): Save Bitmap As writes a picture as
//! PNG, JPEG or TIFF, and Edit Bitmap hands it to another program and
//! reads it back.
//!
//! A drawing keeps a picture's pixels as a PNG, RGB for the screen, and
//! for a print picture (a `.cdr` CMYK image, or a picture in the CMYK
//! colour mode) also its mode and the ink channels it prints from
//! (`Shape::picture_inks`, #1659). A CMYK picture saved as JPEG or TIFF
//! is written in CMYK from those inks (or its pixels through the colour
//! management when it keeps none), with the CMYK writers of the
//! JPEG and TIFF export (#1763); PNG holds no CMYK and is written as the
//! drawing keeps it. A CMYK TIFF read back gives the picture its inks
//! again.

use tracedraw_core::document::ShapeKind;
use tracedraw_core::picture_inks::PictureInks;
use tracedraw_core::{Color, Document, Shape};

/// The kinds of file a picture is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitmapFile {
    Png,
    Jpeg,
    Tiff,
}

impl BitmapFile {
    /// In the order the save dialog lists them.
    pub const ALL: [BitmapFile; 3] = [BitmapFile::Png, BitmapFile::Jpeg, BitmapFile::Tiff];

    /// The file name extension written.
    pub fn ext(self) -> &'static str {
        match self {
            BitmapFile::Png => "png",
            BitmapFile::Jpeg => "jpg",
            BitmapFile::Tiff => "tif",
        }
    }

    /// The extensions the save dialog's filter takes.
    pub fn exts(self) -> &'static [&'static str] {
        match self {
            BitmapFile::Png => &["png"],
            BitmapFile::Jpeg => &["jpg", "jpeg"],
            BitmapFile::Tiff => &["tif", "tiff"],
        }
    }

    /// The name of the save dialog's filter (a format name).
    pub fn filter_name(self) -> &'static str {
        match self {
            BitmapFile::Png => "PNG",
            BitmapFile::Jpeg => "JPEG",
            BitmapFile::Tiff => "TIFF",
        }
    }

    /// The kind a file name's extension names, if one of these.
    pub fn from_name(name: &str) -> Option<BitmapFile> {
        let ext = std::path::Path::new(name)
            .extension()?
            .to_str()?
            .to_ascii_lowercase();
        BitmapFile::ALL
            .into_iter()
            .find(|k| k.exts().contains(&ext.as_str()))
    }
}

/// Whether `s` is a picture in the CMYK colour mode or keeping ink
/// channels that fit its pixels.
pub fn is_cmyk(s: &Shape) -> bool {
    let ShapeKind::Bitmap { png, .. } = &s.kind else {
        return false;
    };
    s.cmyk_bitmap
        || s.picture_inks
            .as_deref()
            .is_some_and(|i| i.fits(png.as_slice()))
}

/// The kind of file a picture is saved as unless the user picks another:
/// the kind of the file it is linked to or was imported from (its
/// name), else TIFF for a CMYK picture, which keeps its inks without
/// loss, else PNG.
pub fn own_kind(s: &Shape) -> BitmapFile {
    let link = s
        .data
        .iter()
        .find(|(k, _)| k == "bitmap.link")
        .map(|(_, v)| v.as_str());
    link.and_then(BitmapFile::from_name)
        .or_else(|| s.name.as_deref().and_then(BitmapFile::from_name))
        .unwrap_or(if is_cmyk(s) || is_lab(s) {
            BitmapFile::Tiff
        } else {
            BitmapFile::Png
        })
}

/// Whether `s` is a picture in the Lab Color mode. PNG holds no Lab, so
/// it is written as a CIE Lab TIFF (#3539).
pub fn is_lab(s: &Shape) -> bool {
    matches!(&s.kind, ShapeKind::Bitmap { png, .. } if crate::bitmap_fx::png_is_lab(png))
}

/// The picture's resolution in dots per inch, from its pixel width and
/// its width on the page.
fn dpi(s: &Shape) -> f64 {
    let ShapeKind::Bitmap { rect, width_px, .. } = &s.kind else {
        return 0.0;
    };
    let c = s.transform.as_coeffs();
    let mm = rect.width().abs() * c[0].hypot(c[1]);
    if mm > 0.0 && mm.is_finite() {
        f64::from(*width_px) * 25.4 / mm
    } else {
        0.0
    }
}

/// JPEG quality of a saved picture.
const QUALITY: u8 = 92;

/// The file of picture `s` of `doc` as `kind`; `icc` is the CMYK
/// profile a CMYK file carries.
pub fn write(
    doc: &Document,
    s: &Shape,
    kind: BitmapFile,
    icc: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let ShapeKind::Bitmap { png, .. } = &s.kind else {
        return Err("not a bitmap".into());
    };
    let dpi = dpi(s);
    if kind == BitmapFile::Png {
        return Ok(png.to_vec());
    }
    if is_cmyk(s) {
        let spots: Vec<(u32, [f32; 4])> = doc.inks.iter().map(|i| (i.id, i.cmyk)).collect();
        let cmyk = |alpha| {
            crate::picture_plates::picture_cmyk(s, &spots, alpha)
                .ok_or("the picture cannot be read")
        };
        let out = match kind {
            BitmapFile::Jpeg => {
                // No transparency in a JPEG: the inks over white paper.
                let (w, h, ink) = cmyk(false)?;
                tracedraw_io::cmyk_bitmap::cmyk_jpeg(w, h, &ink, QUALITY, dpi, icc)
            }
            _ => {
                // A TIFF keeps the transparency as an alpha channel
                // beside the inks (#1925); an opaque picture stays four
                // channels.
                let (w, h, inka) = cmyk(true)?;
                if inka.chunks_exact(5).all(|p| p[4] == 255) {
                    let ink: Vec<u8> = inka
                        .chunks_exact(5)
                        .flat_map(|p| [p[0], p[1], p[2], p[3]])
                        .collect();
                    tracedraw_io::cmyk_bitmap::cmyk_tiff(w, h, &ink, dpi, icc)
                } else {
                    tracedraw_io::cmyk_bitmap::cmyka_tiff(w, h, &inka, dpi, icc)
                }
            }
        };
        return out.ok_or_else(|| "CMYK encoding failed".to_string());
    }
    let img = crate::bitmap_fx::decode(png).ok_or("the picture cannot be read")?;
    if kind == BitmapFile::Tiff && crate::bitmap_fx::png_is_lab(png) {
        // A Lab Color picture stays Lab: it went out as RGB (#3539).
        return Ok(lab_tiff(&img, dpi));
    }
    let gray = crate::bitmap_fx::png_is_gray(png);
    let mut out = std::io::Cursor::new(Vec::new());
    match kind {
        BitmapFile::Jpeg => {
            // No transparency in a JPEG: the picture over white paper.
            let flat = image::RgbImage::from_fn(img.width(), img.height(), |x, y| {
                let p = img.get_pixel(x, y).0;
                let a = u16::from(p[3]);
                image::Rgb(
                    [p[0], p[1], p[2]]
                        .map(|v| ((u16::from(v) * a + 255 * (255 - a) + 127) / 255) as u8),
                )
            });
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, QUALITY);
            if gray {
                image::DynamicImage::ImageRgb8(flat)
                    .to_luma8()
                    .write_with_encoder(enc)
            } else {
                flat.write_with_encoder(enc)
            }
            .map_err(|e| e.to_string())?;
            Ok(crate::export::record_dpi(out.into_inner(), "jpg", dpi))
        }
        _ => {
            let opaque = img.pixels().all(|p| p[3] == 255);
            let dynamic = image::DynamicImage::ImageRgba8(img);
            let dynamic = match (gray, opaque) {
                (true, true) => image::DynamicImage::ImageLuma8(dynamic.to_luma8()),
                (true, false) => image::DynamicImage::ImageLumaA8(dynamic.to_luma_alpha8()),
                (false, true) => image::DynamicImage::ImageRgb8(dynamic.to_rgb8()),
                (false, false) => dynamic,
            };
            dynamic
                .write_to(&mut out, image::ImageFormat::Tiff)
                .map_err(|e| e.to_string())?;
            Ok(out.into_inner())
        }
    }
}

/// A picture read from a file: its pixels and, from a CMYK TIFF, its
/// inks.
pub struct ReadPicture {
    pub rgba: image::RgbaImage,
    /// The ink channels (cyan, magenta, yellow, black) as the grey PNG
    /// `PictureInks` keeps, when the file is CMYK.
    pub planes: Option<Vec<u8>>,
    /// The colour mode the file stores its pixels in (#3025).
    pub mode: FileMode,
}

impl ReadPicture {
    /// The pixels as the PNG a bitmap keeps, and the ink channels that
    /// belong to it.
    pub fn into_png(self) -> Option<(Vec<u8>, Option<Box<PictureInks>>)> {
        let png = if self.planes.is_none() {
            encode_as(&self.rgba, self.mode)?
        } else {
            crate::bitmap_fx::encode(&self.rgba)?
        };
        let inks = self
            .planes
            .map(|p| Box::new(PictureInks::cmyk(&png, p.into())));
        Some((png, inks))
    }
}

/// Read a picture file: a CMYK TIFF (8 bits per ink, with or without
/// alpha) or a CMYK JPEG keeps its inks, and its pixels show them
/// through the colour management, as a CMYK colour of the drawing
/// shows (#2163); every other file is read as pixels.
pub fn read(bytes: &[u8]) -> Result<ReadPicture, String> {
    if let Some(p) = read_cmyk_tiff(bytes) {
        return Ok(p);
    }
    if let Some(p) = read_cmyk_jpeg(bytes) {
        return Ok(p);
    }
    if let Some(p) = read_palette_tiff(bytes) {
        return Ok(p);
    }
    if let Some(p) = read_lab_tiff(bytes) {
        return Ok(p);
    }
    read_within(bytes, crate::files::picture_decode_limit())
}

/// Read a picture file through the image library within `limit` bytes;
/// a TIFF the library turns away (too large for its memory guard, extra
/// channels it takes no colour from) is read again strip by strip or
/// tile by tile ([`read_tiff_by_chunks`], #4036).
fn read_within(bytes: &[u8], limit: u64) -> Result<ReadPicture, String> {
    match crate::files::decode_picture_within(bytes, limit) {
        Ok(img) => Ok(ReadPicture {
            rgba: img.into_rgba8(),
            planes: None,
            mode: file_mode(bytes),
        }),
        Err(e) => read_tiff_by_chunks(bytes, picture_pixel_budget(limit)).ok_or_else(|| e.to_string()),
    }
}

/// Most pixels a picture read piece by piece keeps: as many as the
/// decoding limit holds at four bytes a pixel, and never more than a
/// PDF's pictures read at their own size.
fn picture_pixel_budget(limit: u64) -> u64 {
    (limit / 4).min(tracedraw_io::pdf_import::FULL_PICTURE_PIXELS)
}

/// The colours a TIFF stores, as [`read_tiff_by_chunks`] takes them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TiffInks {
    /// Grey levels (the TIFF reader turns white-is-zero files round).
    Grey,
    Rgb,
    Cmyk,
}

/// A TIFF read one strip or tile at a time into an RGBA picture (#4036).
/// The image library decodes the whole picture into one buffer, and its
/// guard on that buffer turned a 10000 x 21000 pixel TIFF away with
/// "Memory limit exceeded"; here only one piece is decoded at a time, so
/// a picture takes the memory of its pixels and no more. A picture of
/// more than `max_pixels` pixels is left to the library's answer. Grey, RGB and CMYK files are read, 1 to 16
/// bits a sample or 32-bit floating point, chunky or planar, with
/// their alpha (an extra channel the file marks as alpha) and with any
/// other extra channels (a picture's saved selections) left out.
pub fn read_tiff_by_chunks(bytes: &[u8], max_pixels: u64) -> Option<ReadPicture> {
    use tiff::decoder::{ChunkType, Decoder, DecodingResult, Limits};
    use tiff::tags::Tag;
    let tiff_magic = [&b"II*\0"[..], b"MM\0*", b"II+\0", b"MM\0+"];
    if !tiff_magic.iter().any(|m| bytes.starts_with(m)) {
        return None;
    }
    // One piece at a time: a piece may be large (a whole picture in one
    // strip), the tags hold no more than their file.
    let piece = bytes
        .len()
        .max(1 << 20)
        .saturating_mul(64)
        .clamp(256 << 20, 2 << 30);
    let limits = {
        let mut l = Limits::default();
        l.decoding_buffer_size = piece;
        l.intermediate_buffer_size = piece;
        l.ifd_value_size = piece;
        l
    };
    let mut dec = Decoder::new(std::io::Cursor::new(bytes))
        .ok()?
        .with_limits(limits);
    let (w, h) = dec.dimensions().ok()?;
    if w == 0 || h == 0 {
        return None;
    }
    let photometric: u16 = dec
        .find_tag_unsigned(Tag::PhotometricInterpretation)
        .ok()
        .flatten()
        .unwrap_or(1);
    let (inks, colours, bits, samples) = match dec.colortype().ok()? {
        tiff::ColorType::Gray(b) => (TiffInks::Grey, 1, b, 1),
        tiff::ColorType::GrayA(b) => (TiffInks::Grey, 1, b, 2),
        tiff::ColorType::RGB(b) => (TiffInks::Rgb, 3, b, 3),
        tiff::ColorType::RGBA(b) => (TiffInks::Rgb, 3, b, 4),
        tiff::ColorType::CMYK(b) => (TiffInks::Cmyk, 4, b, 4),
        tiff::ColorType::CMYKA(b) => (TiffInks::Cmyk, 4, b, 5),
        tiff::ColorType::Multiband {
            bit_depth,
            num_samples,
        } => match photometric {
            0 | 1 => (
                TiffInks::Grey,
                1,
                bit_depth,
                num_samples,
            ),
            2 if num_samples >= 3 => (TiffInks::Rgb, 3, bit_depth, num_samples),
            5 if num_samples >= 4 => (TiffInks::Cmyk, 4, bit_depth, num_samples),
            _ => return None,
        },
        _ => return None,
    };
    let colours: usize = colours;
    if !matches!(bits, 1 | 2 | 4 | 8 | 16 | 32) || usize::from(samples) < colours {
        return None;
    }
    // The first extra channel is alpha when the file says so (associated
    // or unassociated); the types with alpha in their name have it.
    let extra: Vec<u16> = dec
        .find_tag_unsigned_vec(Tag::ExtraSamples)
        .ok()
        .flatten()
        .unwrap_or_default();
    let alpha = usize::from(samples) > colours
        && (extra.first().is_some_and(|&e| e == 1 || e == 2)
            || (extra.is_empty() && usize::from(samples) == colours + 1));
    let premultiplied = extra.first() == Some(&1);
    let planar = dec
        .find_tag_unsigned::<u16>(Tag::PlanarConfiguration)
        .ok()
        .flatten()
        == Some(2);
    if planar && (inks == TiffInks::Cmyk || dec.get_chunk_type() != ChunkType::Strip) {
        return None;
    }
    if u64::from(w) * u64::from(h) > max_pixels {
        return None;
    }
    let (ow, oh) = (w, h);
    let out_len = usize::try_from(u64::from(ow) * u64::from(oh) * 4).ok()?;
    let mut out = vec![0u8; out_len];
    if !alpha {
        for px in out.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }
    let (cw, ch) = dec.chunk_dimensions();
    if cw == 0 || ch == 0 {
        return None;
    }
    let across = w.div_ceil(cw);
    let per_plane = across * h.div_ceil(ch);
    let planes = if planar { u32::from(samples) } else { 1 };
    let in_chunk = if planar { 1usize } else { usize::from(samples) };
    for plane in 0..planes {
        // A plane of extra channels other than alpha is left out.
        let sample_of_plane = plane as usize;
        if planar && sample_of_plane >= colours + usize::from(alpha) {
            break;
        }
        for k in 0..per_plane {
            let index = plane * per_plane + k;
            let (dw, dh) = dec.chunk_data_dimensions(index);
            let (x0, y0) = ((k % across) * cw, (k / across) * ch);
            let data = dec.read_chunk(index).ok()?;
            let samples_per_row = dw as usize * in_chunk;
            // A sample at (column, row, channel) of the piece, 0 to 1.
            let read: Box<dyn Fn(usize, usize, usize) -> f32 + '_> = match &data {
                DecodingResult::U8(v) if bits == 8 => Box::new(move |x, y, c| {
                    f32::from(v[y * samples_per_row + x * in_chunk + c]) / 255.0
                }),
                DecodingResult::U8(v) => {
                    let row_bytes = (samples_per_row * usize::from(bits)).div_ceil(8);
                    let max = f32::from((1u16 << bits) - 1);
                    Box::new(move |x, y, c| {
                        let bit = (x * in_chunk + c) * usize::from(bits);
                        let byte = v[y * row_bytes + bit / 8];
                        let shift = 8 - usize::from(bits) - bit % 8;
                        f32::from((byte >> shift) & ((1u16 << bits) - 1) as u8) / max
                    })
                }
                DecodingResult::U16(v) => Box::new(move |x, y, c| {
                    f32::from(v[y * samples_per_row + x * in_chunk + c]) / 65535.0
                }),
                DecodingResult::U32(v) => Box::new(move |x, y, c| {
                    v[y * samples_per_row + x * in_chunk + c] as f32 / u32::MAX as f32
                }),
                DecodingResult::F32(v) => Box::new(move |x, y, c| {
                    v[y * samples_per_row + x * in_chunk + c].clamp(0.0, 1.0)
                }),
                DecodingResult::F64(v) => Box::new(move |x, y, c| {
                    v[y * samples_per_row + x * in_chunk + c].clamp(0.0, 1.0) as f32
                }),
                _ => return None,
            };
            let needed = dh as usize * samples_per_row;
            let have = match &data {
                DecodingResult::U8(v) if bits < 8 => {
                    v.len() * 8 / usize::from(bits).max(1)
                }
                DecodingResult::U8(v) => v.len(),
                DecodingResult::U16(v) => v.len(),
                DecodingResult::U32(v) => v.len(),
                DecodingResult::F32(v) => v.len(),
                DecodingResult::F64(v) => v.len(),
                _ => 0,
            };
            if have < needed {
                return None;
            }
            for y in 0..dh as usize {
                let py = y0 as usize + y;
                if py >= h as usize {
                    break;
                }
                let oy = py;
                for x in 0..dw as usize {
                    let px = x0 as usize + x;
                    if px >= w as usize {
                        break;
                    }
                    let o = oy * ow as usize + px;
                    let value = |c: usize| -> u8 { (read(x, y, c) * 255.0).round() as u8 };
                    // The channels this piece holds, as RGBA.
                    let mut rgba = [None; 4];
                    if planar {
                        let v = value(0);
                        match (inks, sample_of_plane) {
                            (TiffInks::Grey, 0) => rgba = [Some(v), Some(v), Some(v), None],
                            (TiffInks::Rgb, c @ 0..=2) => rgba[c] = Some(v),
                            _ => rgba[3] = Some(v),
                        }
                    } else {
                        let a = alpha.then(|| read(x, y, colours));
                        // Premultiplied colours are divided by their alpha.
                        let un = |v: f32| match a {
                            Some(a) if premultiplied && a > 0.0 => (v / a).min(1.0),
                            _ => v,
                        };
                        let to8 = |v: f32| (v * 255.0).round() as u8;
                        let [r, g, b] = match inks {
                            TiffInks::Grey => [to8(un(read(x, y, 0))); 3],
                            TiffInks::Rgb => {
                                [to8(un(read(x, y, 0))), to8(un(read(x, y, 1))), to8(un(read(x, y, 2)))]
                            }
                            TiffInks::Cmyk => {
                                let k = 1.0 - un(read(x, y, 3));
                                [0, 1, 2].map(|c| to8((1.0 - un(read(x, y, c))) * k))
                            }
                        };
                        rgba = [Some(r), Some(g), Some(b), a.map(to8)];
                    }
                    for (c, v) in rgba.into_iter().enumerate() {
                        let Some(v) = v else { continue };
                        out[o * 4 + c] = v;
                    }
                }
            }
        }
    }
    let rgba = image::RgbaImage::from_raw(ow, oh, out)?;
    let mode = match inks {
        TiffInks::Grey if bits == 1 => FileMode::BlackWhite,
        TiffInks::Grey => FileMode::Gray,
        _ => FileMode::Rgb,
    };
    Some(ReadPicture {
        rgba,
        planes: None,
        mode,
    })
}

/// The colour mode a picture file stores its pixels in, among those a
/// bitmap of the drawing keeps (CMYK files keep their inks apart).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileMode {
    #[default]
    Rgb,
    Gray,
    /// One bit per pixel.
    BlackWhite,
    /// A palette of at most 256 colours.
    Indexed,
    /// CIE Lab (a TIFF, #3539).
    Lab,
}

/// The colour mode of picture file `bytes`, from its header: PNG (colour
/// type and bit depth), BMP (bits per pixel), GIF (always a palette),
/// TIFF (photometric interpretation and bits per sample), JPEG (one
/// component is grey) and Photoshop (its colour mode; a duotone keeps
/// its grey picture). Every file came in as RGB, its mode lost (#3025).
pub fn file_mode(bytes: &[u8]) -> FileMode {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.get(12..16) == Some(b"IHDR".as_slice()) {
        if crate::bitmap_fx::png_is_lab(bytes) {
            return FileMode::Lab;
        }
        return match (bytes.get(24), bytes.get(25)) {
            (_, Some(3)) => FileMode::Indexed,
            (Some(1), Some(0)) => FileMode::BlackWhite,
            (_, Some(0) | Some(4)) => FileMode::Gray,
            _ => FileMode::Rgb,
        };
    }
    if bytes.starts_with(b"BM") {
        // BITMAPINFOHEADER and later: bits per pixel after the 14-byte
        // file header, size, width, height and planes.
        return match bytes.get(28..30).map(|b| u16::from_le_bytes([b[0], b[1]])) {
            Some(1) => FileMode::BlackWhite,
            Some(4 | 8) => FileMode::Indexed,
            _ => FileMode::Rgb,
        };
    }
    if bytes.starts_with(b"GIF8") {
        return FileMode::Indexed;
    }
    if bytes.starts_with(b"8BPS") {
        return match bytes.get(24..26).map(|b| u16::from_be_bytes([b[0], b[1]])) {
            Some(0) => FileMode::BlackWhite,
            Some(1 | 8) => FileMode::Gray,
            Some(2) => FileMode::Indexed,
            _ => FileMode::Rgb,
        };
    }
    if let Some(tags) = TiffTags::new(bytes) {
        let bits = tags.short(258).map(|(v, _)| v).unwrap_or(1);
        return match tags.short(262).map(|(v, _)| v) {
            Some(3) => FileMode::Indexed,
            Some(8) => FileMode::Lab,
            Some(0 | 1) if tags.short(277).is_none_or(|(n, _)| n == 1) => {
                if bits == 1 {
                    FileMode::BlackWhite
                } else {
                    FileMode::Gray
                }
            }
            _ => FileMode::Rgb,
        };
    }
    if jpeg_components(bytes) == Some(1) {
        return FileMode::Gray;
    }
    FileMode::Rgb
}

/// The number of components of a JPEG file, from its frame header.
fn jpeg_components(b: &[u8]) -> Option<u8> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut at = 2usize;
    loop {
        while b.get(at) == Some(&0xFF) && b.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        if *b.get(at)? != 0xFF {
            return None;
        }
        let marker = *b.get(at + 1)?;
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            at += 2;
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = usize::from(u16::from_be_bytes([*b.get(at + 2)?, *b.get(at + 3)?]));
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return b.get(at + 9).copied();
        }
        at += 2 + len;
    }
}

/// `img` as the PNG a bitmap keeps in colour mode `mode`: 1-bit, grey or
/// indexed when its pixels allow it (a 1-bit file of two colours other
/// than black and white is indexed), else RGB.
pub fn encode_as(img: &image::RgbaImage, mode: FileMode) -> Option<Vec<u8>> {
    use crate::bitmap_fx as fx;
    let grey = || {
        img.as_raw()
            .chunks_exact(4)
            .all(|p| p[0] == p[1] && p[1] == p[2])
    };
    match mode {
        FileMode::Rgb => fx::encode(img),
        FileMode::Gray if grey() => fx::encode_gray(img),
        FileMode::BlackWhite
            if img.as_raw().chunks_exact(4).all(|p| {
                p[3] == 0 || (p[0] == p[1] && p[1] == p[2] && matches!(p[0], 0 | 255))
            }) =>
        {
            fx::encode_bw(img)
        }
        FileMode::BlackWhite | FileMode::Indexed => {
            fx::encode_indexed(img).or_else(|| fx::encode(img))
        }
        FileMode::Gray => fx::encode(img),
        FileMode::Lab => fx::encode_lab(img),
    }
}

/// The bitmaps a Photoshop file gave keep its colour mode: a Bitmap
/// (1-bit), Grayscale, Duotone or Indexed file came in as RGB (#3025).
pub fn keep_psd_mode(bytes: &[u8], shapes: &mut [Shape]) {
    if !bytes.starts_with(b"8BPS") {
        return;
    }
    let mode = file_mode(bytes);
    if mode == FileMode::Rgb {
        return;
    }
    for s in shapes {
        if let ShapeKind::Bitmap { png, .. } = &mut s.kind {
            let Ok(img) = image::load_from_memory_with_format(png, image::ImageFormat::Png) else {
                continue;
            };
            if let Some(kept) = encode_as(&img.into_rgba8(), mode) {
                *png = kept.into();
            }
        }
    }
}

/// The entries of the first directory of a classic TIFF file.
struct TiffTags<'a> {
    b: &'a [u8],
    le: bool,
    ifd: usize,
    n: usize,
}

impl<'a> TiffTags<'a> {
    fn new(b: &'a [u8]) -> Option<TiffTags<'a>> {
        let le = match b.get(..4)? {
            b"II*\0" => true,
            b"MM\0*" => false,
            _ => return None,
        };
        let mut t = TiffTags {
            b,
            le,
            ifd: 0,
            n: 0,
        };
        t.ifd = t.u32(4)? as usize;
        t.n = usize::from(t.u16(t.ifd)?);
        Some(t)
    }

    fn u16(&self, at: usize) -> Option<u16> {
        let v = self.b.get(at..at + 2)?;
        Some(if self.le {
            u16::from_le_bytes([v[0], v[1]])
        } else {
            u16::from_be_bytes([v[0], v[1]])
        })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let v = self.b.get(at..at + 4)?;
        let v = [v[0], v[1], v[2], v[3]];
        Some(if self.le {
            u32::from_le_bytes(v)
        } else {
            u32::from_be_bytes(v)
        })
    }

    /// The value of tag `tag` when it is one SHORT (or LONG below 65536),
    /// and where the value is stored.
    fn short(&self, tag: u16) -> Option<(u16, usize)> {
        (0..self.n).find_map(|i| {
            let e = self.ifd + 2 + i * 12;
            if self.u16(e)? != tag || self.u32(e + 4)? != 1 {
                return None;
            }
            match self.u16(e + 2)? {
                3 => Some((self.u16(e + 8)?, e + 8)),
                4 => Some((u16::try_from(self.u32(e + 8)?).ok()?, e + 8)),
                _ => None,
            }
        })
    }
}

/// A palette TIFF (photometric interpretation 3, 1 to 8 bits per pixel):
/// its colours, and the Indexed mode. The TIFF reader takes no palette
/// file, so such a file did not open at all (#3025): it is read as grey
/// levels, which are the palette's indices, and those are looked up in
/// its colour map (TIFF 6.0, section 5: 16-bit red, then green, then
/// blue levels).
fn read_palette_tiff(bytes: &[u8]) -> Option<ReadPicture> {
    let tags = TiffTags::new(bytes)?;
    let (photometric, at) = tags.short(262)?;
    if photometric != 3 {
        return None;
    }
    let mut grey = bytes.to_vec();
    let one = if tags.le { [1, 0] } else { [0, 1] };
    grey.get_mut(at..at + 2)?.copy_from_slice(&one);
    let limit = usize::try_from(crate::files::picture_decode_limit()).unwrap_or(usize::MAX);
    let mut limits = tiff::decoder::Limits::default();
    limits.decoding_buffer_size = limits.decoding_buffer_size.max(limit);
    let mut dec = tiff::decoder::Decoder::new(std::io::Cursor::new(grey.as_slice()))
        .ok()?
        .with_limits(limits);
    let bits = match dec.colortype().ok()? {
        tiff::ColorType::Gray(b @ (1 | 2 | 4 | 8)) => b,
        _ => return None,
    };
    let map: Vec<u16> = dec
        .find_tag_unsigned_vec(tiff::tags::Tag::ColorMap)
        .ok()??;
    let colors = 1usize << bits;
    if map.len() < colors * 3 {
        return None;
    }
    let (w, h) = dec.dimensions().ok()?;
    let tiff::decoder::DecodingResult::U8(data) = dec.read_image().ok()? else {
        return None;
    };
    let row = (w as usize * usize::from(bits)).div_ceil(8);
    let level = |v: u16| (v >> 8) as u8;
    let mut rgba = image::RgbaImage::new(w, h);
    for (y, line) in data.chunks(row).take(h as usize).enumerate() {
        for x in 0..w as usize {
            let bit = x * usize::from(bits);
            let byte = *line.get(bit / 8)?;
            let i = if bits == 8 {
                usize::from(byte)
            } else {
                let shift = 8 - usize::from(bits) - bit % 8;
                usize::from(byte >> shift) & (colors - 1)
            };
            let px = [
                level(map[i]),
                level(map[colors + i]),
                level(map[2 * colors + i]),
                255,
            ];
            rgba.put_pixel(x as u32, y as u32, image::Rgba(px));
        }
    }
    Some(ReadPicture {
        rgba,
        planes: None,
        mode: FileMode::Indexed,
    })
}

/// `img` as an uncompressed CIE Lab TIFF (photometric interpretation 8,
/// TIFF 6.0 section 23): 8 bits a channel, L* from 0 to 100 over 0 to
/// 255, a* and b* signed; an alpha channel beside them when a pixel is
/// see-through; its resolution `dpi` when known (#3539).
fn lab_tiff(img: &image::RgbaImage, dpi: f64) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let alpha = !img.pixels().all(|p| p[3] == 255);
    let n: u16 = if alpha { 4 } else { 3 };
    let mut data = Vec::with_capacity(w as usize * h as usize * usize::from(n));
    for p in img.pixels() {
        let [l, a, b] = crate::bitmap_modes::rgb_to_lab([p[0], p[1], p[2]]);
        data.push((l * 255.0 / 100.0).round().clamp(0.0, 255.0) as u8);
        data.push(a.round().clamp(-128.0, 127.0) as i8 as u8);
        data.push(b.round().clamp(-128.0, 127.0) as i8 as u8);
        if alpha {
            data.push(p[3]);
        }
    }
    let dpi = if dpi.is_finite() && dpi > 0.0 {
        dpi
    } else {
        72.0
    };
    // Header, then the values that do not fit a tag, then the pixels,
    // then the directory.
    let mut out = b"II*\0\0\0\0\0".to_vec();
    let bits_at = out.len() as u32;
    for _ in 0..n {
        out.extend_from_slice(&8u16.to_le_bytes());
    }
    let res_at = out.len() as u32;
    let res = (dpi * 100.0).round().clamp(1.0, f64::from(u32::MAX)) as u32;
    out.extend_from_slice(&res.to_le_bytes());
    out.extend_from_slice(&100u32.to_le_bytes());
    let data_at = out.len() as u32;
    out.extend_from_slice(&data);
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let ifd = out.len() as u32;
    out[4..8].copy_from_slice(&ifd.to_le_bytes());
    // (tag, type, count, value): type 3 SHORT, 4 LONG, 5 RATIONAL.
    let mut tags: Vec<(u16, u16, u32, u32)> = vec![
        (256, 4, 1, w),
        (257, 4, 1, h),
        (258, 3, u32::from(n), bits_at),
        (259, 3, 1, 1),
        (262, 3, 1, 8),
        (273, 4, 1, data_at),
        (277, 3, 1, u32::from(n)),
        (278, 4, 1, h),
        (279, 4, 1, data.len() as u32),
        (282, 5, 1, res_at),
        (283, 5, 1, res_at),
        (284, 3, 1, 1),
        (296, 3, 1, 2),
    ];
    if alpha {
        // Unassociated alpha.
        tags.push((338, 3, 1, 2));
    }
    out.extend_from_slice(&(tags.len() as u16).to_le_bytes());
    for (tag, ty, count, value) in tags {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&ty.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

/// A CIE Lab TIFF (photometric interpretation 8, 8 bits a channel,
/// uncompressed, with or without an alpha channel): its pixels in sRGB
/// and the Lab mode. The TIFF reader takes no Lab, so the picture
/// written for Edit Bitmap did not come back (#3539).
fn read_lab_tiff(bytes: &[u8]) -> Option<ReadPicture> {
    let tags = TiffTags::new(bytes)?;
    if tags.short(262)?.0 != 8 || tags.short(259).is_some_and(|(c, _)| c != 1) {
        return None;
    }
    let long = |tag: u16| -> Option<u32> {
        (0..tags.n).find_map(|i| {
            let e = tags.ifd + 2 + i * 12;
            if tags.u16(e)? != tag {
                return None;
            }
            match tags.u16(e + 2)? {
                3 => Some(u32::from(tags.u16(e + 8)?)),
                4 => Some(tags.u32(e + 8)?),
                _ => None,
            }
        })
    };
    let (w, h) = (long(256)?, long(257)?);
    let n = long(277).unwrap_or(1) as usize;
    if !(3..=4).contains(&n) || w == 0 || h == 0 {
        return None;
    }
    let bits_count = (0..tags.n).find_map(|i| {
        let e = tags.ifd + 2 + i * 12;
        (tags.u16(e)? == 258).then(|| tags.u32(e + 4)).flatten()
    });
    if bits_count.is_some_and(|c| c == 1) && tags.short(258).is_some_and(|(b, _)| b != 8) {
        return None;
    }
    // One strip, or strips one after another.
    let count = (0..tags.n).find_map(|i| {
        let e = tags.ifd + 2 + i * 12;
        (tags.u16(e)? == 273).then(|| tags.u32(e + 4)).flatten()
    })?;
    let first = if count == 1 {
        long(273)?
    } else {
        let at = long(273)? as usize;
        tags.u32(at)?
    };
    let len = w as usize * h as usize * n;
    let data = bytes.get(first as usize..(first as usize).checked_add(len)?)?;
    let mut rgba = image::RgbaImage::new(w, h);
    for (p, px) in rgba.pixels_mut().zip(data.chunks_exact(n)) {
        let lab = [
            f32::from(px[0]) * 100.0 / 255.0,
            f32::from(px[1] as i8),
            f32::from(px[2] as i8),
        ];
        let [r, g, b] = crate::bitmap_modes::lab_to_rgb(lab);
        *p = image::Rgba([r, g, b, if n == 4 { px[3] } else { 255 }]);
    }
    Some(ReadPicture {
        rgba,
        planes: None,
        mode: FileMode::Lab,
    })
}

fn read_cmyk_tiff(bytes: &[u8]) -> Option<ReadPicture> {
    if !(bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*")) {
        return None;
    }
    // Print-size pictures too (#2000): the reader's own guard is 256 MiB.
    let limit = usize::try_from(crate::files::picture_decode_limit()).unwrap_or(usize::MAX);
    let mut limits = tiff::decoder::Limits::default();
    limits.decoding_buffer_size = limits.decoding_buffer_size.max(limit);
    limits.intermediate_buffer_size = limits.intermediate_buffer_size.max(limit);
    let mut dec = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes))
        .ok()?
        .with_limits(limits);
    let step = match dec.colortype().ok()? {
        tiff::ColorType::CMYK(8) => 4,
        tiff::ColorType::CMYKA(8) => 5,
        _ => return None,
    };
    let (w, h) = dec.dimensions().ok()?;
    let tiff::decoder::DecodingResult::U8(data) = dec.read_image().ok()? else {
        return None;
    };
    let n = (w as usize).checked_mul(h as usize)?;
    if data.len() < n.checked_mul(step)? {
        return None;
    }
    let px = data.chunks_exact(step).take(n).map(|px| {
        let a = if step == 5 { px[4] } else { 255 };
        ([px[0], px[1], px[2], px[3]], a)
    });
    let (rgba, planes) = inks_to_picture(w, h, px)?;
    picture_with_planes(rgba, planes)
}

/// The pixels of `w` by `h` ink coverages (cyan, magenta, yellow,
/// black, 0 = no ink) with their alpha, row by row: the colours they
/// show through the colour management, and the four ink planes one
/// under the other (the layout `PictureInks` keeps).
fn inks_to_picture(
    w: u32,
    h: u32,
    px: impl Iterator<Item = ([u8; 4], u8)>,
) -> Option<(image::RgbaImage, image::GrayImage)> {
    let n = (w as usize).checked_mul(h as usize)?;
    let mut planes = vec![0u8; n.checked_mul(4)?];
    let mut rgba = Vec::with_capacity(n * 4);
    let mut known: std::collections::HashMap<[u8; 4], [u8; 3]> = std::collections::HashMap::new();
    for (i, (ink, a)) in px.take(n).enumerate() {
        for (k, v) in ink.iter().enumerate() {
            if let Some(p) = planes.get_mut(k * n + i) {
                *p = *v;
            }
        }
        let rgb = *known.entry(ink).or_insert_with(|| {
            let f = |v: u8| f32::from(v) / 255.0;
            Color::Cmyk {
                c: f(ink[0]),
                m: f(ink[1]),
                y: f(ink[2]),
                k: f(ink[3]),
            }
            .to_rgb8()
        });
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
    }
    let rgba = image::RgbaImage::from_raw(w, h, rgba)?;
    let grey = image::GrayImage::from_raw(w, h.checked_mul(4)?, planes)?;
    Some((rgba, grey))
}

/// A read picture from its pixels and its ink planes (as
/// [`inks_to_picture`] lays them out).
fn picture_with_planes(rgba: image::RgbaImage, planes: image::GrayImage) -> Option<ReadPicture> {
    let mut png = std::io::Cursor::new(Vec::new());
    planes.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(ReadPicture {
        rgba,
        planes: Some(png.into_inner()),
        mode: FileMode::Rgb,
    })
}

/// How the four channels of a JPEG file are stored, from its markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JpegInks {
    /// C, M, Y, K as they are (0 = no ink): no Adobe `APP14` marker.
    Plain,
    /// C, M, Y, K inverted (255 = no ink): an Adobe `APP14` marker with
    /// transform 0 (Adobe Technical Note 5116).
    Inverted,
    /// Y, Cb, Cr of C, M, Y, then the inverted K: an Adobe `APP14`
    /// marker with transform 2 (YCCK).
    Ycck,
}

/// The way a JPEG file with four components stores its inks, from the
/// marker segments before its first scan (ITU-T T.81 annex B: the frame
/// header's component count; Adobe Technical Note 5116: the `APP14`
/// segment, "Adobe", version, two flag words, then the transform byte).
/// None for a JPEG of another number of components, an unknown
/// transform, or something else.
fn jpeg_inks(b: &[u8]) -> Option<JpegInks> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut at = 2usize;
    let mut components = None;
    let mut transform = None;
    loop {
        // Fill bytes before a marker.
        while b.get(at) == Some(&0xFF) && b.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        if *b.get(at)? != 0xFF {
            return None;
        }
        let marker = *b.get(at + 1)?;
        // Markers without a length.
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            at += 2;
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        let len = usize::from(u16::from_be_bytes([*b.get(at + 2)?, *b.get(at + 3)?]));
        let body = b.get(at + 4..(at + 2).checked_add(len)?)?;
        match marker {
            // Start of frame (every coding process but DHT 0xC4, JPG 0xC8
            // and DAC 0xCC): precision, height, width, components.
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                components = body.get(5).copied();
            }
            0xEE if body.starts_with(b"Adobe") => {
                transform = Some(*body.get(11)?);
            }
            _ => {}
        }
        at += 2 + len;
    }
    if components? != 4 {
        return None;
    }
    match transform {
        None => Some(JpegInks::Plain),
        Some(0) => Some(JpegInks::Inverted),
        Some(2) => Some(JpegInks::Ycck),
        Some(_) => None,
    }
}

/// The ink coverages of one stored JPEG pixel (four channels as `how`
/// stores them).
fn jpeg_pixel_inks(px: [u8; 4], how: JpegInks) -> [u8; 4] {
    match how {
        JpegInks::Plain => px,
        JpegInks::Inverted => px.map(|v| 255 - v),
        JpegInks::Ycck => {
            // T.81 is silent on colour; the YCbCr of JFIF (ITU-T T.871).
            // The writer turns the CMYK it is given, stored inverted as
            // under transform 0, into "RGB" as 255 less each value, so
            // the colour transform taken back gives C, M and Y as they
            // are, and K stays inverted. Taken as the inverted C, M, Y,
            // a Photoshop CMYK JPEG (YCCK as a rule) came in with its
            // cyan, magenta and yellow inverted (#3025).
            let y = f32::from(px[0]);
            let cb = f32::from(px[1]) - 128.0;
            let cr = f32::from(px[2]) - 128.0;
            let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
            let c = byte(y + 1.402 * cr);
            let m = byte(y - 0.344_136 * cb - 0.714_136 * cr);
            let ye = byte(y + 1.772 * cb);
            [c, m, ye, 255 - px[3]]
        }
    }
}

/// A CMYK JPEG (four components, baseline or progressive, plain,
/// inverted under an Adobe marker, or YCCK): its inks, and its pixels
/// through the colour management. The image library turns such a file
/// into RGB by itself, taking every file as inverted and leaving the
/// colour management out, so pictures came in brighter than the same
/// inks show in the drawing (#2163). Turned as its Exif orientation
/// says. None for any other file, or one too large to read.
fn read_cmyk_jpeg(bytes: &[u8]) -> Option<ReadPicture> {
    use zune_core::colorspace::ColorSpace;
    let how = jpeg_inks(bytes)?;
    let stored = if how == JpegInks::Ycck {
        ColorSpace::YCCK
    } else {
        ColorSpace::CMYK
    };
    let options = zune_core::options::DecoderOptions::default()
        .set_strict_mode(false)
        .set_max_width(usize::MAX)
        .set_max_height(usize::MAX);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(
        zune_core::bytestream::ZCursor::new(bytes),
        options,
    );
    dec.decode_headers().ok()?;
    // The decoder must see the colour space the markers say, so that
    // the channels come out as stored, with no conversion of its own.
    if dec.input_colorspace()? != stored {
        return None;
    }
    let (w, h) = dec.dimensions()?;
    // The decoded channels, the pixels and the planes side by side.
    let n = w.checked_mul(h)?;
    let need = u64::try_from(n.checked_mul(12)?).ok()?;
    if need > crate::files::picture_decode_limit() {
        return None;
    }
    dec.set_options(dec.options().jpeg_set_out_colorspace(stored));
    let data = dec.decode().ok()?;
    if data.len() < n.checked_mul(4)? {
        return None;
    }
    let (w, h) = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
    let px = data
        .chunks_exact(4)
        .map(|p| (jpeg_pixel_inks([p[0], p[1], p[2], p[3]], how), 255));
    let (rgba, planes) = inks_to_picture(w, h, px)?;
    let (rgba, planes) = oriented(bytes, rgba, planes)?;
    picture_with_planes(rgba, planes)
}

/// The pixels and the ink planes of picture file `bytes` turned the way
/// its Exif orientation says, as the pixels of other files are.
fn oriented(
    bytes: &[u8],
    rgba: image::RgbaImage,
    planes: image::GrayImage,
) -> Option<(image::RgbaImage, image::GrayImage)> {
    use image::ImageDecoder;
    let orientation = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_decoder().ok())
        .and_then(|mut d| d.orientation().ok())
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    if orientation == image::metadata::Orientation::NoTransforms {
        return Some((rgba, planes));
    }
    let (w, h) = rgba.dimensions();
    let mut pixels = image::DynamicImage::ImageRgba8(rgba);
    pixels.apply_orientation(orientation);
    let pixels = pixels.into_rgba8();
    let (tw, th) = pixels.dimensions();
    // Each plane turned alone, then laid one under the other again.
    let mut out = image::GrayImage::new(tw, th.checked_mul(4)?);
    for k in 0..4u32 {
        let plane = image::imageops::crop_imm(&planes, 0, k * h, w, h).to_image();
        let mut plane = image::DynamicImage::ImageLuma8(plane);
        plane.apply_orientation(orientation);
        image::imageops::replace(&mut out, &plane.into_luma8(), 0, i64::from(k * th));
    }
    Some((pixels, out))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::app::App;
    use tracedraw_core::geometry::Rect;
    use tracedraw_core::{Command, ShapeId};

    /// A little-endian TIFF of uncompressed strips: `spp` samples of
    /// `bps` bits, `photometric`, chunky or planar, with `extra` as its
    /// ExtraSamples, `rows` rows a strip (#4036).
    #[allow(clippy::too_many_arguments)]
    fn raw_tiff(
        w: u32,
        h: u32,
        spp: u16,
        bps: u16,
        photometric: u16,
        planar: bool,
        extra: &[u16],
        rows: u32,
        data: &[u8],
    ) -> Vec<u8> {
        let planes = if planar { u32::from(spp) } else { 1 };
        let row_bytes = (w as usize * if planar { 1 } else { spp as usize } * bps as usize).div_ceil(8);
        let per_plane = h.div_ceil(rows);
        let mut strips = Vec::new();
        let mut at = 0usize;
        for _ in 0..planes {
            for k in 0..per_plane {
                let n = row_bytes * (rows.min(h - k * rows)) as usize;
                strips.push((8 + at as u32, n as u32));
                at += n;
            }
        }
        assert_eq!(at, data.len(), "data length");
        let mut out = b"II*\0".to_vec();
        let ifd = 8 + data.len() as u32 + (data.len() as u32 & 1);
        out.extend_from_slice(&ifd.to_le_bytes());
        out.extend_from_slice(data);
        if data.len() & 1 == 1 {
            out.push(0);
        }
        let mut entries: Vec<(u16, u16, u32, Vec<u8>)> = vec![
            (256, 4, 1, w.to_le_bytes().to_vec()),
            (257, 4, 1, h.to_le_bytes().to_vec()),
            (258, 3, u32::from(spp), (0..spp).flat_map(|_| bps.to_le_bytes()).collect()),
            (259, 3, 1, 1u16.to_le_bytes().to_vec()),
            (262, 3, 1, photometric.to_le_bytes().to_vec()),
            (273, 4, strips.len() as u32, strips.iter().flat_map(|s| s.0.to_le_bytes()).collect()),
            (277, 3, 1, spp.to_le_bytes().to_vec()),
            (278, 4, 1, rows.to_le_bytes().to_vec()),
            (279, 4, strips.len() as u32, strips.iter().flat_map(|s| s.1.to_le_bytes()).collect()),
            (284, 3, 1, if planar { 2u16 } else { 1 }.to_le_bytes().to_vec()),
        ];
        if !extra.is_empty() {
            entries.push((338, 3, extra.len() as u32, extra.iter().flat_map(|e| e.to_le_bytes()).collect()));
        }
        let n = entries.len() as u32;
        let mut values = ifd + 2 + n * 12 + 4;
        let mut tail = Vec::new();
        out.extend_from_slice(&(n as u16).to_le_bytes());
        for (tag, kind, count, bytes) in &entries {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            if bytes.len() <= 4 {
                let mut v = bytes.clone();
                v.resize(4, 0);
                out.extend_from_slice(&v);
            } else {
                out.extend_from_slice(&values.to_le_bytes());
                tail.extend_from_slice(bytes);
                values += bytes.len() as u32;
            }
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&tail);
        out
    }

    /// #4036: a TIFF too large for the library's guard on its buffer is
    /// read strip by strip, the same pixels the library reads.
    #[test]
    fn a_tiff_beyond_the_decoding_guard_is_read_strip_by_strip() {
        let (w, h) = (37u32, 23u32);
        let data: Vec<u8> = (0..w * h * 3).map(|i| (i * 7 % 251) as u8).collect();
        let tif = raw_tiff(w, h, 3, 8, 2, false, &[], 5, &data);
        let whole = crate::files::decode_picture_within(&tif, 1 << 20).expect("library").into_rgba8();
        assert!(crate::files::decode_picture_within(&tif, 1000).is_err());
        let p = read_within(&tif, 1000 * 1000).expect("read");
        assert_eq!(p.rgba, whole);
        let pieces = read_tiff_by_chunks(&tif, u64::MAX).expect("pieces");
        assert_eq!(pieces.rgba, whole);
        assert_eq!(pieces.mode, FileMode::Rgb);
    }

    /// #4036: the guard's own answer stands for a picture beyond the
    /// pixels a picture may keep.
    #[test]
    fn a_tiff_beyond_the_picture_pixels_is_not_read_in_pieces() {
        let tif = raw_tiff(4, 4, 1, 8, 1, false, &[], 4, &[9; 16]);
        assert!(read_tiff_by_chunks(&tif, 15).is_none());
        assert!(read_tiff_by_chunks(&tif, 16).is_some());
    }

    /// An RGB TIFF with alpha and a saved selection beside it (two extra
    /// channels), which the library takes no colours from: its colours
    /// and alpha come in, the selection is left out.
    #[test]
    fn a_tiff_with_extra_channels_keeps_colour_and_alpha() {
        let px = [[255u8, 0, 0, 128, 7], [0, 0, 255, 255, 200]];
        let data: Vec<u8> = px.iter().flatten().copied().collect();
        let tif = raw_tiff(2, 1, 5, 8, 2, false, &[2, 0], 1, &data);
        let p = read(&tif).expect("read");
        assert_eq!(p.rgba.get_pixel(0, 0).0, [255, 0, 0, 128]);
        assert_eq!(p.rgba.get_pixel(1, 0).0, [0, 0, 255, 255]);
    }

    /// Planar RGB (each colour a plane of strips of its own).
    #[test]
    fn a_planar_tiff_is_read_by_planes() {
        let (w, h) = (3u32, 2u32);
        let r = [10u8, 20, 30, 40, 50, 60];
        let g = [1u8, 2, 3, 4, 5, 6];
        let b = [200u8; 6];
        let data: Vec<u8> = r.iter().chain(&g).chain(&b).copied().collect();
        let tif = raw_tiff(w, h, 3, 8, 2, true, &[], 1, &data);
        let p = read_tiff_by_chunks(&tif, u64::MAX).expect("planar");
        assert_eq!(p.rgba.get_pixel(0, 0).0, [10, 1, 200, 255]);
        assert_eq!(p.rgba.get_pixel(2, 1).0, [60, 6, 200, 255]);
    }

    /// One bit a pixel, white as zero, and 16-bit grey.
    #[test]
    fn bilevel_and_sixteen_bit_grey_tiffs_are_read_in_pieces() {
        // 10 pixels a row: two bytes, the last six bits padding.
        let tif = raw_tiff(10, 2, 1, 1, 0, false, &[], 1, &[0b1000_0000, 0b0100_0000, 0, 0]);
        let p = read_tiff_by_chunks(&tif, u64::MAX).expect("bilevel");
        assert_eq!(p.mode, FileMode::BlackWhite);
        assert_eq!(p.rgba.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(p.rgba.get_pixel(1, 0).0, [255, 255, 255, 255]);
        assert_eq!(p.rgba.get_pixel(9, 0).0, [0, 0, 0, 255]);
        assert_eq!(p.rgba.get_pixel(5, 1).0, [255, 255, 255, 255]);
        let grey: Vec<u8> = [0u16, 65535, 32896].iter().flat_map(|v| v.to_le_bytes()).collect();
        let tif = raw_tiff(3, 1, 1, 16, 1, false, &[], 1, &grey);
        let p = read_tiff_by_chunks(&tif, u64::MAX).expect("grey");
        assert_eq!(p.mode, FileMode::Gray);
        assert_eq!(p.rgba.get_pixel(0, 0).0[0], 0);
        assert_eq!(p.rgba.get_pixel(1, 0).0[0], 255);
        assert_eq!(p.rgba.get_pixel(2, 0).0[0], 128);
    }

    /// Four by two pixels: cyan ink on the left half, black on the
    /// right, as a `.cdr` CMYK image is read (pixels for the screen and
    /// its ink channels).
    fn cmyk_picture(app: &mut App) -> ShapeId {
        let rgb = |x: u32| if x < 2 { [0, 160, 230] } else { [30, 30, 30] };
        let img = image::RgbaImage::from_fn(4, 2, |x, _| {
            let [r, g, b] = rgb(x);
            image::Rgba([r, g, b, 255])
        });
        let png = crate::bitmap_fx::encode(&img).expect("png");
        // Planes C, M, Y, K one under the other.
        let planes = image::GrayImage::from_fn(4, 8, |x, y| {
            let ink = y / 2;
            image::Luma([match (ink, x < 2) {
                (0, true) | (3, false) => 255,
                _ => 0,
            }])
        });
        let mut planes_png = std::io::Cursor::new(Vec::new());
        planes
            .write_to(&mut planes_png, image::ImageFormat::Png)
            .expect("planes");
        let inks = PictureInks::cmyk(&png, planes_png.into_inner().into());
        let id = app
            .new_shape(ShapeKind::Bitmap {
                rect: Rect::new(10.0, 10.0, 10.0 + 4.0 * 25.4 / 300.0, 20.0),
                width_px: 4,
                height_px: 2,
                png: png.into(),
                fx: None,
            })
            .expect("bitmap");
        app.run(Command::SetPictureInks {
            shape: id,
            cmyk: true,
            inks: Some(Box::new(inks)),
        });
        id
    }

    fn rgb_picture(app: &mut App) -> ShapeId {
        let img = image::RgbaImage::from_fn(5, 3, |x, _| image::Rgba([200, 40 * x as u8, 10, 255]));
        app.new_shape(ShapeKind::Bitmap {
            rect: Rect::new(10.0, 10.0, 15.0, 13.0),
            width_px: 5,
            height_px: 3,
            png: crate::bitmap_fx::encode(&img).expect("png").into(),
            fx: None,
        })
        .expect("bitmap")
    }

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("tracedraw-1834-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        d
    }

    fn planes(s: &Shape) -> Vec<u8> {
        let inks = s.picture_inks.as_deref().expect("inks");
        let ShapeKind::Bitmap { png, .. } = &s.kind else {
            panic!("not a bitmap");
        };
        assert!(inks.fits(png.as_slice()), "the inks fit the pixels");
        image::load_from_memory(&inks.png)
            .expect("planes")
            .to_luma8()
            .into_raw()
    }

    /// Save Bitmap As (#1834): a CMYK picture is offered as TIFF and
    /// keeps its ink channels as they are in a CMYK TIFF; as JPEG it is a
    /// four-component CMYK file; PNG stays the pixels as kept.
    #[test]
    fn save_bitmap_as_keeps_the_inks_of_a_cmyk_picture() {
        let mut app = App::headless();
        let id = cmyk_picture(&mut app);
        let s = app.doc().find_shape(id).expect("picture").clone();
        assert!(is_cmyk(&s));
        assert_eq!(own_kind(&s), BitmapFile::Tiff);
        let d = dir("save");

        let tif = d.join("picture.tif");
        app.save_bitmap_to(id, &tif, BitmapFile::Png).expect("tiff");
        let bytes = std::fs::read(&tif).expect("read");
        let mut dec = tiff::decoder::Decoder::new(std::io::Cursor::new(&bytes)).expect("tiff");
        assert_eq!(dec.colortype().expect("type"), tiff::ColorType::CMYK(8));
        let back = read(&bytes).expect("read back");
        assert_eq!(back.planes.is_some(), true);
        let (_, inks) = back.into_png().expect("png");
        let got = image::load_from_memory(&inks.expect("inks").png)
            .expect("planes")
            .to_luma8()
            .into_raw();
        assert_eq!(got, planes(&s), "the inks as the drawing keeps them");
        assert_eq!(
            crate::image_dpi::resolution(&bytes).map(|(x, _)| x.round()),
            Some(300.0)
        );

        let jpg = d.join("picture.jpeg");
        app.save_bitmap_to(id, &jpg, BitmapFile::Png).expect("jpeg");
        let bytes = std::fs::read(&jpg).expect("read");
        let sof = bytes
            .windows(2)
            .position(|w| w == [0xFF, 0xC0])
            .expect("SOF0");
        assert_eq!(bytes[sof + 9], 4, "four components (CMYK)");
        let img = image::load_from_memory(&bytes).expect("decodes").to_rgb8();
        assert_eq!(img.dimensions(), (4, 2));
        // Cyan ink alone shows as no red, full green and blue.
        let p = img.get_pixel(0, 0).0;
        assert!(p[0] < 40 && p[1] > 215 && p[2] > 215, "cyan: {p:?}");

        let png = d.join("picture.png");
        app.save_bitmap_to(id, &png, BitmapFile::Tiff).expect("png");
        let ShapeKind::Bitmap { png: kept, .. } = &s.kind else {
            panic!("bitmap");
        };
        assert_eq!(std::fs::read(&png).expect("read"), kept.to_vec());

        // No extension: the kind asked for.
        let plain = d.join("picture");
        app.save_bitmap_to(id, &plain, BitmapFile::Tiff)
            .expect("plain");
        assert!(std::fs::read(&plain).expect("read").starts_with(b"II*\0"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// #1925: a CMYK picture with transparency (here its right half is
    /// clear) keeps it in Save Bitmap As TIFF, as an alpha channel
    /// beside the full inks, and in Edit Bitmap: the file read back
    /// leaves the picture transparent where it was.
    #[test]
    fn a_cmyk_picture_keeps_its_transparency_in_tiff_and_edit_bitmap() {
        let mut app = App::headless();
        let id = cmyk_picture(&mut app);
        // The same picture with its right half (the black ink) clear.
        let s = app.doc().find_shape(id).expect("picture").clone();
        let ShapeKind::Bitmap { png, .. } = &s.kind else {
            panic!("bitmap");
        };
        let mut px = crate::bitmap_fx::decode(png).expect("pixels");
        for (x, _, p) in px.enumerate_pixels_mut() {
            if x >= 2 {
                p.0[3] = 0;
            }
        }
        let clear = crate::bitmap_fx::encode(&px).expect("png");
        let inks = PictureInks::cmyk(&clear, s.picture_inks.as_deref().expect("inks").png.clone());
        let id = app
            .new_shape(ShapeKind::Bitmap {
                rect: Rect::new(30.0, 10.0, 30.0 + 4.0 * 25.4 / 300.0, 20.0),
                width_px: 4,
                height_px: 2,
                png: clear.into(),
                fx: None,
            })
            .expect("bitmap");
        app.run(Command::SetPictureInks {
            shape: id,
            cmyk: true,
            inks: Some(Box::new(inks)),
        });
        let before = app.doc().find_shape(id).expect("picture").clone();
        let d = dir("alpha");

        let tif = d.join("clear.tif");
        app.save_bitmap_to(id, &tif, BitmapFile::Png).expect("tiff");
        let bytes = std::fs::read(&tif).expect("read");
        let mut dec = tiff::decoder::Decoder::new(std::io::Cursor::new(&bytes)).expect("tiff");
        assert_eq!(dec.colortype().expect("type"), tiff::ColorType::CMYKA(8));
        let back = read(&bytes).expect("read back");
        let alpha: Vec<u8> = back.rgba.pixels().map(|p| p.0[3]).collect();
        assert_eq!(alpha, [255, 255, 0, 0, 255, 255, 0, 0], "clear half kept");
        let (_, inks) = back.into_png().expect("png");
        let got = image::load_from_memory(&inks.expect("inks").png)
            .expect("planes")
            .to_luma8()
            .into_raw();
        assert_eq!(got, planes(&before), "the full inks, not faded");

        // Edit Bitmap: hand the file over, the other program saves it as
        // it is, the picture is read back still clear on the right.
        let path = app.write_bitmap_for_editing(id, &d).expect("written");
        app.run(Command::SetObjectData {
            shape: id,
            data: vec![("bitmap.link".into(), path.display().to_string())],
        });
        let resaved = std::fs::read(&path).expect("read");
        std::fs::write(&path, resaved).expect("write");
        assert!(app.reload_linked_bitmap(id));
        let s = app.doc().find_shape(id).expect("picture");
        assert!(s.cmyk_bitmap);
        let ShapeKind::Bitmap { png, .. } = &s.kind else {
            panic!("bitmap");
        };
        let px = crate::bitmap_fx::decode(png).expect("pixels");
        let alpha: Vec<u8> = px.pixels().map(|p| p.0[3]).collect();
        assert_eq!(alpha, [255, 255, 0, 0, 255, 255, 0, 0], "still clear");
        assert_eq!(planes(s), planes(&before));

        // An opaque CMYK picture stays a four channel file.
        let tif = d.join("opaque.tif");
        let opaque = cmyk_picture(&mut app);
        app.save_bitmap_to(opaque, &tif, BitmapFile::Png)
            .expect("tiff");
        let bytes = std::fs::read(&tif).expect("read");
        let mut dec = tiff::decoder::Decoder::new(std::io::Cursor::new(&bytes)).expect("tiff");
        assert_eq!(dec.colortype().expect("type"), tiff::ColorType::CMYK(8));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// An RGB picture is offered as the kind of file it came from, else
    /// PNG, and is written as JPEG or TIFF when asked.
    #[test]
    fn save_bitmap_as_writes_an_rgb_picture_as_jpeg_or_tiff() {
        let mut app = App::headless();
        let id = rgb_picture(&mut app);
        let s = app.doc().find_shape(id).expect("picture").clone();
        assert!(!is_cmyk(&s));
        assert_eq!(own_kind(&s), BitmapFile::Png);
        let mut named = s.clone();
        named.name = Some("Photo.JPG".into());
        assert_eq!(own_kind(&named), BitmapFile::Jpeg);
        named
            .data
            .push(("bitmap.link".into(), "/pictures/scan.tiff".into()));
        assert_eq!(own_kind(&named), BitmapFile::Tiff, "the linked file first");

        let d = dir("rgb");
        for (name, format) in [
            ("p.jpg", image::ImageFormat::Jpeg),
            ("p.tif", image::ImageFormat::Tiff),
        ] {
            let path = d.join(name);
            app.save_bitmap_to(id, &path, BitmapFile::Png).expect(name);
            let bytes = std::fs::read(&path).expect("read");
            assert_eq!(image::guess_format(&bytes).expect("format"), format);
            let img = image::load_from_memory(&bytes).expect("decodes").to_rgb8();
            assert_eq!(img.dimensions(), (5, 3));
            let p = img.get_pixel(2, 1).0;
            assert!(
                (p[0] as i32 - 200).abs() < 8 && (p[1] as i32 - 80).abs() < 8,
                "{name}: {p:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Edit Bitmap (#1834): a CMYK picture goes to the other program as
    /// a CMYK TIFF with its inks, and the file it saves back gives the
    /// picture its new inks (and the pixels that show them); undo puts
    /// the old ones back. An RGB picture still goes as a PNG.
    #[test]
    fn edit_bitmap_hands_over_and_reads_back_the_inks() {
        let mut app = App::headless();
        let id = cmyk_picture(&mut app);
        let before = app.doc().find_shape(id).expect("picture").clone();
        let d = dir("edit");
        let path = app.write_bitmap_for_editing(id, &d).expect("written");
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("tif"));
        let back = read(&std::fs::read(&path).expect("read")).expect("tiff");
        let (_, inks) = back.into_png().expect("png");
        assert_eq!(
            image::load_from_memory(&inks.expect("inks").png)
                .expect("planes")
                .to_luma8()
                .into_raw(),
            planes(&before)
        );
        app.run(Command::SetObjectData {
            shape: id,
            data: vec![("bitmap.link".into(), path.display().to_string())],
        });
        // The other program saves magenta ink everywhere.
        let magenta: Vec<u8> = (0..8).flat_map(|_| [0u8, 255, 0, 0]).collect();
        let tif = tracedraw_io::cmyk_bitmap::cmyk_tiff(4, 2, &magenta, 300.0, None).expect("tif");
        std::fs::write(&path, tif).expect("write");
        assert!(app.reload_linked_bitmap(id));
        let s = app.doc().find_shape(id).expect("picture").clone();
        assert!(s.cmyk_bitmap);
        let p = planes(&s);
        assert!(p[..8].iter().all(|v| *v == 0), "no cyan");
        assert!(p[8..16].iter().all(|v| *v == 255), "magenta");
        assert!(p[16..].iter().all(|v| *v == 0), "no yellow or black");
        let ShapeKind::Bitmap { png, .. } = &s.kind else {
            panic!("bitmap");
        };
        let px = crate::bitmap_fx::decode(png).expect("pixels");
        let c = px.get_pixel(1, 1).0;
        assert!(
            c[0] > 180 && c[1] < 120 && c[2] > 100,
            "shows magenta: {c:?}"
        );
        app.engine.undo().expect("undo");
        let s = app.doc().find_shape(id).expect("picture");
        assert_eq!(planes(s), planes(&before));

        let rgb = rgb_picture(&mut app);
        let path = app.write_bitmap_for_editing(rgb, &d).expect("written");
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("png"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `jpg` with an Exif `APP1` segment right after its start, holding
    /// orientation `o` (CIPA DC-008: a TIFF header and IFD0 with tag
    /// 274).
    fn with_orientation(jpg: &[u8], o: u16) -> Vec<u8> {
        let mut tiff = b"II*\0".to_vec();
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&274u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&o.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&0u32.to_le_bytes());
        let mut body = b"Exif\0\0".to_vec();
        body.extend(tiff);
        let mut out = jpg[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
        out.extend(body);
        // A file has one Exif segment: the one the writer made for the
        // resolution goes.
        let mut at = 2;
        while jpg.get(at) == Some(&0xFF)
            && jpg.get(at + 1).is_some_and(|m| (0xE0..=0xEF).contains(m))
        {
            let len = usize::from(u16::from_be_bytes([jpg[at + 2], jpg[at + 3]]));
            if !(jpg[at + 1] == 0xE1 && jpg[at + 4..].starts_with(b"Exif")) {
                out.extend_from_slice(&jpg[at..at + 2 + len]);
            }
            at += 2 + len;
        }
        out.extend_from_slice(&jpg[at..]);
        out
    }

    /// `jpg` without its Adobe `APP14` segment.
    fn without_adobe(jpg: &[u8]) -> Vec<u8> {
        let at = jpg
            .windows(2)
            .position(|w| w[0] == 0xFF && w[1] == 0xEE)
            .expect("APP14");
        let len = usize::from(u16::from_be_bytes([jpg[at + 2], jpg[at + 3]]));
        let mut out = jpg[..at].to_vec();
        out.extend_from_slice(&jpg[at + 2 + len..]);
        out
    }

    fn near(a: u8, b: u8, by: u8) -> bool {
        a.abs_diff(b) <= by
    }

    /// The ink planes of a read picture, one under the other.
    fn read_planes(p: &ReadPicture) -> image::GrayImage {
        image::load_from_memory(p.planes.as_deref().expect("inks"))
            .expect("planes")
            .to_luma8()
    }

    /// The bitmap File > Import placed from `bytes` written as `name`.
    fn imported(app: &mut App, d: &std::path::Path, name: &str, bytes: &[u8]) -> Shape {
        let path = d.join(name);
        std::fs::write(&path, bytes).expect("write");
        app.import_bitmap(&path);
        let s = app.selected_shapes();
        assert_eq!(s.len(), 1, "{name} imported: {}", app.status);
        assert!(
            matches!(s[0].kind, ShapeKind::Bitmap { .. }),
            "{name} is a bitmap"
        );
        s[0].clone()
    }

    fn pixel(s: &Shape, x: u32, y: u32) -> [u8; 4] {
        let ShapeKind::Bitmap { png, .. } = &s.kind else {
            panic!("not a bitmap");
        };
        image::load_from_memory(png.as_slice())
            .expect("pixels")
            .to_rgba8()
            .get_pixel(x, y)
            .0
    }

    fn size_px(s: &Shape) -> (u32, u32) {
        match &s.kind {
            ShapeKind::Bitmap {
                width_px,
                height_px,
                ..
            } => (*width_px, *height_px),
            _ => (0, 0),
        }
    }

    /// #2163: a CMYK JPEG (inverted under an Adobe marker, as the CMYK
    /// export writes it) comes in as a CMYK picture with its inks, its
    /// pixels the colour those inks have in the drawing, not the bright
    /// colour the image library makes of them.
    #[test]
    fn a_cmyk_jpeg_imports_with_its_inks_as_the_drawing_shows_them() {
        let d = dir("2163-import");
        let ink: Vec<u8> = (0..16 * 16).flat_map(|_| [153, 153, 0, 0]).collect();
        let jpg = tracedraw_io::cmyk_bitmap::cmyk_jpeg(16, 16, &ink, 95, 300.0, None).expect("jpg");
        let mut app = App::headless();
        let s = imported(&mut app, &d, "cmyk.jpg", &jpg);
        assert!(s.cmyk_bitmap, "a CMYK picture");
        let p = planes(&s);
        let n = 16 * 16;
        assert!(
            p[..2 * n].iter().all(|v| near(*v, 153, 3)),
            "cyan and magenta"
        );
        assert!(
            p[2 * n..].iter().all(|v| near(*v, 0, 3)),
            "no yellow or black"
        );
        let want = Color::Cmyk {
            c: 0.6,
            m: 0.6,
            y: 0.0,
            k: 0.0,
        }
        .to_rgb8();
        // What the image library shows: (255 - ink) for each colour.
        let naive = [102u8, 102, 255];
        assert!(
            (0..3).any(|i| want[i].abs_diff(naive[i]) > 20),
            "the drawing's colour is not the plain inversion: {want:?}"
        );
        let got = pixel(&s, 8, 8);
        assert!(
            (0..3).all(|i| near(got[i], want[i], 4)),
            "{got:?} shows as the drawing's C60 M60 {want:?}"
        );
        // At the resolution the file records.
        let ShapeKind::Bitmap { rect, .. } = &s.kind else {
            panic!("bitmap");
        };
        assert!(
            (rect.width() - 16.0 * 25.4 / 300.0).abs() < 1e-3,
            "{rect:?}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// #2163: without the Adobe marker the four channels are the inks as
    /// they are; with it they are inverted.
    #[test]
    fn a_cmyk_jpeg_without_the_adobe_marker_keeps_its_inks_as_stored() {
        let ink: Vec<u8> = (0..8 * 8).flat_map(|_| [40, 200, 10, 90]).collect();
        let inverted: Vec<u8> = ink.iter().map(|v| 255 - v).collect();
        // Written inverted under the marker, then the marker taken out:
        // the stored channels are the inks.
        let jpg =
            tracedraw_io::cmyk_bitmap::cmyk_jpeg(8, 8, &inverted, 100, 72.0, None).expect("jpg");
        let plain = without_adobe(&jpg);
        assert_eq!(jpeg_inks(&plain), Some(JpegInks::Plain));
        assert_eq!(jpeg_inks(&jpg), Some(JpegInks::Inverted));
        let p = read_planes(&read(&plain).expect("read"));
        assert_eq!(p.dimensions(), (8, 32));
        for (k, want) in [40u8, 200, 10, 90].iter().enumerate() {
            let v = p.get_pixel(3, k as u32 * 8 + 3).0[0];
            assert!(near(v, *want, 2), "ink {k}: {v} for {want}");
        }
    }

    /// A progressive CMYK JPEG (made for this test, inverted under an
    /// Adobe marker): left half C60 M60, right half Y100.
    #[test]
    fn a_progressive_cmyk_jpeg_reads_with_its_inks() {
        let bytes = include_bytes!("../testdata/progressive-cmyk.jpg");
        assert_eq!(jpeg_inks(bytes), Some(JpegInks::Inverted));
        let pic = read(bytes).expect("read");
        assert_eq!(pic.rgba.dimensions(), (16, 16));
        let p = read_planes(&pic);
        let at = |k: u32, x: u32| p.get_pixel(x, k * 16 + 8).0[0];
        assert!(near(at(0, 2), 153, 4) && near(at(1, 2), 153, 4), "C60 M60");
        assert!(near(at(2, 2), 0, 4) && near(at(3, 2), 0, 4));
        assert!(near(at(2, 13), 255, 4) && near(at(0, 13), 0, 4), "Y100");
    }

    /// YCCK (Adobe transform 2): the colour transform taken back gives
    /// cyan, magenta and yellow as they are; black is inverted.
    #[test]
    fn ycck_pixels_give_their_inks() {
        assert_eq!(
            jpeg_pixel_inks([200, 128, 128, 55], JpegInks::Ycck),
            [200, 200, 200, 200]
        );
        // Stored as R 255, G 0, B 0: full cyan, no magenta or yellow.
        let (r, g, b) = (255.0f32, 0.0f32, 0.0f32);
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let cb = 128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b;
        let cr = 128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b;
        let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        let got = jpeg_pixel_inks([byte(y), byte(cb), byte(cr), 255], JpegInks::Ycck);
        assert!(
            near(got[0], 255, 2) && near(got[1], 0, 2) && near(got[2], 0, 2),
            "{got:?}"
        );
        assert_eq!(got[3], 0);
        assert_eq!(jpeg_pixel_inks([1, 2, 3, 4], JpegInks::Plain), [1, 2, 3, 4]);
        assert_eq!(
            jpeg_pixel_inks([1, 2, 3, 4], JpegInks::Inverted),
            [254, 253, 252, 251]
        );
    }

    /// Six squares of 16 by 16 pixels: C100, M100, Y100, K100,
    /// C20 M40 Y60 K10, and no ink, written by libjpeg (ImageMagick 6,
    /// quality 100, no subsampling) as YCCK under an Adobe marker with
    /// transform 2, as Photoshop writes CMYK JPEGs.
    const YCCK_SQUARES: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xEE, 0x00, 0x0E, 0x41, 0x64, 0x6F, 0x62, 0x65, 0x00,
        0x64, 0x00, 0x00, 0x00, 0x00, 0x02, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0xFF, 0xDB, 0x00, 0x43, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0xFF, 0xC0, 0x00, 0x14, 0x08, 0x00,
        0x10, 0x00, 0x60, 0x04, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0x04, 0x11,
        0x00, 0xFF, 0xC4, 0x00, 0x18, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0A, 0x08, 0x0B, 0x09, 0xFF, 0xC4, 0x00,
        0x14, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0xFF, 0xC4, 0x00, 0x18, 0x01, 0x01, 0x00, 0x03, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x09, 0x0A, 0x0B,
        0xFF, 0xC4, 0x00, 0x14, 0x11, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xDA, 0x00, 0x0E, 0x04, 0x01, 0x00, 0x02,
        0x11, 0x03, 0x11, 0x04, 0x00, 0x00, 0x3F, 0x00, 0xF1, 0x7D, 0x54, 0xED, 0xFC, 0x2F, 0xE0,
        0x01, 0x4A, 0x0A, 0xB7, 0x70, 0xAF, 0x00, 0x10, 0x6E, 0xED, 0x30, 0xD5, 0x80, 0x00, 0xC2,
        0xEC, 0x03, 0xA5, 0x43, 0x80, 0x38, 0x02, 0xBA, 0x16, 0x20, 0x81, 0xEE, 0xE5, 0x00, 0x09,
        0x17, 0x57, 0x7A, 0x78, 0x36, 0x80, 0x02, 0x98, 0x15, 0x4E, 0xDF, 0xC0, 0x00, 0xA5, 0x05,
        0x5B, 0xB8, 0x57, 0x80, 0x08, 0x37, 0x76, 0x98, 0x6A, 0xC0, 0x00, 0x61, 0x76, 0x01, 0xD2,
        0xA1, 0xC0, 0x1C, 0x01, 0x5D, 0x0B, 0x10, 0x40, 0xF7, 0x72, 0x80, 0x04, 0x8B, 0xAB, 0xBD,
        0x3C, 0x1B, 0x40, 0x01, 0xFF, 0xD9,
    ];

    /// The same squares written by libjpeg through Pillow as CMYK
    /// inverted under an Adobe marker with transform 0.
    const INVERTED_SQUARES: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xEE, 0x00, 0x0E, 0x41, 0x64, 0x6F, 0x62, 0x65, 0x00, 0x64, 0x00, 0x00,
        0x00, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0xFF, 0xC0, 0x00,
        0x14, 0x08, 0x00, 0x10, 0x00, 0x60, 0x04, 0x43, 0x11, 0x00, 0x4D, 0x11, 0x00, 0x59, 0x11,
        0x00, 0x4B, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04,
        0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01,
        0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D, 0x01, 0x02,
        0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22,
        0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0,
        0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27,
        0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47,
        0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66,
        0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85,
        0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2,
        0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8,
        0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5,
        0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA,
        0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x0E, 0x04,
        0x43, 0x00, 0x4D, 0x00, 0x59, 0x00, 0x4B, 0x00, 0x00, 0x3F, 0x00, 0xFF, 0x00, 0x3F, 0xFA,
        0xFE, 0xFE, 0x2B, 0xFB, 0xF8, 0xAF, 0xEF, 0xE2, 0x8A, 0x28, 0xA2, 0xBF, 0xDF, 0xE2, 0xBF,
        0xC0, 0x1E, 0x8A, 0x28, 0xA2, 0x8A, 0x28, 0xAF, 0xF7, 0xF8, 0xAF, 0xF0, 0x07, 0xA2, 0x8A,
        0x28, 0xA2, 0x8A, 0x2B, 0xFD, 0xFE, 0x2B, 0xFC, 0x01, 0xE8, 0xA2, 0x8A, 0x2B, 0xF1, 0x9E,
        0xBF, 0x8C, 0xFA, 0xFF, 0x00, 0x33, 0x7A, 0xFF, 0x00, 0x72, 0x8A, 0x28, 0xA2, 0x8A, 0xFD,
        0x98, 0xAF, 0xEC, 0xC2, 0xBF, 0xD3, 0x22, 0xBE, 0xD0, 0xA2, 0x8A, 0x28, 0xAF, 0xF0, 0x07,
        0xA2, 0x8A, 0x28, 0xA2, 0x8A, 0x2B, 0xFD, 0xFE, 0x2B, 0xFC, 0x01, 0xE8, 0xA2, 0x8A, 0x28,
        0xA2, 0x8A, 0xFF, 0x00, 0x7F, 0x8A, 0xFF, 0x00, 0x00, 0x7A, 0x28, 0xA2, 0x8A, 0x28, 0xA2,
        0xBF, 0xDF, 0xE2, 0xBF, 0xC0, 0x1E, 0x8A, 0x28, 0xA2, 0xBF, 0x19, 0xEB, 0xF8, 0xCF, 0xAF,
        0xF3, 0x37, 0xAF, 0xF7, 0x28, 0xA2, 0x8A, 0x28, 0xAF, 0xD9, 0x8A, 0xFE, 0xCC, 0x2B, 0xFD,
        0x32, 0x2B, 0xED, 0x0A, 0x28, 0xA2, 0x8A, 0xFF, 0xD9,
    ];

    /// #3025: CMYK JPEGs written by another program come in with the
    /// inks they were written with. A YCCK file (Photoshop's usual CMYK
    /// JPEG) came in with its cyan, magenta and yellow inverted: C100
    /// as M100 Y100, white as C100 M100 Y100.
    #[test]
    fn cmyk_jpegs_of_other_programs_keep_their_inks() {
        let want: [[u8; 4]; 6] = [
            [255, 0, 0, 0],
            [0, 255, 0, 0],
            [0, 0, 255, 0],
            [0, 0, 0, 255],
            [51, 102, 153, 26],
            [0, 0, 0, 0],
        ];
        for (name, file) in [("YCCK", YCCK_SQUARES), ("inverted", INVERTED_SQUARES)] {
            let pic = read(file).expect(name);
            assert_eq!(pic.rgba.dimensions(), (96, 16), "{name}");
            let p = read_planes(&pic);
            for (i, ink) in want.iter().enumerate() {
                let x = i as u32 * 16 + 8;
                let got: Vec<u8> = (0..4).map(|k| p.get_pixel(x, k * 16 + 8).0[0]).collect();
                assert!(
                    got.iter().zip(ink).all(|(g, w)| near(*g, *w, 3)),
                    "{name} square {i}: {got:?}, want {ink:?}"
                );
            }
            // No ink is white.
            assert_eq!(
                &pic.rgba.get_pixel(88, 8).0[..3],
                &[255, 255, 255],
                "{name}"
            );
        }
    }

    /// Broken or other JPEG files are no CMYK JPEG, and reading them
    /// never panics.
    #[test]
    fn broken_jpeg_markers_give_no_inks() {
        let ink: Vec<u8> = (0..8 * 8).flat_map(|_| [10, 20, 30, 40]).collect();
        let jpg = tracedraw_io::cmyk_bitmap::cmyk_jpeg(8, 8, &ink, 90, 72.0, None).expect("jpg");
        let mut rgb = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
            .write_to(&mut rgb, image::ImageFormat::Jpeg)
            .expect("rgb jpeg");
        assert_eq!(jpeg_inks(&rgb.into_inner()), None, "three components");
        for b in [
            &b""[..],
            &[0xFF, 0xD8],
            &[0xFF, 0xD8, 0xFF],
            &[0xFF, 0xD8, 0xFF, 0xEE, 0x00],
            &[0xFF, 0xD8, 0xFF, 0xEE, 0x00, 0x00, 0xFF, 0xDA],
            &[0xFF, 0xD8, 0xFF, 0xEE, 0x00, 0x01, 0xFF, 0xDA],
            &[0xFF, 0xD8, 0xFF, 0xEE, 0xFF, 0xFF, b'A'],
            &[0xFF, 0xD8, 0x00, 0x00],
            b"II*\0",
        ] {
            assert_eq!(jpeg_inks(b), None, "{b:?}");
            assert!(read_cmyk_jpeg(b).is_none());
        }
        // Cut short anywhere: read or refused, never a panic.
        let turned = with_orientation(&jpg, 6);
        for file in [&jpg, &turned] {
            for cut in [3, 20, 40, file.len() / 2, file.len() - 3] {
                let _ = read(&file[..cut]);
            }
        }
        // An unknown Adobe transform with four components.
        let mut odd = jpg.clone();
        let at = odd.windows(5).position(|w| w == b"Adobe").expect("Adobe");
        odd[at + 11] = 7;
        assert_eq!(jpeg_inks(&odd), None);
    }

    /// #2167: a photo whose Exif orientation says it was taken with the
    /// camera on its side comes in upright, its CMYK inks turned with it.
    #[test]
    fn an_exif_orientation_stands_the_picture_upright() {
        let d = dir("2167-orient");
        let mut app = App::headless();
        // 16 by 8: red on the left, blue on the right.
        let img = image::RgbImage::from_fn(16, 8, |x, _| {
            image::Rgb(if x < 8 { [230, 20, 20] } else { [20, 20, 230] })
        });
        let mut jpg = std::io::Cursor::new(Vec::new());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 95)
            .encode_image(&img)
            .expect("jpeg");
        // 6: turned a quarter turn clockwise to stand upright.
        let photo = with_orientation(&jpg.into_inner(), 6);
        let s = imported(&mut app, &d, "photo.jpg", &photo);
        assert_eq!(size_px(&s), (8, 16));
        let top = pixel(&s, 4, 2);
        let bottom = pixel(&s, 4, 13);
        assert!(top[0] > 150 && top[2] < 100, "red on top: {top:?}");
        assert!(bottom[2] > 150 && bottom[0] < 100, "blue below: {bottom:?}");
        let ShapeKind::Bitmap { rect, .. } = &s.kind else {
            panic!("bitmap");
        };
        assert!(rect.height() > rect.width() * 1.9, "{rect:?}");
        // A CMYK JPEG: cyan on the left, black on the right.
        let ink: Vec<u8> = (0..16 * 8)
            .flat_map(|i| {
                if i % 16 < 8 {
                    [255, 0, 0, 0]
                } else {
                    [0, 0, 0, 255]
                }
            })
            .collect();
        let cmyk = tracedraw_io::cmyk_bitmap::cmyk_jpeg(16, 8, &ink, 95, 72.0, None).expect("jpg");
        let s = imported(&mut app, &d, "cmyk.jpg", &with_orientation(&cmyk, 6));
        assert_eq!(size_px(&s), (8, 16));
        let p = planes(&s);
        let at = |k: usize, x: usize, y: usize| p[k * 128 + y * 8 + x];
        assert!(at(0, 4, 2) > 200 && at(3, 4, 2) < 50, "cyan on top");
        assert!(at(3, 4, 13) > 200 && at(0, 4, 13) < 50, "black below");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A minimal uncompressed little-endian TIFF of `w` by `h` pixels
    /// of `bits` bits: the tags given (SHORT values) plus the strip, and
    /// a colour map when `map` is given.
    fn tiff(w: u16, h: u16, tags: &[(u16, u16)], map: Option<&[u16]>, data: &[u8]) -> Vec<u8> {
        let mut entries: Vec<(u16, u16, u32, u32)> =
            tags.iter().map(|&(t, v)| (t, 3, 1, u32::from(v))).collect();
        entries.push((256, 3, 1, u32::from(w)));
        entries.push((257, 3, 1, u32::from(h)));
        entries.push((278, 3, 1, u32::from(h)));
        entries.push((279, 4, 1, data.len() as u32));
        let n = entries.len() + 1 + usize::from(map.is_some());
        let ifd_len = 2 + n * 12 + 4;
        let data_at = 8 + ifd_len;
        let map_at = data_at + data.len();
        entries.push((273, 4, 1, data_at as u32));
        if let Some(m) = map {
            entries.push((320, 3, m.len() as u32, map_at as u32));
        }
        entries.sort_by_key(|e| e.0);
        let mut b = b"II*\0".to_vec();
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (t, ty, count, v) in entries {
            b.extend_from_slice(&t.to_le_bytes());
            b.extend_from_slice(&ty.to_le_bytes());
            b.extend_from_slice(&count.to_le_bytes());
            if ty == 3 && count == 1 {
                b.extend_from_slice(&(v as u16).to_le_bytes());
                b.extend_from_slice(&[0, 0]);
            } else {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(data);
        if let Some(m) = map {
            for v in m {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b
    }

    /// A BMP of `w` by `h` pixels of `bits` (1, 4 or 8) bits with the
    /// palette `pal` (RGB): rows bottom up, each padded to four bytes.
    fn bmp(w: u32, h: u32, bits: u16, pal: &[[u8; 3]], index: impl Fn(u32, u32) -> u8) -> Vec<u8> {
        let row = ((w * u32::from(bits)).div_ceil(32) * 4) as usize;
        let mut pixels = vec![0u8; row * h as usize];
        for y in 0..h {
            let line = &mut pixels[(h - 1 - y) as usize * row..][..row];
            for x in 0..w {
                let i = index(x, y);
                let bit = (x * u32::from(bits)) as usize;
                line[bit / 8] |= i << (8 - bits as usize - bit % 8);
            }
        }
        let offset = 14 + 40 + pal.len() * 4;
        let mut b = b"BM".to_vec();
        b.extend_from_slice(&((offset + pixels.len()) as u32).to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&(offset as u32).to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(w as i32).to_le_bytes());
        b.extend_from_slice(&(h as i32).to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&bits.to_le_bytes());
        b.extend_from_slice(&[0; 4]); // no compression
        b.extend_from_slice(&(pixels.len() as u32).to_le_bytes());
        b.extend_from_slice(&[0; 8]);
        b.extend_from_slice(&(pal.len() as u32).to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        for c in pal {
            b.extend_from_slice(&[c[2], c[1], c[0], 0]);
        }
        b.extend_from_slice(&pixels);
        b
    }

    fn mode(s: &Shape) -> crate::bitmap_fx::ColorMode {
        let ShapeKind::Bitmap { png, .. } = &s.kind else {
            panic!("bitmap");
        };
        crate::bitmap_fx::mode_of(s.cmyk_bitmap, png)
    }

    /// #3025: 1-bit, grayscale and indexed files keep their colour mode
    /// when imported (PNG, BMP, GIF, TIFF and Photoshop); they all came
    /// in as RGB. A palette TIFF, which did not open at all, opens with
    /// its colours.
    #[test]
    fn imported_pictures_keep_their_colour_mode() {
        use crate::bitmap_fx::{self as fx, ColorMode};
        let d = dir("3025-modes");
        let mut app = App::headless();
        let checker = image::RgbaImage::from_fn(10, 3, |x, y| {
            let v = if (x + y) % 2 == 0 { 0 } else { 255 };
            image::Rgba([v, v, v, 255])
        });
        let three = image::RgbaImage::from_fn(10, 3, |x, _| {
            image::Rgba(match x % 3 {
                0 => [200, 30, 30, 255],
                1 => [30, 200, 30, 255],
                _ => [30, 30, 200, 255],
            })
        });
        let gray = {
            let mut out = std::io::Cursor::new(Vec::new());
            image::GrayImage::from_fn(10, 3, |x, _| image::Luma([(x * 25) as u8]))
                .write_to(&mut out, image::ImageFormat::Png)
                .unwrap();
            out.into_inner()
        };
        let gif = {
            let mut out = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(three.clone())
                .write_to(&mut out, image::ImageFormat::Gif)
                .unwrap();
            out.into_inner()
        };
        let pal = [[200, 30, 30], [30, 200, 30], [30, 30, 200]];
        let cases: Vec<(&str, Vec<u8>, ColorMode)> = vec![
            (
                "bw.png",
                fx::encode_bw(&checker).unwrap(),
                ColorMode::BlackWhite,
            ),
            (
                "indexed.png",
                fx::encode_indexed(&three).unwrap(),
                ColorMode::Paletted,
            ),
            ("gray.png", gray, ColorMode::Grayscale),
            ("indexed.gif", gif, ColorMode::Paletted),
            (
                "bw.bmp",
                bmp(10, 3, 1, &[[0, 0, 0], [255, 255, 255]], |x, y| {
                    ((x + y) % 2) as u8
                }),
                ColorMode::BlackWhite,
            ),
            (
                "indexed.bmp",
                bmp(10, 3, 8, &pal, |x, _| (x % 3) as u8),
                ColorMode::Paletted,
            ),
            (
                "bw.tif",
                // 1 bit, white is zero; rows of two bytes.
                tiff(
                    10,
                    3,
                    &[(258, 1), (259, 1), (262, 0), (277, 1)],
                    None,
                    &[0x55, 0x40, 0xAA, 0x80, 0x55, 0x40],
                ),
                ColorMode::BlackWhite,
            ),
            ("rgb.png", fx::encode(&three).unwrap(), ColorMode::Rgb),
        ];
        for (name, bytes, want) in cases {
            let s = imported(&mut app, &d, name, &bytes);
            assert_eq!(mode(&s), want, "{name}");
            assert_eq!(size_px(&s), (10, 3), "{name}");
        }
        // A palette TIFF: 8 bits of index, a colour map of 256 entries
        // (16-bit levels: all reds, all greens, all blues).
        let mut map = vec![0u16; 768];
        for (i, c) in pal.iter().enumerate() {
            map[i] = u16::from(c[0]) * 257;
            map[256 + i] = u16::from(c[1]) * 257;
            map[512 + i] = u16::from(c[2]) * 257;
        }
        let index: Vec<u8> = (0..30).map(|i| (i % 10 % 3) as u8).collect();
        let t = tiff(
            10,
            3,
            &[(258, 8), (259, 1), (262, 3), (277, 1)],
            Some(&map),
            &index,
        );
        assert_eq!(file_mode(&t), FileMode::Indexed);
        let s = imported(&mut app, &d, "indexed.tif", &t);
        assert_eq!(mode(&s), ColorMode::Paletted);
        assert_eq!(pixel(&s, 1, 0), [30, 200, 30, 255]);
        assert_eq!(pixel(&s, 5, 2), [30, 30, 200, 255]);
        // The pixels stay what they were.
        let s = imported(&mut app, &d, "bw2.png", &fx::encode_bw(&checker).unwrap());
        assert_eq!(pixel(&s, 0, 0), [0, 0, 0, 255]);
        assert_eq!(pixel(&s, 1, 0), [255, 255, 255, 255]);
    }

    /// #3025: a 1-bit Bitmap mode Photoshop file opens as a black and
    /// white picture, a grayscale one as a grayscale picture.
    #[test]
    fn photoshop_bitmaps_keep_the_files_mode() {
        let psd = |mode: u16, depth: u16, image: &[u8]| {
            let mut b = b"8BPS".to_vec();
            b.extend_from_slice(&1u16.to_be_bytes());
            b.extend_from_slice(&[0; 6]);
            b.extend_from_slice(&1u16.to_be_bytes());
            b.extend_from_slice(&1u32.to_be_bytes());
            b.extend_from_slice(&10u32.to_be_bytes());
            b.extend_from_slice(&depth.to_be_bytes());
            b.extend_from_slice(&mode.to_be_bytes());
            b.extend_from_slice(&[0; 12]);
            b.extend_from_slice(&0u16.to_be_bytes());
            b.extend_from_slice(image);
            b
        };
        for (bytes, want) in [
            (
                psd(0, 1, &[0b1010_0000, 0b0100_0000]),
                crate::bitmap_fx::ColorMode::BlackWhite,
            ),
            (
                psd(1, 8, &[0, 30, 60, 90, 120, 150, 180, 210, 240, 255]),
                crate::bitmap_fx::ColorMode::Grayscale,
            ),
        ] {
            let mut imported =
                tracedraw_io::psd::parse(&bytes, &mut tracedraw_core::id::IdSource::default())
                    .expect("psd");
            keep_psd_mode(&bytes, &mut imported.shapes);
            assert_eq!(mode(&imported.shapes[0]), want);
        }
    }

    /// #3210, #3063: a JPEG XL file written by the export (lossless,
    /// opaque or with transparency, down to one pixel) is imported and
    /// opened with every pixel as it was.
    #[test]
    fn jpeg_xl_files_export_and_import_losslessly() {
        let d = dir("jxl");
        let mut app = App::headless();
        let mut img = image::RgbaImage::new(37, 23);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x * 7) as u8, (y * 11) as u8, (x ^ y) as u8, 255]);
        }
        let mut clear = img.clone();
        for (x, _, p) in clear.enumerate_pixels_mut() {
            p.0[3] = (x * 6) as u8;
        }
        for (name, pic) in [("opaque.jxl", &img), ("clear.jxl", &clear)] {
            let bytes = crate::export::encode_jxl(pic).expect("jxl");
            assert!(crate::files::is_jxl(&bytes), "{name}");
            let s = imported(&mut app, &d, name, &bytes);
            assert_eq!(size_px(&s), (37, 23), "{name}");
            for (x, y) in [(0, 0), (5, 3), (36, 22), (20, 11)] {
                assert_eq!(pixel(&s, x, y), pic.get_pixel(x, y).0, "{name} ({x}, {y})");
            }
            // Read as a picture too (File > Open of a picture).
            let read = read(&bytes).expect("reads");
            assert_eq!(read.rgba.get_pixel(20, 11), pic.get_pixel(20, 11));
        }
        // One pixel across: written two across, the same colour.
        let one = image::RgbaImage::from_pixel(1, 5, image::Rgba([9, 99, 199, 255]));
        let bytes = crate::export::encode_jxl(&one).expect("jxl");
        let back = crate::files::decode_picture(&bytes)
            .expect("decodes")
            .to_rgba8();
        assert_eq!(back.dimensions(), (2, 5));
        assert!(back.pixels().all(|p| p.0 == [9, 99, 199, 255]));
        // Not a JPEG XL file: an error, not a picture.
        assert!(crate::files::decode_picture(&[0xff, 0x0a, 1, 2, 3]).is_err());
    }

    /// #2167: File > Import reads every common kind of PNG and JPEG:
    /// 8 and 16 bits, grey, with and without alpha, a palette with a
    /// transparent entry, baseline, progressive and CMYK JPEG.
    #[test]
    fn every_common_png_and_jpeg_imports() {
        use image::DynamicImage as Dyn;
        let d = dir("2167-kinds");
        let mut app = App::headless();
        let encode = |img: Dyn, f: image::ImageFormat| {
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, f).expect("encode");
            out.into_inner()
        };
        let rgb = image::RgbImage::from_fn(6, 4, |x, _| {
            image::Rgb(if x < 3 { [230, 20, 20] } else { [20, 20, 230] })
        });
        let dynamic = Dyn::ImageRgb8(rgb);
        let mut half_clear = dynamic.to_rgba8();
        for (x, _, p) in half_clear.enumerate_pixels_mut() {
            if x >= 3 {
                p.0[3] = 0;
            }
        }
        let half_clear = Dyn::ImageRgba8(half_clear);
        let png = image::ImageFormat::Png;
        let jpeg = image::ImageFormat::Jpeg;
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("rgb8.png", encode(dynamic.clone(), png)),
            ("rgba8.png", encode(half_clear.clone(), png)),
            (
                "rgb16.png",
                encode(Dyn::ImageRgb16(dynamic.to_rgb16()), png),
            ),
            (
                "rgba16.png",
                encode(Dyn::ImageRgba16(half_clear.to_rgba16()), png),
            ),
            (
                "grey8.png",
                encode(Dyn::ImageLuma8(dynamic.to_luma8()), png),
            ),
            (
                "grey16.png",
                encode(Dyn::ImageLuma16(dynamic.to_luma16()), png),
            ),
            (
                "greya8.png",
                encode(Dyn::ImageLumaA8(half_clear.to_luma_alpha8()), png),
            ),
            ("baseline.jpg", encode(dynamic.clone(), jpeg)),
            (
                "grey.jpg",
                encode(Dyn::ImageLuma8(dynamic.to_luma8()), jpeg),
            ),
        ];
        for (name, bytes) in &cases {
            let s = imported(&mut app, &d, name, bytes);
            assert_eq!(size_px(&s), (6, 4), "{name}");
            let (left, right) = (pixel(&s, 0, 1), pixel(&s, 5, 1));
            if name.starts_with("grey") {
                assert!(left[0] > right[0] + 20, "{name}: {left:?} {right:?}");
            } else {
                assert!(left[0] > 150 && left[2] < 100, "{name}: red {left:?}");
                if !name.starts_with("rgba") {
                    assert!(right[2] > 150 && right[0] < 100, "{name}: blue {right:?}");
                }
            }
            if name.starts_with("rgba") || name.starts_with("greya") {
                assert_eq!(left[3], 255, "{name}");
                assert_eq!(right[3], 0, "{name}: clear");
            }
            assert!(!s.cmyk_bitmap, "{name} is RGB");
        }
        // Made for these tests: a palette PNG (green, a clear entry) and
        // progressive JPEG files.
        let palette = include_bytes!("../testdata/palette.png");
        let s = imported(&mut app, &d, "palette.png", palette);
        assert_eq!(size_px(&s), (8, 4));
        assert_eq!(pixel(&s, 1, 1), [0, 200, 0, 255]);
        assert_eq!(pixel(&s, 6, 1)[3], 0);
        let progressive = include_bytes!("../testdata/progressive-rgb.jpg");
        let s = imported(&mut app, &d, "progressive.jpg", progressive);
        assert_eq!(size_px(&s), (24, 16));
        let (left, right) = (pixel(&s, 2, 8), pixel(&s, 21, 8));
        assert!(left[0] > 180 && left[2] < 70, "{left:?}");
        assert!(right[2] > 180 && right[0] < 70, "{right:?}");
        let cmyk = include_bytes!("../testdata/progressive-cmyk.jpg");
        let s = imported(&mut app, &d, "progressive-cmyk.jpg", cmyk);
        assert!(s.cmyk_bitmap && s.picture_inks.is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// #2849: an imported JPEG is saved in a .tdraw as that JPEG, not as
    /// a PNG two to three times its size; it opens with the same pixels
    /// and saves as JPEG again. A picture whose pixels change is saved as
    /// PNG; a CMYK JPEG keeps its own way.
    #[test]
    fn imported_jpegs_are_saved_as_the_jpeg() {
        let d = std::env::temp_dir().join(format!("td-jpeg-keep-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        // A photo-like picture: a gradient with fine noise.
        let mut seed = 3u32;
        let img = image::RgbImage::from_fn(480, 320, |x, y| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let n = ((seed >> 16) % 24) as u8;
            image::Rgb([
                (x / 2) as u8 ^ n,
                ((y * 3 / 4) as u8).wrapping_add(n),
                128u8.wrapping_add(n),
            ])
        });
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
            .encode_image(&img)
            .expect("jpeg");
        let mut app = App::headless();
        let s = imported(&mut app, &d, "photo.jpg", &jpeg);
        let ShapeKind::Bitmap { png, .. } = &s.kind else {
            panic!("bitmap")
        };
        assert!(
            png.len() > jpeg.len() * 2,
            "png {} jpeg {}",
            png.len(),
            jpeg.len()
        );
        assert_eq!(
            tracedraw_core::jpeg_source::jpeg_of(png).as_deref(),
            Some(&jpeg[..])
        );
        let file = d.join("photo.tdraw");
        app.save_to(file.clone());
        let bytes = std::fs::read(&file).expect("saved");
        assert!(
            bytes.len() < jpeg.len() + jpeg.len() / 2 + 20_000,
            "{} bytes",
            bytes.len()
        );
        let text = tracedraw_core::document::tdraw_text(&bytes).expect("text");
        assert!(text.contains("\"jpg:/9j/"), "the table holds the JPEG");
        // It opens with the picture's pixels, and saves as JPEG again.
        let mut other = App::headless();
        other.open_path(file.clone());
        let back = other
            .doc()
            .pages
            .iter()
            .flat_map(|p| &p.layers)
            .flat_map(|l| &l.shapes)
            .find(|s| matches!(s.kind, ShapeKind::Bitmap { .. }))
            .cloned()
            .expect("the picture");
        for (x, y) in [(0, 0), (100, 50), (479, 319), (240, 160)] {
            assert_eq!(pixel(&back, x, y), pixel(&s, x, y), "pixel {x},{y}");
        }
        let again = d.join("again.tdraw");
        other.save_to(again.clone());
        let text = tracedraw_core::document::tdraw_text(&std::fs::read(&again).expect("saved"))
            .expect("text");
        assert!(text.contains("\"jpg:/9j/"));
        // Other pixels: saved as PNG.
        let changed = tracedraw_core::SharedBytes::from({
            let mut b = Vec::new();
            image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 4))
                .write_to(&mut std::io::Cursor::new(&mut b), image::ImageFormat::Png)
                .expect("png");
            b
        });
        assert!(tracedraw_core::jpeg_source::jpeg_of(&changed).is_none());
        // A CMYK JPEG is not kept as JPEG (its inks go another way).
        let cmyk = include_bytes!("../testdata/progressive-cmyk.jpg");
        assert!(tracedraw_core::jpeg_source::decode_rgba(cmyk).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A disc of soft-edged colours on a see-through square, `n` pixels
    /// across: many colours, many see-through levels at its edge.
    fn soft_disc(n: u32) -> image::RgbaImage {
        let c = n as f32 / 2.0;
        image::RgbaImage::from_fn(n, n, |x, y| {
            let d = ((x as f32 + 0.5 - c).hypot(y as f32 + 0.5 - c) - (c - 4.0)).clamp(-1.0, 3.0);
            let a = ((1.0 - (d + 1.0) / 4.0) * 255.0).round() as u8;
            image::Rgba([(x * 255 / n) as u8, (y * 255 / n) as u8, 120, a])
        })
    }

    fn bitmap_of(app: &mut App, img: &image::RgbaImage, png: Vec<u8>) -> ShapeId {
        let (w, h) = img.dimensions();
        app.new_shape(ShapeKind::Bitmap {
            rect: Rect::new(0.0, 0.0, w as f64 * 25.4 / 300.0, h as f64 * 25.4 / 300.0),
            width_px: w,
            height_px: h,
            png: png.into(),
            fx: None,
        })
        .expect("bitmap")
    }

    fn png_of(app: &App, id: ShapeId) -> Vec<u8> {
        match &app.doc().find_shape(id).expect("bitmap").kind {
            ShapeKind::Bitmap { png, .. } => png.to_vec(),
            _ => panic!("a bitmap"),
        }
    }

    /// #3539, #3504: Indexed (8-bit) of a see-through picture with soft
    /// edges is an indexed bitmap (a palette PNG with a clear entry), not
    /// RGB; Black and White (1-bit) of one is a true 1-bit bitmap on
    /// white; a Lab Color picture goes to Edit Bitmap as a CIE Lab TIFF,
    /// which reads back as a Lab bitmap with the same colours.
    #[test]
    fn image_modes_of_see_through_and_lab_pictures_stay_in_their_mode() {
        use crate::bitmap_fx::{self as fx, ColorMode};
        let mut app = App::headless();
        let img = soft_disc(64);
        let id = bitmap_of(&mut app, &img, fx::encode(&img).expect("png"));
        app.select(vec![id]);
        app.paletted_bitmaps(&crate::bitmap_modes::PalettedSettings::default());
        let png = png_of(&app, id);
        assert!(fx::png_is_paletted(&png), "indexed, not RGB");
        let out = fx::decode(&png).expect("decodes");
        assert_eq!(
            *out.get_pixel(0, 0),
            image::Rgba([0, 0, 0, 0]),
            "the corner clear"
        );
        assert_eq!(out.get_pixel(32, 32)[3], 255, "the middle opaque");
        // The fitting alone: at most 256 entries whatever comes in.
        let many = image::RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([
                (x * 4) as u8,
                (y * 4) as u8,
                ((x + y) * 2) as u8,
                (x * 3) as u8,
            ])
        });
        assert!(fx::encode_indexed(&many).is_none());
        assert!(fx::png_is_paletted(
            &fx::encode_indexed_fitted(&many).expect("indexed")
        ));

        let id = bitmap_of(&mut app, &img, fx::encode(&img).expect("png"));
        app.select(vec![id]);
        app.black_and_white_bitmaps(&Default::default());
        let png = png_of(&app, id);
        assert_eq!((png[24], png[25]), (1, 0), "1-bit grey, no alpha");
        let out = fx::decode(&png).expect("decodes");
        assert_eq!(
            *out.get_pixel(0, 0),
            image::Rgba([255, 255, 255, 255]),
            "on white"
        );

        for alpha in [false, true] {
            let src = if alpha {
                img.clone()
            } else {
                fx::flatten_on_white(&img)
            };
            let lab = crate::bitmap_modes::lab_mode(&src);
            let id = bitmap_of(&mut app, &lab, fx::encode_lab(&lab).expect("png"));
            let s = app.doc().find_shape(id).expect("bitmap").clone();
            assert!(is_lab(&s));
            assert_eq!(own_kind(&s), BitmapFile::Tiff);
            let d = dir(if alpha { "3539-lab-alpha" } else { "3539-lab" });
            let path = app.write_bitmap_for_editing(id, &d).expect("written");
            assert_eq!(path.extension().and_then(|e| e.to_str()), Some("tif"));
            let bytes = std::fs::read(&path).expect("read");
            assert_eq!(file_mode(&bytes), FileMode::Lab);
            let back = read(&bytes).expect("a Lab TIFF reads");
            assert_eq!(back.mode, FileMode::Lab);
            assert_eq!(back.rgba.dimensions(), lab.dimensions());
            for (a, b) in back.rgba.pixels().zip(lab.pixels()) {
                assert_eq!(a[3], b[3]);
                if b[3] > 0 {
                    // 8 bits of L*, a* and b* round a little: the colours
                    // stay within a just noticeable difference.
                    let la = crate::bitmap_modes::rgb_to_lab([a[0], a[1], a[2]]);
                    let lb = crate::bitmap_modes::rgb_to_lab([b[0], b[1], b[2]]);
                    let de = la
                        .iter()
                        .zip(lb)
                        .map(|(x, y)| (x - y).powi(2))
                        .sum::<f32>()
                        .sqrt();
                    assert!(de < 1.5, "{a:?} {b:?} {de}");
                }
            }
            let (png, _) = back.into_png().expect("png");
            assert_eq!(fx::mode_of(false, &png), ColorMode::Lab);
            let _ = std::fs::remove_dir_all(&d);
        }
    }
}

//! The JPEG file a picture came from, kept to save it as it was (#2849).
//!
//! A picture object holds its pixels as PNG, which every part of the
//! program reads. For a photo PNG is two to three times the JPEG it was
//! imported from, and a `.tdraw` full of photos came out many times the
//! size of the same drawing in CorelDRAW. So when a JPEG is imported
//! whose pixels are exactly the picture's, the JPEG is remembered beside
//! its PNG ([`remember`]), and a packed `.tdraw` writes the JPEG in its
//! table instead of the PNG (`"jpg:"` and its base64). Reading the file
//! decodes the JPEG back into the same pixels as PNG and remembers it
//! again, so the next save keeps it too. A picture whose pixels change
//! (an effect, a crop, a resample) has new PNG bytes with no JPEG, and
//! is saved as PNG.

use crate::bytes::SharedBytes;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

/// The start of a table entry that holds a JPEG file, not PNG.
pub const JPEG_PREFIX: &str = "jpg:";

/// A remembered JPEG: the PNG it stands for (not kept alive by it) and
/// the JPEG file.
struct Source {
    png: Weak<[u8]>,
    jpeg: Arc<[u8]>,
}

/// The remembered JPEGs, by their PNG's hash and length.
type Sources = HashMap<(u64, usize), Vec<Source>>;

static SOURCES: Mutex<Option<Sources>> = Mutex::new(None);

fn key(png: &[u8]) -> (u64, usize) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    png.hash(&mut h);
    (h.finish(), png.len())
}

/// Whether bytes start as a JPEG file does.
pub fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xd8, 0xff])
}

/// The pixels of a JPEG file as 8-bit RGBA: width, height and pixels.
/// `None` for a file that is not a JPEG or does not decode.
pub fn decode_rgba(jpeg: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    use zune_core::colorspace::ColorSpace;
    if !is_jpeg(jpeg) {
        return None;
    }
    let options = zune_core::options::DecoderOptions::default()
        .set_strict_mode(false)
        .set_max_width(usize::MAX)
        .set_max_height(usize::MAX)
        .jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(
        zune_core::bytestream::ZCursor::new(jpeg),
        options,
    );
    dec.decode_headers().ok()?;
    // Ink JPEGs (CMYK) keep their inks another way.
    if matches!(dec.input_colorspace()?, ColorSpace::CMYK | ColorSpace::YCCK) {
        return None;
    }
    let info = dec.info()?;
    let pixels = dec.decode().ok()?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    (pixels.len() == w as usize * h as usize * 4).then_some((w, h, pixels))
}

/// Whether `jpeg` decodes to exactly these RGBA pixels, so that it can
/// stand for them.
pub fn decodes_to(jpeg: &[u8], width: u32, height: u32, rgba: &[u8]) -> bool {
    decode_rgba(jpeg).is_some_and(|(w, h, px)| w == width && h == height && px == rgba)
}

/// Remember that `jpeg` holds exactly the pixels of `png` (the caller
/// checked it, [`decodes_to`]).
pub fn remember(png: &SharedBytes, jpeg: Arc<[u8]>) {
    let mut all = SOURCES.lock().unwrap_or_else(|e| e.into_inner());
    let map = all.get_or_insert_with(HashMap::new);
    // Pictures gone from every drawing are forgotten now and then.
    if map.len() > 64 && map.len().is_power_of_two() {
        map.retain(|_, v| {
            v.retain(|s| s.png.strong_count() > 0);
            !v.is_empty()
        });
    }
    let slot = map.entry(key(png)).or_default();
    slot.retain(|s| s.png.strong_count() > 0);
    slot.push(Source {
        png: Arc::downgrade(png.arc()),
        jpeg,
    });
}

/// The JPEG remembered for these PNG bytes, if any.
pub fn jpeg_of(png: &[u8]) -> Option<Arc<[u8]>> {
    let all = SOURCES.lock().unwrap_or_else(|e| e.into_inner());
    let slot = all.as_ref()?.get(&key(png))?;
    slot.iter()
        .find(|s| s.png.upgrade().is_some_and(|p| *p == *png))
        .map(|s| s.jpeg.clone())
}

/// PNG bytes of a JPEG file, to hold as a picture: the decoded pixels,
/// opaque ones as RGB. The JPEG is remembered for them.
pub fn png_of_jpeg(jpeg: &[u8]) -> Result<SharedBytes, String> {
    let (w, h, rgba) = decode_rgba(jpeg).ok_or("the picture's JPEG does not decode")?;
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    }
    let png = SharedBytes::from(out);
    remember(&png, Arc::from(jpeg));
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remembered_jpeg_is_found_by_its_png_and_forgotten_with_it() {
        let png = SharedBytes::from(vec![1u8, 2, 3, 4, 5]);
        assert!(jpeg_of(&png).is_none());
        remember(&png, Arc::from(&b"\xff\xd8\xffjpeg"[..]));
        assert_eq!(jpeg_of(&png).as_deref(), Some(&b"\xff\xd8\xffjpeg"[..]));
        // The same bytes in another buffer (a copy read back) find it.
        let copy = SharedBytes::from(vec![1u8, 2, 3, 4, 5]);
        assert!(jpeg_of(&copy).is_some());
        drop(png);
        drop(copy);
        // Once no drawing holds the picture, nothing is found for it.
        assert!(jpeg_of(&[1u8, 2, 3, 4, 5]).is_none());
        assert!(decode_rgba(b"not a jpeg").is_none());
    }
}

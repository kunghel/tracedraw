//! A picture's layers kept after Bitmaps > Edit Inside's Finish (#4112).
//!
//! Finish puts the layers together into the picture on the page. The
//! layers can also stay: while the drawing is open (kept in memory, on
//! by default) and, when asked, in the drawing's file (as the picture's
//! object data `pixel.layers`, hidden from the Object Data panel by its
//! dotted name). Either way they carry the fingerprint of the picture
//! they were put together into: when the picture changed since (an undo,
//! another edit, a picture swapped), the layers no longer fit it and the
//! picture opens as it is.

use crate::pixel_edit::{MergeMode, PixelImage, PixelLayer};
use base64::Engine as _;
use image::{GrayImage, RgbaImage};
use serde::{Deserialize, Serialize};
use tracedraw_core::ShapeId;

/// The object data name of layers saved in the drawing's file.
pub const DATA_KEY: &str = "pixel.layers";
/// Pictures whose layers are kept in memory at most: the oldest goes.
const MOST: usize = 8;

/// A fingerprint of a picture's PNG bytes (FNV-1a).
pub fn fingerprint(png: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in png {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h ^ png.len() as u64
}

/// The layers of a picture kept in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Kept {
    pub shape: ShapeId,
    /// The fingerprint of the picture they were put together into.
    pub check: u64,
    /// Every layer with its own pixels, bottom first.
    pub layers: Vec<PixelLayer>,
    pub active: usize,
}

/// The pictures' layers kept while the drawings are open, newest last.
#[derive(Debug, Clone, Default)]
pub struct KeptLayers {
    list: Vec<Kept>,
}

impl KeptLayers {
    /// Keep `pi`'s layers for the picture `shape`, put together into the
    /// PNG `png`.
    pub fn keep(&mut self, shape: ShapeId, png: &[u8], pi: &PixelImage) {
        self.forget(shape);
        self.list.push(Kept {
            shape,
            check: fingerprint(png),
            layers: whole_layers(pi),
            active: pi.active,
        });
        if self.list.len() > MOST {
            self.list.remove(0);
        }
    }

    pub fn forget(&mut self, shape: ShapeId) {
        self.list.retain(|k| k.shape != shape);
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// The layers kept for `shape`, when its picture is still `png`.
    pub fn get(&self, shape: ShapeId, png: &[u8]) -> Option<&Kept> {
        let check = fingerprint(png);
        self.list
            .iter()
            .find(|k| k.shape == shape && k.check == check)
    }
}

/// `pi`'s layers, the one in use with its pixels.
pub fn whole_layers(pi: &PixelImage) -> Vec<PixelLayer> {
    let mut layers = pi.layers.clone();
    if let Some(l) = layers.get_mut(pi.active) {
        l.img = pi.img.clone();
    }
    layers
}

/// Put `layers` (each with its own pixels) in `pi` in place of its one
/// layer, `active` in use. False, and `pi` unchanged, when they do not
/// fit its size.
pub fn restore(pi: &mut PixelImage, mut layers: Vec<PixelLayer>, active: usize) -> bool {
    let size = pi.img.dimensions();
    let fits = !layers.is_empty()
        && active < layers.len()
        && layers.iter().all(|l| {
            l.img.dimensions() == size && l.mask.as_ref().is_none_or(|m| m.dimensions() == size)
        });
    if !fits {
        return false;
    }
    pi.img = std::mem::replace(&mut layers[active].img, RgbaImage::new(0, 0));
    pi.layers = layers;
    pi.active = active;
    true
}

#[derive(Serialize, Deserialize)]
struct SavedLayer {
    name: String,
    opacity: f32,
    merge: MergeMode,
    visible: bool,
    locked: bool,
    clip: bool,
    #[serde(default)]
    adjust: Option<tracedraw_core::BitmapEffect>,
    /// The pixels as base64 PNG.
    img: String,
    #[serde(default)]
    mask: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style: Option<crate::pixel_style::LayerStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<crate::pixel_text::PixelText>,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    /// The fingerprint of the picture they were put together into, in
    /// hex (a JSON number would lose bits).
    check: String,
    active: usize,
    layers: Vec<SavedLayer>,
}

fn png_rgba(img: &RgbaImage) -> Option<String> {
    let png = crate::bitmap_fx::encode(img)?;
    Some(base64::engine::general_purpose::STANDARD.encode(png))
}

fn png_gray(m: &GrayImage) -> Option<String> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let mut buf = Vec::new();
    PngEncoder::new_with_quality(&mut buf, CompressionType::Fast, FilterType::Sub)
        .write_image(m.as_raw(), m.width(), m.height(), image::ExtendedColorType::L8)
        .ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(buf))
}

fn unbase64(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

/// `pi`'s layers as object data text, for the picture put together into
/// the PNG `png`.
pub fn save(pi: &PixelImage, png: &[u8]) -> Option<String> {
    let layers = whole_layers(pi)
        .iter()
        .map(|l| {
            Some(SavedLayer {
                name: l.name.clone(),
                opacity: l.opacity,
                merge: l.merge,
                visible: l.visible,
                locked: l.locked,
                clip: l.clip,
                adjust: l.adjust.clone(),
                img: png_rgba(&l.img)?,
                mask: match &l.mask {
                    Some(m) => Some(png_gray(m)?),
                    None => None,
                },
                style: l.style,
                text: l.text.clone(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    serde_json::to_string(&Saved {
        check: format!("{:016x}", fingerprint(png)),
        active: pi.active,
        layers,
    })
    .ok()
}

/// The layers saved in `text`, when they were put together into the
/// picture `png`; the layer in use.
pub fn load(text: &str, png: &[u8]) -> Option<(Vec<PixelLayer>, usize)> {
    let saved: Saved = serde_json::from_str(text).ok()?;
    if u64::from_str_radix(&saved.check, 16).ok()? != fingerprint(png) {
        return None;
    }
    let layers = saved
        .layers
        .into_iter()
        .map(|s| {
            let img = crate::bitmap_fx::decode(&unbase64(&s.img)?)?;
            let mask = match s.mask {
                Some(m) => Some(
                    image::load_from_memory_with_format(&unbase64(&m)?, image::ImageFormat::Png)
                        .ok()?
                        .to_luma8(),
                ),
                None => None,
            };
            Some(PixelLayer {
                name: s.name,
                img,
                opacity: s.opacity.clamp(0.0, 1.0),
                merge: s.merge,
                visible: s.visible,
                locked: s.locked,
                mask,
                clip: s.clip,
                adjust: s.adjust,
                style: s.style,
                text: s.text,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some((layers, saved.active))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    fn layered() -> PixelImage {
        let mut pi = PixelImage::new(RgbaImage::from_pixel(6, 4, Rgba([200, 10, 10, 255])));
        pi.add_layer("Top".to_string());
        pi.img.put_pixel(1, 1, Rgba([0, 0, 255, 255]));
        assert!(pi.add_layer_mask());
        if let Some(m) = pi.layers[pi.active].mask.as_mut() {
            m.put_pixel(2, 2, Luma([0]));
        }
        pi.layers[pi.active].opacity = 0.5;
        pi
    }

    #[test]
    fn layers_saved_as_text_come_back_only_for_the_same_picture() {
        let pi = layered();
        let png = b"picture as put together";
        let text = save(&pi, png).expect("saved");
        let (layers, active) = load(&text, png).expect("loaded");
        assert_eq!(active, pi.active);
        assert_eq!(layers, whole_layers(&pi));
        assert!(load(&text, b"another picture").is_none());
        let mut back = PixelImage::new(RgbaImage::new(6, 4));
        assert!(restore(&mut back, layers, active));
        assert_eq!(back.img, pi.img);
        assert_eq!(back.layers[0].img, pi.layers[0].img);
        assert!(back.layers[back.active].img.width() == 0);
        // Layers of another size never go in.
        let mut small = PixelImage::new(RgbaImage::new(3, 3));
        assert!(!restore(&mut small, whole_layers(&pi), 1));
        assert_eq!(small.layers.len(), 1);
    }

    #[test]
    fn kept_layers_go_with_their_picture_and_the_oldest_goes_first() {
        let pi = layered();
        let mut kept = KeptLayers::default();
        let id = |n: u64| ShapeId(n);
        kept.keep(id(1), b"a", &pi);
        assert!(kept.get(id(1), b"a").is_some());
        assert!(kept.get(id(1), b"b").is_none(), "the picture changed");
        assert!(kept.get(id(2), b"a").is_none());
        kept.keep(id(1), b"b", &pi);
        assert_eq!(kept.len(), 1, "one entry a picture");
        for n in 2..=MOST as u64 + 1 {
            kept.keep(id(n), b"a", &pi);
        }
        assert_eq!(kept.len(), MOST);
        assert!(kept.get(id(1), b"b").is_none(), "the oldest went");
    }
}

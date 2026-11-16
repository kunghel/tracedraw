//! Image > Resample: a new pixel size for the selected bitmaps, set as
//! pixels, a percentage or a resolution, with the aspect ratio kept or
//! not, and the way pixels are worked out (smooth, sharp, hard pixels for
//! pixel art, or smooth with the detail sharpened again after enlarging).
//! Its "Save as file" part (#1901) writes the resampled picture as JPG,
//! PNG or PDF with a quality that sets the file size, and fits it under a
//! target size typed in KB (for forms that limit uploads).

use crate::app::App;
use image::RgbaImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    /// Lanczos: smooth, no stair steps (photos, most pictures).
    #[default]
    Smooth,
    /// Catmull-Rom: a little crisper.
    Sharp,
    /// Nearest pixel: hard pixels kept (pixel art, icons, screenshots).
    Pixels,
    /// Lanczos, then an unsharp mask scaled with the enlargement, to win
    /// back the edges an enlargement softens.
    Detail,
}

impl Method {
    pub const ALL: [Method; 4] = [
        Method::Smooth,
        Method::Sharp,
        Method::Pixels,
        Method::Detail,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Method::Smooth => "resample.smooth",
            Method::Sharp => "resample.sharp",
            Method::Pixels => "resample.pixels",
            Method::Detail => "resample.detail",
        }
    }
}

/// The picture at `w` x `h` pixels.
pub fn resample(img: &RgbaImage, w: u32, h: u32, method: Method) -> RgbaImage {
    use image::imageops::{resize, FilterType};
    let (w, h) = (w.clamp(1, 20_000), h.clamp(1, 20_000));
    match method {
        Method::Smooth => resize(img, w, h, FilterType::Lanczos3),
        Method::Sharp => resize(img, w, h, FilterType::CatmullRom),
        Method::Pixels => resize(img, w, h, FilterType::Nearest),
        Method::Detail => {
            let out = resize(img, w, h, FilterType::Lanczos3);
            let grow =
                (w as f32 / img.width().max(1) as f32).max(h as f32 / img.height().max(1) as f32);
            if grow <= 1.05 {
                return out;
            }
            // The blur an enlargement adds grows with it.
            let sigma = (0.6 * grow).clamp(0.6, 4.0);
            let amount = (0.35 + 0.25 * grow.ln()).clamp(0.35, 1.2);
            crate::fx::noise::unsharp(&out, sigma, amount, 2.0, true)
        }
    }
}

/// The dialog's fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ResampleState {
    /// The first selected bitmap's pixel size and size on the page (mm).
    pub orig: (u32, u32),
    pub size_mm: (f64, f64),
    pub w: u32,
    pub h: u32,
    pub keep_aspect: bool,
    pub method: Method,
    /// Save as file (#1901): the format, the JPEG quality (1-100, JPG
    /// and PDF), the target size in KB (0: none) and the size the last
    /// estimate gave, with what it was estimated for.
    pub format: FileFormat,
    pub quality: u8,
    pub target_kb: f64,
    pub estimate: Option<(EstimateKey, Option<u64>)>,
}

/// What a file size estimate depends on.
pub type EstimateKey = (u32, u32, Method, FileFormat, u8);

/// The formats Resample saves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileFormat {
    #[default]
    Jpeg,
    Png,
    /// One page the picture's size on the page, the picture as JPEG.
    Pdf,
}

impl FileFormat {
    pub const ALL: [FileFormat; 3] = [FileFormat::Jpeg, FileFormat::Png, FileFormat::Pdf];
    pub fn name(self) -> &'static str {
        match self {
            FileFormat::Jpeg => "JPG",
            FileFormat::Png => "PNG",
            FileFormat::Pdf => "PDF",
        }
    }
    pub fn exts(self) -> &'static [&'static str] {
        match self {
            FileFormat::Jpeg => &["jpg", "jpeg"],
            FileFormat::Png => &["png"],
            FileFormat::Pdf => &["pdf"],
        }
    }
    /// Whether the quality changes the file (PNG is lossless).
    pub fn lossy(self) -> bool {
        self != FileFormat::Png
    }
}

/// The picture flattened on white, as JPEG at `quality` (1-100).
pub fn encode_jpeg(img: &RgbaImage, quality: u8) -> Option<Vec<u8>> {
    let rgb = image::RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let [r, g, b, a] = img.get_pixel(x, y).0;
        let over = |c: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32) + 127) / 255) as u8;
        image::Rgb([over(r), over(g), over(b)])
    });
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
        .encode_image(&rgb)
        .ok()?;
    Some(out)
}

/// A one-page PDF showing `jpeg` (`w` x `h` pixels) at `size_mm`.
pub fn pdf_with_jpeg(jpeg: &[u8], w: u32, h: u32, size_mm: (f64, f64)) -> Vec<u8> {
    let pt = |mm: f64| mm.max(1.0) / 25.4 * 72.0;
    let (pw, ph) = (pt(size_mm.0), pt(size_mm.1));
    let content = format!("q {pw:.3} 0 0 {ph:.3} 0 0 cm /Im0 Do Q\n");
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    let mut obj = |out: &mut Vec<u8>, head: String, stream: Option<&[u8]>| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{head}", offsets.len()).as_bytes());
        if let Some(data) = stream {
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(data);
            out.extend_from_slice(b"\nendstream");
        }
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, "<< /Type /Catalog /Pages 2 0 R >>".into(), None);
    obj(
        &mut out,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        None,
    );
    obj(
        &mut out,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.3} {ph:.3}] \
             /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
        ),
        None,
    );
    obj(
        &mut out,
        format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>",
            jpeg.len()
        ),
        Some(jpeg),
    );
    obj(
        &mut out,
        format!("<< /Length {} >>", content.len()),
        Some(content.as_bytes()),
    );
    let xref = out.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
    for o in &offsets {
        tail.push_str(&format!("{o:010} 00000 n \n"));
    }
    tail.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        offsets.len() + 1
    ));
    out.extend_from_slice(tail.as_bytes());
    out
}

/// The picture as a file in `format`.
pub fn encode_file(
    img: &RgbaImage,
    format: FileFormat,
    quality: u8,
    size_mm: (f64, f64),
) -> Option<Vec<u8>> {
    match format {
        FileFormat::Jpeg => encode_jpeg(img, quality),
        FileFormat::Png => crate::bitmap_fx::encode(img),
        FileFormat::Pdf => {
            let jpeg = encode_jpeg(img, quality)?;
            Some(pdf_with_jpeg(&jpeg, img.width(), img.height(), size_mm))
        }
    }
}

/// The largest settings whose file fits in `target` bytes, from the
/// picture `img` at `w` x `h`: the quality goes down first (JPG, PDF,
/// not under 10), then the pixels, by the square root of how far the
/// file is over. `None` when even a tiny picture does not fit.
pub fn fit_target(
    img: &RgbaImage,
    (mut w, mut h): (u32, u32),
    method: Method,
    format: FileFormat,
    target: u64,
    size_mm: (f64, f64),
) -> Option<(u32, u32, u8, u64)> {
    for _ in 0..12 {
        let pic = resample(img, w, h, method);
        let size = |q: u8| encode_file(&pic, format, q, size_mm).map(|f| f.len() as u64);
        let (mut quality, mut bytes) = (95u8, size(95)?);
        if bytes > target && format.lossy() {
            // The highest quality that fits, found by halving.
            let (mut lo, mut hi) = (10u8, 95u8);
            let at_lo = size(lo)?;
            if at_lo <= target {
                (quality, bytes) = (lo, at_lo);
                while hi - lo > 1 {
                    let mid = (lo + hi) / 2;
                    let b = size(mid)?;
                    if b <= target {
                        (lo, quality, bytes) = (mid, mid, b);
                    } else {
                        hi = mid;
                    }
                }
            } else {
                (quality, bytes) = (lo, at_lo);
            }
        }
        if bytes <= target {
            return Some((w, h, quality, bytes));
        }
        if w <= 8 || h <= 8 {
            return None;
        }
        let k = ((target as f64 / bytes as f64).sqrt() * 0.95).clamp(0.1, 0.95);
        w = ((w as f64 * k).round() as u32).max(8);
        h = ((h as f64 * k).round() as u32).max(8);
    }
    None
}

impl ResampleState {
    pub fn new(orig: (u32, u32), size_mm: (f64, f64)) -> Self {
        ResampleState {
            orig: (orig.0.max(1), orig.1.max(1)),
            size_mm,
            w: orig.0.max(1),
            h: orig.1.max(1),
            keep_aspect: true,
            method: Method::Smooth,
            format: FileFormat::Jpeg,
            quality: 85,
            target_kb: 0.0,
            estimate: None,
        }
    }

    /// Set the width; with the aspect kept, the height follows.
    pub fn set_w(&mut self, w: u32) {
        self.w = w.clamp(1, 20_000);
        if self.keep_aspect {
            self.h = ((self.w as f64 * self.orig.1 as f64 / self.orig.0 as f64).round() as u32)
                .clamp(1, 20_000);
        }
    }

    pub fn set_h(&mut self, h: u32) {
        self.h = h.clamp(1, 20_000);
        if self.keep_aspect {
            self.w = ((self.h as f64 * self.orig.0 as f64 / self.orig.1 as f64).round() as u32)
                .clamp(1, 20_000);
        }
    }

    /// Percent of the original width and height.
    pub fn percent(&self) -> (f64, f64) {
        (
            self.w as f64 / self.orig.0 as f64 * 100.0,
            self.h as f64 / self.orig.1 as f64 * 100.0,
        )
    }

    pub fn set_percent(&mut self, px: f64, py: f64) {
        self.w = ((self.orig.0 as f64 * px / 100.0).round() as u32).clamp(1, 20_000);
        self.h = ((self.orig.1 as f64 * if self.keep_aspect { px } else { py } / 100.0).round()
            as u32)
            .clamp(1, 20_000);
    }

    /// The resolution the new width gives at the size on the page.
    pub fn dpi(&self) -> f64 {
        self.w as f64 / (self.size_mm.0 / 25.4).max(1e-6)
    }

    pub fn set_dpi(&mut self, dpi: f64) {
        let dpi = dpi.clamp(1.0, 4800.0);
        self.w = ((self.size_mm.0 / 25.4 * dpi).round() as u32).clamp(1, 20_000);
        self.h = ((self.size_mm.1 / 25.4 * dpi).round() as u32).clamp(1, 20_000);
        if self.keep_aspect {
            self.set_w(self.w);
        }
    }

    /// Memory of the new picture, bytes (4 per pixel).
    pub fn bytes(&self) -> u64 {
        self.w as u64 * self.h as u64 * 4
    }

    /// What the file size depends on now.
    pub fn estimate_key(&self) -> EstimateKey {
        (self.w, self.h, self.method, self.format, self.quality)
    }
}

impl App {
    /// Image > Resample: the dialog, set from the first selected bitmap.
    pub fn open_resample(&mut self) {
        let Some((_, rect, w, h, _)) = self.bitmap_shapes().into_iter().next() else {
            return;
        };
        self.dialog = crate::ui::dialogs::Dialog::Resample(ResampleState::new(
            (w, h),
            (rect.width(), rect.height()),
        ));
    }

    /// Resample every selected bitmap by the dialog's scale (the first
    /// one gets exactly the size typed), keeping its place and size on
    /// the page. One undo step per bitmap.
    pub fn resample_bitmaps(&mut self, st: &ResampleState) {
        let sx = st.w as f64 / st.orig.0 as f64;
        let sy = st.h as f64 / st.orig.1 as f64;
        for (id, rect, w, h, png) in self.bitmap_shapes() {
            let Some(img) = crate::bitmap_fx::decode(&png) else {
                continue;
            };
            let nw = ((w.max(1) as f64 * sx).round() as u32).max(1);
            let nh = ((h.max(1) as f64 * sy).round() as u32).max(1);
            if (nw, nh) == (img.width(), img.height()) && st.method != Method::Detail {
                continue;
            }
            let out = resample(&img, nw, nh, st.method);
            self.replace_bitmap(id, rect, &out);
        }
    }
}

impl App {
    /// The first selected bitmap at the dialog's size, for its file.
    pub fn resampled_first(&self, st: &ResampleState) -> Option<RgbaImage> {
        let (_, _, _, _, png) = self.bitmap_shapes().into_iter().next()?;
        let img = crate::bitmap_fx::decode(&png)?;
        Some(
            if (st.w, st.h) == (img.width(), img.height()) && st.method != Method::Detail {
                img
            } else {
                resample(&img, st.w, st.h, st.method)
            },
        )
    }

    /// The size the file would have now (`None`: no bitmap).
    pub fn resample_file_size(&self, st: &ResampleState) -> Option<u64> {
        let img = self.resampled_first(st)?;
        encode_file(&img, st.format, st.quality, st.size_mm).map(|f| f.len() as u64)
    }

    /// Fit the file under the target size: the dialog takes the quality
    /// and pixels that do. False when nothing fits.
    pub fn resample_fit_target(&self, st: &mut ResampleState) -> bool {
        let Some((_, _, _, _, png)) = self.bitmap_shapes().into_iter().next() else {
            return false;
        };
        let Some(img) = crate::bitmap_fx::decode(&png) else {
            return false;
        };
        let target = (st.target_kb.max(0.0) * 1024.0) as u64;
        match fit_target(&img, (st.w, st.h), st.method, st.format, target, st.size_mm) {
            Some((w, h, quality, bytes)) => {
                let keep = st.keep_aspect;
                st.keep_aspect = false;
                st.set_w(w);
                st.set_h(h);
                st.keep_aspect = keep;
                st.quality = quality;
                st.estimate = Some((st.estimate_key(), Some(bytes)));
                true
            }
            None => false,
        }
    }

    /// Save the resampled picture as a file where the user says.
    pub fn resample_save_as(&mut self, st: &ResampleState) {
        let Some(img) = self.resampled_first(st) else {
            return;
        };
        let Some(bytes) = encode_file(&img, st.format, st.quality, st.size_mm) else {
            self.status = crate::i18n::tr("resample.save_failed");
            return;
        };
        crate::files::Dialog::new()
            .add_filter(st.format.name(), st.format.exts())
            .set_file_name(format!("picture.{}", st.format.exts()[0]))
            .save_bytes(self, bytes);
    }
}

/// A file size as people read it: KB under a megabyte, else MB.
pub fn size_text(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{:.0} KB", (bytes as f64 / 1024.0).max(1.0))
    } else {
        format!("{:.2} MB", bytes as f64 / 1_048_576.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn sizes_follow_pixels_percent_and_resolution() {
        let mut st = ResampleState::new((400, 200), (101.6, 50.8));
        assert!((st.dpi() - 100.0).abs() < 1e-9);
        st.set_w(800);
        assert_eq!((st.w, st.h), (800, 400));
        st.set_percent(50.0, 999.0);
        assert_eq!((st.w, st.h), (200, 100));
        st.set_dpi(300.0);
        assert_eq!((st.w, st.h), (1200, 600));
        st.keep_aspect = false;
        st.set_h(50);
        assert_eq!((st.w, st.h), (1200, 50));
        st.set_percent(10.0, 20.0);
        assert_eq!((st.w, st.h), (40, 40));
        assert_eq!(st.bytes(), 40 * 40 * 4);
        st.set_w(0);
        assert_eq!(st.w, 1);
    }

    #[test]
    fn methods_keep_pixels_hard_or_smooth_and_detail_sharpens() {
        // A two-pixel checker enlarged four times.
        let img = RgbaImage::from_fn(2, 2, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let hard = resample(&img, 8, 8, Method::Pixels);
        assert!(hard.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255));
        let smooth = resample(&img, 8, 8, Method::Smooth);
        assert!(smooth.pixels().any(|p| p.0[0] > 10 && p.0[0] < 245));
        // A soft edge enlarged: Detail makes it steeper than Smooth.
        let ramp = RgbaImage::from_fn(16, 4, |x, _| {
            let v = if x < 8 { 40 } else { 215 };
            Rgba([v, v, v, 255])
        });
        let s = resample(&ramp, 64, 16, Method::Smooth);
        let d = resample(&ramp, 64, 16, Method::Detail);
        let spread = |im: &RgbaImage| {
            let row: Vec<i32> = (0..im.width())
                .map(|x| im.get_pixel(x, 8).0[0] as i32)
                .collect();
            row.iter().max().copied().unwrap_or(0) - row.iter().min().copied().unwrap_or(0)
        };
        assert!(spread(&d) >= spread(&s), "{} {}", spread(&d), spread(&s));
        assert_eq!((d.width(), d.height()), (64, 16));
    }

    /// A photo-like picture: smooth colour with some noise, so JPEG
    /// quality changes its size.
    fn photo(w: u32, h: u32) -> RgbaImage {
        let mut seed = 7u32;
        RgbaImage::from_fn(w, h, |x, y| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let n = (seed >> 16) as u8 % 40;
            Rgba([
                (x * 255 / w) as u8 ^ n,
                (y * 255 / h) as u8,
                ((x + y) % 256) as u8 ^ n,
                255,
            ])
        })
    }

    #[test]
    fn quality_sets_the_file_size_and_pdf_wraps_the_jpeg() {
        let img = photo(200, 150);
        let low = encode_jpeg(&img, 20).unwrap();
        let high = encode_jpeg(&img, 95).unwrap();
        assert!(low.len() < high.len(), "{} {}", low.len(), high.len());
        assert_eq!(&low[..2], &[0xFF, 0xD8], "a JPEG");
        let pdf = encode_file(&img, FileFormat::Pdf, 20, (50.8, 38.1)).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(text.contains("/Filter /DCTDecode"));
        assert!(
            text.contains("/MediaBox [0 0 144.000 108.000]"),
            "two by 1.5 inches"
        );
        assert!(
            pdf.len() < low.len() + 1200,
            "the PDF is the JPEG and a little"
        );
        // The xref points at each object.
        let xref: usize = text
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(pdf[xref..].starts_with(b"xref"));
        let png = encode_file(&img, FileFormat::Png, 20, (1.0, 1.0)).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn a_target_size_lowers_quality_then_pixels() {
        let img = photo(400, 300);
        let full = encode_jpeg(&img, 95).unwrap().len() as u64;
        // Just under full quality: only the quality drops.
        let (w, h, q, bytes) = fit_target(
            &img,
            (400, 300),
            Method::Smooth,
            FileFormat::Jpeg,
            full * 2 / 3,
            (10.0, 10.0),
        )
        .unwrap();
        assert_eq!((w, h), (400, 300));
        assert!(q < 95 && bytes <= full * 2 / 3, "{q} {bytes}");
        // Far under: the pixels go down too.
        let tiny = encode_jpeg(&img, 10).unwrap().len() as u64 / 4;
        let (w, h, q, bytes) = fit_target(
            &img,
            (400, 300),
            Method::Smooth,
            FileFormat::Jpeg,
            tiny,
            (10.0, 10.0),
        )
        .unwrap();
        assert!(w < 400 && h < 300 && bytes <= tiny, "{w} {h} {q} {bytes}");
        assert!(
            ((w as f64 / h as f64) - 4.0 / 3.0).abs() < 0.05,
            "aspect kept"
        );
        // PNG has no quality: pixels only.
        let png = crate::bitmap_fx::encode(&img).unwrap().len() as u64;
        let (w, _, _, bytes) = fit_target(
            &img,
            (400, 300),
            Method::Smooth,
            FileFormat::Png,
            png / 3,
            (10.0, 10.0),
        )
        .unwrap();
        assert!(w < 400 && bytes <= png / 3);
        assert_eq!(
            fit_target(
                &img,
                (400, 300),
                Method::Smooth,
                FileFormat::Jpeg,
                10,
                (1.0, 1.0)
            ),
            None
        );
        assert_eq!(size_text(2048), "2 KB");
        assert_eq!(size_text(3 * 1_048_576), "3.00 MB");
    }
}

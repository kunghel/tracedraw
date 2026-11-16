//! Opening and importing desktop publishing `.pmd` files (#2822): what
//! `tracedraw_io::pmd_import` recovers from them, the text in one frame
//! on the first page and each picture found inside on a page of its
//! own, with a note of what is left behind (the layout of these files is
//! not published: frame places, type, shapes and pages are not read).
//! See docs/behavior/pmd-import.md.

use crate::app::App;
use crate::i18n::trf;
use tracedraw_core::{
    document::{paper, Layer, Page, Shape, ShapeKind},
    geometry::{Point, Rect},
    Color, Document, Fill,
};

/// The extensions read this way.
pub const EXTENSIONS: [&str; 3] = ["pmd", "pm6", "p65"];

/// The page margin, mm.
const MARGIN: f64 = 20.0;
/// The resolution a picture is placed at when it fits the page, dpi.
const DPI: f64 = 96.0;

pub fn is_pmd(ext: &str) -> bool {
    EXTENSIONS.contains(&ext)
}

/// A drawing made of what a `.pmd` file holds.
pub struct Opened {
    pub doc: Document,
    /// Paragraphs of text and pictures placed.
    pub paragraphs: usize,
    pub pictures: usize,
}

/// The drawing of the `.pmd` file `bytes`, titled `title`; an error
/// when nothing in it could be read.
pub fn document(title: &str, bytes: &[u8]) -> Result<Opened, String> {
    let found = tracedraw_io::pmd_import::read(bytes)?;
    let page = paper::A4;
    let mut doc = App::localized_document(title, page);
    let mut ids = doc.ids().clone();
    let mut pictures = 0;
    if !found.paragraphs.is_empty() {
        let text = found.paragraphs.join("\n");
        let mut shape = tracedraw_io::text_import::parse_txt(text.as_bytes())
            .to_shape(tracedraw_io::text_import::Placement::a4(), &mut ids);
        shape.id = ids.shape();
        shape.fill = Fill::Solid(Color::BLACK);
        shape.stroke = None;
        doc.pages[0].layers[0].shapes.push(shape);
    }
    for (n, pic) in found.pictures.iter().enumerate() {
        let Ok(read) = crate::bitmap_file::read(&pic.bytes) else {
            continue;
        };
        let (w, h) = read.rgba.dimensions();
        // Specks the signature scan took for pictures are left out.
        if w < 8 || h < 8 {
            continue;
        }
        let Some(png) = crate::bitmap_fx::encode(&read.rgba) else {
            continue;
        };
        // The first page when it holds no text, else a page of its own.
        let index = if doc.pages[0].layers[0].shapes.is_empty() {
            0
        } else {
            let p = Page {
                id: ids.page(),
                name: trf("doc.page_n", &[("n", &(doc.pages.len() + 1).to_string())]),
                size: page,
                layers: vec![Layer::new(ids.layer(), doc.pages[0].layers[0].name.clone())],
                guides: Vec::new(),
                background: None,
            };
            doc.pages.push(p);
            doc.pages.len() - 1
        };
        let rect = fit(w, h, page.width, page.height);
        let mut shape = Shape::new(
            ids.shape(),
            ShapeKind::Bitmap {
                rect,
                width_px: w,
                height_px: h,
                png: png.into(),
                fx: None,
            },
        );
        shape.stroke = None;
        shape.name = Some(trf("pmd.picture_n", &[("n", &(n + 1).to_string())]));
        doc.pages[index].layers[0].shapes.push(shape);
        pictures += 1;
    }
    if found.paragraphs.is_empty() && pictures == 0 {
        return Err(crate::i18n::tr("pmd.nothing_read"));
    }
    doc.set_ids(ids);
    Ok(Opened {
        doc,
        paragraphs: found.paragraphs.len(),
        pictures,
    })
}

/// A `w` x `h` pixel picture's place on a page `pw` x `ph` mm: at 96 dpi
/// when it fits within the margins, else shrunk to fit, centred.
fn fit(w: u32, h: u32, pw: f64, ph: f64) -> Rect {
    let (mut mw, mut mh) = (w as f64 * 25.4 / DPI, h as f64 * 25.4 / DPI);
    let (aw, ah) = (pw - 2.0 * MARGIN, ph - 2.0 * MARGIN);
    let k = (aw / mw).min(ah / mh).min(1.0);
    mw *= k;
    mh *= k;
    let c = Point::new(pw / 2.0, ph / 2.0);
    Rect::new(
        c.x - mw / 2.0,
        c.y - mh / 2.0,
        c.x + mw / 2.0,
        c.y + mh / 2.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file holding a story and a picture opens as a text frame on
    /// the first page and the picture on the second; one with nothing
    /// readable does not open.
    #[test]
    fn text_and_pictures_come_in_and_nothing_is_an_error() {
        let mut data = vec![0u8; 64];
        data.extend_from_slice(b"Spring newsletter\rOur garden opens on Sunday.");
        data.extend_from_slice(&[0u8; 32]);
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(40, 20, image::Rgba([0, 128, 0, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        data.extend_from_slice(&png);
        data.extend_from_slice(&[0u8; 16]);
        let o = document("News", &data).expect("opens");
        assert_eq!((o.paragraphs, o.pictures), (2, 1));
        assert_eq!(o.doc.pages.len(), 2);
        let ShapeKind::Text { spans, .. } = &o.doc.pages[0].layers[0].shapes[0].kind else {
            panic!("a text frame");
        };
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "Spring newsletter\nOur garden opens on Sunday.");
        let pic = &o.doc.pages[1].layers[0].shapes[0];
        assert!(matches!(
            pic.kind,
            ShapeKind::Bitmap {
                width_px: 40,
                height_px: 20,
                ..
            }
        ));
        // 40 px at 96 dpi, centred on the page.
        let b = pic.bounds();
        assert!((b.width() - 40.0 * 25.4 / 96.0).abs() < 1e-6);
        assert!((b.center().x - paper::A4.width / 2.0).abs() < 1e-6);
        // The ids stay unique.
        let mut seen = std::collections::HashSet::new();
        for p in &o.doc.pages {
            assert!(seen.insert(format!("p{:?}", p.id)));
            for s in &p.layers[0].shapes {
                assert!(seen.insert(format!("s{:?}", s.id)));
            }
        }
        assert!(document("Empty", &[0u8; 300]).is_err());
        // A large picture is shrunk to the page.
        assert!(fit(4000, 1000, 210.0, 297.0).width() <= 170.0 + 1e-9);
    }
}

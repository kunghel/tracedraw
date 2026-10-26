//! Hindi through a PDF and back (#599): text written by Publish to PDF
//! with its font comes in as the typed words and shapes to the same
//! glyphs as the words themselves. A test binary of its own: the
//! Devanagari test font joins the process's font system here only.

use tracedraw_core::{
    document::text_outline::TextRequest, Document, ParagraphStyle, Point, Shape, ShapeKind, Size,
    TextAlign, TextSpan,
};

const NOTO_DEVA: &[u8] = include_bytes!("../../text/testdata/NotoSansDevanagari-Regular.ttf");
const CORPUS: &str = include_str!("../../text/testdata/devanagari-corpus.txt");
const DEVA: &str = "Noto Sans Devanagari";

fn corpus() -> Vec<&'static str> {
    CORPUS
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

fn fonts() -> &'static tracedraw_text::FontSystem {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        tracedraw_text::add_font_data(NOTO_DEVA.to_vec());
        tracedraw_text::install();
    });
    tracedraw_text::fonts()
}

/// The glyph ids `text` lays out to in the Devanagari font.
fn gids(text: &str) -> Vec<u16> {
    let spans = [TextSpan::new(text, DEVA, 24.0)];
    let para = ParagraphStyle::default();
    fonts()
        .glyph_layout(&TextRequest {
            spans: &spans,
            frame: None,
            align: TextAlign::Left,
            para: &para,
            on_path: None,
        })
        .glyphs
        .iter()
        .map(|g| g.glyph_id)
        .collect()
}

/// The texts of the imported file's text objects, in order.
fn imported_texts(bytes: &[u8]) -> Vec<String> {
    let mut ids = tracedraw_core::id::IdSource::default();
    let imp = tracedraw_io::pdf_import::parse(bytes, &mut ids).expect("reads");
    imp.pages
        .iter()
        .flat_map(|p| p.shapes.iter())
        .filter_map(|s| match &s.kind {
            ShapeKind::Text { spans, .. } => {
                Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn hindi_exported_with_its_font_imports_as_the_typed_words() {
    let _ = fonts();
    let words = corpus();
    let mut doc = Document::new("t", Size::new(200.0, 20.0 + 12.0 * words.len() as f64));
    let layer = doc.pages[0].layers[0].id;
    for (i, w) in words.iter().enumerate() {
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Text {
                spans: vec![TextSpan::new(*w, DEVA, 24.0)],
                origin: Point::new(10.0, 10.0 + 12.0 * i as f64),
                frame: None,
                align: TextAlign::Left,
                para: ParagraphStyle::default(),
                on_path: None,
            },
        );
        s.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK);
        if let Ok(l) = doc.layer_mut(layer) {
            l.shapes.push(s);
        }
    }
    let (bytes, report) =
        tracedraw_io::pdf::document_to_pdf_report(&doc, &tracedraw_io::pdf::PdfOptions::default());
    assert!(report.restricted.is_empty());
    let texts = imported_texts(&bytes);
    let bad: Vec<String> = words
        .iter()
        .filter(|w| !texts.iter().any(|t| t == *w))
        .map(|w| w.to_string())
        .collect();
    assert!(bad.is_empty(), "missing {bad:?} in {texts:?}");
    for t in &texts {
        assert_eq!(gids(t), gids(t.trim()), "{t}");
    }
}

/// Latin text, one glyph for each character, goes without marked
/// content: only clusters that need it get an ActualText.
#[test]
fn latin_text_exported_has_no_actual_text() {
    let _ = fonts();
    let mut doc = Document::new("t", Size::new(100.0, 40.0));
    let layer = doc.pages[0].layers[0].id;
    let id = doc.ids_mut().shape();
    let mut s = Shape::new(
        id,
        ShapeKind::Text {
            spans: vec![TextSpan::new("Hello World", "DejaVu Sans", 24.0)],
            origin: Point::new(10.0, 20.0),
            frame: None,
            align: TextAlign::Left,
            para: ParagraphStyle::default(),
            on_path: None,
        },
    );
    s.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK);
    if let Ok(l) = doc.layer_mut(layer) {
        l.shapes.push(s);
    }
    let bytes = tracedraw_io::pdf::document_to_pdf(&doc);
    let pdf = lopdf::Document::load_mem(&bytes).expect("parses");
    let mut ops = Vec::new();
    for (_, page) in pdf.get_pages() {
        let content = pdf.get_page_content(page).expect("content");
        let content = lopdf::content::Content::decode(&content).expect("decodes");
        ops.extend(content.operations.into_iter().map(|o| o.operator));
    }
    assert!(ops.iter().any(|o| o == "TJ"), "{ops:?}");
    assert!(!ops.iter().any(|o| o == "BDC"), "{ops:?}");
    assert_eq!(imported_texts(&bytes), ["Hello World"]);
}

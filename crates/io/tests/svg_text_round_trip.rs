//! Text written to SVG as text (`SvgText::Text`) and opened again: the
//! runs of a text in their own looks come back as one text, its lines as
//! lines, and Hebrew in the order it is typed. Each run came back as a
//! text of its own, the lines of a paragraph on one line, Hebrew
//! backwards, and the line break and indent between the tags as texts of
//! spaces. A test binary of its own: it lays text out with the real text
//! engine.

use tracedraw_core::{
    id::IdSource, Color, Document, Fill, ParagraphStyle, Point, Shape, ShapeKind, Size, TextAlign,
    TextSpan,
};
use tracedraw_io::svg::{page_to_svg_with, SvgOptions, SvgText};

const FONT: &str = "DejaVu Sans";

fn doc_with(spans: Vec<TextSpan>) -> Document {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(tracedraw_text::install);
    let mut doc = Document::new("t", Size::new(300.0, 200.0));
    let layer = doc.pages[0].layers[0].id;
    let id = doc.ids_mut().shape();
    let mut s = Shape::new(
        id,
        ShapeKind::Text {
            spans,
            origin: Point::new(10.0, 40.0),
            frame: None,
            align: TextAlign::Left,
            para: ParagraphStyle::default(),
            on_path: None,
        },
    );
    s.fill = Fill::Solid(Color::BLACK);
    doc.layer_mut(layer).unwrap().shapes.push(s);
    doc
}

fn texts_in<'a>(shapes: &'a [Shape], out: &mut Vec<&'a [TextSpan]>) {
    for s in shapes {
        match &s.kind {
            ShapeKind::Text { spans, .. } => out.push(spans),
            ShapeKind::Group { children } => texts_in(children, out),
            _ => {}
        }
    }
}

#[test]
fn text_written_to_svg_as_text_opens_as_it_was_typed() {
    let mut bold = TextSpan::new("ld", FONT, 12.0);
    bold.bold = true;
    let cases = [
        vec![
            TextSpan::new("Wor", FONT, 12.0),
            bold,
            TextSpan::new(" next", FONT, 12.0),
        ],
        vec![TextSpan::new(
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD} abc",
            FONT,
            12.0,
        )],
        vec![TextSpan::new("one\ntwo\nthree", FONT, 12.0)],
        vec![TextSpan::new("cafe\u{301} \u{1F600}", FONT, 12.0)],
    ];
    for spans in cases {
        let doc = doc_with(spans.clone());
        let svg = page_to_svg_with(
            &doc,
            0,
            &SvgOptions {
                text: SvgText::Text,
            },
        );
        assert!(!svg.contains(">\n    <tspan"), "{svg}");
        let back = tracedraw_io::svg_import::parse(&svg, &mut IdSource::default()).expect("svg");
        let mut found = Vec::new();
        texts_in(&back.shapes, &mut found);
        let [got] = found.as_slice() else {
            panic!("one text: {:#?}", back.shapes);
        };
        let texts = |s: &[TextSpan]| -> Vec<(String, bool)> {
            s.iter().map(|s| (s.text.clone(), s.bold)).collect()
        };
        assert_eq!(texts(got), texts(&spans), "{svg}");
    }
}

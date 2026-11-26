use super::*;
use tracedraw_core::document::{Ink, Symbol};
use tracedraw_core::{
    Color, Fill, Layer, MasterScope, Page, PageRanges, Rect, Shape, ShapeKind, Size,
};

fn book(name: &str, cmyk: [f32; 4]) -> Document {
    let mut doc = Document::new(name, Size::new(100.0, 200.0));
    let page = Page {
        id: doc.ids_mut().page(),
        name: "2".into(),
        size: Size::new(100.0, 200.0),
        layers: vec![Layer::new(doc.ids_mut().layer(), "Page")],
        guides: Vec::new(),
        background: None,
    };
    doc.pages.push(page);
    let ink = Ink {
        id: 1,
        name: name.into(),
        cmyk,
    };
    let mut logo = Shape::new(
        doc.ids_mut().shape(),
        ShapeKind::Rect {
            rect: Rect::new(10.0, 10.0, 20.0, 20.0),
            radius: 0.0,
            corners: None,
        },
    );
    logo.fill = Fill::Solid(ink.color(1.0));
    logo.stroke = None;
    logo.style = Some(1);
    doc.inks.push(ink);
    doc.symbols.push(Symbol {
        name: name.into(),
        shape: logo,
    });
    let mut range = Layer::master(doc.ids_mut().layer(), name, MasterScope::All);
    range.page_range = Some(PageRanges::parse("1").expect("range"));
    range.shapes.push(Shape::new(
        doc.ids_mut().shape(),
        ShapeKind::SymbolInstance { index: 0 },
    ));
    doc.master.push(range);
    doc
}

fn app_with(doc: Document, page: usize) -> App {
    let mut app = App::headless();
    app.page = doc.pages[page].id;
    app.engine = tracedraw_core::Engine::new(doc);
    app
}

#[test]
fn range_layers_keep_source_membership_when_publishing_selected_pages() {
    let doc = book("Header", [1.0, 0.0, 0.0, 0.0]);
    let marker = doc.master[0].shapes[0].id;
    let mut app = app_with(doc, 1);
    let mut state = PublishState::new(&app);
    state.pages = Pages::Current;
    let current = document_for(&app, &state);
    assert!(current.master.is_empty());
    assert!(current.pages[0].layers.iter().all(|l| l.shapes.is_empty()));
    state.pages = Pages::Range;
    state.range = "2, 1".into();
    let out = document_for(&app, &state);
    assert_eq!(out.pages.len(), 2);
    assert!(out.pages[0].layers.iter().all(|l| l.shapes.is_empty()));
    assert!(out.pages[1].layers.iter().any(|l| !l.shapes.is_empty()));
    app.page = app.doc().pages[0].id;
    app.selection = vec![marker];
    state.pages = Pages::Selection;
    state.fit_selection = false;
    let selected = document_for(&app, &state);
    assert_eq!(selected.pages.len(), 1);
    assert!(selected.pages[0]
        .layers
        .iter()
        .any(|l| l.shapes.iter().any(|s| s.id == marker)));
}

#[test]
fn range_layers_stay_with_their_documents_when_publishing_together() {
    let a = book("Ink A", [1.0, 0.0, 0.0, 0.0]);
    let b = book("Ink B", [0.0, 1.0, 0.0, 0.0]);
    assert_eq!(a.pages[0].id, b.pages[0].id);
    assert_eq!(a.master[0].shapes[0].id, b.master[0].shapes[0].id);
    let mut app = app_with(a, 0);
    app.open_document(b, None);
    let mut state = PublishState::new(&app);
    state.pages = Pages::Documents;
    let out = document_for(&app, &state);
    assert_eq!(out.pages.len(), 4);
    assert!(out.master.is_empty());
    let symbols: Vec<_> = out
        .pages
        .iter()
        .map(|p| {
            p.layers
                .iter()
                .flat_map(|l| &l.shapes)
                .find_map(|s| match s.kind {
                    ShapeKind::SymbolInstance { index } => Some(index),
                    _ => None,
                })
        })
        .collect();
    assert_eq!(symbols, [Some(0), None, Some(1), None]);
    assert_eq!(
        out.symbols
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["Ink B", "Ink A"]
    );
    assert!(out.symbols.iter().all(|s| s.shape.style.is_none()));
    assert_eq!(out.inks.len(), 2);
    assert_ne!(out.inks[0].id, out.inks[1].id);
    for symbol in &out.symbols {
        let Fill::Solid(Color::Spot { ink, .. }) = symbol.shape.fill else {
            panic!("spot logo")
        };
        assert_eq!(
            out.inks.iter().find(|i| i.id == ink).expect("ink").name,
            symbol.name
        );
    }
    let pages: std::collections::HashSet<_> = out.pages.iter().map(|p| p.id).collect();
    assert_eq!(pages.len(), out.pages.len());
    let ids: Vec<_> = out
        .all_layers()
        .flat_map(|l| l.shapes.iter())
        .map(|s| s.id)
        .chain(out.symbols.iter().map(|s| s.shape.id))
        .collect();
    assert_eq!(
        ids.len(),
        ids.iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );
    let bytes = tracedraw_io::pdf::document_to_pdf(&out);
    let imported =
        tracedraw_io::pdf_import::parse(&bytes, &mut Default::default()).expect("PDF pages");
    assert_eq!(
        imported
            .pages
            .iter()
            .map(|p| !p.shapes.is_empty())
            .collect::<Vec<_>>(),
        [true, false, true, false]
    );
}

#[test]
fn duplicating_a_page_to_a_new_document_keeps_its_range_contents() {
    let source = book("Header", [1.0, 0.0, 0.0, 0.0]);
    let mut app = app_with(source.clone(), 1);
    app.duplicate_page_to_new_document();
    assert_eq!(app.doc().pages.len(), 1);
    assert!(app.doc().master.is_empty());
    assert!(app.doc().pages[0]
        .layers
        .iter()
        .all(|l| l.shapes.is_empty()));
    let mut app = app_with(source, 0);
    app.duplicate_page_to_new_document();
    assert_eq!(app.doc().pages.len(), 1);
    assert!(app.doc().pages[0]
        .layers
        .iter()
        .any(|l| !l.shapes.is_empty()));
}

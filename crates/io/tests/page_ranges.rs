use tracedraw_core::{
    Color, Document, Fill, Layer, MasterScope, Page, PageRanges, Rect, Shape, ShapeKind, Size,
};

fn range_book() -> Document {
    let mut doc = Document::new("book", Size::new(20.0, 20.0));
    for _ in 0..2 {
        let page = Page {
            id: doc.ids_mut().page(),
            name: String::new(),
            size: Size::new(20.0, 20.0),
            layers: vec![Layer::new(doc.ids_mut().layer(), "Page")],
            guides: Vec::new(),
            background: None,
        };
        doc.pages.push(page);
    }
    let mut layer = Layer::master(doc.ids_mut().layer(), "Range", MasterScope::All);
    layer.page_range = Some(PageRanges::parse("2").expect("range"));
    let mut marker = Shape::new(
        doc.ids_mut().shape(),
        ShapeKind::Rect {
            rect: Rect::new(2.0, 2.0, 8.0, 8.0),
            radius: 0.0,
            corners: None,
        },
    );
    marker.fill = Fill::Solid(Color::rgb8(255, 0, 0));
    marker.stroke = None;
    layer.shapes.push(marker);
    doc.master.push(layer);
    doc
}

#[test]
fn range_layers_follow_the_page_in_svg_html_and_pdf_output() {
    let doc = range_book();
    for (index, shown) in [false, true, false].into_iter().enumerate() {
        let svg = tracedraw_io::svg::page_to_svg(&doc, index);
        assert_eq!(svg.contains("fill=\"#ff0000\""), shown, "SVG page {index}");
    }
    let html = tracedraw_io::html::document_to_html(&doc);
    assert_eq!(html.matches("fill=\"#ff0000\"").count(), 1);
    fn has_red(shape: &Shape) -> bool {
        shape.fill == Fill::Solid(Color::rgb8(255, 0, 0))
            || match &shape.kind {
                ShapeKind::Group { children } => children.iter().any(has_red),
                ShapeKind::ClipFrame { contents, .. } => contents.iter().any(has_red),
                _ => false,
            }
    }
    for (source, expected) in [
        (doc.clone(), vec![false, true, false]),
        (doc.with_master_layers_on_pages(&[2, 1]), vec![false, true]),
    ] {
        let bytes = tracedraw_io::pdf::document_to_pdf(&source);
        let mut ids = tracedraw_core::id::IdSource::default();
        let back = tracedraw_io::pdf_import::parse(&bytes, &mut ids).expect("PDF pages");
        let shown: Vec<_> = back
            .pages
            .iter()
            .map(|page| page.shapes.iter().any(has_red))
            .collect();
        assert_eq!(shown, expected);
    }
}

#[test]
fn range_output_keeps_missing_blend_endpoints_out_of_svg_and_pdf() {
    use tracedraw_core::live::{Effect, MergeMode};
    use tracedraw_core::{id::IdSource, Affine};

    let mut doc = range_book();
    doc.master[0].page_range = PageRanges::parse("2-3");
    let start = &doc.master[0].shapes[0];
    let mut ids = doc.ids().clone();
    let mut end = start.with_fresh_ids(&mut || ids.shape());
    end.transform_by(Affine::translate((10.0, 0.0)));
    end.fill = Fill::Solid(Color::rgb8(0, 0, 255));
    let mut endpoint = Layer::master(ids.layer(), "Endpoint", MasterScope::All);
    endpoint.page_range = PageRanges::parse("2");
    endpoint.shapes.push(end.clone());
    doc.master.push(endpoint);
    doc.set_ids(ids);
    let start = &mut doc.master[0].shapes[0];
    start.effects = vec![
        Effect::Blend {
            end: Box::new(end.clone()),
            steps: 2,
            accel_objects: 0.0,
            accel_colors: 0.0,
            rotation: 0.0,
            path: None,
            rotate_on_path: false,
            end_id: Some(end.id),
            options: Default::default(),
        },
        Effect::Transparency {
            mask: Fill::Solid(Color::Gray { v: 0.5 }),
            merge: MergeMode::Normal,
            target: 2,
        },
    ];
    let output = doc.with_master_layers_on_pages(&[2]);

    fn pixels(size: Size, shapes: Vec<Shape>) -> Vec<u8> {
        let mut page = Document::new("readback", size);
        page.pages[0].layers[0].shapes = shapes;
        tracedraw_render::render_page_image(&page, page.pages[0].id, 50.8)
            .expect("readback image")
            .data()
            .to_vec()
    }

    let svg_pixels = |source: &Document, index| {
        let svg = tracedraw_io::svg::page_to_svg(source, index);
        let imported =
            tracedraw_io::svg_import::parse(&svg, &mut IdSource::default()).expect("SVG readback");
        pixels(imported.size, imported.shapes)
    };
    let expected = svg_pixels(&doc, 2);
    assert!(expected.chunks_exact(4).any(|p| p[..3] != [255, 255, 255]));
    assert!(svg_pixels(&output, 0) == expected, "SVG range output");

    let pdf_pixels = |source: &Document, index: usize| {
        let pdf = tracedraw_io::pdf::document_to_pdf(source);
        let mut imported =
            tracedraw_io::pdf_import::parse(&pdf, &mut IdSource::default()).expect("PDF readback");
        let page = imported.pages.remove(index);
        pixels(page.size, page.shapes)
    };
    let expected = pdf_pixels(&doc, 2);
    assert!(expected.chunks_exact(4).any(|p| p[..3] != [255, 255, 255]));
    assert!(pdf_pixels(&output, 0) == expected, "PDF range output");
}

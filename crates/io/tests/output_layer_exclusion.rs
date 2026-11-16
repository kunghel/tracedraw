use tracedraw_core::{Color, Document, Fill, Layer, Rect, Shape, ShapeKind, Size};

fn drawing() -> Document {
    let mut doc = Document::new("Output exclusion", Size::new(10.0, 10.0));
    let mut rectangle = |rect, color| {
        let mut shape = Shape::new(
            doc.ids_mut().shape(),
            ShapeKind::Rect {
                rect,
                radius: 0.0,
                corners: None,
            },
        );
        shape.fill = Fill::Solid(color);
        shape.stroke = None;
        shape
    };
    let red = rectangle(Rect::new(1.0, 1.0, 4.0, 4.0), Color::rgb8(255, 0, 0));
    let blue = rectangle(Rect::new(6.0, 1.0, 9.0, 4.0), Color::rgb8(0, 0, 255));
    let green = rectangle(Rect::new(1.0, 6.0, 4.0, 9.0), Color::rgb8(0, 255, 0));
    doc.pages[0].layers[0].shapes.push(blue);
    doc.pages[0].layers[0].locked = true;
    let mut layer = Layer::new(doc.ids_mut().layer(), "Excluded");
    layer.printable = false;
    layer.shapes.push(red);
    doc.pages[0].layers.push(layer);
    let mut master = Layer::new(doc.ids_mut().layer(), "Excluded master");
    master.printable = false;
    master.shapes.push(green);
    doc.master.push(master);
    doc
}

#[test]
fn svg_omits_excluded_layers_but_keeps_locked_visible_content() {
    let doc = drawing();
    let before = doc.to_json().unwrap();
    let svg = tracedraw_io::svg::page_to_svg(&doc, 0);
    assert!(
        !svg.contains("#ff0000"),
        "excluded page layer leaked into SVG"
    );
    assert!(
        !svg.contains("#00ff00"),
        "excluded master layer leaked into SVG"
    );
    assert!(
        svg.contains("#0000ff"),
        "locking must not exclude visible content"
    );
    assert_eq!(doc.to_json().unwrap(), before);
}

#[test]
fn raster_output_omits_excluded_contents_while_canvas_and_native_keep_them() {
    let doc = drawing();
    let before = doc.to_json().unwrap();
    let page = doc.pages[0].id;
    let canvas = tracedraw_render::render_page_image(&doc, page, 25.4).unwrap();
    assert_eq!(canvas.pixel(2, 8).unwrap().red(), 255);
    assert_eq!(canvas.pixel(2, 8).unwrap().blue(), 0);
    assert_eq!(canvas.pixel(2, 2).unwrap().green(), 255);
    assert_eq!(canvas.pixel(2, 2).unwrap().red(), 0);

    let output = tracedraw_io::for_output(&doc);
    let exported = tracedraw_render::render_page_image(&output, page, 25.4).unwrap();
    for (x, y) in [(2, 8), (2, 2)] {
        let p = exported.pixel(x, y).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (255, 255, 255));
    }
    let blue = exported.pixel(7, 8).unwrap();
    assert_eq!((blue.red(), blue.green(), blue.blue()), (0, 0, 255));
    assert!(output
        .all_layers()
        .filter(|l| !l.printable)
        .all(|l| l.shapes.is_empty()));
    assert_eq!(doc.to_json().unwrap(), before);
    let native = Document::from_json(&before).unwrap();
    assert!(native
        .all_layers()
        .filter(|l| !l.printable)
        .all(|l| l.visible && l.shapes.len() == 1));
}

#[test]
fn ordinary_output_is_borrowed_and_prepared_output_is_not_copied_again() {
    let mut doc = drawing();
    let out = tracedraw_io::for_output(&doc);
    assert!(matches!(
        tracedraw_io::for_output(&out),
        std::borrow::Cow::Borrowed(_)
    ));
    for layer in doc.all_layers_mut() {
        layer.printable = true;
    }
    assert!(matches!(
        tracedraw_io::for_output(&doc),
        std::borrow::Cow::Borrowed(_)
    ));
}

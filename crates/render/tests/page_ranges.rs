use tracedraw_core::{
    Color, Document, Fill, Layer, MasterScope, Page, PageRanges, Rect, Shape, ShapeKind, Size,
};

#[test]
fn range_layers_draw_only_on_their_pages_in_images_and_thumbnails() {
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
    for (index, page) in doc.pages.iter().enumerate() {
        for wireframe in [false, true] {
            let options = tracedraw_render::RenderOptions {
                wireframe,
                ..Default::default()
            };
            let image = tracedraw_render::render_page_image_with(&doc, page.id, 25.4, &options)
                .expect("page image");
            let ink = image
                .data()
                .chunks_exact(4)
                .any(|px| px[..3] != [255, 255, 255]);
            assert_eq!(ink, index == 1, "page {index}, wireframe {wireframe}");
        }
        let thumbnail =
            tracedraw_render::render_whole_page_image(&doc, page.id, 20.0).expect("thumbnail");
        let ink = thumbnail
            .data()
            .chunks_exact(4)
            .any(|px| px[..3] != [255, 255, 255]);
        assert_eq!(ink, index == 1, "thumbnail page {index}");
    }
}

fn missing_endpoint_book(
    effects: Vec<tracedraw_core::live::Effect>,
    below: bool,
    shadow: Option<tracedraw_core::Shadow>,
) -> Document {
    use tracedraw_core::live::{BlendOptions, Effect};
    use tracedraw_core::{Point, Vec2};
    let mut doc = Document::new("effects", Size::new(60.0, 40.0));
    for _ in 0..2 {
        let page = Page {
            id: doc.ids_mut().page(),
            name: String::new(),
            size: Size::new(60.0, 40.0),
            layers: vec![Layer::new(doc.ids_mut().layer(), "Page")],
            guides: Vec::new(),
            background: None,
        };
        doc.pages.push(page);
    }
    for page in &mut doc.pages {
        page.background = Some(Fill::Solid(Color::rgb8(60, 110, 150)));
    }
    let mut start = Shape::new(
        doc.ids_mut().shape(),
        ShapeKind::Rect {
            rect: Rect::from_origin_size(Point::new(8.0, 12.0), Size::new(10.0, 10.0)),
            radius: 0.0,
            corners: None,
        },
    );
    start.fill = Fill::Solid(Color::rgb8(240, 30, 20));
    start.stroke = None;
    start.opacity = 0.7;
    start.shadow = shadow;
    let mut end = start.with_fresh_ids(&mut || doc.ids_mut().shape());
    end.transform_by(tracedraw_core::Affine::translate(Vec2::new(14.0, 0.0)));
    end.fill = Fill::Solid(Color::rgb8(30, 210, 40));
    start.effects.push(Effect::Blend {
        end: Box::new(end.clone()),
        steps: 2,
        accel_objects: 0.0,
        accel_colors: 0.0,
        rotation: 0.0,
        path: None,
        rotate_on_path: false,
        end_id: Some(end.id),
        options: BlendOptions {
            steps_below: below,
            ..Default::default()
        },
    });
    start.effects.extend(effects);
    let mut layer = Layer::master(doc.ids_mut().layer(), "Range", MasterScope::All);
    layer.page_range = Some(PageRanges::parse("2-3").expect("start range"));
    layer.in_front = true;
    layer.shapes.push(start);
    doc.master.push(layer);
    let mut layer = Layer::master(doc.ids_mut().layer(), "Endpoint", MasterScope::All);
    layer.page_range = Some(PageRanges::parse("2").expect("end range"));
    layer.in_front = true;
    layer.shapes.push(end);
    doc.master.push(layer);
    doc
}

#[test]
fn range_output_missing_blends_keep_renderer_effects_and_their_scope() {
    use tracedraw_core::live::{Effect, Lens, MergeMode};
    use tracedraw_core::{Shadow, Vec2};
    let cases = vec![
        vec![],
        vec![Effect::Transparency {
            mask: Fill::Solid(Color::Gray { v: 0.35 }),
            merge: MergeMode::Normal,
            target: 2,
        }],
        vec![Effect::GaussianBlur {
            radius: 1.0,
            pins: Vec::new(),
        }],
        vec![Effect::Lens(Lens::Invert)],
        vec![Effect::Glow {
            inner: false,
            color: Color::WHITE,
            opacity: 0.6,
            blur: 1.0,
            merge: MergeMode::Normal,
        }],
        vec![Effect::Glow {
            inner: true,
            color: Color::BLACK,
            opacity: 0.6,
            blur: 1.0,
            merge: MergeMode::Normal,
        }],
        vec![Effect::OuterShadow {
            offset: Vec2::new(3.0, -2.0),
            color: Color::BLACK,
            opacity: 0.6,
            blur: 1.0,
            merge: MergeMode::Normal,
        }],
        vec![Effect::InnerShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::BLACK,
            opacity: 0.6,
            blur: 1.0,
            spread: 0.0,
            merge: MergeMode::Normal,
        }],
        vec![Effect::FillOpacity { opacity: 0.25 }],
    ];
    let shadows = [
        None,
        Some(Shadow::default()),
        Some(Shadow {
            perspective: Some(tracedraw_core::shadow::Perspective::new(
                tracedraw_core::shadow::ShadowEdge::Bottom,
            )),
            ..Default::default()
        }),
    ];
    for (case, effects) in cases.iter().enumerate() {
        for below in [false, true] {
            for shadow in shadows {
                let source = missing_endpoint_book(effects.clone(), below, shadow);
                let output = source.with_master_layers_on_pages(&[2]);
                let loaded = Document::from_json(&output.to_json().expect("native output"))
                    .expect("native readback");
                let expected =
                    tracedraw_render::render_page_image(&source, source.pages[2].id, 50.8)
                        .expect("source image");
                for document in [&output, &loaded] {
                    let actual =
                        tracedraw_render::render_page_image(document, document.pages[0].id, 50.8)
                            .expect("output image");
                    let mismatches = actual
                        .data()
                        .iter()
                        .zip(expected.data())
                        .filter(|(a, b)| a != b)
                        .count();
                    assert_eq!(
                        mismatches, 0,
                        "effect case {case}, below {below}, shadow {shadow:?}"
                    );
                }
            }
        }
    }
}

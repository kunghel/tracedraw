use super::*;
use tracedraw_core::document::PageNumberField;
use tracedraw_core::{MasterScope, PageRanges};

#[test]
fn legacy_range_layer_slots_respect_partial_page_layer_orders() {
    let mut doc = doc_with(Vec::new());
    let mut ids = doc.ids().clone();
    doc.pages.clear();
    for names in [
        vec!["A", "B", "Ink"],
        vec!["A", "Section", "B", "Page", "Footer"],
        vec!["A", "B", "Page", "Footer"],
    ] {
        doc.pages.push(Page {
            id: ids.page(),
            name: String::new(),
            size: Size::new(200.0, 100.0),
            layers: names
                .into_iter()
                .map(|name| Layer::new(ids.layer(), name))
                .collect(),
            guides: Vec::new(),
            background: None,
        });
    }
    doc.set_ids(ids);
    let slots = layer_slots(&doc);
    assert_eq!(
        slots.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["Ink", "Footer", "Page", "B", "Section", "A"]
    );
    for page in &doc.pages {
        let positions: Vec<_> = slots_of(page)
            .iter()
            .map(|slot| slots.iter().position(|s| s == slot).expect("slot"))
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    }
    let one = doc_with(Vec::new());
    assert_eq!(layer_slots(&one), slots_of(&one.pages[0]));
    let mut already = doc.clone();
    let layers = ["A", "Section", "B", "Ink", "Page", "Footer"]
        .into_iter()
        .map(|name| Layer::new(already.ids_mut().layer(), name))
        .collect();
    already.pages[0].layers = layers;
    assert_eq!(
        layer_slots(&already),
        slots_of(&already.pages[0]),
        "an order already satisfying every page stays unchanged"
    );
}

#[test]
fn legacy_range_layer_slots_keep_the_old_order_when_pages_conflict() {
    let mut doc = doc_with(Vec::new());
    let mut ids = doc.ids().clone();
    doc.pages[0].layers = vec![Layer::new(ids.layer(), "A"), Layer::new(ids.layer(), "B")];
    doc.pages.push(Page {
        id: ids.page(),
        name: String::new(),
        size: Size::new(200.0, 100.0),
        layers: vec![Layer::new(ids.layer(), "B"), Layer::new(ids.layer(), "A")],
        guides: Vec::new(),
        background: None,
    });
    doc.set_ids(ids);
    assert_eq!(
        layer_slots(&doc)
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["B", "A"]
    );
}

#[test]
fn range_layers_keep_their_pages_and_stacking_in_every_cdr_version() {
    let mut doc = doc_with(Vec::new());
    let mut ids = doc.ids().clone();
    for number in 2..=3 {
        doc.pages.push(Page {
            id: ids.page(),
            name: number.to_string(),
            size: Size::new(200.0, 100.0),
            layers: vec![Layer::new(ids.layer(), "Page")],
            guides: Vec::new(),
            background: None,
        });
    }
    let rectangle = |id, x| {
        let mut shape = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(x, 10.0, x + 5.0, 15.0),
                radius: 0.0,
                corners: None,
            },
        );
        shape.stroke = None;
        shape.fill = Fill::Solid(Color::BLACK);
        shape
    };
    for page in &mut doc.pages {
        page.layers[0].shapes.push(rectangle(ids.shape(), 40.0));
    }
    for (name, x, range, front) in [
        ("A", 10.0, None, false),
        ("Section", 20.0, Some("2"), false),
        ("B", 30.0, None, false),
        ("Footer", 50.0, Some("2-3"), true),
    ] {
        let mut layer = Layer::master(ids.layer(), name, MasterScope::All);
        layer.page_range = range.map(|r| PageRanges::parse(r).expect("range"));
        layer.in_front = front;
        layer.shapes.push(rectangle(ids.shape(), x));
        if front {
            let mut field = Shape::new(
                ids.shape(),
                ShapeKind::Text {
                    spans: vec![TextSpan::new("number", "Sans", 12.0)],
                    origin: Point::new(80.0, 10.0),
                    frame: None,
                    align: Default::default(),
                    para: Default::default(),
                    on_path: None,
                },
            );
            field.page_number = Some(PageNumberField {
                start_at: 1,
                first_page: 1,
                style: 0,
                prefix: String::new(),
                suffix: String::new(),
                ..Default::default()
            });
            layer.shapes.push(field);
        }
        doc.master.push(layer);
    }
    doc.set_ids(ids);
    let expected = [
        vec![10, 30, 40],
        vec![10, 20, 30, 40, 50],
        vec![10, 30, 40, 50],
    ];
    for version in versions() {
        let bytes = document_to_cdr_version(&doc, version);
        let (back, report) = crate::open_bytes(&bytes, "book").expect("CDR book");
        assert_eq!(back.pages.len(), 3, "{version}: {:?}", report.warnings);
        for (index, page) in back.pages.iter().enumerate() {
            let layers = back.layers_for_page(page.id).expect("page layers");
            let shapes: Vec<_> = layers.iter().flat_map(|l| &l.shapes).collect();
            let positions: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.kind {
                    ShapeKind::Rect { .. } | ShapeKind::Path { closed: true, .. } => {
                        assert!(
                            if version < 15 {
                                matches!(s.kind, ShapeKind::Rect { .. })
                            } else {
                                matches!(s.kind, ShapeKind::Path { closed: true, .. })
                            },
                            "{version}: rectangle representation"
                        );
                        let bounds = s.bounds();
                        assert!((bounds.width() - 5.0).abs() < 0.01, "{version}: {bounds:?}");
                        assert!(
                            (bounds.height() - 5.0).abs() < 0.01,
                            "{version}: {bounds:?}"
                        );
                        assert!(
                            (s.page_path().area().abs() - 25.0).abs() < 0.01,
                            "{version}: rectangle area"
                        );
                        Some(bounds.x0.round() as i64)
                    }
                    ShapeKind::Text { .. } => None,
                    other => panic!("{version}: unexpected marker shape {other:?}"),
                })
                .collect();
            assert_eq!(positions, expected[index], "{version}: page {}", index + 1);
            let labels: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.kind {
                    ShapeKind::Text { spans, .. } => {
                        Some(tracedraw_core::document::spans_text(spans))
                    }
                    _ => None,
                })
                .collect();
            let expected_labels = if index == 0 {
                Vec::new()
            } else {
                vec![(index + 1).to_string()]
            };
            assert_eq!(labels, expected_labels, "{version}: page {}", index + 1);
        }
        let ids: Vec<_> = back
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .map(|s| s.id)
            .collect();
        assert_eq!(
            ids.len(),
            ids.iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            "{version}"
        );
    }
    assert!(doc.master[1].page_range.is_some());
    assert!(doc.master[3].page_range.is_some());
}

#[test]
fn range_output_keeps_blend_steps_when_the_endpoint_is_outside_the_page_range() {
    use tracedraw_core::live::{BlendOptions, Effect};

    fn visible_leaves<'a>(shape: &'a Shape, leaves: &mut Vec<&'a Shape>) {
        if !shape.visible {
            return;
        }
        match &shape.kind {
            ShapeKind::Group { children } => {
                for child in children {
                    visible_leaves(child, leaves);
                }
            }
            _ => leaves.push(shape),
        }
    }

    for below in [false, true] {
        let mut doc = doc_with(Vec::new());
        let mut ids = doc.ids().clone();
        doc.pages.push(Page {
            id: ids.page(),
            name: "2".into(),
            size: Size::new(200.0, 100.0),
            layers: vec![Layer::new(ids.layer(), "Ink")],
            guides: Vec::new(),
            background: None,
        });
        let mut start = Shape::new(
            ids.shape(),
            ShapeKind::Rect {
                rect: Rect::new(10.0, 20.0, 20.0, 30.0),
                radius: 0.0,
                corners: None,
            },
        );
        start.fill = Fill::Solid(Color::rgb8(240, 30, 20));
        start.stroke = None;
        start.opacity = 0.65;
        let mut end = start.with_fresh_ids(&mut || ids.shape());
        end.transform_by(Affine::translate((140.0, 0.0)));
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
        let expected = tracedraw_core::live::break_apart(&start, || ids.shape());
        assert_eq!(expected.len(), 3, "the cached endpoint is not drawn");
        for (name, range, shape) in [("Blend", "1-2", start), ("Endpoint", "1", end)] {
            let mut layer = Layer::master(ids.layer(), name, MasterScope::All);
            layer.page_range = Some(PageRanges::parse(range).expect("range"));
            layer.in_front = true;
            layer.shapes.push(shape);
            doc.master.push(layer);
        }
        doc.set_ids(ids);
        for (source, index) in [(doc.clone(), 1), (doc.with_master_layers_on_pages(&[1]), 0)] {
            for version in versions() {
                let bytes = document_to_cdr_version(&source, version);
                let (normal, report) = crate::open_bytes(&bytes, "range blend").expect("CDR");
                assert_eq!(
                    report.skipped_objects, 0,
                    "{version}: {:?}",
                    report.warnings
                );
                let drawn = open_as_drawn(&bytes);
                for back in [&normal, &drawn] {
                    let mut leaves = Vec::new();
                    for layer in back.layers_for_page(back.pages[index].id).expect("layers") {
                        for shape in &layer.shapes {
                            visible_leaves(shape, &mut leaves);
                        }
                    }
                    assert_eq!(leaves.len(), expected.len(), "{version}, below {below}");
                    for (actual, expected) in leaves.iter().zip(&expected) {
                        assert!(
                            near(actual.bounds(), expected.bounds(), 0.05),
                            "{version}, below {below}: {:?} != {:?}",
                            actual.bounds(),
                            expected.bounds()
                        );
                        let (Fill::Solid(actual_color), Fill::Solid(expected_color)) =
                            (&actual.fill, &expected.fill)
                        else {
                            panic!("{version}, below {below}: blend fill");
                        };
                        assert!(
                            actual_color
                                .to_rgb8()
                                .iter()
                                .zip(expected_color.to_rgb8())
                                .all(|(actual, expected)| actual.abs_diff(expected) <= 1),
                            "{version}, below {below}: fill {:?} != {:?}",
                            actual_color,
                            expected_color
                        );
                        // Tagged records in versions 13 to 15 do not yet
                        // write transparency, as for ordinary page objects.
                        let opacity = if (13..=15).contains(&version) {
                            1.0
                        } else {
                            expected.opacity
                        };
                        assert!(
                            (actual.opacity - opacity).abs() < 0.01,
                            "{version}, below {below}: opacity {} != {}",
                            actual.opacity,
                            opacity
                        );
                    }
                }
            }
        }
    }
}

//! Original synthetic CDR27 object style, without private ZIP members.

use tracedraw_core::{Document, ShapeKind};

#[test]
fn cdr_gaussian_blur_imports_as_an_editable_bitmap_effect() {
    let bytes = include_bytes!("fixtures/gaussian-blur.cdr");
    let (doc, report) = tracedraw_cdr::open_bytes(bytes, "Gaussian control").unwrap();
    assert_eq!(report.version.unwrap().0, 27);
    assert_eq!(report.skipped_objects, 0);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let shapes: Vec<_> = doc.all_layers().flat_map(|l| &l.shapes).collect();
    assert_eq!(shapes.len(), 2);
    assert!(shapes[0].raster_fx.is_none());
    assert!(matches!(shapes[1].kind, ShapeKind::Rect { .. }));
    let effect = &shapes[1].raster_fx.as_ref().unwrap().effects[0];
    assert_eq!(effect.id, "gaussian_blur");
    assert_eq!(effect.params["radius"], 15.0);
    assert_eq!(effect.params["kernel_extent"], 2.0);
    assert!(effect.visible);
    let back = Document::from_json(&doc.to_json().unwrap()).unwrap();
    assert_eq!(
        back.all_layers()
            .flat_map(|l| &l.shapes)
            .nth(1)
            .unwrap()
            .raster_fx,
        shapes[1].raster_fx
    );
}

#[test]
fn cdr_source_member_keeps_hidden_and_unprepared_filter_settings() {
    let bytes = include_bytes!("fixtures/gaussian-blur.cdr");
    for visible in [false, true] {
        let (mut doc, _) = tracedraw_cdr::open_bytes(bytes, "control").unwrap();
        doc.all_layers_mut()
            .flat_map(|l| &mut l.shapes)
            .find_map(|s| s.raster_fx.as_mut())
            .unwrap()
            .effects[0]
            .visible = visible;
        for version in [14, 27] {
            let saved = tracedraw_cdr::write::document_to_cdr_version(&doc, version);
            let (back, _) = tracedraw_cdr::open_bytes(&saved, "back").unwrap();
            let fx = back
                .all_layers()
                .flat_map(|l| &l.shapes)
                .find_map(|s| s.raster_fx.as_ref())
                .expect("private source member restored");
            assert_eq!(fx.effects[0].visible, visible);
            assert_eq!(fx.effects[0].params["radius"], 15.0);
        }
    }
}

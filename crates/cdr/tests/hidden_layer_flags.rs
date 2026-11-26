use std::io::{Cursor, Read};
use tracedraw_core::{document::Layer, Document};

fn sample(name: &str) -> Document {
    let mut zip = zip::ZipArchive::new(Cursor::new(include_bytes!(
        "fixtures/hidden-layers/samples.zip"
    )))
    .unwrap();
    let mut bytes = Vec::new();
    zip.by_name(name).unwrap().read_to_end(&mut bytes).unwrap();
    let (doc, report) = tracedraw_cdr::open_bytes(&bytes, name).unwrap();
    assert_eq!(report.skipped_objects, 0, "{name}");
    doc
}

fn named<'a>(layers: &'a [Layer], name: &str) -> &'a Layer {
    layers.iter().find(|l| l.name == name).unwrap()
}

#[test]
fn hidden_layer_flags_keep_objects_and_native_visibility() {
    let doc = sample("two-layers.cdr");
    for doc in [
        doc.clone(),
        Document::from_json(&doc.to_json().unwrap()).unwrap(),
    ] {
        let layers = &doc.pages[0].layers;
        assert_eq!(layers.len(), 2);
        let hidden = named(layers, "HIDDEN");
        assert!(!hidden.visible);
        assert_eq!(hidden.shapes.len(), 1);
        assert!(
            hidden.shapes[0].visible,
            "layer hiding must not hide its object"
        );
        let visible = named(layers, "VISIBLE");
        assert!(visible.visible);
        assert_eq!(visible.shapes.len(), 1);
        assert!(visible.shapes[0].visible);
    }
}

#[test]
fn hidden_layer_flags_retain_a_hidden_only_page() {
    let only = sample("hidden-only.cdr");
    assert_eq!(only.pages.len(), 1);
    assert!(!named(&only.pages[0].layers, "HIDDEN").visible);
}

#[test]
fn hidden_layer_flags_retain_empty_layers() {
    let empty = sample("hidden-empty.cdr");
    assert_eq!(empty.pages[0].layers.len(), 2);
    let layer = named(&empty.pages[0].layers, "HIDDEN");
    assert!(!layer.visible);
    assert!(layer.shapes.is_empty());
}

#[test]
fn hidden_layer_flags_apply_to_master_layers() {
    let doc = sample("hidden-master.cdr");
    assert_eq!(doc.pages.len(), 1);
    let master = named(&doc.master, "HIDDEN MASTER");
    assert!(!master.visible);
    assert_eq!(master.shapes.len(), 1);
    assert!(master.shapes[0].visible);
    assert!(named(&doc.pages[0].layers, "VISIBLE").visible);
}

#[test]
fn hidden_layer_flags_ignore_missing_malformed_and_unrelated_records() {
    for file in [
        "visible.cdr",
        "missing-flags.cdr",
        "short-flags.cdr",
        "wrong-record-type.cdr",
        "unrelated-bit.cdr",
    ] {
        let doc = sample(file);
        let layer = named(&doc.pages[0].layers, "VISIBLE");
        assert!(layer.visible, "{file}");
        assert_eq!(layer.shapes.len(), 1);
    }
    let doc = sample("guides.cdr");
    let guides = named(&doc.pages[0].layers, "Guides");
    // A guides layer holding artwork shows it, as the file does, and does
    // not print (#4331).
    assert!(guides.visible);
    assert!(!guides.printable);
}

#[test]
fn layer_state_flags_preserve_lock_and_output_exclusion_independently() {
    for (file, name, visible, locked, printable, count) in [
        ("locked.cdr", "LOCKED", true, true, true, 1),
        ("excluded.cdr", "EXCLUDED", true, false, false, 1),
        ("locked-excluded-hidden.cdr", "ALL", false, true, false, 1),
        ("locked-empty.cdr", "LOCKED", true, true, true, 0),
        ("excluded-empty.cdr", "EXCLUDED", true, false, false, 0),
    ] {
        let doc = sample(file);
        for doc in [
            doc.clone(),
            Document::from_json(&doc.to_json().unwrap()).unwrap(),
        ] {
            let layer = named(&doc.pages[0].layers, name);
            assert_eq!(
                (layer.visible, layer.locked, layer.printable),
                (visible, locked, printable),
                "{file}"
            );
            assert_eq!(layer.shapes.len(), count);
            assert!(layer.shapes.iter().all(|s| s.visible && !s.locked));
        }
    }
}

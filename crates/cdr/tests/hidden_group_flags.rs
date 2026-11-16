use std::io::{Cursor, Read};
use tracedraw_core::{Document, Shape, ShapeKind};

fn sample(name: &str) -> (Document, tracedraw_cdr::ParseReport) {
    let mut zip = zip::ZipArchive::new(Cursor::new(include_bytes!(
        "fixtures/hidden-groups/samples.zip"
    )))
    .unwrap();
    let mut bytes = Vec::new();
    zip.by_name(name).unwrap().read_to_end(&mut bytes).unwrap();
    tracedraw_cdr::open_bytes(&bytes, name).unwrap()
}

fn children(shape: &Shape) -> &[Shape] {
    let ShapeKind::Group { children } = &shape.kind else {
        panic!("expected a group, got {:?}", shape.kind);
    };
    children
}

#[test]
fn hidden_group_flags_keep_groups_and_their_children() {
    for (file, count) in [("hidden-two.cdr", 2), ("hidden-one.cdr", 1)] {
        let (doc, report) = sample(file);
        assert_eq!(report.skipped_objects, 0);
        let shapes = &doc.pages[0].layers[0].shapes;
        assert_eq!(shapes.len(), 1);
        assert!(!shapes[0].visible, "{file}: hidden group became visible");
        assert_eq!(children(&shapes[0]).len(), count);
        assert!(children(&shapes[0]).iter().all(|s| s.visible));

        let reopened = Document::from_json(&doc.to_json().unwrap()).unwrap();
        let group = &reopened.pages[0].layers[0].shapes[0];
        assert!(!group.visible);
        assert_eq!(children(group).len(), count);
    }
}

#[test]
fn hidden_group_flags_survive_inside_a_visible_group() {
    let (doc, report) = sample("nested.cdr");
    assert_eq!(report.skipped_objects, 0);
    let outer = &doc.pages[0].layers[0].shapes[0];
    assert!(outer.visible);
    let shapes = children(outer);
    assert_eq!(shapes.len(), 2);
    // The reader reverses front-to-back file order.
    assert!(shapes[0].visible);
    assert!(!shapes[1].visible);
    assert_eq!(children(&shapes[1]).len(), 2);
    assert!(children(&shapes[1]).iter().all(|s| s.visible));
}

#[test]
fn hidden_group_flags_ignore_short_wrong_type_and_unrelated_bits() {
    for file in [
        "visible.cdr",
        "short-flags.cdr",
        "wrong-record-type.cdr",
        "unrelated-flag.cdr",
    ] {
        let (doc, report) = sample(file);
        assert_eq!(report.skipped_objects, 0);
        let group = &doc.pages[0].layers[0].shapes[0];
        assert!(group.visible, "{file}");
        assert_eq!(children(group).len(), 2);
    }
}

#[test]
fn individual_object_flags_preserve_visibility_and_lock_independently() {
    for (file, visible, locked) in [
        ("hidden-object.cdr", false, false),
        ("visible-object.cdr", true, false),
        ("locked-object.cdr", true, true),
        ("hidden-locked-object.cdr", false, true),
        ("hidden-object-argument.cdr", false, false),
        ("short-object-flags.cdr", true, false),
        ("wrong-object-record.cdr", true, false),
    ] {
        let (doc, report) = sample(file);
        assert_eq!(report.skipped_objects, 0);
        let shapes = &doc.pages[0].layers[0].shapes;
        assert_eq!(shapes.len(), 1);
        assert_eq!(
            (shapes[0].visible, shapes[0].locked),
            (visible, locked),
            "{file}"
        );
        let reopened = Document::from_json(&doc.to_json().unwrap()).unwrap();
        let shape = &reopened.pages[0].layers[0].shapes[0];
        assert_eq!((shape.visible, shape.locked), (visible, locked));
    }
}

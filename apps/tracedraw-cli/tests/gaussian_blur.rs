//! The headless importer materializes the filter without a GUI callback.

use std::io::{Read as _, Write as _};
use std::process::Command;

#[test]
fn gaussian_cdr_keeps_editable_source_and_blur_in_png_svg_and_pdf() {
    let input = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/cdr/tests/fixtures/gaussian-blur.cdr"
    );
    let dir = std::env::temp_dir().join(format!("tracedraw-gaussian-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for extension in ["tdraw", "png", "svg", "pdf", "cdr"] {
        let output = dir.join(format!("blur.{extension}"));
        let result = Command::new(env!("CARGO_BIN_EXE_tracedraw-cli"))
            .env("TRACEDRAW_DPI", "96")
            .args(["convert", input])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let native = tracedraw_io::load_native(dir.join("blur.tdraw")).unwrap();
    let shapes = &native.pages[0].layers[0].shapes;
    assert!(
        shapes[0].raster_fx.is_none(),
        "sharp background stays vector"
    );
    assert!(matches!(
        shapes[1].kind,
        tracedraw_core::ShapeKind::Rect { .. }
    ));
    let fx = shapes[1].raster_fx.as_ref().unwrap();
    assert_eq!(fx.effects[0].params["radius"], 15.0);
    assert!(!fx.png.is_empty());
    let (back, _) = tracedraw_cdr::open(dir.join("blur.cdr")).unwrap();
    let restored = back
        .all_layers()
        .flat_map(|l| &l.shapes)
        .find(|s| s.raster_fx.is_some())
        .unwrap();
    assert!(matches!(
        restored.kind,
        tracedraw_core::ShapeKind::Rect { .. }
    ));
    assert_eq!(
        restored.raster_fx.as_ref().unwrap().effects[0].params["radius"],
        15.0
    );
    // A consumer ignoring private members still gets the filtered picture.
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(dir.join("blur.cdr")).unwrap()).unwrap();
    let mut stripped = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let name = file.name().unwrap().into_owned();
        if name.starts_with("tracedraw/") {
            continue;
        }
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        stripped
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        stripped.write_all(&data).unwrap();
    }
    let bytes = stripped.finish().unwrap().into_inner();
    let (appearance, _) = tracedraw_cdr::open_bytes(&bytes, "appearance").unwrap();
    fn pictures(shape: &tracedraw_core::Shape) -> usize {
        match &shape.kind {
            tracedraw_core::ShapeKind::Bitmap { .. } => 1,
            tracedraw_core::ShapeKind::Group { children } => children.iter().map(pictures).sum(),
            _ => 0,
        }
    }
    assert_eq!(
        appearance
            .all_layers()
            .flat_map(|l| &l.shapes)
            .map(pictures)
            .sum::<usize>(),
        1
    );
    assert!(appearance
        .all_layers()
        .flat_map(|l| &l.shapes)
        .all(|s| s.raster_fx.is_none()));
    let png = image::open(dir.join("blur.png")).unwrap().to_rgba8();
    let edge = png.get_pixel(151, 250)[0];
    assert!(edge > 40 && edge < 230, "soft white edge: {edge}");
    assert!(png.get_pixel(130, 250)[0] < 30, "sharp dark background");
    assert!(png.get_pixel(250, 250)[0] > 250, "white middle");
    let svg = std::fs::read_to_string(dir.join("blur.svg")).unwrap();
    assert!(svg.contains("<image"), "filtered object is a picture");
    assert!(
        svg.contains("<path") || svg.contains("<rect"),
        "background remains vector"
    );
    let pdf = std::fs::read(dir.join("blur.pdf")).unwrap();
    assert!(pdf
        .windows(b"/Subtype /Image".len())
        .any(|w| w == b"/Subtype /Image"));
    std::fs::remove_dir_all(dir).unwrap();
}

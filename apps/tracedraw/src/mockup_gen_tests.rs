//! Effects > Mockup: the ready product photos, the design laid on their
//! print area with a mesh, Displace and Multiply, one undo step, a new
//! template on a selected mockup, the user's own photo, and the approval
//! JPEG.

use crate::app::App;
use crate::mockup::{self, Blend, MockupSettings, Surface, OWN_PHOTO, TEMPLATES};
use tracedraw_core::geometry::{Point, Rect, Shape as _};
use tracedraw_core::live::{Effect, MergeMode};
use tracedraw_core::{Color, Fill, ShapeId, ShapeKind};

/// A red square and a blue disc side by side, selected.
fn design(app: &mut App) -> Vec<ShapeId> {
    let a = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(20.0, 20.0, 60.0, 60.0),
            radius: 0.0,
            corners: None,
        })
        .expect("square");
    app.select(vec![a]);
    app.apply_fill(Fill::Solid(Color::rgb8(220, 30, 30)));
    let b = app
        .new_shape(ShapeKind::Ellipse {
            rect: Rect::new(60.0, 20.0, 100.0, 60.0),
            arc: None,
        })
        .expect("disc");
    app.select(vec![b]);
    app.apply_fill(Fill::Solid(Color::rgb8(30, 60, 220)));
    app.select(vec![a, b]);
    vec![a, b]
}

fn settings(id: &str) -> MockupSettings {
    MockupSettings {
        template: id.into(),
        ..Default::default()
    }
}

/// The page drawn over `area` at `zoom` pixels per mm.
fn render(app: &App, area: Rect, zoom: f64) -> image::RgbaImage {
    let doc = app.doc();
    let (w, h) = (
        (area.width() * zoom).round() as u32,
        (area.height() * zoom).round() as u32,
    );
    let opts = tracedraw_render::RenderOptions {
        width: w,
        height: h,
        view: tracedraw_render::ViewTransform {
            zoom,
            origin_x: -area.x0 * zoom,
            origin_y: area.y1 * zoom,
            stretch_y: 1.0,
        },
        ..Default::default()
    };
    let px = tracedraw_render::render_page(doc, app.page, &opts).expect("render");
    let mut img = image::RgbaImage::new(w, h);
    for (o, p) in img.pixels_mut().zip(px.pixels()) {
        let c = p.demultiply();
        *o = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    img
}

/// The mockup group's photo and design.
fn parts(app: &App, id: ShapeId) -> (tracedraw_core::Shape, tracedraw_core::Shape) {
    let g = app.doc().find_shape(id).expect("mockup").clone();
    let ShapeKind::Group { children } = &g.kind else {
        panic!("a group: {:?}", g.kind);
    };
    assert_eq!(children.len(), 2);
    (children[0].clone(), children[1].clone())
}

#[test]
fn every_template_draws_a_photo_with_its_print_area_on_it() {
    assert!(TEMPLATES.len() >= 10 && TEMPLATES.len() <= 20);
    let mut ids: Vec<_> = TEMPLATES.iter().map(|t| t.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), TEMPLATES.len(), "ids are unique");
    for t in &TEMPLATES {
        let img = mockup::photo(t, 160);
        assert_eq!(img.dimensions(), (160, 120), "{}", t.id);
        let (lo, hi) = img.pixels().fold((255u8, 0u8), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
        assert!(hi - lo > 40, "{}: a photo, not a flat colour", t.id);
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.5, 0.5)] {
            let (x, y) = t.surface.at(u, v);
            assert!(
                (0.0..=1600.0).contains(&x) && (0.0..=1200.0).contains(&y),
                "{}: ({u}, {v}) at ({x}, {y})",
                t.id
            );
        }
        assert!(crate::i18n::tr(t.key) != t.key, "{} has a name", t.id);
    }
}

#[test]
fn the_perspective_square_lands_on_the_quad_corners() {
    let q = [(100.0, 50.0), (400.0, 80.0), (380.0, 300.0), (90.0, 330.0)];
    for ((u, v), p) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].iter().zip(q) {
        let (x, y) = mockup::square_to_quad(&q, *u, *v);
        assert!((x - p.0).abs() < 0.01 && (y - p.1).abs() < 0.01, "{u} {v}: {x} {y}");
    }
    // The middle is where the diagonals cross, not the corners' average.
    let (x, y) = mockup::square_to_quad(&q, 0.5, 0.5);
    let cross = |a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)| {
        let den = (a.0 - b.0) * (c.1 - d.1) - (a.1 - b.1) * (c.0 - d.0);
        let t = ((a.0 - c.0) * (c.1 - d.1) - (a.1 - c.1) * (c.0 - d.0)) / den;
        (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
    };
    let m = cross(q[0], q[2], q[1], q[3]);
    assert!((x - m.0).abs() < 0.05 && (y - m.1).abs() < 0.05, "{x} {y} vs {m:?}");
    // A cylinder's middle column is nearer the eye: lower than its sides.
    let c = Surface::Cylinder { cx: 800.0, top: 300.0, bottom: 800.0, r: 200.0, ry: 40.0, span: 140.0 };
    assert!(c.at(0.5, 0.5).1 > c.at(0.0, 0.5).1 + 20.0);
    assert!((c.at(0.5, 0.0).0 - 800.0).abs() < 0.01);
}

#[test]
fn a_mockup_lays_the_design_on_the_shirt_in_one_undo_step() {
    let mut app = App::headless();
    let ids = design(&mut app);
    let before = app.doc().clone();
    app.apply_mockup(&settings("shirt-white"));
    let [m] = app.selection[..] else {
        panic!("the mockup is selected: {:?} ({})", app.selection, app.status);
    };
    for id in &ids {
        assert!(app.doc().find_shape(*id).is_some(), "the design's objects are kept, inside");
    }
    let g = app.doc().find_shape(m).expect("mockup").clone();
    assert_eq!(g.data, vec![(mockup::DATA_KEY.to_string(), "shirt-white".to_string())]);
    let (photo, design) = parts(&app, m);
    assert!(matches!(photo.kind, ShapeKind::Bitmap { width_px: 1600, height_px: 1200, .. }));
    let page = app.page_rect();
    let pb = photo.bounds();
    assert!(page.contains(pb.origin()) && pb.width() > page.width() * 0.8);
    let kinds: Vec<_> = design
        .effects
        .iter()
        .map(|e| match e {
            Effect::MeshEnvelope { .. } => "mesh",
            Effect::Displace { .. } => "displace",
            Effect::Transparency { merge: MergeMode::Multiply, .. } => "multiply",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["mesh", "displace", "multiply"]);
    // The design now sits on the chest of the photo.
    let shown = tracedraw_core::live::evaluate(&design).main.page_path().bounding_box();
    let unit = pb.width() / 1600.0;
    let chest = Rect::new(
        pb.x0 + 620.0 * unit,
        pb.y1 - 760.0 * unit,
        pb.x0 + 980.0 * unit,
        pb.y1 - 290.0 * unit,
    );
    assert!(
        shown.center().distance(Point::new(chest.center().x, shown.center().y)) < 3.0
            && chest.inflate(8.0, 8.0).contains(shown.origin())
            && chest.inflate(8.0, 8.0).contains(Point::new(shown.x1, shown.y1)),
        "{shown:?} in {chest:?}"
    );
    // The design keeps its proportions: twice as wide as tall.
    assert!((shown.width() / shown.height() - 2.0).abs() < 0.35, "{shown:?}");
    // One undo brings the drawing back as it was.
    app.undo();
    assert_eq!(app.doc().pages[0].layers, before.pages[0].layers);
}

#[test]
fn the_print_takes_the_shirt_and_the_mug_and_shows_on_black() {
    for (id, multiply) in [("shirt-white", true), ("shirt-black", false), ("mug-white", true)] {
        let mut app = App::headless();
        design(&mut app);
        app.apply_mockup(&settings(id));
        let m = app.selection[0];
        let (photo, design) = parts(&app, m);
        assert_eq!(
            design.effects.iter().any(|e| matches!(e, Effect::Transparency { .. })),
            multiply,
            "{id}"
        );
        let area = photo.bounds();
        let img = render(&app, area, 3.0);
        let shown = tracedraw_core::live::evaluate(&design).main.page_path().bounding_box();
        // The red square's middle: the left quarter of the design.
        let p = Point::new(shown.x0 + shown.width() * 0.25, shown.center().y);
        let px = img.get_pixel(((p.x - area.x0) * 3.0) as u32, ((area.y1 - p.y) * 3.0) as u32);
        assert!(px[0] > px[1] + 50 && px[0] > px[2] + 50, "{id}: red shows {px:?}");
        // The blue disc's middle.
        let q = Point::new(shown.x0 + shown.width() * 0.75, shown.center().y);
        let px = img.get_pixel(((q.x - area.x0) * 3.0) as u32, ((area.y1 - q.y) * 3.0) as u32);
        assert!(px[2] > px[0] + 50, "{id}: blue shows {px:?}");
    }
}

#[test]
fn the_mug_wraps_the_design_round_its_body() {
    let mut app = App::headless();
    design(&mut app);
    app.apply_mockup(&settings("mug-white"));
    let (_, design) = parts(&app, app.selection[0]);
    let Some(Effect::MeshEnvelope { points, cols, .. }) = design.effects.first() else {
        panic!("a mesh");
    };
    let w = usize::from(*cols) + 1;
    // A level row of the mesh: its middle hangs lower than its ends (the
    // front of the cylinder), and its columns crowd towards the sides.
    let row: Vec<Point> = points[4 * w..5 * w].to_vec();
    assert!(row[w / 2].y < row[0].y.min(row[w - 1].y) - 0.5, "{row:?}");
    let gaps: Vec<f64> = row.windows(2).map(|p| p[1].x - p[0].x).collect();
    assert!(gaps[w / 2] > gaps[0] * 1.05, "{gaps:?}");
}

#[test]
fn a_selected_mockup_takes_another_template_and_the_same_design() {
    let mut app = App::headless();
    let ids = design(&mut app);
    app.apply_mockup(&settings("shirt-white"));
    let first = app.selection[0];
    let photo_rect = parts(&app, first).0.bounds();
    let mut s = settings("tote");
    s.size = 0.5;
    s.blend = Blend::Normal;
    app.apply_mockup(&s);
    let [second] = app.selection[..] else {
        panic!("{:?}", app.selection);
    };
    assert!(app.doc().find_shape(first).is_none());
    let g = app.doc().find_shape(second).expect("mockup");
    assert_eq!(g.data[0].1, "tote");
    let (photo, design) = parts(&app, second);
    assert!((photo.bounds().x0 - photo_rect.x0).abs() < 1e-6, "same place");
    let ShapeKind::Group { children } = &design.kind else {
        panic!();
    };
    assert_eq!(children.iter().map(|c| c.id).collect::<Vec<_>>(), ids);
    assert!(!design.effects.iter().any(|e| matches!(e, Effect::Transparency { .. })), "Normal");
    assert_eq!(app.doc().pages[0].layers.iter().map(|l| l.shapes.len()).sum::<usize>(), 1);
}

#[test]
fn the_own_photo_carries_the_design_and_needs_a_picture() {
    let mut app = App::headless();
    let ids = design(&mut app);
    app.apply_mockup(&settings(OWN_PHOTO));
    assert_eq!(app.status, crate::i18n::tr("status.mockup_select_photo"));
    assert_eq!(app.selection, ids, "nothing changed");
    let img = image::RgbaImage::from_fn(400, 300, |x, y| {
        let v = (120 + (x / 4 + y / 3) % 100) as u8;
        image::Rgba([v, v, v, 255])
    });
    let pic = app.place_image_at(img, Some("Wall".into()), Some((150.0, 150.0))).expect("picture");
    let mut sel = ids.clone();
    sel.push(pic);
    app.select(sel);
    app.apply_mockup(&settings(OWN_PHOTO));
    let m = app.selection[0];
    let (photo, design) = parts(&app, m);
    assert_eq!(photo.id, pic, "the user's picture is the photo");
    let pb = photo.bounds();
    let shown = tracedraw_core::live::evaluate(&design).main.page_path().bounding_box();
    assert!(pb.contains(shown.origin()) && pb.contains(Point::new(shown.x1, shown.y1)), "{shown:?} {pb:?}");
}

#[test]
fn nothing_selected_tells_what_to_select() {
    let mut app = App::headless();
    app.select(vec![]);
    app.apply_mockup(&settings("mug-black"));
    assert_eq!(app.status, crate::i18n::tr("status.mockup_select_design"));
    assert!(app.doc().pages[0].layers.iter().all(|l| l.shapes.is_empty()));
}

#[test]
fn the_approval_jpeg_is_the_photo_with_its_caption() {
    let mut app = App::headless();
    design(&mut app);
    app.apply_mockup(&settings("cards"));
    let plain = app.mockup_jpeg(800, "").expect("jpeg");
    let img = image::load_from_memory_with_format(&plain, image::ImageFormat::Jpeg).expect("decodes");
    assert_eq!((img.width(), img.height()), (800, 600));
    let with = app.mockup_jpeg(800, "Para aprovação").expect("jpeg");
    let img = image::load_from_memory_with_format(&with, image::ImageFormat::Jpeg)
        .expect("decodes")
        .to_rgb8();
    assert_eq!(img.width(), 800);
    assert!(img.height() > 600, "a band for the caption");
    // The band is white with dark letters in it.
    let band: Vec<u8> = (600..img.height())
        .flat_map(|y| (0..400).map(move |x| (x, y)))
        .map(|(x, y)| img.get_pixel(x, y)[0])
        .collect();
    assert!(band.iter().filter(|v| **v > 240).count() > band.len() / 2);
    assert!(band.iter().any(|v| *v < 120), "the caption is written");
}

/// Every template with the test design, into `$MOCKUP_SHOTS`, to look at.
#[test]
#[ignore]
fn mockup_shots() {
    let Some(dir) = std::env::var_os("MOCKUP_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    tracedraw_text::install();
    for t in &TEMPLATES {
        let mut app = App::headless();
        design(&mut app);
        app.apply_mockup(&settings(t.id));
        let bytes = app.mockup_jpeg(800, "").expect("jpeg");
        std::fs::write(dir.join(format!("{}.jpg", t.id)), bytes).unwrap();
    }
}

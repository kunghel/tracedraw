//! #2890 / #2893: shadows stay on their objects and break off them.
use crate::app::App;
use tracedraw_core::geometry::Shape as _;
use tracedraw_core::{
    geometry::{Point, Vec2},
    live::{Effect, MergeMode},
    Color, Command, Fill, Shadow, ShapeId, ShapeKind,
};

fn text(app: &mut App, at: Point, s: &str, mesh: bool) -> ShapeId {
    tracedraw_text::install();
    app.start_text(at, None);
    app.text_insert(s);
    let id = app.text_edit.as_ref().expect("editing").shape;
    app.finish_text();
    let b = app
        .doc()
        .find_shape(id)
        .expect("text")
        .local_path()
        .bounding_box();
    let fill = if mesh {
        Fill::Mesh(tracedraw_core::style::Mesh::new(
            b,
            2,
            2,
            Color::rgb8(40, 200, 90),
        ))
    } else {
        Fill::Solid(Color::rgb8(200, 30, 30))
    };
    app.run(Command::SetFill {
        shapes: vec![id],
        fill,
    });
    id
}

fn shadow_of(app: &App, id: ShapeId) -> Option<Shadow> {
    app.doc().find_shape(id).and_then(|s| s.shadow)
}

fn count(app: &App) -> usize {
    app.doc().pages[0]
        .layers
        .iter()
        .map(|l| l.shapes.len())
        .sum()
}

/// #2890: a drop shadow dragged onto one object, then onto another,
/// stays on the first; the bar's fields and a drag on the same object
/// still make one step of their changes.
#[test]
fn a_new_shadow_leaves_the_last_ones_shadow_alone() {
    let mut app = App::headless();
    app.tool = crate::tools::Tool::DropShadow;
    let d = text(&mut app, Point::new(20.0, 200.0), "D", true);
    let t = text(&mut app, Point::new(80.0, 200.0), "T", false);
    let at = |app: &App, id: ShapeId| app.doc().shape_on_page(id).expect("text").bounds().center();
    // A drag on D: the first frame, then two more.
    let (cd, ct) = (at(&app, d), at(&app, t));
    app.drag_new_shadow(d, cd, cd + Vec2::new(3.0, -3.0), true);
    app.drag_new_shadow(d, cd, cd + Vec2::new(5.0, -5.0), false);
    app.drag_new_shadow(d, cd, cd + Vec2::new(6.0, -6.0), false);
    assert!(shadow_of(&app, d).is_some());
    let steps = app.engine.history_labels().0.len();
    // A drag on T: D keeps its shadow.
    app.drag_new_shadow(t, ct, ct + Vec2::new(4.0, -4.0), true);
    app.drag_new_shadow(t, ct, ct + Vec2::new(7.0, -2.0), false);
    assert!(shadow_of(&app, t).is_some());
    let sd = shadow_of(&app, d).expect("D keeps its shadow");
    assert!(
        (sd.offset - Vec2::new(6.0, -6.0)).hypot() < 1e-9,
        "{:?}",
        sd.offset
    );
    assert_eq!(
        app.engine.history_labels().0.len(),
        steps + 1,
        "T's drag is one step"
    );
    // The bar's field typed into on D again (D chosen): D's shadow alone
    // changes, T's stays.
    app.select(vec![d]);
    let mut sh = sd;
    sh.opacity = 0.3;
    app.shadow_bar_changed(sh, true);
    sh.opacity = 0.4;
    app.shadow_bar_changed(sh, true);
    assert!(shadow_of(&app, t).is_some(), "T keeps its shadow");
    assert!((shadow_of(&app, d).expect("D").opacity - 0.4).abs() < 1e-9);
    // Undo takes back the field, then T's shadow, then D's.
    app.undo();
    assert!((shadow_of(&app, d).expect("D").opacity - sd.opacity).abs() < 1e-9);
    app.undo();
    assert!(shadow_of(&app, t).is_none() && shadow_of(&app, d).is_some());
}

/// #2893: Break Apart (Ctrl+K, the Objects docker's menu) takes every
/// kind of shadow off: a drop shadow and a block shadow come out behind
/// the object, an inner shadow in front of it, from plain or mesh-filled
/// text, on a layer or in a group; the object keeps its look otherwise.
#[test]
fn every_kind_of_shadow_breaks_off_its_object() {
    let mut app = App::headless();
    let t = text(&mut app, Point::new(20.0, 200.0), "T", true);
    app.select(vec![t]);
    app.set_shadow(Some(Shadow::default()), false);
    app.push_effect(
        Effect::BlockShadow {
            offset: Vec2::new(4.0, -4.0),
            color: Color::rgb8(20, 20, 120),
            gap: 0.0,
        },
        true,
    );
    app.push_effect(
        Effect::InnerShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::BLACK,
            opacity: 0.8,
            blur: 1.0,
            merge: MergeMode::Multiply,
        },
        true,
    );
    let before = count(&app);
    let fill = app.doc().find_shape(t).expect("text").fill.clone();
    assert!(app.can_break_shadow_apart());
    assert_eq!(app.break_apart_label(), "menu.object.break_shadows_apart");
    app.break_apart();
    let s = app.doc().find_shape(t).expect("text").clone();
    assert!(s.shadow.is_none(), "the drop shadow is off");
    assert!(s.effects.is_empty(), "{:?}", s.effects);
    assert_eq!(s.fill, fill, "the object keeps its mesh");
    assert!(count(&app) >= before + 3, "{} -> {}", before, count(&app));
    // In the layer: drop shadow, block shadow, the text, inner shadow.
    let layer = &app.doc().pages[0].layers[0];
    let pos = |id: ShapeId| {
        layer
            .shapes
            .iter()
            .position(|s| s.id == id)
            .expect("on the layer")
    };
    let it = pos(t);
    let made = app.selection.clone();
    assert_eq!(made.len(), 3, "{made:?}");
    let kinds: Vec<(usize, bool)> = made
        .iter()
        .map(|id| {
            let s = app.doc().find_shape(*id).expect("made");
            (pos(*id), matches!(s.kind, ShapeKind::Bitmap { .. }))
        })
        .collect();
    assert!(
        kinds[0].1 && kinds[0].0 < it,
        "drop shadow bitmap behind: {kinds:?}"
    );
    assert!(
        !kinds[1].1 && kinds[1].0 < it,
        "block shadow curves behind: {kinds:?}"
    );
    assert!(
        kinds[2].1 && kinds[2].0 > it,
        "inner shadow bitmap in front: {kinds:?}"
    );
    assert!(kinds[0].0 < kinds[1].0, "the drop shadow at the back");
    // The inner shadow keeps its merge mode.
    let inner = app.doc().find_shape(made[2]).expect("inner");
    assert!(matches!(
        inner.effects.as_slice(),
        [Effect::Transparency {
            merge: MergeMode::Multiply,
            ..
        }]
    ));
    // One undo puts them all back.
    app.undo();
    let s = app.doc().find_shape(t).expect("text");
    assert!(s.shadow.is_some() && s.effects.len() == 2);
    assert_eq!(count(&app), before);
}

/// #2893: a block shadow alone on plain text, the text inside a group:
/// Break Apart takes it off there too and its curves stay in the group.
#[test]
fn a_shadow_breaks_off_an_object_inside_a_group() {
    let mut app = App::headless();
    let a = text(&mut app, Point::new(20.0, 200.0), "A", false);
    let b = text(&mut app, Point::new(60.0, 200.0), "B", false);
    app.select(vec![a, b]);
    app.group_selection();
    let group = app.selection[0];
    app.select(vec![a]);
    app.push_effect(
        Effect::BlockShadow {
            offset: Vec2::new(3.0, -3.0),
            color: Color::BLACK,
            gap: 0.0,
        },
        true,
    );
    assert_eq!(app.break_apart_label(), "menu.object.break_shadows_apart");
    app.break_apart();
    assert!(app.doc().find_shape(a).expect("A").effects.is_empty());
    let made = app.selection.clone();
    assert!(!made.is_empty());
    assert_eq!(app.doc().ancestors(made[0]), vec![group], "in the group");
}

/// #2954 (the reporter's two videos, 0.46.0): a block shadow on a curve
/// of several pieces (letters converted to curves), and on a contour
/// broken off its object, vanished when the object was broken apart, and
/// on a single curve it stayed attached. Break Apart now takes the block
/// shadow off as curves behind the object first; a second Break Apart
/// splits the object's pieces, and the shadow curves stay.
#[test]
fn a_block_shadow_survives_break_apart_of_a_curve_of_pieces() {
    use tracedraw_core::geometry::{BezPath, Rect};
    let mut app = App::headless();
    let mut path = BezPath::new();
    for x in [20.0, 60.0] {
        path.move_to((x, 20.0));
        path.line_to((x + 30.0, 20.0));
        path.line_to((x + 30.0, 50.0));
        path.line_to((x, 50.0));
        path.close_path();
    }
    let letters = app
        .new_shape(ShapeKind::Path { path, closed: true })
        .expect("curve");
    let r = app
        .new_shape(ShapeKind::Rect {
            rect: Rect::new(20.0, 100.0, 60.0, 140.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.run(Command::SetFill {
        shapes: vec![letters, r],
        fill: Fill::Solid(Color::rgb8(250, 200, 0)),
    });
    // The rectangle's contour broken off it: a group of its steps.
    app.select(vec![r]);
    app.push_effect(
        Effect::Contour {
            steps: 2,
            offset: 3.0,
            outside: true,
            to_center: false,
            both: false,
            fill_to: Color::BLACK,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: Some(tracedraw_core::shaping::Join::Miter),
            accel_objects: 0.0,
            accel_colors: 0.0,
        },
        true,
    );
    app.break_apart();
    let mut chosen = app.selection.clone();
    chosen.push(letters);
    app.select(chosen.clone());
    app.push_effect(
        Effect::BlockShadow {
            offset: Vec2::new(4.0, -4.0),
            color: Color::rgb8(120, 120, 120),
            gap: 0.0,
        },
        true,
    );
    let before = count(&app);
    assert_eq!(app.break_apart_label(), "menu.object.break_shadows_apart");
    app.break_apart();
    let shadows = app.selection.clone();
    assert_eq!(shadows.len(), chosen.len(), "one shadow each: {shadows:?}");
    assert_eq!(count(&app), before + shadows.len());
    for id in &chosen {
        let s = app.doc().find_shape(*id).expect("object kept");
        assert!(s.effects.is_empty() && s.visible);
    }
    for id in &shadows {
        let s = app.doc().find_shape(*id).expect("shadow");
        assert!(matches!(s.kind, ShapeKind::Path { .. }), "curves");
        assert!(!s.local_path().bounding_box().is_zero_area());
    }
    // Break Apart again splits the letters; the shadows stay.
    app.select(vec![letters]);
    app.break_apart();
    assert_eq!(app.selection.len(), 2, "the two pieces");
    for id in &shadows {
        assert!(app.doc().find_shape(*id).is_some());
    }
}

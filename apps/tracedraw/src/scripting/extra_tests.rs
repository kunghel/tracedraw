//! Dimensions (#2348) and symbols (#2343) from scripts, VBA macros and
//! recorded macros.

use crate::app::App;
use crate::dimensions::{Dimension, Measure};
use crate::scripting::{commands_to_script, run, run_vgcore};
use tracedraw_core::document::{Shape, ShapeKind};

fn js(app: &mut App, src: &str) -> Vec<String> {
    run(app, src).unwrap_or_else(|e| panic!("script failed: {e}"))
}

fn top(app: &App) -> Vec<Shape> {
    app.doc()
        .pages
        .iter()
        .flat_map(|p| p.layers.iter())
        .flat_map(|l| l.shapes.iter().cloned())
        .collect()
}

fn dimensions(app: &App) -> Vec<Dimension> {
    top(app)
        .iter()
        .filter_map(|s| Dimension::of(s).map(|d| d.transformed(s.transform)))
        .collect()
}

#[test]
fn a_script_makes_live_dimensions() {
    let mut app = App::headless();
    let out = js(
        &mut app,
        "const d = ActiveLayer.CreateParallelDimension(10, 10, 40, 50, 0, 60);
         const h = ActiveLayer.CreateHorizontalDimension(0, 0, 30, 20, 15, -10);
         const a = ActiveLayer.CreateAngularDimension(0, 0, 10, 0, 0, 10, 5, 5);
         print(d.Dimension.kind, d.Dimension.value.toFixed(3), h.Dimension.value.toFixed(3),
               a.Dimension.value.toFixed(3), ActiveLayer.CreateRectangle(0,0,1,1).Dimension);",
    );
    assert_eq!(out, vec!["linear 50.000 30.000 90.000 null"]);
    let dims = dimensions(&app);
    assert_eq!(dims.len(), 3, "three live dimensions");
    let Measure::Linear { a, b, .. } = dims[1].measure else {
        panic!("linear")
    };
    assert!(
        (a.y - b.y).abs() < 1e-9,
        "the horizontal one measures along x"
    );
}

#[test]
fn a_vba_macro_makes_a_linear_dimension() {
    let mut app = App::headless();
    let out = run_vgcore(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const d = ActiveLayer.CreateLinearDimension(cdrDimensionHorizontal, [0, 0], [40, 30]);
         const s = ActiveLayer.CreateLinearDimension(cdrDimensionSlanted, {x: 0, y: 0}, {x: 30, y: 40}, true, 0, 50);
         print(cdrDimensionVertical);",
        "Macro",
    )
    .unwrap();
    assert_eq!(out, vec!["1"]);
    let dims = dimensions(&app);
    assert_eq!(dims.len(), 2);
    assert!((dims[0].value() - 40.0).abs() < 1e-6);
    assert!((dims[1].value() - 50.0).abs() < 1e-6);
}

/// A VBA macro makes horizontal and vertical dimensions from numbers, as
/// a script of the Scripts panel does (#3517: error 438 before).
#[test]
fn a_vba_macro_makes_horizontal_and_vertical_dimensions() {
    let mut app = App::headless();
    run_vgcore(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const layer = ActiveDocument.ActiveLayer;
         layer.CreateHorizontalDimension(0, 100, 200, 100, 100, 110);
         layer.CreateVerticalDimension(200, 0, 200, 100, 210, 50);
         layer.CreateParallelDimension(0, 0, 30, 40, 0, 50);",
        "Macro",
    )
    .unwrap();
    let dims = dimensions(&app);
    assert_eq!(dims.len(), 3, "three live dimensions");
    assert!((dims[0].value() - 200.0).abs() < 1e-6);
    assert!((dims[1].value() - 100.0).abs() < 1e-6);
    assert!((dims[2].value() - 50.0).abs() < 1e-6);
    let Measure::Linear { at, .. } = dims[0].measure else {
        panic!("linear")
    };
    assert!(
        (at.y - 110.0).abs() < 1e-6,
        "the line passes through the text point"
    );
}

#[test]
fn a_recorded_dimension_plays_back_as_one() {
    let mut app = App::headless();
    js(
        &mut app,
        "ActiveLayer.CreateParallelDimension(10, 10, 40, 50, 0, 60);",
    );
    let shape = top(&app).pop().expect("dimension");
    let layer = app.doc().pages[0].layers[0].id;
    let script = commands_to_script(&[tracedraw_core::Command::AddShape { layer, shape }]);
    assert!(
        script.contains("layer.CreateParallelDimension(10.000, 10.000, 40.000, 50.000"),
        "{script}"
    );
    let mut again = App::headless();
    js(&mut again, &script);
    assert_eq!(dimensions(&again), dimensions(&app));
}

/// Dimensions drawn with the Dimension tools and Add Dimensions while a
/// macro is recorded are in the recorded script, and play back as live
/// dimensions (#2348: only the rectangle was recorded).
#[test]
fn dimensions_drawn_while_recording_are_recorded() {
    use tracedraw_core::geometry::Point;
    let mut app = App::headless();
    app.toggle_recording();
    let rect = app
        .new_shape(ShapeKind::Rect {
            rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 50.0, 30.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
    app.create_dimension(
        Point::new(10.0, 10.0),
        Point::new(50.0, 10.0),
        Point::new(30.0, 0.0),
    );
    app.create_dimension(
        Point::new(50.0, 10.0),
        Point::new(50.0, 30.0),
        Point::new(60.0, 20.0),
    );
    app.create_dimension(
        Point::new(10.0, 10.0),
        Point::new(50.0, 30.0),
        Point::new(20.0, 40.0),
    );
    app.create_angular_dimension(
        Point::new(10.0, 10.0),
        Point::new(50.0, 10.0),
        Point::new(10.0, 30.0),
        Point::new(20.0, 20.0),
    );
    app.select(vec![rect]);
    app.add_dimensions(false);
    app.toggle_recording();
    let script = app.script_source.clone();
    for call in [
        "layer.CreateHorizontalDimension(10.000, 10.000, 50.000, 10.000, 30.000, 0.000)",
        "layer.CreateVerticalDimension(50.000, 10.000, 50.000, 30.000, 60.000, 20.000)",
        "layer.CreateParallelDimension(10.000, 10.000, 50.000, 30.000, 20.000, 40.000)",
        "layer.CreateAngularDimension(10.000, 10.000, 50.000, 10.000, 10.000, 30.000",
    ] {
        assert!(script.contains(call), "{call} missing from:\n{script}");
    }
    // The tool's four and Add Dimensions' two.
    assert_eq!(script.matches("Dimension(").count(), 6, "{script}");
    let mut again = App::headless();
    js(&mut again, &script);
    assert_eq!(dimensions(&again).len(), 6);
}

/// A script's dimension between an object's corners holds to them, as the
/// Dimension tool's does: the object made wider, the dimension measures
/// the new width (#2348: script dimensions did not follow).
#[test]
fn a_script_dimension_follows_its_object() {
    let mut app = App::headless();
    js(
        &mut app,
        "const r = ActiveLayer.CreateRectangle(10, 10, 40, 20);
         ActiveLayer.CreateHorizontalDimension(10, 10, 50, 10, 30, 0);
         ActiveLayer.CreateHorizontalDimension(100, 100, 140, 100, 120, 90);",
    );
    let dims = dimensions(&app);
    assert_eq!(dims[0].anchors.len(), 2, "{:?}", dims[0].anchors);
    assert!(dims[1].anchors.is_empty(), "nothing under the second");
    let rect = top(&app)
        .iter()
        .find(|s| matches!(s.kind, ShapeKind::Rect { .. }))
        .expect("rect")
        .id;
    app.run(tracedraw_core::Command::TransformShapes {
        shapes: vec![rect],
        transform: tracedraw_core::geometry::Affine::translate((10.0, 0.0))
            * tracedraw_core::geometry::Affine::scale_non_uniform(2.0, 1.0)
            * tracedraw_core::geometry::Affine::translate((-10.0, 0.0)),
    });
    app.bake_dimensions();
    let dims = dimensions(&app);
    assert!((dims[0].value() - 80.0).abs() < 1e-6, "{}", dims[0].value());
    assert!((dims[1].value() - 40.0).abs() < 1e-6);
}

/// Four "parts": a box and a dot, three alike (two with a label), one
/// recoloured.
const PARTS: &str = "
    function part(x, label, red) {
      const r = ActiveLayer.CreateRectangle(x, 0, 20, 10);
      const e = ActiveLayer.CreateEllipse(x + 2, 2, 4, 4);
      if (red) e.Fill.ApplyUniformFill(Color.RGB(255, 0, 0));
      const ids = [r.id, e.id];
      if (label) ids.push(ActiveLayer.CreateArtisticText(x + 8, 3, label, 'Arial', 8).id);
      return new ShapeRange(ids).Group();
    }
    const first = part(0, 'A1');
    part(30, 'B2');
    part(60, null);
    part(90, 'C3', true);
";

#[test]
fn similar_groups_become_one_symbol_and_keep_their_text() {
    let mut app = App::headless();
    let out = js(
        &mut app,
        &format!(
            "{PARTS}
             const r = first.SymbolizeSimilar('Part');
             print(r.replaced, r.with_text, r.skipped, r.symbol_name, ActiveDocument.Symbols.length);"
        ),
    );
    assert_eq!(out, vec!["3 2 1 Part 1"]);
    let shapes = top(&app);
    let instances = |s: &Shape| match &s.kind {
        ShapeKind::SymbolInstance { .. } => 1,
        ShapeKind::Group { children } => children
            .iter()
            .filter(|c| matches!(c.kind, ShapeKind::SymbolInstance { .. }))
            .count(),
        _ => 0,
    };
    assert_eq!(shapes.iter().map(instances).sum::<usize>(), 3);
    let mut labels: Vec<String> = shapes
        .iter()
        .flat_map(|s| match &s.kind {
            ShapeKind::Group { children } => children.clone(),
            _ => vec![],
        })
        .filter_map(|c| match &c.kind {
            ShapeKind::Text { spans, .. } => Some(spans.iter().map(|t| t.text.as_str()).collect()),
            _ => None,
        })
        .collect();
    labels.sort();
    assert_eq!(labels, vec!["A1", "B2", "C3"], "labels stay editable text");
    // The instances sit where the parts were.
    let b = shapes
        .iter()
        .find(|s| matches!(s.kind, ShapeKind::SymbolInstance { .. }))
        .map(|s| app.doc().shape_on_page(s.id).expect("instance").bounds())
        .expect("bare instance");
    assert!((b.x0 - 60.0).abs() < 1e-6 && b.y0.abs() < 1e-6, "{b:?}");
    // Running it again reuses the symbol.
    let again = js(
        &mut app,
        &format!(
            "{PARTS}
             const r = first.SymbolizeSimilar();
             print(r.replaced, ActiveDocument.Symbols.length);"
        ),
    );
    assert_eq!(again, vec!["3 1"]);
}

#[test]
fn a_script_creates_and_places_symbols() {
    let mut app = App::headless();
    let out = js(
        &mut app,
        "const r = ActiveLayer.CreateRectangle(10, 10, 5, 5);
         const s = r.CreateSymbol('Pin');
         const p = ActiveLayer.PlaceSymbol('Pin', 50, 0);
         print(s.name, ActiveDocument.Symbols[0].Name, p.PositionX, p.PositionY);",
    );
    assert_eq!(out, vec!["Pin Pin 50 0"]);
    assert_eq!(app.doc().symbols.len(), 1);
    let kinds: Vec<bool> = top(&app)
        .iter()
        .map(|s| matches!(s.kind, ShapeKind::SymbolInstance { .. }))
        .collect();
    assert_eq!(kinds, vec![true, true], "the rectangle became an instance");
}

fn backgrounds(app: &App) -> Vec<Option<tracedraw_core::Fill>> {
    top(app)
        .iter()
        .filter_map(|s| match &s.kind {
            ShapeKind::Text { spans, .. } => Some(spans[0].background.clone()),
            _ => None,
        })
        .collect()
}

/// A script sets and reads the characters' background apart from their
/// fill, and takes it off (#3518).
#[test]
fn a_script_sets_the_text_background() {
    use tracedraw_core::{Color, Fill};
    let mut app = App::headless();
    let out = js(
        &mut app,
        "const t = ActiveLayer.CreateArtisticText(10, 10, 'Hi', 'Arial', 12);
         t.Fill.ApplyUniformFill(Color.RGB(255, 255, 255));
         print(t.TextBackground === null);
         t.TextBackground = Color.RGB(0, 0, 0);
         print(t.TextBackground.Hex);",
    );
    assert_eq!(out, vec!["true", "#000000"]);
    assert_eq!(backgrounds(&app), vec![Some(Fill::Solid(Color::BLACK))]);
    let id = top(&app)[0].id.raw();
    js(&mut app, &format!("new Shape({id}).TextBackground = null;"));
    assert_eq!(backgrounds(&app), vec![None]);
}

/// A VBA macro sets the background of a story (#3518).
#[test]
fn a_vba_macro_sets_the_text_background() {
    use tracedraw_core::{Color, Fill};
    let mut app = App::headless();
    run_vgcore(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const t = ActiveLayer.CreateArtisticText(10, 10, 'Hi');
         t.Text.Story.BackgroundColor = Application.CreateCMYKColor(0, 0, 0, 100);",
        "Macro",
    )
    .unwrap();
    assert_eq!(
        backgrounds(&app),
        vec![Some(Fill::Solid(Color::cmyk_pct(0.0, 0.0, 0.0, 100.0)))]
    );
}

/// A background set in the Text panel while a macro is recorded is in
/// the recorded script and plays back (#3518: it was left out).
#[test]
fn a_recorded_text_background_plays_back() {
    use tracedraw_core::{Color, Fill};
    let mut app = App::headless();
    js(
        &mut app,
        "ActiveLayer.CreateArtisticText(10, 10, 'Hi', 'Arial', 12);",
    );
    let id = top(&app)[0].id;
    app.select(vec![id]);
    app.toggle_recording();
    let black = Fill::Solid(Color::BLACK);
    let b = black.clone();
    crate::ui::text_docker::edit_spans(&mut app, move |sp| sp.background = Some(b.clone()));
    let cmds = app.recording.clone().expect("recording");
    let script = commands_to_script(&cmds);
    assert!(script.contains("set_text_style"), "{script}");
    let mut again = App::headless();
    js(
        &mut again,
        "ActiveLayer.CreateArtisticText(10, 10, 'Hi', 'Arial', 12);",
    );
    // The recorded id names the text made first in the other drawing too.
    let other = top(&again)[0].id.raw();
    js(
        &mut again,
        &script.replace(&format!("id\":{}", id.raw()), &format!("id\":{other}")),
    );
    assert_eq!(backgrounds(&again), vec![Some(black)]);
}

/// A dimension's label size changed on the Dimension bar while a macro is
/// recorded is in the recorded script, after the dimension it was drawn
/// with, and plays back on the dimension the macro draws (#4026: only
/// the dimension was recorded, at the size new dimensions had).
#[test]
fn a_dimension_label_size_change_is_recorded_and_plays_back() {
    use tracedraw_core::geometry::Point;
    let mut app = App::headless();
    app.toggle_recording();
    app.create_dimension(
        Point::new(10.0, 10.0),
        Point::new(50.0, 10.0),
        Point::new(30.0, 0.0),
    );
    let dim = top(&app).pop().expect("dimension").id;
    app.select(vec![dim]);
    app.set_dimension_style(|s| s.text_size = 30.0);
    assert_eq!(dimensions(&app)[0].style.text_size, 30.0);
    app.toggle_recording();
    let script = app.script_source.clone();
    let made = script.find("layer.CreateHorizontalDimension(").expect(&script);
    let sized = script
        .find("ActiveSelection.SetDimensionStyle({\"text_size\":30.0});")
        .expect(&script);
    assert!(made < sized, "{script}");
    let mut again = App::headless();
    js(&mut again, &script);
    let dims = dimensions(&again);
    assert_eq!(dims.len(), 1, "{script}");
    assert_eq!(dims[0].style.text_size, 30.0);
}

/// Scripts and VBA macros read and set a dimension's label size and the
/// other settings of its style; other objects are left alone and a
/// setting the style does not have is an error (#4026).
#[test]
fn scripts_and_macros_set_a_dimensions_label_size() {
    let mut app = App::headless();
    let out = js(
        &mut app,
        "const d = ActiveLayer.CreateParallelDimension(10, 10, 40, 50, 0, 60);
         const r = ActiveLayer.CreateRectangle(0, 0, 5, 5);
         d.LabelSize = 300;
         print(d.LabelSize, r.LabelSize, new ShapeRange([d.id, r.id]).SetDimensionStyle({precision: 3, font: 'Arial'}));",
    );
    assert_eq!(out, vec!["300 null 1"]);
    let d = &dimensions(&app)[0];
    assert_eq!(d.style.text_size, 300.0);
    assert_eq!(d.style.precision, 3);
    assert_eq!(d.style.font, "Arial");
    assert!(run(&mut app, "ActiveSelection.SetDimensionStyle({label_size: 3});").is_err());

    let mut app = App::headless();
    let out = run_vgcore(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const d = ActiveLayer.CreateLinearDimension(cdrDimensionHorizontal, [0, 0], [40, 30]);
         ActiveSelection.LabelSize = 24;
         print(d.LabelSize);",
        "Macro",
    )
    .unwrap();
    assert_eq!(out, vec!["24"]);
    assert_eq!(dimensions(&app)[0].style.text_size, 24.0);
}

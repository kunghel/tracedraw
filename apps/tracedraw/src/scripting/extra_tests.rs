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

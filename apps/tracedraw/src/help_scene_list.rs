//! The scenes the Help pages show (see `help_scenes.rs`): one drawing
//! and the steps taken on it for each picture. Named after the Help
//! topic, with `-1`, `-2` for the steps of one section.

use crate::app::App;
use crate::help_scenes::{add, ellipse, line, rect, solid, Crop, Scene, Step};
use crate::tools::Tool;
use tracedraw_core::geometry::{Affine, BezPath, Point, Rect, Size};
use tracedraw_core::live::Effect;
use tracedraw_core::style::{Fountain, FountainKind, Stop, Texture, TextureKind};
use tracedraw_core::{Color, Command, CurvePreset, Fill, ShapeId, ShapeKind, Stroke, TextSpan};

/// Three overlapping objects, all selected: a square and two circles.
fn three_overlapping(app: &mut App) -> Vec<ShapeId> {
    let a = add(
        app,
        rect(20.0, 30.0, 80.0, 90.0),
        solid(70, 130, 220),
        line(0.5),
    );
    let b = add(
        app,
        ellipse(55.0, 45.0, 115.0, 105.0),
        solid(240, 170, 40),
        line(0.5),
    );
    let c = add(
        app,
        ellipse(40.0, 5.0, 100.0, 65.0),
        solid(90, 190, 120),
        line(0.5),
    );
    app.select(vec![a, b, c]);
    vec![a, b, c]
}

/// The selected objects ungrouped and pushed away from their middle by
/// `k` of their distance, so each piece shows on its own.
fn explode(app: &mut App, k: f64) {
    app.ungroup_selection();
    let ids = app.selection.clone();
    let Some(all) = app.selection_bounds() else {
        return;
    };
    let c = all.center();
    for id in &ids {
        let Some(s) = app.doc().find_shape(*id) else {
            continue;
        };
        let d = (s.bounds().center() - c) * k;
        app.run(Command::TransformShapes {
            shapes: vec![*id],
            transform: Affine::translate(d),
        });
    }
    app.select(ids);
}

fn curve(points: &[(f64, f64)], closed: bool) -> ShapeKind {
    let mut p = BezPath::new();
    for (i, q) in points.iter().enumerate() {
        if i == 0 {
            p.move_to(*q);
        } else {
            p.line_to(*q);
        }
    }
    if closed {
        p.close_path();
    }
    ShapeKind::Path { path: p, closed }
}

/// A smooth wave from `x0` to `x1` round height `y`.
fn wave(x0: f64, x1: f64, y: f64, amp: f64) -> ShapeKind {
    let mut p = BezPath::new();
    p.move_to((x0, y));
    let n = 4;
    let w = (x1 - x0) / n as f64;
    for i in 0..n {
        let a = x0 + w * i as f64;
        let s = if i % 2 == 0 { 1.0 } else { -1.0 };
        p.curve_to(
            (a + w * 0.35, y + amp * s),
            (a + w * 0.65, y + amp * s),
            (a + w, y),
        );
    }
    ShapeKind::Path {
        path: p,
        closed: false,
    }
}

fn text(app: &mut App, s: &str, x: f64, y: f64, size: f64, color: Color) -> ShapeId {
    let span = TextSpan::new(s, "Selawik", size);
    add(
        app,
        ShapeKind::Text {
            spans: vec![span],
            origin: Point::new(x, y),
            frame: None,
            align: Default::default(),
            para: Default::default(),
            on_path: None,
        },
        Fill::Solid(color),
        None,
    )
}

fn set_effects(app: &mut App, id: ShapeId, effects: Vec<Effect>) {
    app.run(Command::SetEffects { shape: id, effects });
}

/// The view framed on `r` (page millimetres) instead of the drawing.
fn frame_on(app: &mut App, r: Rect) {
    app.view.fit(r, app.canvas_rect);
}

fn mesh_envelope(app: &mut App) {
    let mut f = Fountain::two(
        FountainKind::Linear,
        Color::rgb8(250, 90, 60),
        Color::rgb8(250, 200, 60),
        0.0,
    );
    f.angle = 0.0;
    let id = add(
        app,
        rect(20.0, 20.0, 100.0, 60.0),
        Fill::Fountain(f),
        line(0.5),
    );
    let t = text(app, "MESH", 36.0, 32.0, 48.0, Color::rgb8(40, 40, 90));
    app.select(vec![id, t]);
    app.group_selection();
    app.envelope_mesh = (4, 2);
    app.add_mesh_envelope();
    let Some(gid) = app.selection.first().copied() else {
        return;
    };
    let Some(mut effects) = app.doc().find_shape(gid).map(|s| s.effects.clone()) else {
        return;
    };
    if let Some(Effect::MeshEnvelope { points, .. }) = effects.last_mut() {
        // Five points a row, three rows: the top middle up, the bottom
        // middle up, the right column out.
        if points.len() == 15 {
            points[12].y += 14.0;
            points[2].y += 10.0;
            points[4].x += 8.0;
            points[14].x += 8.0;
        }
    }
    set_effects(app, gid, effects);
    app.set_tool(Tool::Envelope);
}

/// The Shape Generator panel open on `kind`, its default settings, and
/// what Create made from them in the drawing window. The generators that
/// fill an outline or read a picture's tones take a selected shape as
/// their source first (Use Selection); the tone ones then leave out the
/// source, so the marks show alone. Slice cuts a selected badge.
fn shape_generator(app: &mut App, kind: crate::shape_gen::GenKind) {
    use crate::shape_gen::GenKind;
    open_shape_generator(app, kind);
    match kind {
        GenKind::SliceGrid => {
            let a = add(
                app,
                rect(20.0, 20.0, 120.0, 80.0),
                solid(240, 170, 40),
                None,
            );
            let t = text(app, "SLICE", 33.0, 38.0, 72.0, Color::rgb8(40, 40, 90));
            app.select(vec![a, t]);
            app.shape_gen_slice_selection();
        }
        k if k.uses_source() => {
            let source = if k.uses_tone() {
                let f = Fountain::two(FountainKind::Radial, Color::WHITE, Color::BLACK, 0.0);
                add(
                    app,
                    ellipse(20.0, 20.0, 100.0, 100.0),
                    Fill::Fountain(f),
                    None,
                )
            } else {
                add(
                    app,
                    ellipse(20.0, 20.0, 100.0, 100.0),
                    solid(230, 236, 250),
                    line(0.4),
                )
            };
            app.select(vec![source]);
            app.shape_gen_use_selection();
            app.shape_gen_create();
            if matches!(k, GenKind::Stipple | GenKind::Halftone) {
                app.run(Command::DeleteShapes {
                    shapes: vec![source],
                });
            }
        }
        _ => {
            app.shape_gen_create();
        }
    }
}

/// The Shape Generator panel open on `kind` with its first settings.
fn open_shape_generator(app: &mut App, kind: crate::shape_gen::GenKind) {
    app.shape_gen = Default::default();
    app.shape_gen.kind = kind;
    app.show_dockers = true;
    let tab = crate::app::DockerTab::ShapeGenerator;
    if !app.open_dockers.contains(&tab) {
        app.open_dockers.push(tab);
    }
    app.docker_tab = tab;
}

/// A window tall enough to show the whole Shape Generator panel, its
/// preview and its buttons.
const TALL: usize = 1080;

/// A Shape Generator scene for each generator, named after its section.
macro_rules! shape_gen_scene {
    ($name:literal, $kind:ident) => {
        Scene {
            name: concat!("objects-shape-generator-", $name),
            ready: Some(|app, _| shape_generator(app, crate::shape_gen::GenKind::$kind)),
            crop: Crop::Window,
            ..Default::default()
        }
    };
}

pub(crate) fn all() -> Vec<Scene> {
    vec![
        // How to use the Shape Generator, step by step: the menu, the
        // generator list, a setting changed with the pointer on Create,
        // what Create made, and the made group changed with Update.
        Scene {
            name: "objects-shape-generator-use-1",
            fit: false,
            steps: &[
                Step::Click(166.0, 12.0),
                Step::Hover(196.0, 66.0),
                Step::Hover(470.0, 66.0),
                Step::Hover(548.0, 144.0),
            ],
            crop: Crop::Window,
            ..Default::default()
        },
        Scene {
            name: "objects-shape-generator-use-2",
            ready: Some(|app, _| open_shape_generator(app, crate::shape_gen::GenKind::DotGrid)),
            fit: false,
            steps: &[Step::Click(1045.0, 158.0), Step::Hover(990.0, 186.0)],
            crop: Crop::Window,
            height: TALL,
            ..Default::default()
        },
        Scene {
            name: "objects-shape-generator-use-3",
            ready: Some(|app, _| {
                open_shape_generator(app, crate::shape_gen::GenKind::DotGrid);
                app.shape_gen.cell_shape = crate::shape_gen::CellShape::Square;
            }),
            fit: false,
            steps: &[Step::Hover(1003.0, 951.0)],
            crop: Crop::Window,
            height: TALL,
            ..Default::default()
        },
        Scene {
            name: "objects-shape-generator-use-4",
            ready: Some(|app, _| {
                open_shape_generator(app, crate::shape_gen::GenKind::DotGrid);
                app.shape_gen.cell_shape = crate::shape_gen::CellShape::Square;
            }),
            fit: false,
            steps: &[Step::Click(1003.0, 951.0), Step::Hover(700.0, 960.0)],
            crop: Crop::Window,
            height: TALL,
            ..Default::default()
        },
        Scene {
            name: "objects-shape-generator-use-5",
            ready: Some(|app, _| {
                open_shape_generator(app, crate::shape_gen::GenKind::DotGrid);
                app.shape_gen.cell_shape = crate::shape_gen::CellShape::Square;
                app.shape_gen_create();
                app.shape_gen.cell_shape = crate::shape_gen::CellShape::Diamond;
            }),
            fit: false,
            steps: &[Step::Hover(900.0, 986.0)],
            crop: Crop::Window,
            height: TALL,
            ..Default::default()
        },
        shape_gen_scene!("dot-grid", DotGrid),
        shape_gen_scene!("truchet", Truchet),
        shape_gen_scene!("tessellation", Tessellation),
        shape_gen_scene!("line-waves", LineWaves),
        shape_gen_scene!("rings", Rings),
        shape_gen_scene!("field-lines", FieldLines),
        shape_gen_scene!("iso-blocks", IsoBlocks),
        shape_gen_scene!("cone", ConeTemplate),
        shape_gen_scene!("voronoi", Voronoi),
        shape_gen_scene!("stipple", Stipple),
        shape_gen_scene!("halftone", Halftone),
        shape_gen_scene!("spirograph", Spirograph),
        shape_gen_scene!("guilloche", Guilloche),
        shape_gen_scene!("fractal", Fractal),
        shape_gen_scene!("tiling", Tiling),
        shape_gen_scene!("hatch", Hatch),
        shape_gen_scene!("contour", Contour),
        shape_gen_scene!("maze", Maze),
        shape_gen_scene!("space-curve", SpaceCurve),
        shape_gen_scene!("slice", SliceGrid),
        // Shape Builder: the part under the pointer is hatched.
        Scene {
            name: "lines-shapes-cut-and-erase-shape-builder-1",
            setup: |app| {
                three_overlapping(app);
                app.set_tool(Tool::ShapeBuilder);
            },
            steps: &[Step::Move(67.0, 57.0)],
            ..Default::default()
        },
        // A drag across three parts, held before the button comes up.
        Scene {
            name: "lines-shapes-cut-and-erase-shape-builder-2",
            setup: |app| {
                three_overlapping(app);
                app.set_tool(Tool::ShapeBuilder);
            },
            steps: &[
                Step::Press(67.0, 57.0),
                Step::Drag(75.0, 70.0),
                Step::Drag(85.0, 75.0),
                Step::Drag(95.0, 80.0),
                Step::Drag(105.0, 85.0),
            ],
            ..Default::default()
        },
        // The parts crossed are one new object.
        Scene {
            name: "lines-shapes-cut-and-erase-shape-builder-3",
            setup: |app| {
                three_overlapping(app);
                app.set_tool(Tool::ShapeBuilder);
            },
            steps: &[
                Step::Press(67.0, 57.0),
                Step::Drag(75.0, 70.0),
                Step::Drag(85.0, 75.0),
                Step::Drag(95.0, 80.0),
                Step::Drag(105.0, 85.0),
                Step::Release(105.0, 85.0),
                Step::Move(140.0, 20.0),
            ],
            ..Default::default()
        },
        // Combine Shapes > Break into Pieces, the pieces pulled apart.
        Scene {
            name: "lines-shapes-shaping-pieces-1",
            setup: |app| {
                three_overlapping(app);
                app.pieces(crate::pieces::Pieces::Break);
                explode(app, 0.35);
            },
            ..Default::default()
        },
        // Combine Shapes > Piece Outlines.
        Scene {
            name: "lines-shapes-shaping-pieces-2",
            setup: |app| {
                three_overlapping(app);
                app.pieces(crate::pieces::Pieces::Outlines);
                app.select(Vec::new());
            },
            ..Default::default()
        },
        // Combine Shapes > Cut into Grid, its window open.
        Scene {
            name: "lines-shapes-shaping-cutting",
            setup: |app| {
                let id = add(
                    app,
                    ellipse(20.0, 20.0, 110.0, 90.0),
                    solid(200, 70, 120),
                    line(0.5),
                );
                app.select(vec![id]);
                app.grid_cut = crate::cut_tools::GridCut {
                    open: true,
                    rows: 3,
                    cols: 4,
                    gap_x: 3.0,
                    gap_y: 3.0,
                    ..Default::default()
                };
            },
            crop: Crop::Window,
            ..Default::default()
        },
        // Curvature Pen: four clicks, the pointer showing the next part.
        Scene {
            name: "lines-shapes-lines-curvature-pen",
            setup: |app| app.set_tool(Tool::CurvaturePen),
            fit: false,
            ready: Some(|app, _| frame_on(app, Rect::new(0.0, 0.0, 150.0, 100.0))),
            steps: &[
                Step::Press(20.0, 30.0),
                Step::Release(20.0, 30.0),
                Step::Press(50.0, 75.0),
                Step::Release(50.0, 75.0),
                Step::Press(80.0, 35.0),
                Step::Release(80.0, 35.0),
                Step::Press(110.0, 70.0),
                Step::Release(110.0, 70.0),
                Step::Move(132.0, 40.0),
            ],
            ..Default::default()
        },
        // A mesh envelope bent by its grid points, the Envelope tool on.
        Scene {
            name: "effects-envelope-mesh",
            setup: mesh_envelope,
            ..Default::default()
        },
        // Layer effects: Halftone in CMYK ink, one colour and lines.
        Scene {
            name: "effects-effects-docker-halftone",
            setup: |app| {
                let ball = |c: Color| {
                    let mut f = Fountain::two(FountainKind::Radial, Color::WHITE, c, 0.0);
                    f.angle = 0.0;
                    Fill::Fountain(f)
                };
                let ht = |pattern: u8, mode: u8, color: Color| Effect::Halftone {
                    pattern,
                    mode,
                    size: 2.4,
                    scale: 1.0,
                    softness: 0.0,
                    angle: 45.0,
                    center_x: 0.5,
                    center_y: 0.5,
                    clip: true,
                    color,
                };
                let a = add(
                    app,
                    ellipse(10.0, 10.0, 60.0, 60.0),
                    ball(Color::rgb8(220, 40, 90)),
                    None,
                );
                set_effects(app, a, vec![ht(0, 1, Color::BLACK)]);
                let b = add(
                    app,
                    ellipse(70.0, 10.0, 120.0, 60.0),
                    ball(Color::rgb8(30, 60, 140)),
                    None,
                );
                set_effects(app, b, vec![ht(2, 2, Color::rgb8(30, 60, 140))]);
                let c = add(
                    app,
                    rect(130.0, 10.0, 180.0, 60.0),
                    ball(Color::rgb8(20, 20, 20)),
                    None,
                );
                set_effects(app, c, vec![ht(3, 2, Color::rgb8(20, 20, 20))]);
                app.select(vec![a]);
            },
            ..Default::default()
        },
        // Layer effects: rough edge, sketch fill, shape frame, trim
        // marks and soft edge.
        Scene {
            name: "effects-effects-docker-hand-made",
            setup: |app| {
                use tracedraw_core::rough::FrameForm;
                let blue = solid(70, 130, 220);
                let a = add(app, ellipse(5.0, 10.0, 45.0, 50.0), blue.clone(), line(0.6));
                set_effects(
                    app,
                    a,
                    vec![Effect::Roughen {
                        size: 1.6,
                        detail: 6,
                        smooth: false,
                        seed: 3,
                    }],
                );
                let b = add(
                    app,
                    ellipse(55.0, 10.0, 95.0, 50.0),
                    solid(230, 80, 60),
                    line(0.6),
                );
                set_effects(
                    app,
                    b,
                    vec![Effect::Scribble {
                        angle: 35.0,
                        spacing: 2.2,
                        width: 0.7,
                        loose: 1.0,
                        seed: 1,
                        curvy: 0.5,
                        overlap: 0.6,
                        spacing_var: 0.3,
                        width_var: 0.3,
                    }],
                );
                let c = add(
                    app,
                    rect(110.0, 15.0, 140.0, 45.0),
                    solid(90, 190, 120),
                    line(0.6),
                );
                set_effects(
                    app,
                    c,
                    vec![Effect::ShapeFrame {
                        form: FrameForm::Rounded,
                        pad: 4.0,
                        radius: 4.0,
                    }],
                );
                let d = add(
                    app,
                    rect(160.0, 15.0, 190.0, 45.0),
                    solid(240, 170, 40),
                    None,
                );
                set_effects(
                    app,
                    d,
                    vec![Effect::TrimMarks {
                        length: 5.0,
                        gap: 2.0,
                        width: 0.3,
                    }],
                );
                let e = add(
                    app,
                    ellipse(205.0, 10.0, 245.0, 50.0),
                    solid(140, 70, 200),
                    None,
                );
                set_effects(app, e, vec![Effect::Feather { radius: 6.0 }]);
                app.select(Vec::new());
            },
            ..Default::default()
        },
        // A colour points gradient, its points on the canvas.
        Scene {
            name: "color-fills-color-points",
            setup: |app| {
                let mut f = Fountain::two(
                    FountainKind::Points,
                    Color::rgb8(250, 70, 90),
                    Color::rgb8(60, 120, 250),
                    0.0,
                );
                f.stops.push(Stop::new(0.5, Color::rgb8(250, 220, 60)));
                f.stops.push(Stop::new(0.75, Color::rgb8(60, 200, 140)));
                f.points = vec![
                    Point::new(-0.8, 0.7),
                    Point::new(0.8, -0.6),
                    Point::new(0.6, 0.8),
                    Point::new(-0.6, -0.7),
                ];
                let id = add(app, rect(20.0, 20.0, 110.0, 80.0), Fill::Fountain(f), None);
                app.select(vec![id]);
                app.set_tool(Tool::InteractiveFill);
            },
            ..Default::default()
        },
        // The two holographic textures.
        Scene {
            name: "color-fills-holographic",
            setup: |app| {
                let holo = |kind| {
                    Fill::Texture(Texture {
                        kind,
                        color_a: Color::rgb8(40, 40, 60),
                        color_b: Color::rgb8(220, 220, 230),
                        scale: 8.0,
                        seed: 7,
                    })
                };
                let _ = add(
                    app,
                    ellipse(10.0, 10.0, 80.0, 60.0),
                    holo(TextureKind::Holographic),
                    None,
                );
                let _ = add(
                    app,
                    rect(95.0, 10.0, 165.0, 60.0),
                    holo(TextureKind::HoloGlitter),
                    None,
                );
                app.select(Vec::new());
            },
            ..Default::default()
        },
        // Blend: made with the Blend tool, then bent by its round handle.
        Scene {
            name: "effects-blend-bend",
            setup: |app| {
                let _ = add(
                    app,
                    ellipse(10.0, 20.0, 30.0, 40.0),
                    solid(240, 60, 80),
                    line(0.4),
                );
                let _ = add(
                    app,
                    rect(130.0, 15.0, 160.0, 45.0),
                    solid(40, 110, 230),
                    line(0.4),
                );
                app.select(Vec::new());
                app.set_tool(Tool::Blend);
            },
            fit: false,
            ready: Some(|app, _| frame_on(app, Rect::new(0.0, -20.0, 170.0, 80.0))),
            steps: &[
                Step::Press(20.0, 30.0),
                Step::Drag(60.0, 30.0),
                Step::Drag(110.0, 30.0),
                Step::Drag(145.0, 30.0),
                Step::Release(145.0, 30.0),
            ],
            after: Some(|app| {
                if let Some(c) = app.blend_controls() {
                    let at = c.bend_point() + tracedraw_core::geometry::Vec2::new(0.0, 30.0);
                    app.drag_blend_handle_to(crate::blend_tool::BlendHandle::Bend, at, false);
                }
            }),
            ..Default::default()
        },
        // Curve Text: circle, arch, bowl and wave from the text bar.
        Scene {
            name: "text-adding-text-curve-text",
            setup: |app| {
                let ink = Color::rgb8(40, 50, 110);
                let mut last = None;
                for (k, x, y) in [
                    (CurvePreset::Circle, 10.0, 60.0),
                    (CurvePreset::ArcUp, 90.0, 70.0),
                    (CurvePreset::ArcDown, 90.0, 30.0),
                    (CurvePreset::Wave, 10.0, 10.0),
                ] {
                    let id = text(app, "TRACEDRAW CURVE TEXT", x, y, 14.0, ink);
                    app.select(vec![id]);
                    app.set_text_curve(Some(k));
                    last = Some(id);
                }
                app.select(last.into_iter().collect());
            },
            ..Default::default()
        },
        // Text on a path: radial letters round a dial.
        Scene {
            name: "text-adding-text-radial",
            setup: |app| {
                let id = text(
                    app,
                    "6 7 8 9 10 11 12 1 2 3 4 5",
                    40.0,
                    40.0,
                    14.0,
                    Color::rgb8(30, 30, 30),
                );
                app.select(vec![id]);
                app.set_text_curve(Some(CurvePreset::Circle));
                if let Some(s) = app.doc().find_shape(id) {
                    let mut kind = s.kind.clone();
                    if let ShapeKind::Text {
                        on_path: Some(p), ..
                    } = &mut kind
                    {
                        p.orientation = tracedraw_core::PathTextOrientation::Radial;
                    }
                    app.run(Command::SetShapeKind { shape: id, kind });
                }
            },
            ..Default::default()
        },
        // Paragraph text set as a type block.
        Scene {
            name: "text-adding-text-type-block",
            setup: |app| {
                let mut para = tracedraw_core::ParagraphStyle::default();
                para.type_block = true;
                let span = TextSpan::new("SMALL STEPS EVERY DAY MAKE BIG IDEAS", "Selawik", 18.0);
                let id = add(
                    app,
                    ShapeKind::Text {
                        spans: vec![span],
                        origin: Point::new(20.0, 110.0),
                        frame: Some(Size::new(70.0, 90.0)),
                        align: Default::default(),
                        para,
                        on_path: None,
                    },
                    Fill::Solid(Color::rgb8(30, 30, 60)),
                    None,
                );
                app.select(vec![id]);
            },
            ..Default::default()
        },
        // Fit Objects to Path, Live: the objects stay on the wave.
        Scene {
            name: "objects-fit-objects-to-path-live",
            setup: |app| {
                let mut ids = Vec::new();
                for (i, c) in [
                    (240, 80, 80),
                    (240, 170, 40),
                    (90, 190, 120),
                    (60, 140, 230),
                    (150, 90, 210),
                    (240, 80, 80),
                ]
                .iter()
                .enumerate()
                {
                    let x = 10.0 + i as f64 * 12.0;
                    ids.push(add(
                        app,
                        ellipse(x, 80.0, x + 9.0, 89.0),
                        solid(c.0, c.1, c.2),
                        None,
                    ));
                }
                let path = add(app, wave(10.0, 150.0, 40.0, 25.0), Fill::None, line(0.4));
                ids.push(path);
                app.select(ids);
                app.fit_path_follow_selection();
                app.fit_path.live = true;
                app.fit_path_edited = true;
                app.fit_objects_to_path();
                app.refresh_live_fits();
                app.select(vec![path]);
            },
            ..Default::default()
        },
        // Symmetry, Rotational: six copies turned round the centre.
        Scene {
            name: "objects-symmetry-rotational",
            setup: |app| {
                let id = add(
                    app,
                    ellipse(46.0, 52.0, 58.0, 90.0),
                    solid(90, 170, 220),
                    line(0.5),
                );
                set_effects(
                    app,
                    id,
                    vec![Effect::Symmetry {
                        center: Point::new(52.0, 50.0),
                        angle: 90.0,
                        lines: 6,
                        rotate: true,
                    }],
                );
                app.select(vec![id]);
            },
            ..Default::default()
        },
        // Object > Insert > Light Flare on a dark page.
        Scene {
            name: "objects-light-flare-insert",
            setup: |app| {
                let page = app.page_rect();
                let c = page.center();
                let _ = add(
                    app,
                    rect(c.x - 55.0, c.y - 75.0, c.x + 90.0, c.y + 45.0),
                    solid(14, 22, 48),
                    None,
                );
                let st = crate::ui::flare::Flare::default();
                let _ = app.insert_flare(&st);
                app.select(Vec::new());
            },
            crop: Crop::Canvas,
            ..Default::default()
        },
        // View > Separations: one ink of the page shown alone.
        Scene {
            name: "printing-print-view-separations",
            setup: |app| {
                let c = |c, m, y, k| Fill::Solid(Color::cmyk_pct(c, m, y, k));
                // Orange and purple carry magenta, the green none.
                let _ = add(
                    app,
                    ellipse(10.0, 10.0, 60.0, 60.0),
                    c(0.0, 60.0, 100.0, 0.0),
                    None,
                );
                let _ = add(
                    app,
                    ellipse(70.0, 10.0, 120.0, 60.0),
                    c(60.0, 90.0, 0.0, 0.0),
                    None,
                );
                let _ = add(
                    app,
                    ellipse(130.0, 10.0, 180.0, 60.0),
                    c(80.0, 0.0, 100.0, 0.0),
                    None,
                );
                let _ = text(
                    app,
                    "Magenta plate",
                    10.0,
                    68.0,
                    28.0,
                    Color::cmyk_pct(0.0, 100.0, 0.0, 0.0),
                );
                app.select(Vec::new());
                app.set_view_plate(Some(1));
            },
            ..Default::default()
        },
        // An outline whose gradient runs along the line.
        Scene {
            name: "lines-shapes-outlines-along-the-line",
            setup: |app| {
                let f = Fountain::two(
                    FountainKind::Linear,
                    Color::rgb8(250, 60, 90),
                    Color::rgb8(40, 120, 250),
                    0.0,
                );
                let mut s = Stroke::new(Color::BLACK, 5.0);
                s.fill = Some(Box::new(Fill::Fountain(f)));
                s.fill_along = true;
                s.cap = tracedraw_core::LineCap::Round;
                let id = add(
                    app,
                    curve(
                        &[
                            (20.0, 20.0),
                            (120.0, 20.0),
                            (120.0, 80.0),
                            (40.0, 80.0),
                            (40.0, 40.0),
                            (100.0, 40.0),
                            (100.0, 60.0),
                        ],
                        false,
                    ),
                    Fill::None,
                    Some(s),
                );
                app.select(vec![id]);
            },
            ..Default::default()
        },
    ]
}

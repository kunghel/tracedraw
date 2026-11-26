//! Math Shapes: the plane, the curves, the figures and the group in the
//! drawing.

use crate::math_shapes::*;
use kurbo::{Point, Rect, Shape as _};
use tracedraw_core::ShapeKind;

fn graph(plots: Vec<Plot>) -> MathSettings {
    let mut s = MathSettings::default();
    apply_preset(&mut s, Preset::EmptyPlane);
    s.plots = plots;
    s
}

fn func(f: &str) -> Plot {
    Plot {
        f: f.into(),
        ..Plot::default()
    }
}

fn curve_points(d: &Drawn, i: usize) -> Vec<Point> {
    let mut pts = Vec::new();
    for it in &d.items {
        if it.part == Part::Plot(i) && it.line.is_some() {
            for el in it.path.elements() {
                if let Some(p) = el.end_point() {
                    pts.push(p);
                }
            }
        }
    }
    pts
}

#[test]
fn the_plane_has_its_grid_axes_ticks_and_numbers() {
    let mut s = graph(Vec::new());
    s.plane.x_min = -5.0;
    s.plane.x_max = 5.0;
    s.plane.y_min = -3.0;
    s.plane.y_max = 3.0;
    s.plane.unit_x = 10.0;
    s.plane.unit_y = 10.0;
    s.plane.grid_x = 1.0;
    s.plane.grid_y = 1.0;
    s.plane.tick_x = 1.0;
    s.plane.tick_y = 1.0;
    s.plane.minor = false;
    let d = draw(&s);
    assert_eq!(d.frame, Some(Rect::new(0.0, 0.0, 100.0, 60.0)));
    // 11 vertical and 7 level grid lines, two points each.
    let grid = d
        .items
        .iter()
        .find(|i| i.part == Part::Grid)
        .expect("grid");
    assert_eq!(grid.path.elements().len(), (11 + 7) * 2);
    // The axes cross at the origin, (50, 30) mm.
    let axes: Vec<&Item> = d.items.iter().filter(|i| i.part == Part::Axes).collect();
    assert!(axes.len() >= 4, "two lines, two arrow heads, ticks");
    let ys: Vec<f64> = axes[0].path.elements().iter().filter_map(|e| e.end_point()).map(|p| p.y).collect();
    assert!(ys.iter().all(|y| (y - 30.0).abs() < 1e-9));
    // Numbers -5..5 without 0 and the end under the arrow, -3..3 likewise,
    // and one 0 at the origin.
    let nums: Vec<&str> = d
        .labels
        .iter()
        .filter(|l| l.part == Part::Numbers)
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(nums.len(), 9 + 5 + 1, "{nums:?}");
    assert!(nums.contains(&"-5") && nums.contains(&"2") && !nums.contains(&"5"));
    assert_eq!(nums.iter().filter(|t| **t == "0").count(), 1);
    // Axis names.
    assert!(d.labels.iter().any(|l| l.text == "x") && d.labels.iter().any(|l| l.text == "y"));
}

#[test]
fn axes_stay_at_the_edge_when_zero_is_out_of_range() {
    let mut s = graph(Vec::new());
    s.plane.x_min = 2.0;
    s.plane.x_max = 8.0;
    s.plane.y_min = 1.0;
    s.plane.y_max = 5.0;
    let d = draw(&s);
    let axis = d.items.iter().find(|i| i.part == Part::Axes).expect("axis");
    let p = axis.path.elements()[0].end_point().expect("start");
    assert!((p.y - 0.0).abs() < 1e-9, "the x axis on the bottom edge");
    assert!(!d.labels.iter().any(|l| l.part == Part::Numbers && l.text == "0"));
}

#[test]
fn numbers_in_multiples_of_pi() {
    assert_eq!(fmt_pi(std::f64::consts::PI), "π");
    assert_eq!(fmt_pi(-std::f64::consts::PI / 2.0), "-π/2");
    assert_eq!(fmt_pi(1.5 * std::f64::consts::PI), "3π/2");
    assert_eq!(fmt_pi(2.0 * std::f64::consts::PI), "2π");
    assert_eq!(fmt_pi(0.0), "0");
    assert_eq!(fmt_num(0.30000000000000004, 3), "0.3");
    assert_eq!(fmt_num(-0.0001, 2), "0");
    let mut s = MathSettings::default();
    apply_preset(&mut s, Preset::SineCosine);
    let d = draw(&s);
    assert!(d.labels.iter().any(|l| l.text == "π/2"));
    assert!(d.labels.iter().any(|l| l.text == "-2π"));
}

#[test]
fn a_function_follows_its_formula_inside_the_plane() {
    let s = graph(vec![func("x^2 - 2x - 3")]);
    let d = draw(&s);
    assert!(d.errors.is_empty());
    let m = Map::of(&s.plane);
    let pts = curve_points(&d, 0);
    assert!(pts.len() > 20);
    let frame = d.frame.expect("frame");
    for p in &pts {
        assert!(frame.inflate(1e-6, 1e-6).contains(*p), "{p:?}");
        let x = p.x / m.ux + m.x_min;
        let y = p.y / m.uy + m.y_min;
        // On the curve (points on the plane's edge are where it was cut).
        if p.y > 1e-6 && p.y < frame.y1 - 1e-6 {
            assert!((y - (x * x - 2.0 * x - 3.0)).abs() < 0.05, "{x} {y}");
        }
    }
}

#[test]
fn an_asymptote_breaks_the_curve() {
    let mut s = MathSettings::default();
    apply_preset(&mut s, Preset::Tangent);
    let d = draw(&s);
    let runs = d
        .items
        .iter()
        .filter(|i| i.part == Part::Plot(0) && i.line.is_some())
        .flat_map(|i| i.path.elements().iter().filter(|e| matches!(e, kurbo::PathEl::MoveTo(_))))
        .count();
    // tan on [-2π, 2π] has 4 asymptotes: 5 branches.
    assert_eq!(runs, 5);
    // 1/x: two branches, none joining across x = 0.
    let d = draw(&graph(vec![func("1/x")]));
    let n = d.items.iter().filter(|i| i.part == Part::Plot(0)).count();
    assert_eq!(n, 2);
}

#[test]
fn undefined_parts_are_left_out() {
    let d = draw(&graph(vec![func("sqrt(x)")]));
    let pts = curve_points(&d, 0);
    let x0 = d.frame.expect("frame").center().x;
    assert!(pts.iter().all(|p| p.x >= x0 - 1e-6));
    assert!(pts.iter().any(|p| (p.x - x0).abs() < 0.05), "starts at the origin");
}

#[test]
fn zeros_intercept_and_turning_points_are_marked() {
    let f = |x: f64| x * x - 2.0 * x - 3.0;
    let marks = function_marks(&f, -4.0, 6.0);
    let has = |x: f64, y: f64| marks.iter().any(|&(a, b)| (a - x).abs() < 1e-6 && (b - y).abs() < 1e-6);
    assert!(has(-1.0, 0.0) && has(3.0, 0.0), "{marks:?}");
    assert!(has(0.0, -3.0));
    assert!(has(1.0, -4.0));
    assert_eq!(marks.len(), 4, "{marks:?}");
    // A pole is not a zero.
    let g = |x: f64| 1.0 / x;
    assert!(function_marks(&g, -2.0, 2.0).iter().all(|&(_, y)| y != 0.0));
}

#[test]
fn polar_and_parametric_curves_close() {
    let mut s = MathSettings::default();
    apply_preset(&mut s, Preset::Rose);
    let d = draw(&s);
    assert!(d.errors.is_empty());
    let outline = d
        .items
        .iter()
        .find(|i| i.part == Part::Plot(0) && i.line.is_some())
        .expect("rose");
    assert!(outline.closed);
    // r = 4 cos(5θ): every point within 4 units (40 mm) of the origin.
    let m = Map::of(&s.plane);
    let o = m.pt(0.0, 0.0);
    for el in outline.path.elements() {
        if let Some(p) = el.end_point() {
            assert!(p.distance(o) <= 40.0 + 0.05);
        }
    }
    // Filled inside, once: five petals over a full turn are drawn twice,
    // and a fill of both rounds would leave them empty (even-odd).
    let fill = d
        .items
        .iter()
        .find(|i| i.part == Part::Plot(0) && i.fill.is_some())
        .expect("rose fill");
    let pts = |p: &kurbo::BezPath| p.elements().len();
    let (f, l) = (pts(&fill.path), pts(&outline.path));
    assert!(f * 10 < l * 7, "fill {f} points, outline {l}");
    assert!(fill.path.area().abs() > 100.0);
    // A figure eight that starts at its crossing keeps both loops.
    let eight: Vec<Point> = (0..=400)
        .map(|i| {
            let t = i as f64 / 400.0 * std::f64::consts::TAU;
            Point::new(10.0 * t.sin(), 10.0 * t.sin() * t.cos())
        })
        .collect();
    assert_eq!(first_return(&eight), Some(400));
    // An open curve has no inside.
    assert_eq!(first_return(&eight[..300]), None);
    apply_preset(&mut s, Preset::Lissajous);
    let d = draw(&s);
    assert!(d.items.iter().any(|i| i.part == Part::Plot(0) && i.closed));
}

#[test]
fn an_implicit_circle_is_found() {
    let s = graph(vec![Plot {
        kind: PlotKind::Implicit,
        f: "x^2 + y^2 = 16".into(),
        ..Plot::default()
    }]);
    let d = draw(&s);
    let m = Map::of(&s.plane);
    let o = m.pt(0.0, 0.0);
    let pts = curve_points(&d, 0);
    assert!(pts.len() > 50);
    for p in &pts {
        assert!((p.distance(o) - 4.0 * m.ux).abs() < 0.2, "{p:?}");
    }
    let closed = d.items.iter().filter(|i| i.part == Part::Plot(0) && i.closed).count();
    assert_eq!(closed, 1, "one closed loop");
    // A hyperbola: two branches, across no pole.
    let s = graph(vec![Plot {
        kind: PlotKind::Implicit,
        f: "x y = 4".into(),
        ..Plot::default()
    }]);
    let d = draw(&s);
    assert_eq!(d.items.iter().filter(|i| i.part == Part::Plot(0)).count(), 2);
}

#[test]
fn points_with_names_and_guides() {
    let pts = parse_points("A(1, 2); B(-3; 4) (pi, sqrt(4))", &[0.0; 8], false);
    assert_eq!(pts.len(), 3);
    assert_eq!(pts[0], ("A".to_string(), 1.0, 2.0));
    assert_eq!(pts[1], ("B".to_string(), -3.0, 4.0));
    assert!((pts[2].1 - std::f64::consts::PI).abs() < 1e-12 && pts[2].2 == 2.0);
    let mut s = MathSettings::default();
    apply_preset(&mut s, Preset::Points);
    let d = draw(&s);
    assert!(d.labels.iter().any(|l| l.text == "A(1; 1)"));
    assert!(d.items.iter().any(|i| i.part == Part::Plot(0) && i.line.as_ref().is_some_and(|l| l.dash == Dash::Dashed)));
}

#[test]
fn bad_formulas_are_reported_not_drawn() {
    let d = draw(&graph(vec![func("2x +"), func("sin(x)")]));
    assert_eq!(d.errors.len(), 1);
    assert_eq!(d.errors[0].0, 0);
    assert!(curve_points(&d, 0).is_empty());
    assert!(!curve_points(&d, 1).is_empty());
}

#[test]
fn parameters_change_the_curve() {
    let mut s = graph(vec![func("a x + b")]);
    s.params = [2.0, 1.0, 0.0, 0.0, 0.0];
    let m = Map::of(&s.plane);
    let d = draw(&s);
    let p = curve_points(&d, 0)
        .into_iter()
        .min_by(|a, b| (a.x - m.x(1.0)).abs().total_cmp(&(b.x - m.x(1.0)).abs()))
        .expect("point");
    assert!((p.y / m.uy + m.y_min - 3.0).abs() < 0.05);
}

#[test]
fn degrees_turn_the_trigonometry() {
    let mut s = graph(vec![func("sin(x)")]);
    s.plane.x_min = 0.0;
    s.plane.x_max = 360.0;
    s.plane.unit_x = 0.3;
    s.plane.grid_x = 30.0;
    s.plane.tick_x = 90.0;
    s.plane.y_min = -1.5;
    s.plane.y_max = 1.5;
    s.degrees = true;
    let d = draw(&s);
    let m = Map::of(&s.plane);
    let top = curve_points(&d, 0)
        .into_iter()
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("top");
    assert!(((top.x / m.ux + m.x_min) - 90.0).abs() < 1.0);
}

#[test]
fn every_preset_draws_without_errors() {
    for p in Preset::ALL {
        let mut s = MathSettings::default();
        apply_preset(&mut s, p);
        let d = draw(&s);
        assert!(d.errors.is_empty(), "{p:?}: {:?}", d.errors);
        assert!(d.bounds().is_some(), "{p:?}");
        if p != Preset::EmptyPlane {
            assert!(
                d.items.iter().any(|i| matches!(i.part, Part::Plot(_))),
                "{p:?} draws a curve"
            );
        }
    }
}

#[test]
fn triangles_from_each_kind_of_measure() {
    let f = |by: TriBy| Figure {
        kind: FigureKind::Triangle,
        tri_by: by,
        side_a: 3.0,
        side_b: 4.0,
        side_c: 5.0,
        angle_a: 60.0,
        angle_b: 60.0,
        ..Figure::default()
    };
    let t = solve_triangle(&f(TriBy::Sss)).expect("3 4 5");
    assert!((t.angles[2] - 90.0).abs() < 1e-9);
    assert!((t.area() - 6.0).abs() < 1e-9);
    assert!((t.a.distance(t.b) - 5.0).abs() < 1e-9);
    assert!((t.b.distance(t.c) - 3.0).abs() < 1e-9);
    assert!((t.c.distance(t.a) - 4.0).abs() < 1e-9);
    let r = solve_triangle(&f(TriBy::Right)).expect("right");
    assert!((r.sides[2] - 5.0).abs() < 1e-9 && (r.angles[2] - 90.0).abs() < 1e-9);
    // SAS: b = 4, c = 5, A = 60°: a² = 16 + 25 - 20 = 21.
    let sas = solve_triangle(&f(TriBy::Sas)).expect("sas");
    assert!((sas.sides[0] - 21f64.sqrt()).abs() < 1e-9);
    // ASA: A = B = 60°, c = 5: equilateral.
    let asa = solve_triangle(&f(TriBy::Asa)).expect("asa");
    assert!(asa.sides.iter().all(|s| (s - 5.0).abs() < 1e-9));
    let eq = solve_triangle(&f(TriBy::Equilateral)).expect("eq");
    assert!(eq.angles.iter().all(|a| (a - 60.0).abs() < 1e-9));
    let iso = solve_triangle(&f(TriBy::Isosceles)).expect("iso");
    assert!((iso.angles[0] - iso.angles[1]).abs() < 1e-9);
    // Sides that cannot close, angles over 180°.
    let bad = Figure {
        side_a: 1.0,
        side_b: 2.0,
        side_c: 5.0,
        ..f(TriBy::Sss)
    };
    assert!(solve_triangle(&bad).is_none());
    let bad = Figure {
        angle_a: 120.0,
        angle_b: 70.0,
        ..f(TriBy::Asa)
    };
    assert!(solve_triangle(&bad).is_none());
    let s = MathSettings {
        mode: Mode::Figure,
        figure: bad,
        ..Default::default()
    };
    assert!(draw(&s).impossible);
    assert!(figure_facts(&s.figure).is_empty());
}

#[test]
fn a_triangle_with_everything_shown() {
    let fig = Figure {
        kind: FigureKind::Triangle,
        tri_by: TriBy::Sss,
        side_a: 30.0,
        side_b: 40.0,
        side_c: 50.0,
        heights: true,
        medians: true,
        bisectors: true,
        incircle: true,
        circumcircle: true,
        ..Figure::default()
    };
    let d = draw(&MathSettings {
        mode: Mode::Figure,
        figure: fig,
        ..Default::default()
    });
    assert!(!d.impossible);
    let texts: Vec<&str> = d.labels.iter().map(|l| l.text.as_str()).collect();
    for t in ["A", "B", "C", "a = 30 mm", "b = 40 mm", "c = 50 mm", "90°"] {
        assert!(texts.contains(&t), "{t} in {texts:?}");
    }
    // The triangle, then aids: 3 heights, 3 medians, 3 bisectors, two
    // circles with their centres; a right-angle square at C.
    assert_eq!(d.items.iter().filter(|i| i.part == Part::Figure).count(), 1);
    assert!(d.items.iter().filter(|i| i.part == Part::Aids).count() >= 9 + 4 + 3);
    // The circumcircle of a right triangle has the hypotenuse as diameter.
    let circles: Vec<f64> = d
        .items
        .iter()
        .filter(|i| i.part == Part::Aids && i.closed && i.line.is_some())
        .map(|i| i.path.bounding_box().width() / 2.0)
        .collect();
    assert!(circles.iter().any(|r| (r - 25.0).abs() < 0.01), "{circles:?}");
    // Inradius (a + b - c) / 2 = 10.
    assert!(circles.iter().any(|r| (r - 10.0).abs() < 0.01), "{circles:?}");
}

#[test]
fn circles_polygons_ellipses_and_arcs() {
    let mut f = Figure {
        kind: FigureKind::Circle,
        circle_by: CircleBy::Area,
        circle_value: std::f64::consts::PI * 400.0,
        ..Figure::default()
    };
    assert!((circle_radius(&f) - 20.0).abs() < 1e-9);
    f.circle_by = CircleBy::Circumference;
    f.circle_value = std::f64::consts::TAU * 15.0;
    assert!((circle_radius(&f) - 15.0).abs() < 1e-9);
    let d = draw(&MathSettings {
        mode: Mode::Figure,
        figure: f.clone(),
        ..Default::default()
    });
    let c = d.items.iter().find(|i| i.part == Part::Figure).expect("circle");
    assert!((c.path.bounding_box().width() - 30.0).abs() < 0.01);
    assert!(d.labels.iter().any(|l| l.text == "r = 15 mm"));
    // A hexagon of side 30 has circumradius 30, area 3√3/2 · 900.
    let hex = Figure {
        kind: FigureKind::Polygon,
        sides: 6,
        by_side: true,
        poly_value: 30.0,
        ..Figure::default()
    };
    assert!((polygon_radius(&hex) - 30.0).abs() < 1e-9);
    let facts = figure_facts(&hex);
    let area = facts.iter().find(|f| f.0 == "math.fact_area").expect("area").1;
    assert!((area - 1.5 * 3f64.sqrt() * 900.0).abs() < 1e-6);
    let interior = facts.iter().find(|f| f.0 == "math.fact_interior").expect("angle").1;
    assert!((interior - 120.0).abs() < 1e-9);
    // A five-pointed star {5/2} is one closed outline of 5 corners.
    let star = Figure {
        kind: FigureKind::Polygon,
        sides: 5,
        star: 2,
        ..Figure::default()
    };
    let d = draw(&MathSettings {
        mode: Mode::Figure,
        figure: star,
        ..Default::default()
    });
    let fig = d.items.iter().find(|i| i.part == Part::Figure).expect("star");
    assert_eq!(fig.path.elements().len(), 6);
    // Ellipse foci at ±c, c² = a² - b².
    let el = Figure {
        kind: FigureKind::Ellipse,
        rx: 50.0,
        ry: 30.0,
        foci: true,
        ..Figure::default()
    };
    let focal = figure_facts(&el).iter().find(|f| f.0 == "math.fact_focal").expect("c").1;
    assert!((focal - 40.0).abs() < 1e-9);
    // A quarter sector of radius 20.
    let arc = Figure {
        kind: FigureKind::Arc,
        arc_kind: ArcKind::Sector,
        arc_radius: 20.0,
        arc_start: 0.0,
        arc_sweep: 90.0,
        ..Figure::default()
    };
    let d = draw(&MathSettings {
        mode: Mode::Figure,
        figure: arc,
        ..Default::default()
    });
    let sec = d.items.iter().find(|i| i.part == Part::Figure).expect("sector");
    let b = sec.path.bounding_box();
    assert!(sec.closed && (b.width() - 20.0).abs() < 0.01 && (b.height() - 20.0).abs() < 0.01);
    assert!((sec.path.area().abs() - std::f64::consts::PI * 100.0).abs() < 0.1);
}

#[test]
fn nonsense_settings_are_kept_in_range() {
    let mut s = MathSettings::default();
    s.plane.x_min = f64::NAN;
    s.plane.x_max = f64::INFINITY;
    s.plane.unit_x = 0.0;
    s.plane.grid_x = 0.0;
    s.plane.tick_y = -3.0;
    s.plots = (0..40).map(Plot::nth).collect();
    s.plots[0].samples = 0;
    s.figure.sides = 0;
    s.figure.star = 99;
    let t = s.sanitized();
    assert!(t.plane.x_min.is_finite() && t.plane.x_max > t.plane.x_min);
    assert!(t.plane.unit_x > 0.0 && t.plane.grid_x > 0.0 && t.plane.tick_y > 0.0);
    assert_eq!(t.plots.len(), MAX_PLOTS);
    assert!(t.plots[0].samples >= 20);
    assert!(t.figure.sides >= 3 && t.figure.star <= t.figure.sides);
    // A tiny grid step does not make millions of lines.
    let mut s = graph(Vec::new());
    s.plane.grid_x = 1e-9;
    s.plane.grid_y = 1e-9;
    s.plane.minor = true;
    let d = draw(&s);
    let lines: usize = d
        .items
        .iter()
        .filter(|i| i.part == Part::Grid)
        .map(|i| i.path.elements().len() / 2)
        .sum();
    assert!(lines < 10_000, "{lines}");
}

#[test]
fn settings_survive_the_object_data() {
    let mut s = MathSettings::default();
    apply_preset(&mut s, Preset::Superformula);
    s.figure.kind = FigureKind::Arc;
    let json = serde_json::to_string(&s).expect("json");
    let back: MathSettings = serde_json::from_str(&json).expect("back");
    assert_eq!(back, s);
    // Older or partial data fills in the defaults.
    let partial: MathSettings = serde_json::from_str(r#"{"mode":"figure"}"#).expect("partial");
    assert_eq!(partial.mode, Mode::Figure);
    assert_eq!(partial.plane, MathSettings::default().plane);
}

#[test]
fn create_and_update_in_one_step_each() {
    let mut app = crate::app::App::headless();
    apply_preset(&mut app.math, Preset::Parabola);
    let depth = app.engine.history_labels().0.len();
    let id = app.math_create().expect("created");
    assert_eq!(app.engine.history_labels().0.len(), depth + 1);
    assert_eq!(app.selection, vec![id]);
    let g = app.doc().find_shape(id).expect("group").clone();
    assert!(g.data.iter().any(|(k, _)| k == DATA_KEY));
    let ShapeKind::Group { children } = &g.kind else {
        panic!("a group")
    };
    // Grid, axes, numbers and the curve each in a group of their own,
    // the numbers as text.
    let names: Vec<String> = children.iter().filter_map(|c| c.name.clone()).collect();
    assert!(names.len() >= 3, "{names:?}");
    fn texts(s: &tracedraw_core::Shape, n: &mut usize) {
        match &s.kind {
            ShapeKind::Text { .. } => *n += 1,
            ShapeKind::Group { children } => children.iter().for_each(|c| texts(c, n)),
            _ => {}
        }
    }
    let mut n = 0;
    texts(&g, &mut n);
    assert!(n > 10, "{n} texts");
    // The settings come back from the group; Update replaces it in place.
    let (sel, back) = app.math_selected().expect("selected");
    assert_eq!(sel, id);
    assert_eq!(back.plots, app.math.sanitized().plots);
    let before = app.selection_bounds().expect("bounds").center();
    app.math.plots[0].f = "-x^2 + 4".into();
    let id2 = app.math_update().expect("updated");
    assert_ne!(id, id2);
    assert!(app.doc().find_shape(id).is_none());
    assert_eq!(app.engine.history_labels().0.len(), depth + 2);
    let after = app.selection_bounds().expect("bounds").center();
    assert!(before.distance(after) < 2.0, "{before:?} {after:?}");
    app.undo();
    assert!(app.doc().find_shape(id).is_some());
    // A figure.
    app.math.mode = Mode::Figure;
    app.math.figure.kind = FigureKind::Polygon;
    let id3 = app.math_create().expect("figure");
    assert!(app.doc().find_shape(id3).is_some());
    // An impossible triangle makes nothing and says so.
    app.math.figure.kind = FigureKind::Triangle;
    app.math.figure.tri_by = TriBy::Sss;
    app.math.figure.side_a = 1.0;
    app.math.figure.side_b = 1.0;
    app.math.figure.side_c = 10.0;
    let depth = app.engine.history_labels().0.len();
    assert!(app.math_create().is_none());
    assert_eq!(app.engine.history_labels().0.len(), depth);
}

#[test]
fn smooth_curves_have_few_nodes() {
    let mut app = crate::app::App::headless();
    apply_preset(&mut app.math, Preset::SineCosine);
    app.math.plane.show = false;
    app.math.plots.truncate(1);
    app.math.plots[0].label = false;
    app.math.smooth = true;
    let id = app.math_create().expect("made");
    let g = app.doc().find_shape(id).expect("g").clone();
    let path = match &g.kind {
        ShapeKind::Group { children } => children
            .iter()
            .find_map(|c| match &c.kind {
                ShapeKind::Path { path, .. } => Some(path.clone()),
                _ => None,
            })
            .expect("curve"),
        ShapeKind::Path { path, .. } => path.clone(),
        _ => panic!("curve"),
    };
    assert!(path.elements().len() < 60, "{} nodes", path.elements().len());
}

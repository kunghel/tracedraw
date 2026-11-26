//! The Math Shapes panel (Window > Panels > Math Shapes, Object > Create >
//! Math Shapes): a Graph mode (a Cartesian plane with curves from
//! formulas) and a Figure mode (circle, triangle, polygon, ellipse, arc
//! from their measures), a live preview, Create and Update.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::math_expr::{self, Expr};
use crate::math_shapes::{
    self as ms, apply_preset, error_text, figure_facts, Anchor, ArcKind, CircleBy, Dash, Drawn,
    FigureKind, MathSettings, MeasureUnit, Mode, Plot, PlotKind, Preset, TriBy,
};
use crate::theme::Tokens;
use crate::ui::btn_icons::IconUi;
use crate::ui::docker_form::{indented, row, section, wide_button};
use crate::ui::tips::RichTip;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use kurbo::PathEl;
use std::sync::Arc;

/// Errors and impossible figures.
const ERR: Color32 = Color32::from_rgb(0xD0, 0x30, 0x30);

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// The drawing for `s`, kept while the settings stay the same.
fn drawn_for(ui: &Ui, s: &MathSettings) -> Arc<Drawn> {
    let id = egui::Id::new("math_shapes_preview");
    let cached: Option<(MathSettings, Arc<Drawn>)> = ui.ctx().data(|d| d.get_temp(id));
    if let Some((key, d)) = cached {
        if &key == s {
            return d;
        }
    }
    let d = Arc::new(ms::draw(s));
    ui.ctx()
        .data_mut(|m| m.insert_temp(id, (s.clone(), d.clone())));
    d
}

/// Even-odd fill of polygons `runs` as horizontal strips one pixel high.
fn fill_mesh(runs: &[Vec<Pos2>], color: Color32) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let (mut y0, mut y1) = (f32::INFINITY, f32::NEG_INFINITY);
    for r in runs {
        for p in r {
            y0 = y0.min(p.y);
            y1 = y1.max(p.y);
        }
    }
    if !y0.is_finite() || y1 - y0 > 4000.0 {
        return mesh;
    }
    let mut y = y0.floor() + 0.5;
    let mut xs: Vec<f32> = Vec::new();
    while y < y1 {
        xs.clear();
        for r in runs {
            let n = r.len();
            for i in 0..n {
                let (a, b) = (r[i], r[(i + 1) % n]);
                if (a.y <= y) != (b.y <= y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        for pair in xs.chunks_exact(2) {
            let r = Rect::from_min_max(Pos2::new(pair[0], y - 0.5), Pos2::new(pair[1], y + 0.5));
            mesh.add_colored_rect(r, color);
        }
        y += 1.0;
    }
    mesh
}

/// The preview: everything drawn, fitted into the panel's width.
fn preview(ui: &mut Ui, d: &Drawn) {
    let w = ui.available_width().clamp(140.0, 360.0);
    let Some(b) = d.bounds() else {
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 60.0), Sense::hover());
        ui.painter().rect_filled(r, 2.0, Color32::WHITE);
        return;
    };
    let aspect = (b.height() / b.width().max(1e-9)) as f32;
    let h = (w * aspect).clamp(60.0, 320.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 2.0, Color32::WHITE);
    painter.rect_stroke(r, 2.0, Stroke::new(1.0, Tokens::border()), egui::StrokeKind::Inside);
    let inner = r.shrink(6.0);
    let scale = (inner.width() as f64 / b.width().max(1e-9))
        .min(inner.height() as f64 / b.height().max(1e-9));
    let ox = inner.center().x as f64 - b.center().x * scale;
    let oy = inner.center().y as f64 + b.center().y * scale;
    let map = |p: kurbo::Point| Pos2::new((ox + p.x * scale) as f32, (oy - p.y * scale) as f32);
    let tol = 0.4 / scale.max(1e-9);
    for it in &d.items {
        let mut runs: Vec<Vec<Pos2>> = Vec::new();
        let mut closed: Vec<bool> = Vec::new();
        tracedraw_core::geometry::flatten(&it.path, tol, &mut |el| match el {
            PathEl::MoveTo(q) => {
                runs.push(vec![map(q)]);
                closed.push(false);
            }
            PathEl::LineTo(q) => {
                if let Some(run) = runs.last_mut() {
                    run.push(map(q));
                }
            }
            PathEl::ClosePath => {
                if let Some(c) = closed.last_mut() {
                    *c = true;
                }
            }
            _ => {}
        });
        if let Some(fc) = it.fill {
            painter.add(egui::Shape::mesh(fill_mesh(&runs, c32(fc))));
        }
        if let Some(l) = &it.line {
            let width = ((l.width * scale) as f32).clamp(0.5, 4.0);
            let stroke = Stroke::new(width, c32(l.color));
            for (run, cl) in runs.into_iter().zip(closed) {
                if run.len() < 2 {
                    continue;
                }
                if l.dash != Dash::Solid {
                    let (on, off) = if l.dash == Dash::Dashed { (4.0, 3.0) } else { (1.2, 2.4) };
                    let mut pts = run;
                    if cl {
                        if let Some(first) = pts.first().copied() {
                            pts.push(first);
                        }
                    }
                    painter.extend(egui::Shape::dashed_line(&pts, stroke, on, off));
                } else if cl {
                    painter.add(egui::Shape::closed_line(run, stroke));
                } else {
                    painter.add(egui::Shape::line(run, stroke));
                }
            }
        }
    }
    for l in &d.labels {
        let px = (ms::em(l.size_pt) * scale) as f32;
        if px < 4.0 {
            continue;
        }
        let align = match l.anchor {
            Anchor::Left => egui::Align2::LEFT_CENTER,
            Anchor::Center => egui::Align2::CENTER_CENTER,
            Anchor::Right => egui::Align2::RIGHT_CENTER,
        };
        painter.text(
            map(l.at),
            align,
            &l.text,
            egui::FontId::proportional(px.min(28.0)),
            c32(l.color),
        );
    }
}

fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    v: &mut T,
    all: &[T],
    key_of: fn(T) -> &'static str,
    tip: &str,
) {
    crate::ui::keynav::ComboBox::from_id_salt(id)
        .selected_text(tr(key_of(*v)))
        .width(ui.available_width().clamp(110.0, 190.0))
        .show_ui(ui, |ui| {
            for k in all {
                ui.selectable_value(v, *k, tr(key_of(*k)));
            }
        })
        .response
        .rich_tip(crate::ui::tips::keyed(tip));
}

fn num(ui: &mut Ui, v: &mut f64, lo: f64, hi: f64, dec: usize, suffix: &str, tip: &str) -> bool {
    ui.add(
        crate::ui::field::NumField::new(v)
            .range(lo..=hi)
            .speed(0.1)
            .max_decimals(dec)
            .suffix(suffix)
            .width(70.0),
    )
    .rich_tip(crate::ui::tips::keyed(tip))
    .changed()
}

fn color_btn(ui: &mut Ui, c: &mut [u8; 3], tip: &str) {
    ui.color_edit_button_srgb(c)
        .rich_tip(crate::ui::tips::keyed(tip));
}

fn check(ui: &mut Ui, v: &mut bool, key: &str) -> bool {
    ui.checkbox(v, tr(key))
        .rich_tip(crate::ui::tips::keyed(key))
        .changed()
}

/// A formula field; its error (if any) in red under it.
fn formula(ui: &mut Ui, text: &mut String, tip: &str, hint: &str) {
    let w = ui.available_width().max(80.0);
    ui.add(
        egui::TextEdit::singleline(text)
            .font(egui::TextStyle::Monospace)
            .hint_text(hint)
            .desired_width(w),
    )
    .rich_tip(crate::ui::tips::keyed(tip));
}

/// A formula's row: its short name, then the field over the rest of the width.
fn formula_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).monospace());
        add(ui);
    });
}

fn error_line(ui: &mut Ui, src: &str) {
    if let Err(e) = Expr::parse(src) {
        indented(ui, |ui| {
            ui.label(RichText::new(error_text(&e)).color(ERR).small());
        });
    }
}

/// A range written with π where it is a simple multiple of it.
fn range_row(ui: &mut Ui, label: &str, lo: &mut f64, hi: &mut f64, tip: &str, degrees: bool) {
    row(ui, label, |ui| {
        let (dec, suffix) = if degrees { (2, "°") } else { (4, "") };
        num(ui, lo, -1e6, 1e6, dec, suffix, tip);
        ui.label("…");
        num(ui, hi, -1e6, 1e6, dec, suffix, tip);
        if !degrees {
            let txt = format!("{} … {}", ms::fmt_pi(*lo), ms::fmt_pi(*hi));
            if txt.contains('π') {
                ui.label(RichText::new(txt).weak().small());
            }
        }
    });
}

fn plot_card(ui: &mut Ui, i: usize, pl: &mut Plot, degrees: bool, remove: &mut Option<usize>, moved: &mut Option<(usize, bool)>, count: usize) {
    let id = egui::Id::new(("math_plot", i));
    let title = match pl.kind {
        PlotKind::Function => format!("y = {}", pl.f.trim()),
        PlotKind::Parametric => format!("({}; {})", pl.f.trim(), pl.g.trim()),
        PlotKind::Polar => format!("r = {}", pl.f.trim()),
        PlotKind::Implicit | PlotKind::Points => pl.f.trim().to_string(),
    };
    let short: String = title.chars().take(34).collect();
    let (toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, i == 0)
        .show_header(ui, |ui| {
            ui.checkbox(&mut pl.visible, "")
                .rich_tip(crate::ui::tips::keyed("math.visible"));
            color_btn(ui, &mut pl.color, "math.curve_color");
            ui.label(RichText::new(short).monospace().color(c32(pl.color)));
        })
        .body(|ui| {
            row(ui, &tr("math.kind"), |ui| {
                combo(ui, ("math_kind", i), &mut pl.kind, &PlotKind::ALL, PlotKind::key, "math.kind");
            });
            match pl.kind {
                PlotKind::Function => {
                    formula_row(ui, "y =", |ui| formula(ui, &mut pl.f, "math.formula_f", "x^2 - 2x - 3"));
                    error_line(ui, &pl.f);
                    indented(ui, |ui| {
                        check(ui, &mut pl.domain, "math.domain");
                    });
                    if pl.domain {
                        row(ui, &tr("math.domain_range"), |ui| {
                            num(ui, &mut pl.d_min, -1e6, 1e6, 4, "", "math.domain_range");
                            ui.label("≤ x ≤");
                            num(ui, &mut pl.d_max, -1e6, 1e6, 4, "", "math.domain_range");
                        });
                    }
                }
                PlotKind::Parametric => {
                    formula_row(ui, "x(t) =", |ui| formula(ui, &mut pl.f, "math.formula_x", "4cos(t)"));
                    error_line(ui, &pl.f);
                    formula_row(ui, "y(t) =", |ui| formula(ui, &mut pl.g, "math.formula_y", "4sin(t)"));
                    error_line(ui, &pl.g);
                    range_row(ui, &tr("math.t_range"), &mut pl.t_min, &mut pl.t_max, "math.t_range", degrees);
                }
                PlotKind::Polar => {
                    formula_row(ui, "r(θ) =", |ui| formula(ui, &mut pl.f, "math.formula_r", "4cos(3θ)"));
                    error_line(ui, &pl.f);
                    range_row(ui, &tr("math.theta_range"), &mut pl.t_min, &mut pl.t_max, "math.theta_range", degrees);
                }
                PlotKind::Implicit => {
                    formula_row(ui, &tr("math.equation"), |ui| {
                        formula(ui, &mut pl.f, "math.equation", "x^2 + y^2 = 9")
                    });
                    error_line(ui, &pl.f);
                }
                PlotKind::Points => {
                    formula_row(ui, &tr("math.points"), |ui| {
                        formula(ui, &mut pl.f, "math.points", "A(1, 2); B(3, -1)")
                    });
                }
            }
            if pl.kind != PlotKind::Points {
                row(ui, &tr("math.samples"), |ui| {
                    ui.add(crate::ui::field::NumField::new(&mut pl.samples).range(20..=5000).width(70.0))
                        .rich_tip(crate::ui::tips::keyed("math.samples"));
                });
            }
            row(ui, &tr("math.line"), |ui| {
                num(ui, &mut pl.width, 0.0, 20.0, 2, " mm", "math.line");
                combo(ui, ("math_dash", i), &mut pl.dash, &Dash::ALL, Dash::key, "math.dash");
            });
            if pl.kind != PlotKind::Points {
                indented(ui, |ui| {
                    check(ui, &mut pl.fill, if pl.kind == PlotKind::Function { "math.fill_under" } else { "math.fill_inside" });
                    if pl.fill {
                        color_btn(ui, &mut pl.fill_color, "math.fill_color");
                    }
                });
            }
            indented(ui, |ui| {
                check(ui, &mut pl.label, if pl.kind == PlotKind::Points { "math.point_labels" } else { "math.curve_label" });
            });
            if matches!(pl.kind, PlotKind::Function | PlotKind::Points) {
                indented(ui, |ui| {
                    check(ui, &mut pl.marks, if pl.kind == PlotKind::Points { "math.point_guides" } else { "math.marks" });
                });
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(i > 0, crate::ui::btn_icons::Ib::new("math.curve_up").small())
                    .rich_tip(crate::ui::tips::keyed("math.curve_up"))
                    .clicked()
                {
                    *moved = Some((i, true));
                }
                if ui
                    .add_enabled(i + 1 < count, crate::ui::btn_icons::Ib::new("math.curve_down").small())
                    .rich_tip(crate::ui::tips::keyed("math.curve_down"))
                    .clicked()
                {
                    *moved = Some((i, false));
                }
                if ui
                    .add(crate::ui::btn_icons::Ib::new("math.remove_curve").small())
                    .rich_tip(crate::ui::tips::keyed("math.remove_curve"))
                    .clicked()
                {
                    *remove = Some(i);
                }
            });
        });
    toggle.rich_tip_with(|| crate::ui::tips::keyed("docker.expand_row"));
}

fn graph_controls(ui: &mut Ui, s: &mut MathSettings) {
    row(ui, &tr("math.preset"), |ui| {
        let mut picked = None;
        crate::ui::keynav::ComboBox::from_id_salt("math_preset")
            .selected_text(tr("math.preset_pick"))
            .width(ui.available_width().clamp(110.0, 190.0))
            .show_ui(ui, |ui| {
                for p in Preset::ALL {
                    if ui.selectable_label(false, tr(p.key())).clicked() {
                        picked = Some(p);
                    }
                }
            })
            .response
            .rich_tip(crate::ui::tips::keyed("math.preset"));
        if let Some(p) = picked {
            apply_preset(s, p);
        }
    });
    section(ui, Some(&tr("math.curves")));
    let count = s.plots.len();
    let mut remove = None;
    let mut moved = None;
    let degrees = s.degrees;
    for (i, pl) in s.plots.iter_mut().enumerate() {
        plot_card(ui, i, pl, degrees, &mut remove, &mut moved, count);
    }
    if let Some(i) = remove {
        if i < s.plots.len() {
            s.plots.remove(i);
        }
    }
    if let Some((i, up)) = moved {
        let j = if up { i.wrapping_sub(1) } else { i + 1 };
        if i < s.plots.len() && j < s.plots.len() {
            s.plots.swap(i, j);
        }
    }
    if ui
        .add_enabled(
            s.plots.len() < ms::MAX_PLOTS,
            crate::ui::btn_icons::Ib::new("math.add_curve"),
        )
        .rich_tip(crate::ui::tips::keyed("math.add_curve"))
        .clicked()
    {
        let n = s.plots.len();
        s.plots.push(Plot::nth(n));
    }
    section(ui, Some(&tr("math.parameters")));
    let used: Vec<bool> = (0..5)
        .map(|k| {
            let slot = math_expr::A + k;
            s.plots.iter().any(|p| {
                [&p.f, &p.g]
                    .iter()
                    .any(|t| Expr::parse(t).is_ok_and(|e| e.uses(slot)))
            })
        })
        .collect();
    for (k, name) in ["a", "b", "c", "d", "k"].iter().enumerate() {
        row(ui, &format!("{name} ="), |ui| {
            let v = &mut s.params[k];
            let lo = v.min(-10.0);
            let hi = v.max(10.0);
            ui.add(crate::ui::Slider::new(v, lo..=hi).max_decimals(3))
                .rich_tip(crate::ui::tips::keyed("math.parameter"));
            if !used[k] {
                ui.label(RichText::new(tr("math.unused")).weak().small());
            }
        });
    }
    indented(ui, |ui| {
        check(ui, &mut s.degrees, "math.degrees");
    });
    indented(ui, |ui| {
        check(ui, &mut s.smooth, "math.smooth");
    });
    plane_controls(ui, s);
    egui::CollapsingHeader::new(tr("math.help"))
        .id_salt("math_help")
        .show(ui, |ui| {
            ui.label(RichText::new(tr("math.help_text")).small());
        });
}

fn plane_controls(ui: &mut Ui, s: &mut MathSettings) {
    let p = &mut s.plane;
    section(ui, Some(&tr("math.plane")));
    indented(ui, |ui| {
        check(ui, &mut p.show, "math.show_plane");
    });
    row(ui, &tr("math.x_range"), |ui| {
        num(ui, &mut p.x_min, -1e6, 1e6, 4, "", "math.x_range");
        ui.label("…");
        num(ui, &mut p.x_max, -1e6, 1e6, 4, "", "math.x_range");
    });
    row(ui, &tr("math.y_range"), |ui| {
        num(ui, &mut p.y_min, -1e6, 1e6, 4, "", "math.y_range");
        ui.label("…");
        num(ui, &mut p.y_max, -1e6, 1e6, 4, "", "math.y_range");
    });
    let id = egui::Id::new("math_same_scale");
    let mut same = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(p.unit_x == p.unit_y);
    row(ui, &tr("math.unit"), |ui| {
        let cx = num(ui, &mut p.unit_x, 0.01, 1000.0, 2, " mm", "math.unit");
        ui.label("×");
        let cy = num(ui, &mut p.unit_y, 0.01, 1000.0, 2, " mm", "math.unit");
        if same && cx {
            p.unit_y = p.unit_x;
        } else if same && cy {
            p.unit_x = p.unit_y;
        }
    });
    indented(ui, |ui| {
        if check(ui, &mut same, "math.same_scale") && same {
            p.unit_y = p.unit_x;
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, same));
    // The plane's size; changing it changes the units.
    row(ui, &tr("math.size"), |ui| {
        let (sx, sy) = (p.x_max - p.x_min, p.y_max - p.y_min);
        let mut w = sx * p.unit_x;
        let mut h = sy * p.unit_y;
        if num(ui, &mut w, 1.0, 5000.0, 1, " mm", "math.size") && sx > 0.0 {
            p.unit_x = w / sx;
            if same {
                p.unit_y = p.unit_x;
            }
        }
        ui.label("×");
        if num(ui, &mut h, 1.0, 5000.0, 1, " mm", "math.size") && sy > 0.0 {
            p.unit_y = h / sy;
            if same {
                p.unit_x = p.unit_y;
            }
        }
    });
    if !p.show {
        return;
    }
    section(ui, Some(&tr("math.axes")));
    indented(ui, |ui| {
        check(ui, &mut p.axes, "math.axes_on");
        if p.axes {
            check(ui, &mut p.arrows, "math.arrows");
        }
    });
    if p.axes {
        indented(ui, |ui| {
            check(ui, &mut p.axis_names, "math.axis_names");
            check(ui, &mut p.ticks, "math.ticks");
        });
        row(ui, &tr("math.axis_line"), |ui| {
            num(ui, &mut p.axis_width, 0.0, 20.0, 2, " mm", "math.axis_line");
            color_btn(ui, &mut p.axis_color, "math.axis_line");
        });
    }
    if p.axes && p.ticks {
        row(ui, &tr("math.tick_step"), |ui| {
            num(ui, &mut p.tick_x, 1e-6, 1e6, 4, "", "math.tick_step");
            ui.label("×");
            num(ui, &mut p.tick_y, 1e-6, 1e6, 4, "", "math.tick_step");
        });
        row(ui, &tr("math.tick_len"), |ui| {
            num(ui, &mut p.tick_len, 0.0, 20.0, 2, " mm", "math.tick_len");
        });
    }
    indented(ui, |ui| {
        check(ui, &mut p.numbers, "math.numbers");
    });
    if p.numbers {
        indented(ui, |ui| {
            check(ui, &mut p.pi_x, "math.pi_x");
            check(ui, &mut p.origin, "math.origin");
        });
        if !(p.axes && p.ticks) {
            row(ui, &tr("math.tick_step"), |ui| {
                num(ui, &mut p.tick_x, 1e-6, 1e6, 4, "", "math.tick_step");
                ui.label("×");
                num(ui, &mut p.tick_y, 1e-6, 1e6, 4, "", "math.tick_step");
            });
        }
        row(ui, &tr("math.number_size"), |ui| {
            num(ui, &mut p.number_pt, 2.0, 72.0, 1, " pt", "math.number_size");
            color_btn(ui, &mut p.number_color, "math.number_size");
        });
        row(ui, &tr("math.font"), |ui| {
            ui.add(egui::TextEdit::singleline(&mut p.font).desired_width(120.0))
                .rich_tip(crate::ui::tips::keyed("math.font"));
        });
    }
    section(ui, Some(&tr("math.grid")));
    indented(ui, |ui| {
        check(ui, &mut p.grid, "math.grid_on");
        check(ui, &mut p.border, "math.border");
    });
    if p.grid {
        row(ui, &tr("math.grid_step"), |ui| {
            num(ui, &mut p.grid_x, 1e-6, 1e6, 4, "", "math.grid_step");
            ui.label("×");
            num(ui, &mut p.grid_y, 1e-6, 1e6, 4, "", "math.grid_step");
        });
        row(ui, &tr("math.grid_line"), |ui| {
            num(ui, &mut p.grid_width, 0.0, 20.0, 2, " mm", "math.grid_line");
            color_btn(ui, &mut p.grid_color, "math.grid_line");
        });
        indented(ui, |ui| {
            check(ui, &mut p.minor, "math.minor");
        });
        if p.minor {
            row(ui, &tr("math.minor_div"), |ui| {
                ui.add(crate::ui::field::NumField::new(&mut p.minor_div).range(2..=20).width(70.0))
                    .rich_tip(crate::ui::tips::keyed("math.minor_div"));
            });
            row(ui, &tr("math.minor_line"), |ui| {
                num(ui, &mut p.minor_width, 0.0, 20.0, 2, " mm", "math.minor_line");
                color_btn(ui, &mut p.minor_color, "math.minor_line");
            });
        }
    }
}

fn figure_controls(ui: &mut Ui, s: &mut MathSettings) {
    let f = &mut s.figure;
    row(ui, &tr("math.figure"), |ui| {
        combo(ui, "math_fig", &mut f.kind, &FigureKind::ALL, FigureKind::key, "math.figure");
    });
    let mm = " mm";
    match f.kind {
        FigureKind::Circle => {
            row(ui, &tr("math.given"), |ui| {
                combo(ui, "math_circle_by", &mut f.circle_by, &CircleBy::ALL, CircleBy::key, "math.given");
            });
            let suffix = if f.circle_by == CircleBy::Area { " mm²" } else { mm };
            row(ui, &tr(f.circle_by.key()), |ui| {
                num(ui, &mut f.circle_value, 0.01, 1e5, 3, suffix, "math.given");
            });
        }
        FigureKind::Triangle => {
            row(ui, &tr("math.given"), |ui| {
                combo(ui, "math_tri_by", &mut f.tri_by, &TriBy::ALL, TriBy::key, "math.given");
            });
            let side = |ui: &mut Ui, label: &str, v: &mut f64| {
                row(ui, label, |ui| {
                    num(ui, v, 0.01, 1e5, 3, mm, "math.side");
                });
            };
            let angle = |ui: &mut Ui, label: &str, v: &mut f64| {
                row(ui, label, |ui| {
                    num(ui, v, 0.01, 179.99, 3, "°", "math.angle");
                });
            };
            match f.tri_by {
                TriBy::Sss => {
                    side(ui, "a (BC)", &mut f.side_a);
                    side(ui, "b (CA)", &mut f.side_b);
                    side(ui, "c (AB)", &mut f.side_c);
                }
                TriBy::Sas => {
                    side(ui, "b (CA)", &mut f.side_b);
                    angle(ui, "Â", &mut f.angle_a);
                    side(ui, "c (AB)", &mut f.side_c);
                }
                TriBy::Asa => {
                    angle(ui, "Â", &mut f.angle_a);
                    side(ui, "c (AB)", &mut f.side_c);
                    angle(ui, "B̂", &mut f.angle_b);
                }
                TriBy::Right => {
                    side(ui, "a (BC)", &mut f.side_a);
                    side(ui, "b (CA)", &mut f.side_b);
                }
                TriBy::Isosceles => {
                    side(ui, "a = b", &mut f.side_a);
                    side(ui, "c (AB)", &mut f.side_c);
                }
                TriBy::Equilateral => side(ui, "a = b = c", &mut f.side_a),
            }
        }
        FigureKind::Polygon => {
            row(ui, &tr("math.sides"), |ui| {
                ui.add(crate::ui::field::NumField::new(&mut f.sides).range(3..=200).width(70.0))
                    .rich_tip(crate::ui::tips::keyed("math.sides"));
            });
            row(ui, &tr("math.given"), |ui| {
                let mut by = f.by_side;
                ui.radio_value(&mut by, true, tr("math.by_side"))
                    .rich_tip(crate::ui::tips::keyed("math.given"));
                ui.radio_value(&mut by, false, tr("math.by_circumradius"))
                    .rich_tip(crate::ui::tips::keyed("math.given"));
                f.by_side = by;
            });
            row(ui, &tr(if f.by_side { "math.by_side" } else { "math.by_circumradius" }), |ui| {
                num(ui, &mut f.poly_value, 0.01, 1e5, 3, mm, "math.given");
            });
            row(ui, &tr("math.star"), |ui| {
                let hi = (f.sides.saturating_sub(1) / 2).max(1);
                ui.add(crate::ui::field::NumField::new(&mut f.star).range(1..=hi).width(70.0))
                    .rich_tip(crate::ui::tips::keyed("math.star"));
            });
            row(ui, &tr("math.rotation"), |ui| {
                num(ui, &mut f.rotation, -360.0, 360.0, 2, "°", "math.rotation");
            });
        }
        FigureKind::Ellipse => {
            row(ui, &tr("math.semi_axes"), |ui| {
                num(ui, &mut f.rx, 0.01, 1e5, 3, mm, "math.semi_axes");
                ui.label("×");
                num(ui, &mut f.ry, 0.01, 1e5, 3, mm, "math.semi_axes");
            });
            row(ui, &tr("math.rotation"), |ui| {
                num(ui, &mut f.rotation, -360.0, 360.0, 2, "°", "math.rotation");
            });
        }
        FigureKind::Arc => {
            row(ui, &tr("math.arc_type"), |ui| {
                combo(ui, "math_arc_kind", &mut f.arc_kind, &ArcKind::ALL, ArcKind::key, "math.arc_type");
            });
            row(ui, &tr("math.by_radius"), |ui| {
                num(ui, &mut f.arc_radius, 0.01, 1e5, 3, mm, "math.given");
            });
            row(ui, &tr("math.arc_start"), |ui| {
                num(ui, &mut f.arc_start, -360.0, 360.0, 2, "°", "math.arc_start");
            });
            row(ui, &tr("math.arc_sweep"), |ui| {
                num(ui, &mut f.arc_sweep, 0.1, 360.0, 2, "°", "math.arc_sweep");
            });
        }
    }
    section(ui, Some(&tr("math.show")));
    ui.horizontal_wrapped(|ui| {
        ui.add_space(crate::ui::docker_form::LABEL_W * 0.3);
        check(ui, &mut f.names, "math.names");
        check(ui, &mut f.measures, "math.measures");
        if !matches!(f.kind, FigureKind::Circle | FigureKind::Ellipse) {
            check(ui, &mut f.angles, "math.angles");
        }
        if f.kind != FigureKind::Triangle {
            check(ui, &mut f.center, "math.center");
            check(
                ui,
                &mut f.radius_line,
                match f.kind {
                    FigureKind::Polygon => "math.apothem",
                    FigureKind::Ellipse => "math.axes_lines",
                    _ => "math.radius_line",
                },
            );
        }
        match f.kind {
            FigureKind::Circle => {
                check(ui, &mut f.diameter_line, "math.diameter_line");
            }
            FigureKind::Triangle => {
                check(ui, &mut f.heights, "math.heights");
                check(ui, &mut f.medians, "math.medians");
                check(ui, &mut f.bisectors, "math.bisectors");
                check(ui, &mut f.incircle, "math.incircle");
                check(ui, &mut f.circumcircle, "math.circumcircle");
            }
            FigureKind::Polygon => {
                check(ui, &mut f.diagonals, "math.diagonals");
                check(ui, &mut f.incircle, "math.incircle");
                check(ui, &mut f.circumcircle, "math.circumcircle");
            }
            FigureKind::Ellipse => {
                check(ui, &mut f.foci, "math.foci");
            }
            FigureKind::Arc => {}
        }
    });
    section(ui, Some(&tr("math.style")));
    row(ui, &tr("math.line"), |ui| {
        num(ui, &mut f.width, 0.0, 20.0, 2, mm, "math.line");
        color_btn(ui, &mut f.color, "math.line");
    });
    indented(ui, |ui| {
        check(ui, &mut f.fill, "math.fill_inside");
        if f.fill {
            color_btn(ui, &mut f.fill_color, "math.fill_color");
        }
    });
    row(ui, &tr("math.aid_color"), |ui| {
        color_btn(ui, &mut f.aid_color, "math.aid_color");
    });
    row(ui, &tr("math.label_size"), |ui| {
        num(ui, &mut f.label_pt, 2.0, 72.0, 1, " pt", "math.label_size");
    });
    row(ui, &tr("math.measure_unit"), |ui| {
        combo(ui, "math_unit", &mut f.unit, &MeasureUnit::ALL, MeasureUnit::key, "math.measure_unit");
    });
    row(ui, &tr("math.decimals"), |ui| {
        ui.add(crate::ui::field::NumField::new(&mut f.decimals).range(0..=4).width(70.0))
            .rich_tip(crate::ui::tips::keyed("math.decimals"));
    });
    row(ui, &tr("math.font"), |ui| {
        ui.add(egui::TextEdit::singleline(&mut f.font).desired_width(120.0))
            .rich_tip(crate::ui::tips::keyed("math.font"));
    });
    let facts = figure_facts(f);
    section(ui, Some(&tr("math.facts")));
    if facts.is_empty() {
        indented(ui, |ui| {
            ui.label(RichText::new(tr("math.impossible")).color(ERR));
        });
    }
    for (key, v, kind) in facts {
        let text = match kind {
            0 => format!("{}°", ms::fmt_num(v, 2)),
            1 => ms::fmt_len(v, f),
            2 => ms::fmt_area(v, f),
            _ => ms::fmt_num(v, 4),
        };
        row(ui, &tr(key), |ui| {
            ui.label(RichText::new(text).monospace());
        });
    }
}

pub fn math_shapes(app: &mut App, ui: &mut Ui) {
    ui.spacing_mut().slider_width = 100.0;
    // A Math Shapes group picked in the drawing loads its settings, so
    // that it can be changed and updated.
    let selected = app.math_selected();
    let id = egui::Id::new("math_shapes_loaded");
    let loaded: Option<tracedraw_core::ShapeId> = ui
        .ctx()
        .data(|d| d.get_temp::<Option<tracedraw_core::ShapeId>>(id))
        .flatten();
    if let Some((sid, s)) = &selected {
        if loaded != Some(*sid) {
            app.math = s.clone();
            ui.ctx().data_mut(|d| d.insert_temp(id, Some(*sid)));
        }
    } else if loaded.is_some() {
        ui.ctx()
            .data_mut(|d| d.insert_temp::<Option<tracedraw_core::ShapeId>>(id, None));
    }
    ui.horizontal(|ui| {
        ui.add_space(crate::ui::docker_form::LABEL_W * 0.3);
        if ui
            .iselectable(app.math.mode == Mode::Graph, "math.mode_graph")
            .rich_tip(crate::ui::tips::keyed("math.mode_graph"))
            .clicked()
        {
            app.math.mode = Mode::Graph;
        }
        if ui
            .iselectable(app.math.mode == Mode::Figure, "math.mode_figure")
            .rich_tip(crate::ui::tips::keyed("math.mode_figure"))
            .clicked()
        {
            app.math.mode = Mode::Figure;
        }
    });
    let d = drawn_for(ui, &app.math);
    ui.add_space(4.0);
    preview(ui, &d);
    for (i, e) in &d.errors {
        ui.label(
            RichText::new(trf(
                "math.curve_error",
                &[("n", &(i + 1).to_string()), ("error", &error_text(e))],
            ))
            .color(ERR)
            .small(),
        );
    }
    let create_tip = crate::ui::tips::keyed("math.create");
    if wide_button(ui, "math.create", !d.impossible, &create_tip) {
        app.math_create();
    }
    if selected.is_some() {
        let tip = crate::ui::tips::keyed("math.update");
        if wide_button(ui, "math.update", !d.impossible, &tip) {
            app.math_update();
        }
    }
    section(ui, None);
    match app.math.mode {
        Mode::Graph => graph_controls(ui, &mut app.math),
        Mode::Figure => figure_controls(ui, &mut app.math),
    }
}

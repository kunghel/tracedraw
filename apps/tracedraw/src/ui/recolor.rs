//! Tools > Recolor: every colour of the selected objects turned round
//! the hue circle, made more or less saturated, lighter or darker, or
//! tinted with one colour, each colour keeping its lightness next to the
//! others so the drawing's shading stays (`Command::Recolor`, see
//! `tracedraw_core::recolor`). The change shows on the drawing while the
//! dialog is open; OK keeps it as one undo step, Cancel takes it back.
//! Each colour of the selection is listed with the colour it becomes, so
//! any of them can be swapped for another one by one (#3609).
//! See docs/behavior/recolor.md.

use crate::ui::btn_icons::IconUi;
use crate::app::App;
use crate::i18n::{tr, trf};
use crate::ui::dialogs::window;
use egui::{Context, RichText};
use tracedraw_core::recolor::{recolor_document, Recolor};
use tracedraw_core::{Color, Command, ShapeId};

/// The dialog's values.
#[derive(Debug, Clone, PartialEq)]
pub struct RecolorState {
    pub recolor: Recolor,
    /// Tint on, with the colour in `tint_color`.
    pub tint_on: bool,
    pub tint_color: Color,
    /// The objects it works on (the selection when it opened).
    pub shapes: Vec<ShapeId>,
    /// Each colour of those objects and the colour it becomes (the same
    /// colour until one is picked for it).
    pub colors: Vec<(Color, Color)>,
    /// The colours picked in the list, to change together (D#4203).
    pub picked: Vec<bool>,
    /// The colour Replace Picked gives the picked colours.
    pub replace_with: Color,
    /// How close (0 to 100) Pick Similar takes colours to the first one
    /// picked.
    pub similar: f32,
    /// What the drawing shows now.
    shown: Option<Recolor>,
}

impl RecolorState {
    pub fn new(shapes: Vec<ShapeId>) -> Self {
        RecolorState {
            recolor: Recolor::default(),
            tint_on: false,
            tint_color: Color::rgb8(30, 110, 220),
            shapes,
            colors: Vec::new(),
            picked: Vec::new(),
            replace_with: Color::rgb8(220, 40, 40),
            similar: 15.0,
            shown: None,
        }
    }

    /// What the values ask for.
    pub fn asked(&self) -> Recolor {
        Recolor {
            tint: self.tint_on.then_some(self.tint_color),
            swaps: self
                .colors
                .iter()
                .filter(|(from, to)| from != to)
                .copied()
                .collect(),
            ..self.recolor.clone()
        }
    }
}

impl RecolorState {
    /// Every picked colour becomes `to` (D#4203).
    pub fn replace_picked(&mut self, to: Color) {
        for (i, (_, t)) in self.colors.iter_mut().enumerate() {
            if self.picked.get(i).copied().unwrap_or(false) {
                *t = to;
            }
        }
    }

    /// Pick every colour within `similar` (0 to 100, as far as black is
    /// from white) of the first one picked, or of the first colour.
    pub fn pick_similar(&mut self) {
        self.picked.resize(self.colors.len(), false);
        let Some(first) = self
            .picked
            .iter()
            .position(|p| *p)
            .or((!self.colors.is_empty()).then_some(0))
        else {
            return;
        };
        let rgb = |c: Color| c.to_rgb_f32();
        let base = rgb(self.colors[first].0);
        let reach = self.similar.clamp(0.0, 100.0) / 100.0 * 3f32.sqrt();
        for (i, (c, _)) in self.colors.iter().enumerate() {
            let v = rgb(*c);
            let d = ((v[0] - base[0]).powi(2) + (v[1] - base[1]).powi(2) + (v[2] - base[2]).powi(2)).sqrt();
            if d <= reach + 1e-6 {
                self.picked[i] = true;
            }
        }
    }
}

/// Every colour of `shapes` (and what they hold), each once, in the order
/// met; registration colour left out.
pub fn colors_of(doc: &tracedraw_core::Document, shapes: &[ShapeId]) -> Vec<Color> {
    let seen = std::cell::RefCell::new(Vec::<Color>::new());
    for id in shapes {
        if let Some(s) = doc.find_shape(*id) {
            let mut s = s.clone();
            tracedraw_core::color_convert::map_shape_colors(&mut s, &|c| {
                let mut seen = seen.borrow_mut();
                if !matches!(c, Color::Registration) && !seen.contains(&c) {
                    seen.push(c);
                }
                c
            });
        }
    }
    seen.into_inner()
}

impl App {
    /// Tools > Recolor: its dialog, for the selected objects.
    pub fn recolor_dialog(&mut self) {
        if self.selection.is_empty() {
            self.status = tr("recolor.nothing_selected");
            return;
        }
        let mut st = RecolorState::new(self.selection.clone());
        st.colors = colors_of(self.doc(), &st.shapes)
            .into_iter()
            .map(|c| (c, c))
            .collect();
        self.dialog = crate::ui::dialogs::Dialog::Recolor(st);
    }

    /// Recolour `shapes` by `r` as one undo step; the status line says how
    /// many colours changed. False when none did.
    pub fn recolor_shapes(&mut self, shapes: Vec<ShapeId>, r: Recolor) -> bool {
        if shapes.is_empty() || r.is_identity() {
            return false;
        }
        // How many colours change, on a copy (the drawing's data is shared).
        let mut copy = self.doc().clone();
        let n = recolor_document(&mut copy, &shapes, &r);
        if n == 0 {
            self.status = tr("recolor.none");
            return false;
        }
        self.run(Command::Recolor { shapes, recolor: r });
        self.status = trf("recolor.done", &[("n", &n.to_string())]);
        true
    }

    /// Show the dialog's values on the drawing: taken back and made anew
    /// only when a value changes.
    pub fn preview_recolor(&mut self, st: &mut RecolorState) {
        let now = st.asked();
        if st.shown.as_ref() == Some(&now) && self.recolor_preview != now.is_identity() {
            return;
        }
        self.end_recolor_preview();
        if !now.is_identity() {
            self.engine.begin_preview();
            self.recolor_preview = true;
            let _ = self.engine.run(&Command::Recolor {
                shapes: st.shapes.clone(),
                recolor: now.clone(),
            });
        }
        st.shown = Some(now);
    }

    /// Take back what the Recolor dialog shows on the drawing.
    pub fn end_recolor_preview(&mut self) {
        if std::mem::take(&mut self.recolor_preview) {
            self.engine.withdraw_preview();
        }
    }
}

/// A swatch of `c` (not clickable), with its values as the tip.
fn color_chip(ui: &mut egui::Ui, c: Color) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::new(18.0, 18.0), egui::Sense::click());
    let [r, g, b] = c.to_rgb8();
    ui.painter()
        .rect_filled(rect, 2.0, egui::Color32::from_rgb(r, g, b));
    ui.painter().rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, crate::theme::Tokens::control_border()),
        egui::StrokeKind::Inside,
    );
    resp.on_hover_text(crate::ui::palette::swatch_tip("", c))
}

/// A slider row: label, slider, value.
fn slider(ui: &mut egui::Ui, key: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.label(tr(key));
    ui.add(
        egui::Slider::new(v, range)
            .step_by(1.0)
            .suffix(if key == "recolor.hue" { "°" } else { "%" }),
    );
    ui.end_row();
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut RecolorState, close: &mut bool) {
    let mut help = false;
    let mut ok = false;
    let mut reset = false;
    window(ctx, tr("recolor.title")).show(ctx, |ui| {
        ui.set_width(400.0);
        egui::Grid::new("recolor_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut st.tint_on, tr("recolor.tint"));
                });
                ui.add_enabled_ui(st.tint_on, |ui| {
                    crate::ui::color_picker::color_button(ui, &mut st.tint_color);
                });
                ui.end_row();
                slider(ui, "recolor.hue", &mut st.recolor.hue, -180.0..=180.0);
                slider(
                    ui,
                    "recolor.saturation",
                    &mut st.recolor.saturation,
                    -100.0..=100.0,
                );
                slider(
                    ui,
                    "recolor.lightness",
                    &mut st.recolor.lightness,
                    -100.0..=100.0,
                );
            });
        // Each colour of the selection and the one it becomes (#3609).
        if !st.colors.is_empty() {
            ui.add_space(6.0);
            ui.strong(tr("recolor.colors"));
            st.picked.resize(st.colors.len(), false);
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (i, (from, to)) in st.colors.iter_mut().enumerate() {
                            ui.push_id(("recolor_swap", i), |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    // A click on the colour picks it to
                                    // change with others (D#4203).
                                    let picked = &mut st.picked[i];
                                    let r = color_chip(ui, *from);
                                    if r.clicked() {
                                        *picked = !*picked;
                                    }
                                    if *picked {
                                        ui.painter().rect_stroke(
                                            r.rect.expand(2.0),
                                            3.0,
                                            egui::Stroke::new(2.0, crate::theme::Tokens::ACCENT),
                                            egui::StrokeKind::Outside,
                                        );
                                    }
                                    ui.label(">");
                                    crate::ui::color_picker::color_button(ui, to);
                                });
                            });
                        }
                    });
                });
            ui.label(RichText::new(tr("recolor.colors_note")).weak());
            // Several colours at once (D#4203): pick them (a click on
            // each, all, or those close to the first), then give them all
            // one colour.
            ui.horizontal_wrapped(|ui| {
                if ui.ibutton("recolor.pick_all").clicked() {
                    st.picked.iter_mut().for_each(|p| *p = true);
                }
                if ui.ibutton("recolor.pick_none").clicked() {
                    st.picked.iter_mut().for_each(|p| *p = false);
                }
                if ui
                    .ibutton("recolor.pick_similar")
                    .on_hover_text(tr("recolor.pick_similar_tip"))
                    .clicked()
                {
                    st.pick_similar();
                }
                ui.add(
                    egui::Slider::new(&mut st.similar, 0.0..=100.0)
                        .suffix(" %")
                        .fixed_decimals(0),
                );
            });
            let any = st.picked.iter().any(|p| *p);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(any, |ui| {
                    let mut to = st.replace_with;
                    crate::ui::color_picker::color_button(ui, &mut to);
                    st.replace_with = to;
                    if ui.ibutton("recolor.replace_picked").clicked() {
                        st.replace_picked(to);
                    }
                });
            });
        }
        ui.add_space(4.0);
        ui.checkbox(&mut st.recolor.spots, tr("recolor.spots"));
        ui.add_space(4.0);
        ui.label(RichText::new(tr("recolor.note")).weak());
        ui.add_space(4.0);
        reset = ui.ibutton("recolor.reset").clicked();
        let (o, h) = crate::ui::dialogs::help_ok_cancel(ui, close);
        ok = o;
        help = h;
    });
    if reset {
        st.recolor = Recolor {
            spots: st.recolor.spots,
            ..Recolor::default()
        };
        st.tint_on = false;
        for (from, to) in st.colors.iter_mut() {
            *to = *from;
        }
    }
    if help {
        app.open_dialog_help();
    }
    if ok {
        app.end_recolor_preview();
        let shapes = st.shapes.clone();
        app.recolor_shapes(shapes, st.asked());
    } else if *close {
        app.end_recolor_preview();
    } else {
        app.preview_recolor(st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::Dialog;
    use tracedraw_core::{Fill, ShapeKind};

    fn rect(app: &mut App, c: Color) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(c),
        });
        id
    }

    fn fill_of(app: &App, id: ShapeId) -> Option<Fill> {
        app.doc().find_shape(id).map(|s| s.fill.clone())
    }

    /// The dialog shows the change while open, Cancel takes it back with
    /// no step left, OK keeps it as one step that Undo takes back.
    #[test]
    fn preview_cancel_and_ok_as_one_step() {
        let mut app = App::headless();
        app.show_welcome = false;
        let red = Color::rgb8(200, 40, 40);
        let a = rect(&mut app, red);
        let b = rect(&mut app, red);
        app.select(vec![a]);
        app.recolor_dialog();
        let Dialog::Recolor(st) = &app.dialog else {
            panic!("open");
        };
        let mut st = st.clone();
        let steps = app.engine.history_labels().0.len();
        st.recolor.hue = 120.0;
        app.preview_recolor(&mut st);
        assert_ne!(fill_of(&app, a), Some(Fill::Solid(red)), "shown");
        assert_eq!(
            fill_of(&app, b),
            Some(Fill::Solid(red)),
            "only the selection"
        );
        app.end_recolor_preview();
        assert_eq!(fill_of(&app, a), Some(Fill::Solid(red)), "cancel");
        assert_eq!(app.engine.history_labels().0.len(), steps);
        st.shown = None;
        app.preview_recolor(&mut st);
        app.end_recolor_preview();
        assert!(app.recolor_shapes(st.shapes.clone(), st.asked()));
        let turned = fill_of(&app, a);
        assert_ne!(turned, Some(Fill::Solid(red)));
        assert_eq!(app.engine.history_labels().0.len(), steps + 1);
        app.undo();
        assert_eq!(fill_of(&app, a), Some(Fill::Solid(red)));
    }

    /// D#4203: several colours picked (close reds by Pick Similar) all
    /// take one colour at once; the far one stays.
    #[test]
    fn several_picked_colours_take_one_colour() {
        let mut app = App::headless();
        app.show_welcome = false;
        let red = Color::rgb8(200, 40, 40);
        let red2 = Color::rgb8(210, 50, 45);
        let blue = Color::rgb8(40, 40, 200);
        let ids = vec![rect(&mut app, red), rect(&mut app, red2), rect(&mut app, blue)];
        app.select(ids.clone());
        app.recolor_dialog();
        let Dialog::Recolor(st) = &app.dialog else {
            panic!("open");
        };
        let mut st = st.clone();
        let at = |c: Color| st.colors.iter().position(|(f, _)| *f == c).expect("listed");
        let (ir, ir2, ib) = (at(red), at(red2), at(blue));
        st.picked = vec![false; st.colors.len()];
        st.picked[ir] = true;
        st.similar = 10.0;
        st.pick_similar();
        assert!(st.picked[ir] && st.picked[ir2] && !st.picked[ib], "{:?}", st.picked);
        let green = Color::rgb8(40, 160, 40);
        st.replace_picked(green);
        app.end_recolor_preview();
        assert!(app.recolor_shapes(st.shapes.clone(), st.asked()));
        assert_eq!(fill_of(&app, ids[0]), Some(Fill::Solid(green)));
        assert_eq!(fill_of(&app, ids[1]), Some(Fill::Solid(green)));
        assert_eq!(fill_of(&app, ids[2]), Some(Fill::Solid(blue)));
    }

    /// The dialog lists each colour of the selection; one swapped for
    /// another changes only where that colour is, in the selection, as
    /// one step; Reset takes the swaps back (#3609).
    #[test]
    fn each_colour_of_the_selection_can_be_swapped() {
        let mut app = App::headless();
        app.show_welcome = false;
        let red = Color::rgb8(200, 40, 40);
        let blue = Color::rgb8(40, 40, 200);
        let green = Color::rgb8(40, 160, 40);
        let a = rect(&mut app, red);
        let b = rect(&mut app, blue);
        let c = rect(&mut app, red);
        for id in [a, b, c] {
            app.run(Command::SetStroke {
                shapes: vec![id],
                stroke: None,
            });
        }
        app.select(vec![a, b]);
        app.recolor_dialog();
        let Dialog::Recolor(st) = &app.dialog else {
            panic!("open");
        };
        let mut st = st.clone();
        assert_eq!(st.colors, vec![(red, red), (blue, blue)]);
        assert!(st.asked().is_identity());
        st.colors[0].1 = green;
        app.preview_recolor(&mut st);
        assert_eq!(fill_of(&app, a), Some(Fill::Solid(green)), "shown");
        app.end_recolor_preview();
        let steps = app.engine.history_labels().0.len();
        assert!(app.recolor_shapes(st.shapes.clone(), st.asked()));
        assert_eq!(fill_of(&app, a), Some(Fill::Solid(green)));
        assert_eq!(fill_of(&app, b), Some(Fill::Solid(blue)), "not swapped");
        assert_eq!(fill_of(&app, c), Some(Fill::Solid(red)), "not selected");
        assert_eq!(app.engine.history_labels().0.len(), steps + 1);
        app.undo();
        assert_eq!(fill_of(&app, a), Some(Fill::Solid(red)));
    }

    #[test]
    fn nothing_selected_opens_nothing() {
        let mut app = App::headless();
        app.show_welcome = false;
        app.recolor_dialog();
        assert!(!matches!(app.dialog, Dialog::Recolor(_)));
    }
}

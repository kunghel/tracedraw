//! Effects > Bend with Pins: the selection drawn in a preview, pins
//! stuck in it with a click and dragged, the bent outline shown as it
//! goes; OK makes the selected objects curves and bends them, in one
//! step to undo.

use crate::app::App;
use crate::i18n::tr;
use crate::pins::{self, Pin};
use crate::ui::dialogs::{ok_cancel, window, Dialog};
use crate::ui::tips::RichTip;
use egui::{Color32, Context, Ui};
use tracedraw_core::geometry::{Affine, Point, Rect};
use tracedraw_core::{BezPath, Command};

/// The dialog's state: the pins, how far each reaches, the selection's
/// outlines on the page and the part of the page the preview shows.
#[derive(Debug, Clone, PartialEq)]
pub struct PinsState {
    pub pins: Vec<Pin>,
    pub reach: f64,
    pub outlines: Vec<BezPath>,
    pub view: Rect,
    pub drag: Option<usize>,
}

impl Default for PinsState {
    fn default() -> Self {
        Self {
            pins: Vec::new(),
            reach: 2.0,
            outlines: Vec::new(),
            view: Rect::new(0.0, 0.0, 100.0, 100.0),
            drag: None,
        }
    }
}

impl App {
    /// Open the dialog on the selection.
    pub fn open_pins_dialog(&mut self) {
        let mut outlines = Vec::new();
        for s in self.selected_shapes() {
            pins::page_outlines(&s, Affine::IDENTITY, &mut outlines);
        }
        let b = outlines
            .iter()
            .map(tracedraw_core::geometry::Shape::bounding_box)
            .reduce(|a, b| a.union(b));
        let Some(b) = b.filter(|b| b.is_finite()) else {
            return;
        };
        // Room around the objects to drag pins out into.
        let pad = 0.25 * b.width().max(b.height()).max(1.0);
        self.dialog = Dialog::Pins(PinsState {
            outlines,
            view: b.inflate(pad, pad),
            ..Default::default()
        });
    }

    /// Bend the selected objects by `st`'s pins: each made curves (its
    /// envelope and other warps made part of it), then bent, in one step
    /// to undo. False when nothing bent.
    pub fn apply_pins(&mut self, st: &PinsState) -> bool {
        if st.pins.is_empty() {
            return false;
        }
        let symbols = self.doc().symbols.clone();
        let mut cmds = Vec::new();
        for s in self.selected_shapes() {
            let engine = &mut self.engine;
            let mut next_id = || engine.new_shape_id();
            let mut c = s
                .converted_to_curves(&symbols, &mut next_id)
                .unwrap_or_else(|| s.clone());
            if !pins::bend_shape(&mut c, Affine::IDENTITY, &st.pins, st.reach) {
                continue;
            }
            cmds.push(Command::SetShapeKind {
                shape: s.id,
                kind: c.kind,
            });
            if c.effects != s.effects || c.parked_effects != s.parked_effects {
                cmds.push(Command::SetEffectStack {
                    shape: s.id,
                    effects: c.effects,
                    parked: c.parked_effects,
                });
            }
        }
        if cmds.is_empty() {
            return false;
        }
        if let Err(e) = self.engine.run_batch("Bend with Pins", &cmds) {
            self.status = e.to_string();
            return false;
        }
        self.raster.borrow_mut().invalidate();
        true
    }
}

/// The preview: the outlines as they were (light) and bent (dark), the
/// pins as dots with a line from where each was stuck. A click on an
/// empty spot sticks a pin, a drag moves one, a right click takes it out.
fn preview(ui: &mut Ui, st: &mut PinsState) {
    let w = ui.available_width().max(240.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 300.0), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, Color32::WHITE);
    let v = st.view;
    let scale = (rect.width() as f64 / v.width()).min(rect.height() as f64 / v.height());
    let off = egui::vec2(
        (rect.width() - (v.width() * scale) as f32) / 2.0,
        (rect.height() - (v.height() * scale) as f32) / 2.0,
    );
    let to = |q: Point| {
        egui::pos2(
            rect.left() + off.x + ((q.x - v.x0) * scale) as f32,
            rect.bottom() - off.y - ((q.y - v.y0) * scale) as f32,
        )
    };
    let from = |s: egui::Pos2| {
        Point::new(
            v.x0 + (s.x - rect.left() - off.x) as f64 / scale,
            v.y0 + (rect.bottom() - off.y - s.y) as f64 / scale,
        )
    };
    let draw = |path: &BezPath, color: Color32, width: f32| {
        let mut pts: Vec<egui::Pos2> = Vec::new();
        let flush = |pts: &mut Vec<egui::Pos2>| {
            if pts.len() > 1 {
                p.add(egui::Shape::line(
                    std::mem::take(pts),
                    egui::Stroke::new(width, color),
                ));
            }
            pts.clear();
        };
        let mut start = None;
        kurbo::flatten(path, 0.2 / scale.max(1e-6), |el| match el {
            kurbo::PathEl::MoveTo(q) => {
                flush(&mut pts);
                start = Some(to(q));
                pts.push(to(q));
            }
            kurbo::PathEl::LineTo(q) => pts.push(to(q)),
            kurbo::PathEl::ClosePath => {
                if let Some(s) = start {
                    pts.push(s);
                }
            }
            _ => {}
        });
        flush(&mut pts);
    };
    let bent = !st.pins.is_empty();
    for o in &st.outlines {
        draw(o, Color32::from_gray(if bent { 0xC8 } else { 0x40 }), 1.0);
        if bent {
            let b = pins::bend_path(o, Affine::IDENTITY, &st.pins, st.reach);
            draw(&b, Color32::from_rgb(0x20, 0x50, 0xA0), 1.5);
        }
    }
    let hit =
        |s: egui::Pos2, pins: &[Pin]| pins.iter().position(|pin| to(pin.to).distance(s) <= 7.0);
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.drag_started() {
            st.drag = hit(pos, &st.pins);
        }
        if resp.dragged() {
            if let Some(i) = st.drag {
                st.pins[i].to = from(pos);
            }
        }
        if resp.clicked() && hit(pos, &st.pins).is_none() {
            let at = from(pos);
            let reach = st.reach;
            st.pins.push(Pin {
                from: pins::unbend_point(&st.pins, at, reach),
                to: at,
            });
        }
        if resp.secondary_clicked() {
            if let Some(i) = hit(pos, &st.pins) {
                st.pins.remove(i);
            }
        }
    }
    if resp.drag_stopped() {
        st.drag = None;
    }
    for pin in &st.pins {
        let (a, b) = (to(pin.from), to(pin.to));
        if a.distance(b) > 1.0 {
            p.line_segment([a, b], egui::Stroke::new(1.0, Color32::from_gray(0x90)));
        }
        p.circle_filled(b, 5.0, Color32::from_rgb(0xD0, 0x30, 0x30));
        p.circle_stroke(b, 5.0, egui::Stroke::new(1.0, Color32::WHITE));
    }
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut PinsState, close: &mut bool) {
    window(ctx, tr("pins.title")).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.label(
            egui::RichText::new(tr("pins.how"))
                .small()
                .color(crate::theme::Tokens::text_dim()),
        );
        preview(ui, st);
        ui.horizontal(|ui| {
            ui.label(tr("pins.reach"));
            ui.add(egui::Slider::new(&mut st.reach, 1.0..=3.0).step_by(0.1))
                .rich_tip(crate::ui::tips::keyed("pins.reach"));
            if ui
                .add_enabled(!st.pins.is_empty(), crate::ui::btn_icons::Ib::new("pins.clear"))
                .rich_tip(crate::ui::tips::keyed("pins.clear"))
                .clicked()
            {
                st.pins.clear();
            }
        });
        ui.add_space(6.0);
        if ok_cancel(ui, close) {
            let st2 = st.clone();
            if st2.pins.is_empty() || app.apply_pins(&st2) {
                *close = true;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{Shape, ShapeKind};

    #[test]
    fn pins_bend_the_selection_in_one_step() {
        let mut app = App::headless();
        let layer = app.active_layer().unwrap();
        let id = app.engine.new_shape_id();
        let rect = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 100.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        app.run(Command::AddShape { layer, shape: rect });
        app.select(vec![id]);
        app.open_pins_dialog();
        let Dialog::Pins(mut st) = app.dialog.clone() else {
            panic!("{:?}", app.dialog);
        };
        assert_eq!(st.outlines.len(), 1);
        assert!(st.view.contains(Point::new(50.0, 5.0)));
        // No pins: OK changes nothing.
        assert!(!app.apply_pins(&st));
        st.pins = vec![
            Pin {
                from: Point::new(5.0, 5.0),
                to: Point::new(5.0, 5.0),
            },
            Pin {
                from: Point::new(95.0, 5.0),
                to: Point::new(85.0, 45.0),
            },
        ];
        assert!(app.apply_pins(&st));
        let s = app.doc().find_shape(id).unwrap().clone();
        let ShapeKind::Path { path, .. } = &s.kind else {
            panic!("{:?}", s.kind);
        };
        let b = tracedraw_core::geometry::Shape::bounding_box(path);
        assert!(b.y1 > 40.0 && b.x0.abs() < 3.0, "{b:?}");
        app.undo();
        let s = app.doc().find_shape(id).unwrap();
        assert!(matches!(s.kind, ShapeKind::Rect { .. }));
    }
}

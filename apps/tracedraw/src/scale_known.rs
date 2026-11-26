//! Bitmaps > Scale to Known Size (#4215): a line or a rectangle drawn
//! over something of known size in a photo (the width of a door, a
//! wheel) and the real size typed in: the photo, and whatever else is
//! selected with it, grows or shrinks so the line measures that size.
//! The line stays put, the objects scaled about it, and goes once the
//! scale is done unless Keep the measuring object is ticked.

use crate::app::App;
use crate::i18n::tr;
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::{Affine, PathEl, Point},
    Command, ShapeId,
};

/// The dialog's choices; the size is in the drawing's units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleKnown {
    /// The real size of what the measuring object covers.
    pub size: f64,
    /// For a measuring object that is not a straight line: its width
    /// (false) or its height (true).
    pub height: bool,
    /// The measuring object stays after the scale.
    pub keep: bool,
}

impl Default for ScaleKnown {
    fn default() -> Self {
        ScaleKnown {
            size: 100.0,
            height: false,
            keep: false,
        }
    }
}

/// The two ends of a straight line on the page: an open curve of one
/// segment.
fn line_ends(s: &Shape) -> Option<(Point, Point)> {
    let ShapeKind::Path { path, closed } = &s.kind else {
        return None;
    };
    if *closed {
        return None;
    }
    let els = path.elements();
    if els.len() != 2 {
        return None;
    }
    let (PathEl::MoveTo(a), PathEl::LineTo(b)) = (els[0], els[1]) else {
        return None;
    };
    Some((s.transform * a, s.transform * b))
}

/// What a measuring object covers on the page, mm: a straight line's
/// length, else its box's width or height.
pub fn measured(s: &Shape, height: bool) -> Option<f64> {
    let v = match line_ends(s) {
        Some((a, b)) => (b - a).hypot(),
        None => {
            let b = s.bounds();
            if height {
                b.height()
            } else {
                b.width()
            }
        }
    };
    (v.is_finite() && v > 1e-6).then_some(v)
}

impl App {
    /// The measuring object of the selection and the objects it scales:
    /// the first selected object that is not a picture measures, the
    /// others scale. `None` without two objects.
    pub fn scale_known_parts(&self) -> Option<(Shape, Vec<ShapeId>)> {
        if self.selection.len() < 2 {
            return None;
        }
        let shapes = self.selected_shapes();
        let reference = shapes
            .iter()
            .find(|s| !matches!(s.kind, ShapeKind::Bitmap { .. }))
            .or_else(|| shapes.first())?
            .clone();
        let others: Vec<ShapeId> = shapes
            .iter()
            .map(|s| s.id)
            .filter(|id| *id != reference.id)
            .collect();
        (!others.is_empty()).then_some((reference, others))
    }

    /// Scale to Known Size is offered: two objects or more selected.
    pub fn can_scale_known(&self) -> bool {
        self.scale_known_parts().is_some()
    }

    /// Open Scale to Known Size with the measured size filled in.
    pub fn open_scale_known(&mut self) {
        let Some((reference, _)) = self.scale_known_parts() else {
            self.status = tr("status.scale_known_need");
            return;
        };
        let mut st = self.scale_known;
        // The size as drawn to start with, to type the real one over.
        if let Some(mm) = measured(&reference, st.height) {
            st.size = self.units.from_mm(mm);
        }
        self.dialog = crate::ui::dialogs::Dialog::ScaleKnown(st);
    }

    /// Scale the selection so its measuring object covers `real_mm`: the
    /// other selected objects scale about the measuring object's middle,
    /// which then goes unless `keep`. One undo step. False when nothing
    /// could be measured.
    pub fn scale_to_known(&mut self, real_mm: f64, height: bool, keep: bool) -> bool {
        let Some((reference, others)) = self.scale_known_parts() else {
            return false;
        };
        let Some(now) = measured(&reference, height) else {
            return false;
        };
        let k = real_mm / now;
        if !(k.is_finite() && k > 0.0) {
            return false;
        }
        let c = reference.bounds().center();
        let transform =
            Affine::translate(c.to_vec2()) * Affine::scale(k) * Affine::translate(-c.to_vec2());
        let mut cmds = vec![Command::TransformShapes {
            shapes: others.clone(),
            transform,
        }];
        if !keep {
            cmds.push(Command::DeleteShapes {
                shapes: vec![reference.id],
            });
        }
        if let Err(e) = self.engine.run_batch("Scale to Known Size", &cmds) {
            self.status = e.to_string();
            return false;
        }
        self.select(others);
        true
    }
}

/// The Scale to Known Size dialog.
pub fn dialog(app: &mut App, ctx: &egui::Context, st: &mut ScaleKnown, close: &mut bool) {
    use crate::ui::tips::RichTip as _;
    let u = app.units;
    let parts = app.scale_known_parts();
    let now = parts
        .as_ref()
        .and_then(|(r, _)| measured(r, st.height).map(|mm| (mm, line_ends(r).is_some())));
    let mut ok = false;
    crate::ui::dialogs::window(ctx, tr("dialog.scale_known")).show(ctx, |ui| {
        ui.set_min_width(320.0);
        ui.label(
            egui::RichText::new(tr("dialog.scale_known_hint"))
                .color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(6.0);
        egui::Grid::new("scale_known_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr("dialog.scale_known_measured"));
                ui.label(match now {
                    Some((mm, _)) => format!("{:.3} {}", u.from_mm(mm), u.short()),
                    None => "-".into(),
                });
                ui.end_row();
                // Not a straight line: which side of its box measures.
                if now.is_some_and(|(_, line)| !line) {
                    ui.label(tr("dialog.scale_known_side"));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut st.height, false, tr("dialog.scale_known_width"))
                            .rich_tip(crate::ui::tips::keyed("dialog.scale_known_side"));
                        ui.radio_value(&mut st.height, true, tr("dialog.scale_known_height"))
                            .rich_tip(crate::ui::tips::keyed("dialog.scale_known_side"));
                    });
                    ui.end_row();
                }
                ui.label(tr("dialog.scale_known_size"));
                ui.add(
                    crate::ui::field::NumField::new(&mut st.size)
                        .range(u.from_mm(0.001)..=u.from_mm(1.0e7))
                        .speed(0.1)
                        .max_decimals(3)
                        .suffix(format!(" {}", u.short())),
                )
                .rich_tip(crate::ui::tips::keyed("dialog.scale_known_size"));
                ui.end_row();
                if let Some((mm, _)) = now {
                    let k = u.to_mm(st.size) / mm;
                    if k.is_finite() {
                        ui.label(tr("dialog.scale_known_factor"));
                        ui.label(format!("{:.2} %", k * 100.0));
                        ui.end_row();
                    }
                }
            });
        ui.add_space(4.0);
        ui.checkbox(&mut st.keep, tr("dialog.scale_known_keep"))
            .rich_tip(crate::ui::tips::keyed("dialog.scale_known_keep"));
        ui.add_space(6.0);
        ui.add_enabled_ui(now.is_some(), |ui| {
            if crate::ui::dialogs::ok_cancel(ui, close) {
                ok = true;
            }
        });
        if now.is_none() {
            ui.horizontal(|ui| {
                if ui.button(tr("dialog.cancel")).clicked() {
                    *close = true;
                }
            });
        }
    });
    if ok {
        app.scale_known = *st;
        app.scale_to_known(u.to_mm(st.size), st.height, st.keep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{BezPath, Rect};

    fn picture(app: &mut App) -> ShapeId {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(4, 4)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("png");
        app.new_shape(ShapeKind::Bitmap {
            rect: Rect::new(0.0, 0.0, 200.0, 100.0),
            width_px: 4,
            height_px: 4,
            png: png.into_inner().into(),
            fx: None,
        })
        .expect("picture")
    }

    /// A line across a door 50 mm wide in the photo, told the door is
    /// 800 mm: the photo grows 16 times about the line, the line goes;
    /// a rectangle measures by its width or height and can stay.
    #[test]
    fn a_photo_scales_to_the_size_of_a_measured_line() {
        let mut app = App::headless();
        let photo = picture(&mut app);
        let mut p = BezPath::new();
        p.move_to((20.0, 50.0));
        p.line_to((70.0, 50.0));
        let line = app
            .new_shape(ShapeKind::Path {
                path: p,
                closed: false,
            })
            .expect("line");
        app.select(vec![line, photo]);
        assert!(app.can_scale_known());
        assert!(app.scale_to_known(800.0, false, false));
        assert!(app.doc().find_shape(line).is_none(), "the line goes");
        assert_eq!(app.selection, vec![photo]);
        let b = app.doc().find_shape(photo).expect("photo").bounds();
        assert!((b.width() - 3200.0).abs() < 1e-6, "{b:?}");
        // About the line's middle (45, 50), which stays where it was.
        assert!((b.x0 - (45.0 - 45.0 * 16.0)).abs() < 1e-6, "{b:?}");
        // A rectangle by its height, kept.
        let rect = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![photo, rect]);
        assert!(app.scale_to_known(20.0, true, true));
        assert!(app.doc().find_shape(rect).is_some(), "kept");
        let b2 = app.doc().find_shape(photo).expect("photo").bounds();
        assert!((b2.width() - 1600.0).abs() < 1e-6, "{b2:?}");
        // One object alone: nothing to scale.
        app.select(vec![photo]);
        assert!(!app.can_scale_known());
        assert!(!app.scale_to_known(10.0, false, false));
    }
}

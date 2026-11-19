//! Object > Insert > Formula (#2183): a formula typed in the backslash
//! syntax, with a preview, inserted as one curve object that keeps its
//! source; with a formula selected, the same dialog edits it.

use crate::app::App;
use crate::formula::{self, FormulaSpec};
use crate::i18n::{tr, trf};
use crate::ui::dialogs::{ok_cancel, window, Dialog};
use crate::ui::tips::RichTip;
use egui::{Color32, Context, TextureOptions, Ui};
use tracedraw_core::geometry::{Affine, Point};
use tracedraw_core::{Command, Fill, Shape, ShapeId, ShapeKind};

/// The dialog's state: the settings, and the formula object edited (none
/// when inserting a new one).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormulaState {
    pub spec: FormulaSpec,
    pub editing: Option<ShapeId>,
}

impl App {
    /// The selected formula, if exactly one is selected.
    pub fn selected_formula(&self) -> Option<(ShapeId, FormulaSpec)> {
        let [id] = self.selection.as_slice() else {
            return None;
        };
        let s = self.doc().find_shape(*id)?;
        Some((*id, FormulaSpec::of_shape(s)?))
    }

    /// Open the dialog: on the selected formula, else for a new one (in
    /// the settings used last).
    pub fn open_formula_dialog(&mut self) {
        let st = match self.selected_formula() {
            Some((id, spec)) => FormulaState {
                spec,
                editing: Some(id),
            },
            None => FormulaState {
                spec: self.last_formula.clone().unwrap_or_default(),
                editing: None,
            },
        };
        self.dialog = Dialog::Formula(st);
    }

    /// Insert the formula of `st`, or put it in place of the one edited
    /// (keeping that object's place, size, colour and outline). The id of
    /// the formula object.
    pub fn apply_formula(&mut self, st: &FormulaState) -> Option<ShapeId> {
        let laid = formula::layout(&st.spec);
        if laid.path.elements().is_empty() {
            self.status = tr("formula.empty");
            return None;
        }
        self.last_formula = Some(st.spec.clone());
        let laid_path = page_curve(&laid.path);
        let data = vec![(formula::DATA_KEY.to_string(), st.spec.to_data())];
        if let Some(id) = st.editing {
            let old = self.doc().find_shape(id)?.clone();
            // The new curve with its top left where the old one's was.
            let at = match &old.kind {
                ShapeKind::Path { path, .. } => {
                    let b = tracedraw_core::geometry::Shape::bounding_box(path);
                    Point::new(b.x0, b.y1)
                }
                _ => Point::ZERO,
            };
            let h = tracedraw_core::geometry::Shape::bounding_box(&laid_path).height();
            let path = Affine::translate((at.x, at.y - h)) * laid_path;
            let cmds = [
                Command::SetShapeKind {
                    shape: id,
                    kind: ShapeKind::Path { path, closed: true },
                },
                Command::SetObjectData { shape: id, data },
            ];
            if let Err(e) = self.engine.run_batch("Edit Formula", &cmds) {
                self.status = e.to_string();
                return None;
            }
            self.select(vec![id]);
            return Some(id);
        }
        let layer = self.active_layer()?;
        let b = tracedraw_core::geometry::Shape::bounding_box(&laid_path);
        let c = self.page_rect().center();
        let path = Affine::translate((c.x - b.width() / 2.0, c.y - b.height() / 2.0)) * laid_path;
        let id = self.engine.new_shape_id();
        let mut shape = Shape::new(id, ShapeKind::Path { path, closed: true });
        shape.fill = Fill::Solid(tracedraw_core::Color::BLACK);
        shape.stroke = None;
        shape.name = Some(tr("formula.object_name"));
        shape.data = data;
        if let Err(e) = self
            .engine
            .run_batch("Insert Formula", &[Command::AddShape { layer, shape }])
        {
            self.status = e.to_string();
            return None;
        }
        self.select(vec![id]);
        Some(id)
    }
}

/// The laid-out curve for the page, which has Y up. The layout is Y down,
/// as the preview draws it, and went onto the page as it was: every
/// formula came in upside down (#3155). The box stays where it was.
fn page_curve(laid: &tracedraw_core::BezPath) -> tracedraw_core::BezPath {
    let h = tracedraw_core::geometry::Shape::bounding_box(laid).height();
    Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, h]) * laid.clone()
}

/// The preview: the formula drawn black on white, fitted to the room.
fn preview(ui: &mut Ui, spec: &FormulaSpec) -> Vec<String> {
    let w = ui.available_width().max(200.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 120.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, Color32::WHITE);
    let laid = formula::layout(spec);
    let b = tracedraw_core::geometry::Shape::bounding_box(&laid.path);
    if laid.path.elements().is_empty() || !b.is_finite() || b.width() <= 0.0 {
        return laid.unknown;
    }
    let ppp = ui.ctx().pixels_per_point();
    let room = rect.shrink(8.0);
    // Pixels per mm: the real size at 96 dpi, smaller when it is too big.
    let scale = (96.0 / 25.4f64)
        .min(room.width() as f64 / b.width())
        .min(room.height() as f64 / b.height().max(1e-6));
    let (pw, ph) = (
        ((b.width() * scale) as f32 * ppp).ceil().max(1.0) as u32,
        ((b.height() * scale) as f32 * ppp).ceil().max(1.0) as u32,
    );
    let Some(mut pix) = tiny_skia::Pixmap::new(pw, ph) else {
        return laid.unknown;
    };
    let s = scale * ppp as f64;
    let mut pb = tiny_skia::PathBuilder::new();
    for el in laid.path.elements() {
        use kurbo::PathEl::*;
        let f = |q: kurbo::Point| ((q.x * s) as f32, (q.y * s) as f32);
        match *el {
            MoveTo(a) => {
                let (x, y) = f(a);
                pb.move_to(x, y)
            }
            LineTo(a) => {
                let (x, y) = f(a);
                pb.line_to(x, y)
            }
            QuadTo(a, c) => {
                let ((x1, y1), (x, y)) = (f(a), f(c));
                pb.quad_to(x1, y1, x, y)
            }
            CurveTo(a, c, d) => {
                let ((x1, y1), (x2, y2), (x, y)) = (f(a), f(c), f(d));
                pb.cubic_to(x1, y1, x2, y2, x, y)
            }
            ClosePath => pb.close(),
        }
    }
    if let Some(path) = pb.finish() {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        paint.anti_alias = true;
        pix.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }
    let img = egui::ColorImage::from_rgba_premultiplied([pw as usize, ph as usize], pix.data());
    let tex = ui
        .ctx()
        .load_texture("formula_preview", img, TextureOptions::LINEAR);
    let size = egui::vec2(pw as f32 / ppp, ph as f32 / ppp);
    let r = egui::Rect::from_center_size(room.center(), size);
    p.image(
        tex.id(),
        r,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    laid.unknown
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut FormulaState, close: &mut bool) {
    let title = if st.editing.is_some() {
        tr("formula.edit_title")
    } else {
        tr("formula.title")
    };
    window(ctx, title).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.label(tr("formula.source"));
        let edit = ui
            .add(
                egui::TextEdit::multiline(&mut st.spec.source)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            )
            .rich_tip(crate::ui::tips::keyed("formula.source"));
        if !ui.memory(|m| m.focused().is_some()) {
            edit.request_focus();
        }
        ui.label(
            egui::RichText::new(tr("formula.syntax"))
                .small()
                .color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(tr("formula.font"));
            let (pick, _) =
                crate::ui::tips::tip_over(ui, crate::ui::tips::keyed("formula.font"), |ui| {
                    crate::ui::font_list::font_list_live(
                        app,
                        ui,
                        "formula_font",
                        &st.spec.font,
                        200.0,
                    )
                });
            if let Some(p) = pick {
                st.spec.font = p.family;
            }
            ui.label(tr("formula.size"));
            ui.add(
                crate::ui::field::NumField::new(&mut st.spec.size_pt)
                    .range(4.0..=400.0)
                    .speed(0.5)
                    .suffix(" pt"),
            )
            .rich_tip(crate::ui::tips::keyed("formula.size"));
        });
        ui.add_space(4.0);
        ui.label(tr("formula.preview"));
        let unknown = preview(ui, &st.spec);
        if !unknown.is_empty() {
            ui.colored_label(
                Color32::from_rgb(0xC4, 0x2B, 0x1C),
                trf("formula.unknown", &[("names", &unknown.join(" "))]),
            );
        }
        ui.add_space(6.0);
        if ok_cancel(ui, close) {
            let st2 = st.clone();
            if app.apply_formula(&st2).is_some() {
                *close = true;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_formula_is_inserted_as_a_curve_and_edited_again() {
        let mut app = App::headless();
        app.open_formula_dialog();
        let Dialog::Formula(mut st) = app.dialog.clone() else {
            panic!("{:?}", app.dialog);
        };
        assert!(st.editing.is_none());
        st.spec.source = r"\frac{a}{b}".into();
        let id = app.apply_formula(&st).expect("inserted");
        let s = app.doc().find_shape(id).expect("shape").clone();
        assert!(matches!(s.kind, ShapeKind::Path { closed: true, .. }));
        assert_eq!(s.fill, Fill::Solid(tracedraw_core::Color::BLACK));
        assert_eq!(FormulaSpec::of_shape(&s), Some(st.spec.clone()));
        let before = s.bounds();
        // Painted red and moved: editing keeps both.
        let red = Fill::Solid(tracedraw_core::Color::Rgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        });
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: red.clone(),
        });
        app.run(Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::translate((10.0, 5.0)),
        });
        app.open_formula_dialog();
        let Dialog::Formula(mut st) = app.dialog.clone() else {
            panic!();
        };
        assert_eq!(st.editing, Some(id));
        assert_eq!(st.spec.source, r"\frac{a}{b}");
        st.spec.source = r"\frac{a}{b} + \sqrt{c}".into();
        assert_eq!(app.apply_formula(&st), Some(id));
        let s = app.doc().find_shape(id).unwrap().clone();
        assert_eq!(s.fill, red);
        let after = s.bounds();
        assert!(
            (after.x0 - before.x0 - 10.0).abs() < 1e-6,
            "{before:?} {after:?}"
        );
        assert!(after.width() > before.width() * 1.5);
        assert_eq!(FormulaSpec::of_shape(&s).unwrap().source, st.spec.source);
        // One undo step back to the fraction.
        app.undo();
        let s = app.doc().find_shape(id).unwrap();
        assert_eq!(FormulaSpec::of_shape(s).unwrap().source, r"\frac{a}{b}");
        // A new formula starts from the settings used last.
        app.select(vec![]);
        app.open_formula_dialog();
        let Dialog::Formula(st) = app.dialog.clone() else {
            panic!();
        };
        assert_eq!(st.spec.source, r"\frac{a}{b} + \sqrt{c}");
    }

    /// A formula comes in upright: the page has Y up, the layout Y down,
    /// and every formula came in upside down. The exponent of `a^{2}` is
    /// the highest ink of the object, and an edited formula keeps its top
    /// left corner.
    #[test]
    fn a_formula_comes_in_upright() {
        let mut app = App::headless();
        app.open_formula_dialog();
        let Dialog::Formula(mut st) = app.dialog.clone() else {
            panic!("{:?}", app.dialog);
        };
        st.spec.source = "a^{2}".into();
        let id = app.apply_formula(&st).expect("inserted");
        let s = app.doc().find_shape(id).unwrap().clone();
        let ShapeKind::Path { path, .. } = &s.kind else {
            panic!("{:?}", s.kind);
        };
        let b = tracedraw_core::geometry::Shape::bounding_box(path);
        let mid = b.x0 + 0.6 * b.width();
        let top = |right: bool| {
            path.elements()
                .iter()
                .filter_map(|e| e.end_point())
                .filter(|p| (p.x > mid) == right)
                .map(|p| p.y)
                .fold(f64::MIN, f64::max)
        };
        assert!(
            top(true) > top(false) + 0.2,
            "{} {} {b:?}",
            top(true),
            top(false)
        );
        assert!((top(true) - b.y1).abs() < 0.2, "{} {b:?}", top(true));
        // A taller formula in its place keeps the top left corner.
        app.open_formula_dialog();
        let Dialog::Formula(mut st) = app.dialog.clone() else {
            panic!();
        };
        st.spec.source = r"\frac{a^{2}}{b}".into();
        app.apply_formula(&st).expect("edited");
        let s = app.doc().find_shape(id).unwrap();
        let after = s.bounds();
        assert!(
            (after.x0 - b.x0).abs() < 1e-6 && (after.y1 - b.y1).abs() < 1e-6,
            "{b:?} {after:?}"
        );
        assert!(after.height() > b.height());
    }

    #[test]
    fn an_empty_formula_inserts_nothing() {
        let mut app = App::headless();
        let st = FormulaState {
            spec: FormulaSpec {
                source: "  ".into(),
                ..Default::default()
            },
            editing: None,
        };
        assert!(app.apply_formula(&st).is_none());
        assert_eq!(app.status, tr("formula.empty"));
    }
}

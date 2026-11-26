//! Text > Text Height (#4094): the size of a text given as the height its
//! capital letters stand (the height of an H, which has no descender), or
//! of its small letters (an x) when it has no capitals, rather than as a
//! point size. The point sizes of all the text's characters change by the
//! same factor, so sizes within the text keep their proportions. See
//! docs/behavior/text.md, Text height.

use crate::app::App;
use crate::i18n::tr;
use crate::ui::tips::RichTip;
use tracedraw_core::document::{ShapeKind, TextSpan};
use tracedraw_core::geometry::Shape as _;
use tracedraw_core::{Command, Shape};

/// The letter measured: an H when the text has a capital letter, an x
/// otherwise.
fn reference(spans: &[TextSpan]) -> &'static str {
    if spans.iter().any(|s| s.text.chars().any(char::is_uppercase)) {
        "H"
    } else {
        "x"
    }
}

/// How tall the text's capitals (or small letters) stand on the page,
/// mm, as the first character's font and size draw them and the object's
/// own scale stretches them.
pub fn text_height_mm(s: &Shape) -> Option<f64> {
    let ShapeKind::Text { spans, .. } = &s.kind else {
        return None;
    };
    let first = spans.iter().find(|sp| !sp.text.trim().is_empty())?;
    let probe = TextSpan {
        text: reference(spans).to_string(),
        ..first.clone()
    };
    let b = tracedraw_text::fonts().outline(&[probe]).path.bounding_box();
    let h = b.height();
    if !(h.is_finite() && h > 1e-9) {
        return None;
    }
    let c = s.transform.as_coeffs();
    let scale_y = c[2].hypot(c[3]);
    Some(h * if scale_y > 1e-9 { scale_y } else { 1.0 })
}

impl App {
    /// Text > Text Height...: the window opens on the first selected
    /// text's height, in the drawing's units.
    pub fn open_text_height(&mut self) {
        let Some(mm) = self.text_shapes().first().and_then(text_height_mm) else {
            return;
        };
        self.dialog = crate::ui::dialogs::Dialog::TextHeight {
            height: self.units.from_mm(mm),
        };
    }

    /// Every selected text sized so its capitals (or small letters) stand
    /// `mm` tall; one undo step for all of them.
    pub fn set_text_height(&mut self, mm: f64) {
        if !(mm.is_finite() && mm > 0.0) {
            return;
        }
        let mut cmds = Vec::new();
        for s in self.text_shapes() {
            let Some(now) = text_height_mm(&s) else {
                continue;
            };
            let k = mm / now;
            if (k - 1.0).abs() < 1e-9 {
                continue;
            }
            let ShapeKind::Text {
                mut spans,
                origin,
                frame,
                align,
                para,
                on_path,
            } = s.kind.clone()
            else {
                continue;
            };
            for sp in spans.iter_mut() {
                sp.size_pt *= k;
                sp.baseline_shift_pt *= k;
            }
            cmds.push(Command::SetShapeKind {
                shape: s.id,
                kind: ShapeKind::Text {
                    spans,
                    origin,
                    frame,
                    align,
                    para,
                    on_path,
                },
            });
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.run_batch_keeping_lines("Text Height", &cmds) {
            log::warn!("text height: {e}");
        }
    }
}

/// The Text Height window: one field, the height in the drawing's units.
pub fn dialog(app: &mut App, ctx: &egui::Context, height: &mut f64, close: &mut bool) {
    let u = app.units;
    crate::ui::dialogs::window(ctx, tr("dialog.text_height")).show(ctx, |ui| {
        ui.label(tr("dialog.text_height_desc"));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(tr("dialog.text_height_field"));
            ui.add(
                crate::ui::field::NumField::new(height)
                    .range(u.from_mm(0.01)..=u.from_mm(5000.0))
                    .speed(0.1)
                    .suffix(format!(" {}", u.short())),
            )
            .rich_tip(crate::ui::tips::pair(
                "dialog.text_height_field",
                "dialog.text_height_desc",
            ));
        });
        ui.add_space(4.0);
        if crate::ui::dialogs::ok_cancel(ui, close) {
            app.set_text_height(u.to_mm(*height));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(app: &mut App, s: &str, pt: f64) -> tracedraw_core::ShapeId {
        let mut a = TextSpan::new(s, "Sans", pt);
        a.bold = false;
        let mut b = a.clone();
        b.text = "!".into();
        b.size_pt = pt * 2.0;
        app.new_shape(ShapeKind::Text {
            spans: vec![a, b],
            origin: Default::default(),
            frame: None,
            align: Default::default(),
            para: Default::default(),
            on_path: None,
        })
        .expect("text")
    }

    /// #4094: a text given a height of 25.4 mm has capitals 25.4 mm tall,
    /// its other sizes in proportion, in one undo step; a text without
    /// capitals is measured by its small letters.
    #[test]
    fn text_height_sets_the_height_of_the_capitals() {
        let mut app = App::headless();
        let id = text(&mut app, "Hello", 24.0);
        app.select(vec![id]);
        let shape = |app: &App| app.doc().find_shape(id).cloned().expect("text");
        let before = text_height_mm(&shape(&app)).expect("measured");
        // A capital is shorter than the point size says.
        assert!(before > 0.0 && before < 24.0 * 25.4 / 72.0, "{before}");
        app.set_text_height(25.4);
        let after = text_height_mm(&shape(&app)).expect("measured");
        assert!((after - 25.4).abs() < 0.05, "{after}");
        let ShapeKind::Text { spans, .. } = shape(&app).kind else {
            panic!("text");
        };
        assert!((spans[1].size_pt / spans[0].size_pt - 2.0).abs() < 1e-9);
        app.undo();
        assert!((text_height_mm(&shape(&app)).expect("m") - before).abs() < 1e-9);
        // The window opens on the height, in the drawing's units.
        app.open_text_height();
        assert!(matches!(
            app.dialog,
            crate::ui::dialogs::Dialog::TextHeight { height } if (app.units.to_mm(height) - before).abs() < 1e-6
        ));
        // Small letters only: an x is measured, shorter than an H.
        let small = text(&mut app, "hello", 24.0);
        let s = app.doc().find_shape(small).cloned().expect("text");
        let x = text_height_mm(&s).expect("measured");
        assert!(x < before, "{x} {before}");
    }
}

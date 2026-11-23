//! Object > Transformations > Transform Each: every selected object
//! scaled, moved and turned by the same amounts, each about its own
//! centre, in one undo step; with Random on, each object takes its own
//! amount, somewhere between none and the amount set.

use crate::app::App;
use crate::i18n::tr;
use crate::ui::dialogs::{ok_cancel, window, Dialog};
use crate::ui::tips::RichTip;
use egui::Context;
use tracedraw_core::geometry::{Affine, Rect, Vec2};
use tracedraw_core::Command;

/// The dialog's settings, kept for the next time it opens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformEach {
    /// Horizontal and vertical scale, percent.
    pub scale: [f64; 2],
    /// Horizontal and vertical move, mm (up positive).
    pub offset: [f64; 2],
    /// Turn, degrees counterclockwise.
    pub angle: f64,
    pub random: bool,
}

impl Default for TransformEach {
    fn default() -> Self {
        TransformEach {
            scale: [100.0, 100.0],
            offset: [0.0, 0.0],
            angle: 0.0,
            random: false,
        }
    }
}

impl TransformEach {
    /// The transform of an object whose box is `b`, the amounts taken in
    /// the share `k` (0 none, 1 all) of each of scale H, scale V, move H,
    /// move V and turn.
    pub fn affine(&self, b: Rect, k: [f64; 5]) -> Affine {
        let part = |full: f64, none: f64, k: f64| none + (full - none) * k.clamp(0.0, 1.0);
        let sx = part(self.scale[0], 100.0, k[0]) / 100.0;
        let sy = part(self.scale[1], 100.0, k[1]) / 100.0;
        let dx = part(self.offset[0], 0.0, k[2]);
        let dy = part(self.offset[1], 0.0, k[3]);
        let a = part(self.angle, 0.0, k[4]).to_radians();
        let c = b.center().to_vec2();
        Affine::translate(c + Vec2::new(dx, dy))
            * Affine::rotate(a)
            * Affine::scale_non_uniform(sx.max(1e-6), sy.max(1e-6))
            * Affine::translate(-c)
    }
}

/// Shares for one object: all of each amount, or random ones.
fn shares(random: bool, rng: &mut u64) -> [f64; 5] {
    if !random {
        return [1.0; 5];
    }
    let mut k = [0.0; 5];
    for v in &mut k {
        // xorshift64*
        *rng ^= *rng >> 12;
        *rng ^= *rng << 25;
        *rng ^= *rng >> 27;
        let x = rng.wrapping_mul(0x2545_F491_4F6C_DD1D);
        *v = (x >> 11) as f64 / (1u64 << 53) as f64;
    }
    k
}

impl App {
    pub fn open_transform_each(&mut self) {
        if !self.selection.is_empty() {
            self.dialog = Dialog::TransformEach(self.last_transform_each);
        }
    }

    /// Transform each selected object by `st`, about its own centre; the
    /// random amounts drawn from `seed`. One undo step.
    pub fn transform_each(&mut self, st: &TransformEach, seed: u64) {
        let shapes = self.selected_shapes();
        let boxes = self.selected_bounds();
        if shapes.is_empty() {
            return;
        }
        self.last_transform_each = *st;
        let mut rng = seed | 1;
        let mark = self.engine.begin_group();
        let mut ids = Vec::new();
        for (s, b) in shapes.iter().zip(boxes.iter()) {
            let t = st.affine(*b, shares(st.random, &mut rng));
            self.run(Command::TransformShapes {
                shapes: vec![s.id],
                transform: t,
            });
            ids.push(s.id);
        }
        self.apply_hinting(&ids);
        self.reflow_chains_of(&ids);
        self.engine.end_group(mark, "Transform Each");
        self.select(ids);
    }
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut TransformEach, close: &mut bool) {
    let units = app.units;
    window(ctx, tr("transform_each.title")).show(ctx, |ui| {
        ui.set_width(320.0);
        egui::Grid::new("transform_each")
            .num_columns(3)
            .show(ui, |ui| {
                ui.label(tr("transform_each.scale"));
                for v in &mut st.scale {
                    ui.add(
                        crate::ui::field::NumField::new(v)
                            .range(1.0..=1000.0)
                            .speed(1.0)
                            .suffix(" %"),
                    )
                    .rich_tip(crate::ui::tips::keyed("transform_each.scale"));
                }
                ui.end_row();
                ui.label(tr("transform_each.move"));
                for v in &mut st.offset {
                    let mut u = units.from_mm(*v);
                    if ui
                        .add(
                            crate::ui::field::NumField::new(&mut u)
                                .speed(0.1)
                                .suffix(format!(" {}", units.short())),
                        )
                        .rich_tip(crate::ui::tips::keyed("transform_each.move"))
                        .changed()
                    {
                        *v = units.to_mm(u);
                    }
                }
                ui.end_row();
                ui.label(tr("transform_each.angle"));
                ui.add(
                    crate::ui::field::NumField::new(&mut st.angle)
                        .range(-360.0..=360.0)
                        .speed(1.0)
                        .suffix("°"),
                )
                .rich_tip(crate::ui::tips::keyed("transform_each.angle"));
                ui.end_row();
            });
        ui.checkbox(&mut st.random, tr("transform_each.random"))
            .rich_tip(crate::ui::tips::keyed("transform_each.random"));
        ui.add_space(6.0);
        if ok_cancel(ui, close) {
            let seed = web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1);
            let st = *st;
            app.transform_each(&st, seed);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::ShapeKind;

    fn rect(app: &mut App, x: f64) -> tracedraw_core::ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x, 0.0, x + 10.0, 10.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    }

    #[test]
    fn each_object_scales_about_its_own_centre_in_one_step() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0);
        let b = rect(&mut app, 50.0);
        app.select(vec![a, b]);
        let st = TransformEach {
            scale: [200.0, 50.0],
            offset: [1.0, 2.0],
            ..Default::default()
        };
        app.transform_each(&st, 7);
        let boxes = app.selected_bounds();
        // Each twice as wide and half as tall about its own centre, moved.
        assert!((boxes[0].x0 - -4.0).abs() < 1e-6, "{boxes:?}");
        assert!((boxes[0].width() - 20.0).abs() < 1e-6);
        assert!((boxes[0].height() - 5.0).abs() < 1e-6);
        assert!((boxes[0].center().y - 7.0).abs() < 1e-6);
        assert!((boxes[1].center().x - 56.0).abs() < 1e-6, "{boxes:?}");
        app.undo();
        let boxes = app.selected_bounds();
        assert!((boxes[1].width() - 10.0).abs() < 1e-6);
        assert_eq!(app.last_transform_each, st);
    }

    #[test]
    fn random_amounts_lie_between_none_and_the_amount_set() {
        let mut app = App::headless();
        let ids: Vec<_> = (0..8)
            .map(|i| rect(&mut app, f64::from(i) * 20.0))
            .collect();
        app.select(ids);
        let st = TransformEach {
            scale: [300.0, 300.0],
            random: true,
            ..Default::default()
        };
        app.transform_each(&st, 12345);
        let w: Vec<f64> = app.selected_bounds().iter().map(|b| b.width()).collect();
        assert!(w.iter().all(|w| (10.0..=30.0).contains(w)), "{w:?}");
        // Not all the same.
        assert!(w.iter().any(|x| (x - w[0]).abs() > 0.5), "{w:?}");
    }
}

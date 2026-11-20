//! Object > Symbol > Stretch Zones: four lines set in from the sides of
//! the selected instance's symbol. A stretched instance keeps the corner
//! zones their size, stretches the side zones along their side and the
//! middle zone both ways; every instance of the symbol follows.

use crate::app::App;
use crate::i18n::tr;
use crate::ui::dialogs::{ok_cancel, window, Dialog};
use crate::ui::tips::RichTip;
use egui::{Color32, Context, Ui};
use tracedraw_core::document::ScaleGrid;
use tracedraw_core::Command;

/// The dialog's state: the symbol, whether it has zones, the four insets
/// (mm of the symbol's own size) and that size.
#[derive(Debug, Clone, PartialEq)]
pub struct StretchZonesState {
    pub index: usize,
    pub on: bool,
    pub grid: ScaleGrid,
    pub width: f64,
    pub height: f64,
}

impl Default for StretchZonesState {
    fn default() -> Self {
        Self {
            index: 0,
            on: true,
            grid: ScaleGrid::default(),
            width: 10.0,
            height: 10.0,
        }
    }
}

impl App {
    /// The symbol of the one selected instance.
    fn selected_symbol_index(&self) -> Option<usize> {
        let id = self.selected_symbol_instance()?;
        match self.doc().find_shape(id)?.kind {
            tracedraw_core::ShapeKind::SymbolInstance { index } => Some(index),
            _ => None,
        }
    }

    /// Whether Stretch Zones applies: one instance selected.
    pub fn can_edit_stretch_zones(&self) -> bool {
        self.selected_symbol_index().is_some()
    }

    /// Open the dialog on the selected instance's symbol: its zones, or a
    /// quarter of its shorter side in from each side.
    pub fn open_stretch_zones_dialog(&mut self) {
        let Some(index) = self.selected_symbol_index() else {
            return;
        };
        let Some(sym) = self.doc().symbols.get(index) else {
            return;
        };
        let b = sym.shape.bounds();
        let (w, h) = (b.width().max(0.0), b.height().max(0.0));
        let q = 0.25 * w.min(h);
        let (on, grid) = match sym.shape.scale_grid.as_deref().copied() {
            Some(g) => (true, g),
            None => (
                true,
                ScaleGrid {
                    left: q,
                    right: q,
                    top: q,
                    bottom: q,
                },
            ),
        };
        self.dialog = Dialog::StretchZones(StretchZonesState {
            index,
            on,
            grid,
            width: w,
            height: h,
        });
    }

    /// Set the symbol's zones from `st`, one undo step.
    pub fn apply_stretch_zones(&mut self, st: &StretchZonesState) -> bool {
        let grid = st.on.then_some(st.grid);
        if let Err(e) = self.engine.run_batch(
            "Stretch Zones",
            &[Command::SetSymbolScaleGrid {
                index: st.index,
                grid,
            }],
        ) {
            self.status = e.to_string();
            return false;
        }
        self.raster.borrow_mut().invalidate();
        true
    }
}

/// A sketch of the nine zones: the symbol's box with the four lines.
fn sketch(ui: &mut Ui, st: &StretchZonesState) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(200.0, 120.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, Color32::WHITE);
    let (w, h) = (st.width.max(1e-6), st.height.max(1e-6));
    let room = rect.shrink(10.0);
    let scale = (room.width() as f64 / w).min(room.height() as f64 / h) as f32;
    let size = egui::vec2(w as f32 * scale, h as f32 * scale);
    let r = egui::Rect::from_center_size(room.center(), size);
    let ink = egui::Stroke::new(1.0, Color32::from_gray(0x40));
    p.rect_stroke(r, 0.0, ink, egui::StrokeKind::Inside);
    if !st.on {
        return;
    }
    let line = egui::Stroke::new(1.0, Color32::from_rgb(0x20, 0x70, 0xD0));
    let g = st.grid;
    let x0 = r.left() + (g.left as f32 * scale).min(size.x);
    let x1 = r.right() - (g.right as f32 * scale).min(size.x);
    let y0 = r.top() + (g.top as f32 * scale).min(size.y);
    let y1 = r.bottom() - (g.bottom as f32 * scale).min(size.y);
    for x in [x0, x1] {
        p.line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.bottom())], line);
    }
    for y in [y0, y1] {
        p.line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], line);
    }
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut StretchZonesState, close: &mut bool) {
    window(ctx, tr("stretch_zones.title")).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.checkbox(&mut st.on, tr("stretch_zones.on"))
            .rich_tip(crate::ui::tips::keyed("stretch_zones.on"));
        ui.add_enabled_ui(st.on, |ui| {
            egui::Grid::new("stretch_zones").show(ui, |ui| {
                let rows = [
                    ("stretch_zones.left", st.width),
                    ("stretch_zones.right", st.width),
                    ("stretch_zones.top", st.height),
                    ("stretch_zones.bottom", st.height),
                ];
                for (i, (key, max)) in rows.into_iter().enumerate() {
                    let v = match i {
                        0 => &mut st.grid.left,
                        1 => &mut st.grid.right,
                        2 => &mut st.grid.top,
                        _ => &mut st.grid.bottom,
                    };
                    ui.label(tr(key));
                    ui.add(
                        crate::ui::field::NumField::new(v)
                            .range(0.0..=max.max(0.0))
                            .speed(0.1)
                            .suffix(" mm"),
                    )
                    .rich_tip(crate::ui::tips::keyed(key));
                    ui.end_row();
                }
            });
        });
        sketch(ui, st);
        ui.add_space(6.0);
        if ok_cancel(ui, close) {
            let st2 = st.clone();
            if app.apply_stretch_zones(&st2) {
                *close = true;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Affine, Rect};
    use tracedraw_core::{Shape, ShapeId, ShapeKind};

    #[test]
    fn zones_go_on_the_symbol_of_the_selected_instance() {
        let mut app = App::headless();
        let def = Shape::new(
            ShapeId(0),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 40.0, 20.0),
                radius: 0.0,
                corners: None,
            },
        );
        app.run(Command::AddSymbol {
            symbol: tracedraw_core::Symbol {
                name: "Frame".into(),
                shape: def,
            },
        });
        let layer = app.active_layer().unwrap();
        let id = app.engine.new_shape_id();
        let mut inst = Shape::new(id, ShapeKind::SymbolInstance { index: 0 });
        inst.transform = Affine::scale_non_uniform(2.0, 1.0);
        app.run(Command::AddShape { layer, shape: inst });
        app.select(vec![]);
        assert!(!app.can_edit_stretch_zones());
        app.select(vec![id]);
        assert!(app.can_edit_stretch_zones());
        app.open_stretch_zones_dialog();
        let Dialog::StretchZones(mut st) = app.dialog.clone() else {
            panic!("{:?}", app.dialog);
        };
        // A quarter of the shorter side in from each side.
        assert_eq!(st.grid.left, 5.0);
        assert_eq!((st.width, st.height), (40.0, 20.0));
        st.grid.left = 8.0;
        assert!(app.apply_stretch_zones(&st));
        let grid = |app: &App| app.doc().symbols[0].shape.scale_grid.as_deref().copied();
        assert_eq!(grid(&app).map(|g| g.left), Some(8.0));
        st.on = false;
        assert!(app.apply_stretch_zones(&st));
        assert_eq!(grid(&app), None);
        app.undo();
        assert_eq!(grid(&app).map(|g| g.left), Some(8.0));
    }
}

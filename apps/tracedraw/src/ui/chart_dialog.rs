//! Object > Insert > Chart from Data: a table typed or pasted, a kind of
//! chart and its size, with a preview, inserted as one group of plain
//! objects that keeps its table; with a chart selected, the same dialog
//! edits it.

use crate::app::App;
use crate::chart::{self, ChartKind, ChartSpec};
use crate::i18n::tr;
use crate::ui::dialogs::{ok_cancel, window, Dialog};
use crate::ui::tips::RichTip;
use egui::{Color32, Context, Ui};
use tracedraw_core::geometry::{Affine, Point};
use tracedraw_core::{Command, Fill, Shape, ShapeId, ShapeKind};

/// The dialog's state: the settings, and the chart edited (none when
/// inserting a new one).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChartState {
    pub spec: ChartSpec,
    pub editing: Option<ShapeId>,
}

fn kind_key(k: ChartKind) -> String {
    format!("chart.kind_{}", k.key())
}

impl App {
    /// The selected chart, if exactly one is selected.
    pub fn selected_chart(&self) -> Option<(ShapeId, ChartSpec)> {
        let [id] = self.selection.as_slice() else {
            return None;
        };
        let s = self.doc().find_shape(*id)?;
        Some((*id, ChartSpec::of_shape(s)?))
    }

    /// Open the dialog: on the selected chart, else for a new one (in
    /// the settings used last, with the text font).
    pub fn open_chart_dialog(&mut self) {
        let st = match self.selected_chart() {
            Some((id, spec)) => ChartState {
                spec,
                editing: Some(id),
            },
            None => ChartState {
                spec: self.last_chart.clone().unwrap_or_else(|| ChartSpec {
                    font: self.text_font.clone(),
                    ..Default::default()
                }),
                editing: None,
            },
        };
        self.dialog = Dialog::Chart(st);
    }

    /// Insert the chart of `st` in the middle of the page, or put it in
    /// place of the one edited, keeping that one's top left corner. The
    /// id of the chart group.
    pub fn apply_chart(&mut self, st: &ChartState) -> Option<ShapeId> {
        let mut kids = {
            let engine = &mut self.engine;
            let mut next = || engine.new_shape_id();
            chart::build(&st.spec, &mut next)
        };
        let Some(kids_ref) = kids.as_mut() else {
            self.status = tr("chart.empty");
            return None;
        };
        self.last_chart = Some(st.spec.clone());
        let data = vec![(chart::DATA_KEY.to_string(), st.spec.to_data())];
        if let Some(id) = st.editing {
            let old = self.doc().find_shape(id)?.clone();
            // The new chart's ink lined up with the old one's top left.
            let b = self.visual_bounds(&old);
            // The group's own transform (a move, a scale) still applies:
            // the shift goes in through its inverse.
            let mut probe = old.clone();
            probe.kind = ShapeKind::Group {
                children: kids_ref.clone(),
            };
            let n = self.visual_bounds(&probe);
            let inv = old.transform.inverse();
            let d = inv * Point::new(b.x0, b.y1) - inv * Point::new(n.x0, n.y1);
            move_all(kids_ref, Affine::translate(d));
            let cmds = [
                Command::SetShapeKind {
                    shape: id,
                    kind: ShapeKind::Group {
                        children: kids.unwrap_or_default(),
                    },
                },
                Command::SetObjectData { shape: id, data },
            ];
            if let Err(e) = self.engine.run_batch("Edit Chart", &cmds) {
                self.status = e.to_string();
                return None;
            }
            self.raster.borrow_mut().invalidate();
            self.select(vec![id]);
            return Some(id);
        }
        let layer = self.active_layer()?;
        let c = self.page_rect().center();
        let (w, h) = (st.spec.width_mm.max(10.0), st.spec.height_mm.max(10.0));
        move_all(kids_ref, Affine::translate((c.x - w / 2.0, c.y - h / 2.0)));
        let id = self.engine.new_shape_id();
        let mut group = Shape::new(
            id,
            ShapeKind::Group {
                children: kids.unwrap_or_default(),
            },
        );
        group.fill = Fill::None;
        group.stroke = None;
        group.name = Some(tr("chart.object_name"));
        group.data = data;
        if let Err(e) = self.engine.run_batch(
            "Insert Chart",
            &[Command::AddShape {
                layer,
                shape: group,
            }],
        ) {
            self.status = e.to_string();
            return None;
        }
        self.select(vec![id]);
        Some(id)
    }
}

/// Move the chart's objects (paths and texts) by `t`.
fn move_all(shapes: &mut [Shape], t: Affine) {
    for s in shapes {
        match &mut s.kind {
            ShapeKind::Path { path, .. } => *path = t * path.clone(),
            ShapeKind::Text { origin, .. } => *origin = t * *origin,
            _ => {}
        }
    }
}

/// The preview: the chart's objects drawn on white, fitted to the room.
fn preview(ui: &mut Ui, spec: &ChartSpec) {
    let w = ui.available_width().max(200.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 200.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, Color32::WHITE);
    let mut n = 0u64;
    let mut next = || {
        n += 1;
        ShapeId(n)
    };
    let Some(shapes) = chart::build(spec, &mut next) else {
        return;
    };
    let room = rect.shrink(8.0);
    let (cw, ch) = (spec.width_mm.max(10.0), spec.height_mm.max(10.0));
    let scale = (room.width() as f64 / cw).min(room.height() as f64 / ch);
    let off = egui::vec2(
        (room.width() - (cw * scale) as f32) / 2.0,
        (room.height() - (ch * scale) as f32) / 2.0,
    );
    let to = |q: Point| {
        egui::pos2(
            room.left() + off.x + (q.x * scale) as f32,
            room.bottom() - off.y - (q.y * scale) as f32,
        )
    };
    let color = |f: &Fill| match f {
        Fill::Solid(c) => {
            let [r, g, b] = c.to_rgb8();
            Some(Color32::from_rgb(r, g, b))
        }
        _ => None,
    };
    for s in &shapes {
        match &s.kind {
            ShapeKind::Path { path, closed } => {
                let mut pts = Vec::new();
                let mut polys = Vec::new();
                kurbo::flatten(path, 0.1, |el| match el {
                    kurbo::PathEl::MoveTo(q) => {
                        if pts.len() > 1 {
                            polys.push(std::mem::take(&mut pts));
                        }
                        pts.clear();
                        pts.push(to(q));
                    }
                    kurbo::PathEl::LineTo(q) => pts.push(to(q)),
                    _ => {}
                });
                if pts.len() > 1 {
                    polys.push(pts);
                }
                for poly in polys {
                    if *closed {
                        if let Some(c) = color(&s.fill) {
                            // Not every shape is convex; fan triangles from
                            // the first point fill pies, bars and bands.
                            let mut mesh = egui::Mesh::default();
                            for q in &poly {
                                mesh.colored_vertex(*q, c);
                            }
                            for i in 1..poly.len().saturating_sub(1) {
                                mesh.add_triangle(0, i as u32, i as u32 + 1);
                            }
                            p.add(mesh);
                        }
                    } else if let Some(k) = &s.stroke {
                        let [r, g, b] = k.color.to_rgb8();
                        let width = (k.width * scale).max(0.5) as f32;
                        p.add(egui::Shape::line(
                            poly,
                            egui::Stroke::new(width, Color32::from_rgb(r, g, b)),
                        ));
                    }
                }
            }
            ShapeKind::Text {
                spans,
                origin,
                align,
                ..
            } => {
                let size = (spans[0].size_pt * 25.4 / 72.0 * scale) as f32;
                let anchor = match align {
                    tracedraw_core::TextAlign::Right => egui::Align2::RIGHT_BOTTOM,
                    tracedraw_core::TextAlign::Center => egui::Align2::CENTER_BOTTOM,
                    _ => egui::Align2::LEFT_BOTTOM,
                };
                p.text(
                    to(*origin) + egui::vec2(0.0, size * 0.2),
                    anchor,
                    &spans[0].text,
                    egui::FontId::proportional(size.max(4.0)),
                    Color32::from_gray(0x33),
                );
            }
            _ => {}
        }
    }
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut ChartState, close: &mut bool) {
    let title = if st.editing.is_some() {
        tr("chart.edit_title")
    } else {
        tr("chart.title")
    };
    window(ctx, title).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.horizontal(|ui| {
            ui.label(tr("chart.kind"));
            crate::ui::keynav::ComboBox::from_id_salt("chart_kind")
                .width(200.0)
                .selected_text(tr(&kind_key(st.spec.kind)))
                .show_ui(ui, |ui| {
                    for k in ChartKind::ALL {
                        if ui
                            .selectable_label(st.spec.kind == k, tr(&kind_key(k)))
                            .clicked()
                        {
                            st.spec.kind = k;
                        }
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("chart.kind"));
        });
        ui.add_space(4.0);
        ui.label(tr("chart.data"));
        ui.add(
            egui::TextEdit::multiline(&mut st.spec.data)
                .font(egui::TextStyle::Monospace)
                .desired_rows(6)
                .desired_width(f32::INFINITY),
        )
        .rich_tip(crate::ui::tips::keyed("chart.data"));
        ui.label(
            egui::RichText::new(tr(if st.spec.kind == ChartKind::Scatter {
                "chart.data_hint_scatter"
            } else {
                "chart.data_hint"
            }))
            .small()
            .color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(tr("chart.width"));
            ui.add(
                crate::ui::field::NumField::new(&mut st.spec.width_mm)
                    .range(10.0..=2000.0)
                    .speed(1.0)
                    .suffix(" mm"),
            )
            .rich_tip(crate::ui::tips::keyed("chart.width"));
            ui.label(tr("chart.height"));
            ui.add(
                crate::ui::field::NumField::new(&mut st.spec.height_mm)
                    .range(10.0..=2000.0)
                    .speed(1.0)
                    .suffix(" mm"),
            )
            .rich_tip(crate::ui::tips::keyed("chart.height"));
        });
        ui.horizontal(|ui| {
            ui.label(tr("chart.font"));
            let (pick, _) =
                crate::ui::tips::tip_over(ui, crate::ui::tips::keyed("chart.font"), |ui| {
                    crate::ui::font_list::font_list_live(
                        app,
                        ui,
                        "chart_font",
                        &st.spec.font,
                        180.0,
                    )
                });
            if let Some(p) = pick {
                st.spec.font = p.family;
            }
            ui.label(tr("chart.size"));
            ui.add(
                crate::ui::field::NumField::new(&mut st.spec.size_pt)
                    .range(2.0..=72.0)
                    .speed(0.5)
                    .suffix(" pt"),
            )
            .rich_tip(crate::ui::tips::keyed("chart.size"));
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.spec.legend, tr("chart.legend"))
                .rich_tip(crate::ui::tips::keyed("chart.legend"));
            ui.checkbox(&mut st.spec.values, tr("chart.values"))
                .rich_tip(crate::ui::tips::keyed("chart.values"));
        });
        ui.add_space(4.0);
        ui.label(tr("chart.preview"));
        preview(ui, &st.spec);
        ui.add_space(6.0);
        if ok_cancel(ui, close) {
            let st2 = st.clone();
            if app.apply_chart(&st2).is_some() {
                *close = true;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chart_is_inserted_as_a_group_and_edited_again() {
        let mut app = App::headless();
        app.open_chart_dialog();
        let Dialog::Chart(mut st) = app.dialog.clone() else {
            panic!("{:?}", app.dialog);
        };
        assert!(st.editing.is_none());
        st.spec.data = "\tA\nx\t1\ny\t2".into();
        let id = app.apply_chart(&st).expect("inserted");
        let s = app.doc().find_shape(id).expect("shape").clone();
        assert!(matches!(s.kind, ShapeKind::Group { .. }));
        assert_eq!(ChartSpec::of_shape(&s), Some(st.spec.clone()));
        let before = app.visual_bounds(&s);
        app.run(Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::translate((10.0, 5.0)),
        });
        app.open_chart_dialog();
        let Dialog::Chart(mut st) = app.dialog.clone() else {
            panic!();
        };
        assert_eq!(st.editing, Some(id));
        st.spec.kind = ChartKind::Bars;
        st.spec.data = "\tA\tB\nx\t1\t3\ny\t2\t4".into();
        assert_eq!(app.apply_chart(&st), Some(id));
        let s = app.doc().find_shape(id).unwrap().clone();
        let after = app.visual_bounds(&s);
        assert!(
            (after.x0 - before.x0 - 10.0).abs() < 0.5,
            "{before:?} {after:?}"
        );
        assert_eq!(ChartSpec::of_shape(&s).unwrap().kind, ChartKind::Bars);
        // One undo step back to the columns.
        app.undo();
        let s = app.doc().find_shape(id).unwrap();
        assert_eq!(ChartSpec::of_shape(s).unwrap().kind, ChartKind::Columns);
        // A new chart starts from the settings used last.
        app.select(vec![]);
        app.open_chart_dialog();
        let Dialog::Chart(st) = app.dialog.clone() else {
            panic!();
        };
        assert_eq!(st.spec.kind, ChartKind::Bars);
        assert!(st.editing.is_none());
    }

    #[test]
    fn a_table_without_numbers_inserts_nothing() {
        let mut app = App::headless();
        let st = ChartState {
            spec: ChartSpec {
                data: "a\tb".into(),
                ..Default::default()
            },
            editing: None,
        };
        assert!(app.apply_chart(&st).is_none());
        assert_eq!(app.status, tr("chart.empty"));
    }
}

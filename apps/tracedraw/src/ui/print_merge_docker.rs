//! The Print Merge docker (Window > Panels > Print Merge, and after
//! Finish in the Print Merge dialog), as in the reference program: Create
//! and Edit for the data source; the kind of field to insert (Text, Image,
//! QR code); the data source's columns; for an image, a placeholder
//! picture and Align and scale; Insert Field and Update Field; Create
//! Merged Document and Print Merged Document. See
//! docs/behavior/print-merge.md.

use crate::app::App;
use crate::i18n::tr;
use crate::print_merge::{Scale, ANCHOR_DATA, SCALE_DATA};
use crate::theme::Tokens;
use crate::ui::dialogs::Dialog;
use egui::{Rect as ERect, Sense, Stroke, Ui, Vec2};
use tracedraw_core::geometry::Rect;
use tracedraw_core::{Command, Fill, Shape, ShapeId, ShapeKind};

/// The docker's choices.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeDocker {
    /// The kind of field Insert Field makes: 0 text, 1 image, 2 QR code.
    pub kind: usize,
    /// The data source column chosen.
    pub column: usize,
    /// The picture an image placeholder shows (empty: an empty frame).
    pub placeholder: String,
    /// The point of the placeholder a picture is placed by: 0 top left to
    /// 8 bottom right, row by row.
    pub anchor: u8,
    pub scale: Scale,
    /// Place Inside Frame puts the placeholder picture in the same place
    /// on every page too (#598).
    pub all_pages: bool,
}

impl Default for MergeDocker {
    fn default() -> Self {
        Self {
            kind: 0,
            column: 0,
            placeholder: String::new(),
            anchor: 4,
            scale: Scale::Fit,
            all_pages: false,
        }
    }
}

/// The side of an image or QR placeholder made without a picture, mm.
const PLACEHOLDER_MM: f64 = 40.0;
/// The longest side a placeholder picture is given, mm.
const PLACEHOLDER_MAX_MM: f64 = 80.0;

/// The field a placeholder stands for: a text holding only `<Field>`, or
/// an object named `<Field>`.
pub fn field_of(s: &Shape) -> Option<String> {
    let whole = |t: &str| {
        let t = t.trim();
        (t.len() > 2
            && t.starts_with('<')
            && t.ends_with('>')
            && !t[1..t.len() - 1].contains(['<', '>']))
        .then(|| t[1..t.len() - 1].to_string())
    };
    match &s.kind {
        ShapeKind::Text { spans, .. } => {
            let text: String = spans.iter().map(|sp| sp.text.as_str()).collect();
            whole(&text)
        }
        _ => s.name.as_deref().and_then(whole),
    }
}

impl App {
    /// Insert Field: `<column>` at the caret of the text being edited, else
    /// a new text holding it in the middle of the view; for an image a
    /// frame named `<column>` (with the placeholder picture inside, when
    /// one is chosen) keeping the Align and scale; for a QR code a square
    /// named `<column>`.
    pub fn insert_merge_field(&mut self) {
        let Some(name) = self
            .merge_state
            .as_ref()
            .and_then(|st| st.headers.get(self.merge_docker.column))
            .cloned()
        else {
            return;
        };
        let field = format!("<{name}>");
        // The middle of the view, else (no window yet) of the page.
        let at = Some(self.view_center())
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .unwrap_or_else(|| self.page_rect().center());
        if self.merge_docker.kind == 0 {
            if self.text_edit.is_some() {
                self.insert_text(&field);
            } else {
                self.start_text(at, None);
                self.text_insert(&field);
                self.finish_text();
            }
            return;
        }
        let Some(layer) = self.active_layer() else {
            return;
        };
        let d = self.merge_docker.clone();
        let picture = (d.kind == 1 && !d.placeholder.trim().is_empty())
            .then(|| crate::print_merge::picture(&d.placeholder, None))
            .flatten();
        let (w, h) = match &picture {
            Some(pic) => {
                let k = (PLACEHOLDER_MAX_MM / pic.mm.0.max(pic.mm.1)).min(1.0);
                (pic.mm.0 * k, pic.mm.1 * k)
            }
            None => (PLACEHOLDER_MM, PLACEHOLDER_MM),
        };
        let rect = Rect::new(
            at.x - w / 2.0,
            at.y - h / 2.0,
            at.x + w / 2.0,
            at.y + h / 2.0,
        );
        let id = self.engine.new_shape_id();
        let mut frame = Shape::new(
            id,
            ShapeKind::Rect {
                rect,
                radius: 0.0,
                corners: None,
            },
        );
        frame.name = Some(field);
        frame.fill = Fill::None;
        if d.kind == 1 {
            frame.data = placement_data(d.scale, d.anchor);
        }
        let mut cmds = vec![Command::AddShape {
            layer,
            shape: frame,
        }];
        if let Some(p) = picture {
            let pid = self.engine.new_shape_id();
            let mut pic = Shape::new(
                pid,
                ShapeKind::Bitmap {
                    rect: crate::print_merge::place(rect, p.mm.0, p.mm.1, d.scale, d.anchor),
                    width_px: p.px.0,
                    height_px: p.px.1,
                    png: p.png,
                    fx: None,
                },
            );
            pic.stroke = None;
            pic.fill = Fill::None;
            cmds.push(Command::AddShape { layer, shape: pic });
            cmds.push(Command::PlaceInside {
                contents: vec![pid],
                frame: id,
            });
        }
        match self.engine.run_batch("Insert Print Merge Field", &cmds) {
            Ok(()) => self.select(vec![id]),
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Place Inside Frame: each selected shape (a circle, an oval, any
    /// outline, a clip frame) becomes the frame of the chosen picture
    /// field, keeping the Align and scale shown; every card's picture then
    /// goes inside it, also inside a group (#460). The placeholder
    /// picture, when one is chosen, goes into a frame on the page.
    pub fn place_merge_field_inside(&mut self) {
        let Some(name) = self
            .merge_state
            .as_ref()
            .and_then(|st| st.headers.get(self.merge_docker.column))
            .cloned()
        else {
            return;
        };
        let field = format!("<{name}>");
        let d = self.merge_docker.clone();
        let frames: Vec<Shape> = self
            .selected_rc()
            .iter()
            .filter(|s| !matches!(s.kind, ShapeKind::Text { .. } | ShapeKind::Group { .. }))
            .cloned()
            .collect();
        if frames.is_empty() {
            self.status = tr("merge.place_inside_none");
            return;
        }
        let picture = (!d.placeholder.trim().is_empty())
            .then(|| crate::print_merge::picture(&d.placeholder, None))
            .flatten();
        let layer = self.active_layer();
        let mut cmds = Vec::new();
        for s in &frames {
            cmds.push(Command::SetShapeName {
                shape: s.id,
                name: Some(field.clone()),
            });
            let mut data: Vec<(String, String)> = s
                .data
                .iter()
                .filter(|(k, _)| k != SCALE_DATA && k != ANCHOR_DATA)
                .cloned()
                .collect();
            data.extend(placement_data(d.scale, d.anchor));
            cmds.push(Command::SetObjectData { shape: s.id, data });
            // The stand-in picture, in a frame on the page.
            let on_page = self.doc().top_level_of(s.id) == Some(s.id);
            if let (Some(p), Some(layer), true) = (&picture, layer, on_page) {
                if let ShapeKind::ClipFrame { contents, .. } = &s.kind {
                    if !contents.is_empty() {
                        continue;
                    }
                }
                let pid = self.engine.new_shape_id();
                let mut pic = Shape::new(
                    pid,
                    ShapeKind::Bitmap {
                        rect: crate::print_merge::place(
                            s.bounds(),
                            p.mm.0,
                            p.mm.1,
                            d.scale,
                            d.anchor,
                        ),
                        width_px: p.px.0,
                        height_px: p.px.1,
                        png: p.png.clone(),
                        fx: None,
                    },
                );
                pic.stroke = None;
                pic.fill = Fill::None;
                cmds.push(Command::AddShape { layer, shape: pic });
                cmds.push(Command::PlaceInside {
                    contents: vec![pid],
                    frame: s.id,
                });
            }
        }
        // The same picture in the same place on the other pages: in the
        // matching shape there, else in a copy of the frame (#598).
        if let (true, Some(p)) = (d.all_pages, &picture) {
            cmds.extend(self.frames_on_other_pages(&frames, &field, p, &d));
        }
        let ids: Vec<ShapeId> = frames.iter().map(|s| s.id).collect();
        match self.engine.run_batch("Place Inside Frame", &cmds) {
            Ok(()) => {
                self.select(ids);
                self.status = tr("merge.place_inside_done");
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Place Inside Frame on every other page (#598): for each frame on
    /// the page, the shape of the same kind and place on another page
    /// takes the field and the picture; a page without one gets a copy of
    /// the frame with the picture inside.
    fn frames_on_other_pages(
        &mut self,
        frames: &[Shape],
        field: &str,
        p: &crate::print_merge::Picture,
        d: &MergeDocker,
    ) -> Vec<Command> {
        let current = self.page;
        let pages: Vec<(Option<tracedraw_core::LayerId>, Vec<Shape>)> = self
            .doc()
            .pages
            .iter()
            .filter(|pg| pg.id != current)
            .map(|pg| {
                let layer = pg
                    .layers
                    .iter()
                    .find(|l| l.visible && !l.locked)
                    .map(|l| l.id);
                let shapes = pg
                    .layers
                    .iter()
                    .flat_map(|l| l.shapes.iter().cloned())
                    .collect();
                (layer, shapes)
            })
            .collect();
        let same_place = |a: &Shape, b: &Shape| {
            let (ra, rb) = (a.bounds(), b.bounds());
            std::mem::discriminant(&a.kind) == std::mem::discriminant(&b.kind)
                && (ra.x0 - rb.x0).abs() < 0.01
                && (ra.y0 - rb.y0).abs() < 0.01
                && (ra.x1 - rb.x1).abs() < 0.01
                && (ra.y1 - rb.y1).abs() < 0.01
        };
        // Frames on the page itself (not inside a group).
        let on_page: Vec<&Shape> = frames
            .iter()
            .filter(|s| self.doc().top_level_of(s.id) == Some(s.id))
            .collect();
        let mut cmds = Vec::new();
        for (layer, shapes) in pages {
            let Some(layer) = layer else {
                continue;
            };
            for s in on_page.iter().copied() {
                let pid = self.engine.new_shape_id();
                let target = match shapes.iter().find(|o| same_place(o, s)) {
                    Some(o) => o.id,
                    None => {
                        let id = self.engine.new_shape_id();
                        let mut copy = crate::app::reid_pub(s.clone(), id, &mut self.engine);
                        if let ShapeKind::ClipFrame { contents, .. } = &mut copy.kind {
                            contents.clear();
                        }
                        cmds.push(Command::AddShape { layer, shape: copy });
                        id
                    }
                };
                cmds.push(Command::SetShapeName {
                    shape: target,
                    name: Some(field.to_string()),
                });
                let mut data: Vec<(String, String)> = s
                    .data
                    .iter()
                    .filter(|(k, _)| k != SCALE_DATA && k != ANCHOR_DATA)
                    .cloned()
                    .collect();
                data.extend(placement_data(d.scale, d.anchor));
                cmds.push(Command::SetObjectData {
                    shape: target,
                    data,
                });
                let mut pic = Shape::new(
                    pid,
                    ShapeKind::Bitmap {
                        rect: crate::print_merge::place(
                            s.bounds(),
                            p.mm.0,
                            p.mm.1,
                            d.scale,
                            d.anchor,
                        ),
                        width_px: p.px.0,
                        height_px: p.px.1,
                        png: p.png.clone(),
                        fx: None,
                    },
                );
                pic.stroke = None;
                pic.fill = Fill::None;
                cmds.push(Command::AddShape { layer, shape: pic });
                cmds.push(Command::PlaceInside {
                    contents: vec![pid],
                    frame: target,
                });
            }
        }
        cmds
    }

    /// The selected placeholders (see [`field_of`]).
    pub fn selected_merge_fields(&self) -> Vec<ShapeId> {
        self.selected_rc()
            .iter()
            .filter(|s| field_of(s).is_some())
            .map(|s| s.id)
            .collect()
    }

    /// Update Field: the selected placeholders stand for the chosen column,
    /// and image placeholders take the Align and scale shown.
    pub fn update_merge_fields(&mut self) {
        let Some(name) = self
            .merge_state
            .as_ref()
            .and_then(|st| st.headers.get(self.merge_docker.column))
            .cloned()
        else {
            return;
        };
        let field = format!("<{name}>");
        let d = self.merge_docker.clone();
        let mut cmds = Vec::new();
        for s in self.selected_rc().iter() {
            if field_of(s).is_none() {
                continue;
            }
            match &s.kind {
                ShapeKind::Text {
                    spans,
                    origin,
                    frame,
                    align,
                    para,
                    on_path,
                } => {
                    let mut spans = spans.clone();
                    spans.truncate(1);
                    if let Some(sp) = spans.first_mut() {
                        sp.text = field.clone();
                    }
                    cmds.push(Command::SetShapeKind {
                        shape: s.id,
                        kind: ShapeKind::Text {
                            spans,
                            origin: *origin,
                            frame: *frame,
                            align: *align,
                            para: para.clone(),
                            on_path: on_path.clone(),
                        },
                    });
                }
                _ => {
                    cmds.push(Command::SetShapeName {
                        shape: s.id,
                        name: Some(field.clone()),
                    });
                    if d.kind == 1 {
                        let mut data: Vec<(String, String)> = s
                            .data
                            .iter()
                            .filter(|(k, _)| k != SCALE_DATA && k != ANCHOR_DATA)
                            .cloned()
                            .collect();
                        data.extend(placement_data(d.scale, d.anchor));
                        cmds.push(Command::SetObjectData { shape: s.id, data });
                    }
                }
            }
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Update Print Merge Field", &cmds) {
            self.status = e.to_string();
        }
    }

    /// Create Merged Document: the current page filled once per record,
    /// in a new drawing; `print` then opens Print for it.
    pub fn create_merged_document(&mut self, print: bool) {
        let Some(st) = self.merge_state.clone() else {
            return;
        };
        match crate::print_merge::merged_document(self, &st) {
            Ok(doc) => {
                let n = doc.pages.len();
                self.open_document(doc, None);
                self.status = crate::i18n::trf("status.merge_done", &[("n", &n.to_string())]);
                if print {
                    self.print();
                }
            }
            Err(e) => self.status = e,
        }
    }
}

/// Align and scale as object data.
fn placement_data(scale: Scale, anchor: u8) -> Vec<(String, String)> {
    vec![
        (SCALE_DATA.to_string(), scale.id().to_string()),
        (ANCHOR_DATA.to_string(), anchor.to_string()),
    ]
}

/// The anchor grid: nine points, the chosen one filled.
fn anchor_grid(ui: &mut Ui, anchor: &mut u8) {
    const CELL: f32 = 13.0;
    let (r, _) = ui.allocate_exact_size(Vec2::splat(CELL * 3.0), Sense::hover());
    let p = ui.painter();
    p.rect_stroke(
        r.shrink(CELL / 2.0),
        0.0,
        Stroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Middle,
    );
    for i in 0..9u8 {
        let c = r.min + Vec2::new((i % 3) as f32 + 0.5, (i / 3) as f32 + 0.5) * CELL;
        let cell = ERect::from_center_size(c, Vec2::splat(CELL - 3.0));
        let resp = ui
            .interact(cell, ui.id().with(("merge_anchor", i)), Sense::click())
            .on_hover_text(tr("merge.anchor"));
        if resp.clicked() {
            *anchor = i;
        }
        let on = *anchor == i;
        let p = ui.painter();
        p.rect(
            cell,
            1.0,
            if on { Tokens::text() } else { Tokens::panel() },
            Stroke::new(1.0, Tokens::text_dim()),
            egui::StrokeKind::Inside,
        );
    }
}

pub fn print_merge(app: &mut App, ui: &mut Ui) {
    let loaded = app.merge_state.is_some();
    let full = ui.available_width();
    ui.horizontal(|ui| {
        let w = (full - 8.0) / 2.0;
        if ui
            .add_enabled(
                !loaded,
                egui::Button::new(tr("merge.create")).min_size(Vec2::new(w, 26.0)),
            )
            .on_hover_text(tr("menu.file.print_merge_create"))
            .clicked()
        {
            app.dialog = Dialog::PrintMerge(Default::default());
        }
        if ui
            .add_enabled(
                loaded,
                egui::Button::new(tr("merge.edit")).min_size(Vec2::new(w, 26.0)),
            )
            .on_hover_text(tr("menu.file.print_merge_edit"))
            .clicked()
        {
            if let Some(st) = app.merge_state.clone() {
                app.dialog = Dialog::PrintMerge(st);
            }
        }
    });
    ui.separator();
    let headers: Vec<String> = app
        .merge_state
        .as_ref()
        .map(|st| st.headers.clone())
        .unwrap_or_default();
    let folder: Option<String> = app
        .merge_state
        .as_ref()
        .and_then(|st| st.pictures.as_ref())
        .map(|p| p.display().to_string());
    if headers.is_empty() {
        ui.label(
            egui::RichText::new(tr("merge.no_data"))
                .color(Tokens::text_dim())
                .size(11.0),
        );
    }
    ui.add_enabled_ui(!headers.is_empty(), |ui| {
        let d = &mut app.merge_docker;
        ui.horizontal(|ui| {
            for (i, key) in ["merge.kind_text", "merge.kind_image", "merge.kind_qr"]
                .iter()
                .enumerate()
            {
                ui.radio_value(&mut d.kind, i, tr(key));
            }
        });
        ui.add_space(4.0);
        ui.label(tr("merge.columns"));
        d.column = d.column.min(headers.len().saturating_sub(1));
        egui::ComboBox::from_id_salt("merge_columns")
            .width(full - 8.0)
            .selected_text(headers.get(d.column).cloned().unwrap_or_default())
            .show_ui(ui, |ui| {
                for (i, h) in headers.iter().enumerate() {
                    ui.selectable_value(&mut d.column, i, h);
                }
            });
        let mut browse = false;
        let mut pick_folder = false;
        let mut clear_folder = false;
        if d.kind == 1 {
            // Where picture cells that name only a file are looked for
            // first: one folder of photos for all the cards (#460).
            ui.add_space(4.0);
            ui.label(tr("merge.pictures_folder"));
            ui.horizontal(|ui| {
                pick_folder = ui
                    .button("\u{1F4C2}")
                    .on_hover_text(tr("merge.pictures_folder_tip"))
                    .clicked();
                let shown = folder
                    .clone()
                    .unwrap_or_else(|| tr("merge.pictures_folder_data"));
                ui.label(
                    egui::RichText::new(shown)
                        .color(Tokens::text_dim())
                        .size(11.0),
                );
                if folder.is_some() && ui.small_button("\u{2715}").clicked() {
                    clear_folder = true;
                }
            });
            ui.add_space(4.0);
            ui.label(tr("merge.placeholder"));
            ui.horizontal(|ui| {
                browse = ui
                    .button("\u{1F4C2}")
                    .on_hover_text(tr("merge.placeholder_tip"))
                    .clicked();
                ui.add(
                    egui::TextEdit::singleline(&mut d.placeholder)
                        .desired_width(ui.available_width()),
                );
            });
            ui.checkbox(&mut d.all_pages, tr("merge.all_pages"))
                .on_hover_text(tr("merge.all_pages_tip"));
            ui.add_space(4.0);
            ui.label(tr("merge.align_scale"));
            ui.horizontal(|ui| {
                anchor_grid(ui, &mut d.anchor);
                egui::ComboBox::from_id_salt("merge_scale")
                    .width(ui.available_width())
                    .selected_text(tr(d.scale.key()))
                    .show_ui(ui, |ui| {
                        for s in Scale::ALL {
                            ui.selectable_value(&mut d.scale, s, tr(s.key()));
                        }
                    });
            });
        }
        ui.add_space(6.0);
        let wide = Vec2::new(full - 8.0, 26.0);
        if ui
            .add(egui::Button::new(tr("merge.insert_field")).min_size(wide))
            .clicked()
        {
            app.insert_merge_field();
        }
        if app.merge_docker.kind == 1 {
            // A shape of the user's as the picture's frame (#460): the
            // one selected, else the one clicked next, the pointer the
            // Place Inside arrow meanwhile (#577).
            if ui
                .add(
                    egui::Button::new(tr("merge.place_inside"))
                        .min_size(wide)
                        .selected(app.pending_merge_frame),
                )
                .on_hover_text(tr("merge.place_inside_tip"))
                .clicked()
            {
                if app.selection.is_empty() {
                    app.pending_merge_frame = true;
                    app.status = tr("merge.place_inside_click");
                } else {
                    app.place_merge_field_inside();
                }
            }
        }
        let can_update = !app.selected_merge_fields().is_empty();
        if ui
            .add_enabled(
                can_update,
                egui::Button::new(tr("merge.update_field")).min_size(wide),
            )
            .clicked()
        {
            app.update_merge_fields();
        }
        ui.separator();
        let records = app
            .merge_state
            .as_ref()
            .is_some_and(|st| !st.rows.is_empty());
        if ui
            .add_enabled(
                records,
                egui::Button::new(tr("merge.create_document")).min_size(wide),
            )
            .clicked()
        {
            app.create_merged_document(false);
        }
        if ui
            .add_enabled(
                records,
                egui::Button::new(tr("merge.print_document")).min_size(wide),
            )
            .clicked()
        {
            app.create_merged_document(true);
        }
        if pick_folder {
            if let Some(dir) = crate::files::pick_folder() {
                if let Some(st) = app.merge_state.as_mut() {
                    st.pictures = Some(dir);
                }
            }
        }
        if clear_folder {
            if let Some(st) = app.merge_state.as_mut() {
                st.pictures = None;
            }
        }
        if browse {
            crate::files::Dialog::new()
                .add_filter(
                    "Pictures",
                    &["png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp"],
                )
                .pick_file(app, |app, path| {
                    app.merge_docker.placeholder = path.display().to_string();
                });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print_merge::FieldKind;
    use crate::ui::dialogs::PrintMergeState;

    fn data(dir: &std::path::Path) -> PrintMergeState {
        PrintMergeState {
            headers: vec!["Name".into(), "Photo".into(), "Code".into()],
            rows: vec![
                vec!["Ana".into(), "wide.png".into(), "A-1".into()],
                vec!["Bob".into(), "wide.png".into(), "B-2".into()],
            ],
            kinds: vec![FieldKind::Text, FieldKind::Image, FieldKind::Qr],
            path: Some(dir.join("cards.csv")),
            ..Default::default()
        }
    }

    /// With "On every page" Place Inside Frame puts the placeholder
    /// picture in the same place on the other pages: in the matching
    /// shape where there is one, in a copy of the frame elsewhere (#598).
    #[test]
    fn the_placeholder_goes_in_the_same_place_on_every_page() {
        let dir = std::env::temp_dir().join(format!("td_merge_all_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbaImage::from_pixel(40, 40, image::Rgba([200, 0, 0, 255]))
            .save(dir.join("stand_in.png"))
            .unwrap();
        let mut app = App::headless();
        app.merge_state = Some(data(&dir));
        let oval = |app: &mut App| {
            app.new_shape(ShapeKind::Ellipse {
                rect: Rect::new(10.0, 10.0, 50.0, 40.0),
                arc: None,
            })
            .expect("ellipse")
        };
        let first_page = app.page;
        let first = oval(&mut app);
        app.add_page();
        let second = oval(&mut app);
        app.add_page();
        let third_page = app.page;
        app.page = first_page;
        app.merge_docker.kind = 1;
        app.merge_docker.column = 1;
        app.merge_docker.placeholder = dir.join("stand_in.png").display().to_string();
        app.merge_docker.all_pages = true;
        app.select(vec![first]);
        app.place_merge_field_inside();
        let framed = |app: &App, id: ShapeId| {
            let s = app.doc().find_shape(id).expect("frame");
            s.name.as_deref() == Some("<Photo>")
                && matches!(&s.kind, ShapeKind::ClipFrame { contents, .. }
                    if contents.iter().any(|c| matches!(c.kind, ShapeKind::Bitmap { .. })))
        };
        assert!(framed(&app, first));
        assert!(framed(&app, second), "the matching oval on page 2");
        let third: Vec<ShapeId> = app
            .doc()
            .pages
            .iter()
            .find(|p| p.id == third_page)
            .expect("page 3")
            .layers
            .iter()
            .flat_map(|l| l.shapes.iter().map(|s| s.id))
            .collect();
        assert_eq!(third.len(), 1, "a copy of the frame on page 3");
        assert!(framed(&app, third[0]));
        // One step undoes it all.
        app.undo();
        assert!(!framed(&app, second));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn walk<'a>(s: &'a Shape, out: &mut Vec<&'a Shape>) {
        out.push(s);
        match &s.kind {
            ShapeKind::Group { children } => children.iter().for_each(|c| walk(c, out)),
            ShapeKind::ClipFrame { frame, contents } => {
                walk(frame, out);
                contents.iter().for_each(|c| walk(c, out));
            }
            _ => {}
        }
    }

    fn all_of(page: &tracedraw_core::document::Page) -> Vec<&Shape> {
        let mut all = Vec::new();
        for s in page.layers.iter().flat_map(|l| &l.shapes) {
            walk(s, &mut all);
        }
        all
    }

    /// Insert Field makes a text, an image frame (with the placeholder
    /// picture inside) and a QR square; the merge puts each record's
    /// picture in the frame by its Align and scale and turns the square
    /// into the code; Create Merged Document leaves the drawing as it
    /// was (#437).
    #[test]
    fn the_docker_inserts_fields_and_makes_a_merged_document() {
        let dir = std::env::temp_dir().join(format!("td_merge_docker_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 96 px = 25.4 mm wide, 48 px = 12.7 mm tall.
        image::RgbaImage::from_pixel(96, 48, image::Rgba([0, 0, 200, 255]))
            .save(dir.join("wide.png"))
            .unwrap();
        image::RgbaImage::from_pixel(10, 10, image::Rgba([90, 90, 90, 255]))
            .save(dir.join("stand_in.png"))
            .unwrap();
        let mut app = App::headless();
        app.merge_state = Some(data(&dir));
        // Text.
        app.merge_docker.column = 0;
        app.insert_merge_field();
        // Image: fitted at the top left of its frame.
        app.merge_docker.kind = 1;
        app.merge_docker.column = 1;
        app.merge_docker.placeholder = dir.join("stand_in.png").display().to_string();
        app.merge_docker.scale = Scale::Fit;
        app.merge_docker.anchor = 0;
        app.insert_merge_field();
        let frame = app.selection[0];
        let s = app.doc().find_shape(frame).cloned().expect("frame");
        assert_eq!(s.name.as_deref(), Some("<Photo>"));
        assert!(
            matches!(s.kind, ShapeKind::ClipFrame { .. }),
            "{:?}",
            s.kind
        );
        assert_eq!(crate::print_merge::placement_of(&s), (Scale::Fit, 0));
        let area = s.bounds();
        // QR code.
        app.merge_docker.kind = 2;
        app.merge_docker.column = 2;
        app.insert_merge_field();
        let page = app.doc().page(app.page).unwrap().clone();
        let texts: Vec<String> = all_of(&page)
            .iter()
            .filter_map(|s| field_of(s).filter(|_| matches!(s.kind, ShapeKind::Text { .. })))
            .collect();
        assert_eq!(texts, vec!["Name".to_string()]);

        let before = app.doc().clone();
        app.create_merged_document(false);
        assert_ne!(app.active_doc, 0, "a drawing of its own");
        let doc = app.doc().clone();
        assert_eq!(doc.pages.len(), 2, "one page per record");
        let first = all_of(&doc.pages[0]);
        let names: Vec<String> = first
            .iter()
            .filter_map(|s| match &s.kind {
                ShapeKind::Text { spans, .. } => {
                    Some(spans.iter().map(|sp| sp.text.as_str()).collect())
                }
                _ => None,
            })
            .collect();
        assert!(names.contains(&"Ana".to_string()), "{names:?}");
        // The record's picture, alone in the frame (the stand-in gone),
        // fitted (2 : 1 in a square) at its top left.
        let pics: Vec<Rect> = first
            .iter()
            .filter_map(|s| match &s.kind {
                ShapeKind::Bitmap { width_px: 96, .. } => Some(s.bounds()),
                ShapeKind::Bitmap { .. } => panic!("the stand-in stayed"),
                _ => None,
            })
            .collect();
        assert_eq!(pics.len(), 1);
        let b = pics[0];
        assert!((b.width() - area.width()).abs() < 1e-6, "{b:?} {area:?}");
        assert!((b.height() - area.width() / 2.0).abs() < 1e-6, "{b:?}");
        assert!(
            (b.x0 - area.x0).abs() < 1e-6 && (b.y1 - area.y1).abs() < 1e-6,
            "{b:?} {area:?}"
        );
        // The QR square became the code.
        assert!(first.iter().any(
            |s| s.name.as_deref() == Some("<Code>") && matches!(s.kind, ShapeKind::Path { .. })
        ));
        // The drawing it came from keeps its placeholders.
        app.switch_document(0);
        assert_eq!(app.doc(), &before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Place Inside Frame makes the user's own shapes the picture's frame,
    /// one on the page and one inside a moved group of the card's design;
    /// the merge puts each record's photo inside both, clipped to the
    /// ellipse, in place (#460).
    #[test]
    fn place_inside_frame_uses_the_users_shapes_also_in_a_group() {
        use tracedraw_core::geometry::Affine;
        let dir = std::env::temp_dir().join(format!("td_merge_own_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbaImage::from_pixel(96, 48, image::Rgba([0, 0, 200, 255]))
            .save(dir.join("wide.png"))
            .unwrap();
        let mut app = App::headless();
        app.merge_state = Some(data(&dir));
        let ellipse = |app: &mut App, r: Rect| {
            app.new_shape(ShapeKind::Ellipse { rect: r, arc: None })
                .expect("ellipse")
        };
        let back = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 90.0, 50.0),
                radius: 0.0,
                corners: None,
            })
            .expect("card");
        let inner = ellipse(&mut app, Rect::new(10.0, 10.0, 40.0, 40.0));
        app.select(vec![back, inner]);
        app.group_selection();
        let group = app.selection[0];
        app.engine
            .run(&Command::TransformShapes {
                shapes: vec![group],
                transform: Affine::translate((100.0, 20.0)),
            })
            .unwrap();
        let alone = ellipse(&mut app, Rect::new(10.0, 120.0, 50.0, 140.0));
        // Nothing selected: the button says what to do.
        app.merge_docker.kind = 1;
        app.merge_docker.column = 1;
        app.select(Vec::new());
        app.place_merge_field_inside();
        assert_eq!(app.status, tr("merge.place_inside_none"));
        app.select(vec![inner]);
        app.place_merge_field_inside();
        app.select(vec![alone]);
        app.place_merge_field_inside();
        for id in [inner, alone] {
            let s = app.doc().find_shape(id).cloned().expect("frame");
            assert_eq!(s.name.as_deref(), Some("<Photo>"));
            assert_eq!(app.selected_merge_fields().len(), 1);
        }
        let inner_box = app.doc().shape_on_page(inner).expect("inner").bounds();
        let alone_box = app.doc().shape_on_page(alone).expect("alone").bounds();
        app.create_merged_document(false);
        let doc = app.doc().clone();
        assert_eq!(doc.pages.len(), 2);
        // Drawn: blue at the ellipse's middle, clipped off just outside
        // its curve where the photo's box still reaches.
        let zoom = 4.0;
        let area = Rect::new(0.0, 0.0, 200.0, 200.0);
        let opts = tracedraw_render::RenderOptions {
            width: (area.width() * zoom) as u32,
            height: (area.height() * zoom) as u32,
            view: tracedraw_render::ViewTransform {
                zoom,
                origin_x: -area.x0 * zoom,
                origin_y: area.y1 * zoom,
            },
            ..Default::default()
        };
        let px = tracedraw_render::render_page(&doc, doc.pages[0].id, &opts).expect("render");
        let blue = |x: f64, y: f64| {
            let c = px
                .pixel(((x - area.x0) * zoom) as u32, ((area.y1 - y) * zoom) as u32)
                .expect("pixel");
            c.blue() > 150 && c.red() < 60
        };
        let c = inner_box.center();
        assert!(blue(c.x, c.y), "the photo in the grouped ellipse");
        assert!(
            !blue(inner_box.x0 + 0.8, c.y + 7.0),
            "clipped by the ellipse"
        );
        assert!(blue(alone_box.center().x, alone_box.center().y));
        for b in [inner_box, alone_box] {
            // The copies have ids of their own; find the frames by name.
            let found: Vec<Shape> = all_of(&doc.pages[0])
                .into_iter()
                .filter(|s| s.name.as_deref() == Some("<Photo>"))
                .cloned()
                .collect();
            let clip = found
                .iter()
                .find(|s| {
                    let on = doc.shape_on_page(s.id).expect("on page").bounds();
                    (on.x0 - b.x0).abs() < 1e-6 && (on.y0 - b.y0).abs() < 1e-6
                })
                .unwrap_or_else(|| panic!("a frame where {b:?} was: {found:?}"));
            let ShapeKind::ClipFrame { frame, contents } = &clip.kind else {
                panic!("not a frame: {:?}", clip.kind);
            };
            assert!(
                matches!(frame.kind, ShapeKind::Ellipse { .. }),
                "the user's ellipse"
            );
            assert_eq!(contents.len(), 1, "the record's photo");
            assert!(matches!(
                contents[0].kind,
                ShapeKind::Bitmap { width_px: 96, .. }
            ));
            // Fitted in the ellipse's box, in the frame's own space.
            let pic = contents[0].bounds();
            let fb = frame.bounds();
            assert!((pic.width() - fb.width()).abs() < 1e-6, "{pic:?} {fb:?}");
            assert!(
                pic.x0 >= fb.x0 - 1e-6 && pic.y1 <= fb.y1 + 1e-6,
                "{pic:?} {fb:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Update Field points the selected placeholder at the chosen column
    /// and gives an image placeholder the Align and scale shown.
    #[test]
    fn update_field_changes_the_column_and_the_placement() {
        let mut app = App::headless();
        app.merge_state = Some(data(std::path::Path::new(".")));
        app.merge_docker.kind = 1;
        app.merge_docker.column = 1;
        app.insert_merge_field();
        let id = app.selection[0];
        assert_eq!(app.selected_merge_fields(), vec![id]);
        app.merge_docker.column = 0;
        app.merge_docker.scale = Scale::Stretch;
        app.merge_docker.anchor = 8;
        app.update_merge_fields();
        let s = app.doc().find_shape(id).cloned().expect("placeholder");
        assert_eq!(s.name.as_deref(), Some("<Name>"));
        assert_eq!(crate::print_merge::placement_of(&s), (Scale::Stretch, 8));
        assert_eq!(
            app.engine.history_labels().0.last(),
            Some(&"Update Print Merge Field")
        );
    }

    #[test]
    fn the_docker_draws_with_and_without_data() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let frame = |app: &mut App| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 700.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| print_merge(app, ui));
            out.textures_delta.clear();
        };
        frame(&mut app);
        app.merge_state = Some(data(std::path::Path::new(".")));
        for kind in 0..3 {
            app.merge_docker.kind = kind;
            frame(&mut app);
        }
    }
}

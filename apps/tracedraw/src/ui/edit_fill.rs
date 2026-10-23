//! The Edit Fill dialog (F11, #361): the fill types as pictures across the
//! top, then the chosen type's settings. A gradient shows a preview of the
//! fill, the fill picker with the built-in gradients, its type, how it
//! repeats, the colour band with its nodes (a click picks one, a drag
//! moves it, a double click on the band adds one, on a node removes it),
//! the picked node's colour, transparency, position and midpoint, Reverse,
//! and the transform (rotation, centre, edge pad). Uniform colours get the
//! colour viewer; patterns, textures and meshes their editors. OK applies
//! what was changed, with Overprint fill, in one undo step; with nothing
//! selected it sets the fill of new objects. See
//! docs/behavior/gradient-fill.md.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use egui::{Color32, Pos2, Rect, Sense, Stroke as EStroke, Ui, Vec2};
use tracedraw_core::patch::{FillField, FillMix};
use tracedraw_core::style::{Fountain, FountainKind, FountainRepeat, Stop};
use tracedraw_core::{Color, Fill};

/// What the dialog keeps while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct EditFillState {
    /// The fill as it was (the first object's, or the default).
    pub old: Fill,
    /// The fill being edited.
    pub fill: Fill,
    pub mix: FillMix,
    /// Fields set in the dialog that apply even when equal to the value
    /// shown (a type picked for objects of other types).
    pub touched: Vec<FillField>,
    /// The picked node of a gradient (an index into its stops).
    pub node: usize,
    /// Overprint fill as shown (`None`: the objects differ) and as it was.
    pub overprint: Option<bool>,
    pub overprint_old: Option<bool>,
    /// Nothing is selected: the dialog edits the default fill.
    pub defaults: bool,
    /// The hue the colour viewer shows (kept for greys).
    pub hue: f32,
}

/// Width of the band of colours.
const BAND_W: f32 = 320.0;

impl App {
    /// Open the Edit Fill dialog on the selection's fill (or the default
    /// fill when nothing is selected).
    pub fn open_edit_fill(&mut self) {
        let targets = self.paint_targets();
        let defaults = targets.is_empty();
        let old = match targets.first() {
            Some(s) => s.fill.clone(),
            None => self.default_fill(),
        };
        let mix = if defaults {
            FillMix::default()
        } else {
            self.selection_fill_mix()
        };
        let overprint_old = if defaults {
            Some(false)
        } else {
            let on = targets.iter().filter(|s| s.overprint_fill).count();
            if on == 0 {
                Some(false)
            } else if on == targets.len() {
                Some(true)
            } else {
                None
            }
        };
        let hue = old.preview_color().map_or(0.0, |c| c.to_hsb().0 as f32);
        self.dialog = crate::ui::dialogs::Dialog::EditFill(Box::new(EditFillState {
            fill: old.clone(),
            old,
            mix,
            touched: Vec::new(),
            node: 0,
            overprint: overprint_old,
            overprint_old,
            defaults,
            hue,
        }));
    }

    /// OK in the Edit Fill dialog.
    pub fn apply_edit_fill(&mut self, st: &EditFillState) {
        if st.defaults {
            if st.fill != st.old {
                self.request_default_change(crate::defaults::DefaultChange::Fill(st.fill.clone()));
            }
            return;
        }
        let before = self.engine.last_step_id();
        if st.fill != st.old || !st.touched.is_empty() {
            self.patch_selection_fill(&st.old, &st.fill, &st.touched);
        }
        if let Some(on) = st.overprint.filter(|_| st.overprint != st.overprint_old) {
            let ids: Vec<_> = self.paint_targets().iter().map(|s| s.id).collect();
            let cmd = tracedraw_core::Command::SetOverprint {
                shapes: ids,
                fill: Some(on),
                outline: None,
            };
            if self.engine.last_step_id() != before {
                let _ = self.engine.amend(&cmd);
            } else {
                self.run(cmd);
            }
        }
    }
}

/// The dialog's window.
pub fn dialog(app: &mut App, ctx: &egui::Context, st: &mut EditFillState, close: &mut bool) {
    let mut ok = false;
    let mut help = false;
    crate::ui::dialogs::window(ctx, tr("edit_fill.title")).show(ctx, |ui| {
        ui.set_min_width(600.0);
        kinds(ui, st);
        ui.separator();
        match &mut st.fill {
            Fill::None => {
                ui.label(egui::RichText::new(tr("edit_fill.no_fill")).color(Tokens::text_dim()));
            }
            Fill::Solid(_) => uniform(ui, st),
            Fill::Fountain(_) => fountain(ui, st),
            Fill::Pattern(p) => {
                ui.horizontal_top(|ui| {
                    preview(ui, &Fill::Pattern(p.clone()));
                    ui.vertical(|ui| crate::ui::dockers::pattern_editor(ui, p));
                });
            }
            Fill::Texture(t) => {
                ui.horizontal_top(|ui| {
                    preview(ui, &Fill::Texture(t.clone()));
                    ui.vertical(|ui| crate::ui::dockers::texture_editor(ui, t));
                });
            }
            Fill::Mesh(_) => {
                ui.label(egui::RichText::new(tr("docker.mesh_hint")).color(Tokens::text_dim()));
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            help = ui
                .add_sized([27.0, 27.0], egui::Button::new("?"))
                .on_hover_text(tr("dialog.help"))
                .clicked();
            if !st.defaults {
                let mut on = st.overprint.unwrap_or(false);
                if ui
                    .add(
                        egui::Checkbox::new(&mut on, tr("edit_fill.overprint"))
                            .indeterminate(st.overprint.is_none()),
                    )
                    .changed()
                {
                    st.overprint = Some(on);
                }
            }
            ok = crate::ui::dialogs::ok_cancel(ui, close);
        });
    });
    if help {
        app.open_dialog_help();
    }
    if ok {
        app.apply_edit_fill(st);
    }
}

/// The fill types as pictures: none, uniform, gradient, pattern, texture,
/// mesh.
fn kinds(ui: &mut Ui, st: &mut EditFillState) {
    let kind = tracedraw_core::patch::fill_kind(&st.fill);
    let shown = (!st.mix.kind || !st.touched.is_empty()).then_some(kind);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, key) in [
            "docker.fill_none",
            "docker.fill_uniform",
            "docker.fill_fountain",
            "docker.fill_pattern",
            "docker.fill_texture",
            "docker.fill_mesh",
        ]
        .iter()
        .enumerate()
        {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 30.0), Sense::click());
            if shown == Some(i) {
                ui.painter().rect_filled(rect, 3.0, Tokens::tool_active());
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 3.0, Tokens::tool_hover());
            }
            crate::ui::dockers::fill_kind_picture(
                ui.painter(),
                Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
                i,
            );
            if resp.on_hover_text(tr(key)).clicked() && shown != Some(i) {
                st.fill = new_of_kind(&st.fill, i);
                st.touched = vec![FillField::All];
                st.node = 0;
            }
        }
    });
}

/// A fill of kind `i` (as [`kinds`] lists them) from `from`'s colour.
pub fn new_of_kind(from: &Fill, i: usize) -> Fill {
    use tracedraw_core::style::{Pattern, PatternTile, Texture, TextureKind};
    let base = from
        .preview_color()
        .unwrap_or(Color::cmyk_pct(0.0, 0.0, 0.0, 20.0));
    match i {
        0 => Fill::None,
        1 => Fill::Solid(base),
        2 => Fill::linear(base, Color::WHITE, 0.0),
        3 => Fill::Pattern(Pattern::TwoColor {
            tile: PatternTile::Checker,
            front: base,
            back: Color::WHITE,
            size_mm: 10.0,
        }),
        4 => Fill::Texture(Texture {
            kind: TextureKind::Clouds,
            color_a: base,
            color_b: Color::WHITE,
            scale: 20.0,
            seed: 1,
        }),
        _ => Fill::Mesh(tracedraw_core::Mesh::new(
            tracedraw_core::geometry::Rect::new(0.0, 0.0, 100.0, 100.0),
            2,
            2,
            base,
        )),
    }
}

/// A uniform colour: the viewer, the colour's model and its values.
fn uniform(ui: &mut Ui, st: &mut EditFillState) {
    let Fill::Solid(c) = &mut st.fill else {
        return;
    };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(260.0);
            if let Some((h, s, b)) = crate::ui::color_docker::viewer(ui, st.hue, *c) {
                st.hue = h;
                *c = crate::ui::color_picker::from_viewer(
                    h,
                    s,
                    b,
                    crate::ui::color_picker::edit_model(*c),
                );
            }
        });
        ui.vertical(|ui| {
            ui.label(tr("edit_fill.color"));
            crate::ui::color_picker::color_button(ui, c);
            ui.label(
                egui::RichText::new(c.to_hex())
                    .monospace()
                    .color(Tokens::text_dim()),
            );
        });
    });
}

/// A gradient: preview and fill picker on the left; type, repeat, band,
/// node and transform on the right.
fn fountain(ui: &mut Ui, st: &mut EditFillState) {
    let Fill::Fountain(f) = &mut st.fill else {
        return;
    };
    st.node = st.node.min(f.stops.len().saturating_sub(1));
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(200.0);
            preview(ui, &Fill::Fountain(f.clone()));
            ui.add_space(4.0);
            ui.label(tr("edit_fill.fill_picker"));
            picker(ui, f);
        });
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("docker.type"));
                for k in FountainKind::ALL {
                    if ui
                        .selectable_label(f.kind == k, tr(crate::ui::dockers::fountain_kind_key(k)))
                        .clicked()
                    {
                        f.kind = k;
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("docker.repeat"));
                for (m, key) in [
                    (FountainRepeat::None, "edit_fill.repeat_default"),
                    (FountainRepeat::Mirror, "edit_fill.repeat_mirror"),
                    (FountainRepeat::Repeat, "edit_fill.repeat"),
                ] {
                    if ui.selectable_label(f.repeat == m, tr(key)).clicked() {
                        f.repeat = m;
                        if m != FountainRepeat::None && f.repeats < 2 {
                            f.repeats = 2;
                        }
                    }
                }
                if f.repeat != FountainRepeat::None {
                    ui.add(
                        crate::ui::field::NumField::new(&mut f.repeats)
                            .range(1..=Fountain::MAX_REPEATS),
                    );
                }
            });
            ui.add_space(4.0);
            band(ui, f, &mut st.node);
            node_fields(ui, f, st.node);
            ui.add_space(4.0);
            crate::ui::chrome::section(ui, &tr("edit_fill.transform"));
            transform(ui, f);
        });
    });
}

/// The fill drawn on a square, with a checker behind what is clear.
fn preview(ui: &mut Ui, fill: &Fill) {
    let size = Vec2::splat(180.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let key = egui::Id::new(("edit_fill_preview", format!("{fill:?}")));
    let tex: Option<egui::TextureHandle> = ui.data(|d| d.get_temp(key));
    let tex = tex.or_else(|| {
        let pm = tracedraw_render::render_fill_image(
            fill,
            tracedraw_core::geometry::Rect::new(0.0, 0.0, 50.0, 50.0),
            90.0,
        )?;
        let img = egui::ColorImage::from_rgba_premultiplied(
            [pm.width() as usize, pm.height() as usize],
            pm.data(),
        );
        let t = ui
            .ctx()
            .load_texture("edit_fill_preview", img, egui::TextureOptions::LINEAR);
        ui.data_mut(|d| d.insert_temp(key, t.clone()));
        Some(t)
    });
    let p = ui.painter();
    // A checker for what the fill leaves clear.
    let cell = 10.0;
    for y in 0..(size.y / cell) as i32 {
        for x in 0..(size.x / cell) as i32 {
            let c = if (x + y) % 2 == 0 {
                Color32::from_gray(255)
            } else {
                Color32::from_gray(220)
            };
            p.rect_filled(
                Rect::from_min_size(
                    rect.min + Vec2::new(x as f32 * cell, y as f32 * cell),
                    Vec2::splat(cell),
                ),
                0.0,
                c,
            );
        }
    }
    if let Some(t) = tex {
        p.image(
            t.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    p.rect_stroke(
        rect,
        0.0,
        EStroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Inside,
    );
}

/// The built-in gradients; a click takes its colours.
fn picker(ui: &mut Ui, f: &mut Fountain) {
    egui::ScrollArea::vertical()
        .id_salt("edit_fill_picker")
        .max_height(110.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(3.0);
                for (i, (key, _)) in crate::gradient_presets::PRESETS.iter().enumerate() {
                    let Some(stops) = crate::gradient_presets::stops(i) else {
                        continue;
                    };
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(36.0, 22.0), Sense::click());
                    crate::ui::dockers::gradient_swatch(ui.painter(), r, &stops);
                    let line = if resp.hovered() {
                        Tokens::ACCENT
                    } else {
                        Tokens::border()
                    };
                    ui.painter().rect_stroke(
                        r,
                        2.0,
                        EStroke::new(1.0, line),
                        egui::StrokeKind::Inside,
                    );
                    if resp.on_hover_text(tr(key)).clicked() {
                        f.stops = stops;
                        if let Some(a) = crate::gradient_presets::angle(i)
                            .filter(|_| f.kind == FountainKind::Linear)
                        {
                            f.angle = a;
                        }
                    }
                }
            });
        });
}

/// Where stop position `t` is across the band `r`.
fn band_x(r: Rect, t: f64) -> f32 {
    r.left() + r.width() * t.clamp(0.0, 1.0) as f32
}

/// The band of colours with the nodes under it.
fn band(ui: &mut Ui, f: &mut Fountain, node: &mut usize) {
    let (area, resp) = ui.allocate_exact_size(Vec2::new(BAND_W, 40.0), Sense::click_and_drag());
    let band = Rect::from_min_size(
        area.min + Vec2::new(6.0, 2.0),
        Vec2::new(BAND_W - 12.0, 22.0),
    );
    let p = ui.painter();
    crate::ui::dockers::gradient_swatch(p, band, &f.blended_stops());
    p.rect_stroke(
        band,
        0.0,
        EStroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Inside,
    );
    // The nodes: small squares in their colour below the band, the
    // picked one with a mark above it.
    let node_rect = |t: f64| {
        Rect::from_center_size(
            Pos2::new(band_x(band, t), band.bottom() + 8.0),
            Vec2::splat(9.0),
        )
    };
    for (i, s) in f.stops.iter().enumerate() {
        let r = node_rect(s.pos);
        let [cr, cg, cb] = s.color.to_rgb8();
        p.rect_filled(r, 0.0, Color32::from_rgb(cr, cg, cb));
        let stroke = if i == *node {
            EStroke::new(2.0, Tokens::ACCENT)
        } else {
            EStroke::new(1.0, Tokens::text_dim())
        };
        p.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Outside);
    }
    let at = resp.interact_pointer_pos();
    let hit = |at: Pos2| {
        f.stops
            .iter()
            .position(|s| node_rect(s.pos).expand(3.0).contains(at))
    };
    let t_at = |at: Pos2| f64::from((at.x - band.left()) / band.width()).clamp(0.0, 1.0);
    if resp.double_clicked() {
        if let Some(at) = at {
            match hit(at) {
                Some(i) if i > 0 && i + 1 < f.stops.len() => {
                    f.stops.remove(i);
                    *node = i - 1;
                }
                Some(_) => {}
                None => {
                    if let Some(i) = add_node(f, t_at(at)) {
                        *node = i;
                    }
                }
            }
        }
    } else if resp.drag_started() || resp.clicked() {
        if let Some(i) = at.and_then(hit) {
            *node = i;
        }
    }
    if resp.dragged() {
        if let Some(at) = at {
            move_node(f, *node, t_at(at));
        }
    }
    ui.horizontal(|ui| {
        if ui
            .button(tr("edit_fill.reverse"))
            .on_hover_text(tr("edit_fill.reverse_tip"))
            .clicked()
        {
            reverse(f);
            *node = f.stops.len().saturating_sub(1) - (*node).min(f.stops.len().saturating_sub(1));
        }
        ui.label(
            egui::RichText::new(tr("edit_fill.band_hint"))
                .size(11.0)
                .color(Tokens::text_dim()),
        );
    });
}

/// Add a node at `t` in the colour and opacity the gradient has there;
/// its index (none when a node is there already).
pub fn add_node(f: &mut Fountain, t: f64) -> Option<usize> {
    let i = f.stops.iter().position(|s| s.pos > t)?;
    if i == 0 || f.stops[i].pos - t < 1e-3 || t - f.stops[i - 1].pos < 1e-3 {
        return None;
    }
    let at = tracedraw_core::style::sample_stop(&f.blended_stops(), t);
    f.stops.insert(
        i,
        Stop {
            opacity: at.opacity,
            ..Stop::new(t, at.color)
        },
    );
    Some(i)
}

/// Move node `i` to `t`, kept between its neighbours (the ends stay).
pub fn move_node(f: &mut Fountain, i: usize, t: f64) {
    let n = f.stops.len();
    if i == 0 || i + 1 >= n {
        return;
    }
    let lo = f.stops[i - 1].pos + 1e-3;
    let hi = f.stops[i + 1].pos - 1e-3;
    if lo <= hi {
        f.stops[i].pos = t.clamp(lo, hi);
    }
}

/// The colours the other way round: each node at `1 - pos`, the
/// midpoints mirrored with them.
pub fn reverse(f: &mut Fountain) {
    let n = f.stops.len();
    let mids: Vec<f64> = f.stops.iter().map(|s| s.mid).collect();
    f.stops.reverse();
    for (k, s) in f.stops.iter_mut().enumerate() {
        s.pos = 1.0 - s.pos;
        // The blend after this node was the one before the old node.
        s.mid = if k + 1 < n {
            1.0 - mids[n - 2 - k]
        } else {
            0.5
        };
    }
}

/// The picked node's colour, transparency, position and midpoint.
fn node_fields(ui: &mut Ui, f: &mut Fountain, node: usize) {
    let n = f.stops.len();
    let Some(s) = f.stops.get_mut(node) else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label(tr("edit_fill.node_color"));
        crate::ui::color_picker::color_button(ui, &mut s.color);
        ui.label(tr("toolbar.node_transparency"));
        let mut tr_pct = f64::from((1.0 - s.opacity) * 100.0).round();
        if ui
            .add(
                crate::ui::field::NumField::new(&mut tr_pct)
                    .range(0.0..=100.0)
                    .speed(1.0)
                    .suffix(" %")
                    .width(54.0),
            )
            .changed()
        {
            s.opacity = (1.0 - tr_pct / 100.0).clamp(0.0, 1.0) as f32;
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("toolbar.node_position"));
        let mut pos = f.stops[node].pos * 100.0;
        if ui
            .add_enabled(
                node > 0 && node + 1 < n,
                crate::ui::field::NumField::new(&mut pos)
                    .range(0.0..=100.0)
                    .speed(1.0)
                    .suffix(" %")
                    .width(54.0),
            )
            .changed()
        {
            move_node(f, node, pos / 100.0);
        }
        ui.label(tr("edit_fill.midpoint"));
        let mut mid = f.stops[node].mid * 100.0;
        if ui
            .add_enabled(
                node + 1 < n,
                crate::ui::field::NumField::new(&mut mid)
                    .range(1.0..=99.0)
                    .speed(1.0)
                    .suffix(" %")
                    .width(54.0),
            )
            .on_hover_text(tr("edit_fill.midpoint_tip"))
            .changed()
        {
            f.stops[node].mid = (mid / 100.0).clamp(0.01, 0.99);
        }
    });
}

/// Rotation, centre and edge pad, mirror and inversion.
fn transform(ui: &mut Ui, f: &mut Fountain) {
    ui.horizontal(|ui| {
        ui.label(tr("edit_fill.rotate"));
        ui.add(
            crate::ui::field::NumField::new(&mut f.angle)
                .range(-360.0..=360.0)
                .speed(1.0)
                .max_decimals(1)
                .suffix("°")
                .width(60.0),
        );
        if !matches!(f.kind, FountainKind::Linear | FountainKind::Shaped) {
            ui.label(tr("edit_fill.x"));
            let mut x = f.offset.x * 100.0;
            if ui
                .add(
                    crate::ui::field::NumField::new(&mut x)
                        .range(-100.0..=100.0)
                        .suffix(" %")
                        .width(54.0),
                )
                .changed()
            {
                f.offset.x = x / 100.0;
            }
            ui.label(tr("edit_fill.y"));
            let mut y = f.offset.y * 100.0;
            if ui
                .add(
                    crate::ui::field::NumField::new(&mut y)
                        .range(-100.0..=100.0)
                        .suffix(" %")
                        .width(54.0),
                )
                .changed()
            {
                f.offset.y = y / 100.0;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("edit_fill.edge_pad"));
        let mut pad = f.edge_pad * 100.0;
        if ui
            .add(
                crate::ui::field::NumField::new(&mut pad)
                    .range(0.0..=49.0)
                    .suffix(" %")
                    .width(54.0),
            )
            .changed()
        {
            f.edge_pad = pad / 100.0;
        }
        ui.checkbox(&mut f.bilinear, tr("docker.bilinear"))
            .on_hover_text(tr("docker.bilinear_tip"));
        ui.checkbox(&mut f.invert, tr("docker.invert_colours"))
            .on_hover_text(tr("docker.invert_colours_tip"));
    });
    if f.handles.is_some() {
        // Nodes placed with Interactive Fill: back to the angle and centre.
        if ui
            .button(tr("edit_fill.reset_nodes"))
            .on_hover_text(tr("edit_fill.reset_nodes_tip"))
            .clicked()
        {
            f.handles = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::ShapeKind;

    fn two() -> Fountain {
        Fountain::two(FountainKind::Linear, Color::BLACK, Color::WHITE, 0.0)
    }

    #[test]
    fn nodes_are_added_moved_and_reversed() {
        let mut f = two();
        assert_eq!(add_node(&mut f, 0.25), Some(1));
        assert_eq!(add_node(&mut f, 0.25), None);
        move_node(&mut f, 1, 0.9);
        assert!((f.stops[1].pos - 0.9).abs() < 1e-9);
        move_node(&mut f, 1, 2.0);
        assert!(f.stops[1].pos < 1.0);
        move_node(&mut f, 0, 0.5);
        assert_eq!(f.stops[0].pos, 0.0);
        f.stops[0].mid = 0.2;
        let first = f.stops[0].color;
        reverse(&mut f);
        assert_eq!(f.stops.last().map(|s| s.color), Some(first));
        assert_eq!(f.stops[0].pos, 0.0);
        // The blend that was first is last now, its midpoint mirrored.
        let k = f.stops.len() - 2;
        assert!((f.stops[k].mid - 0.8).abs() < 1e-9, "{:?}", f.stops);
        reverse(&mut f);
        assert!((f.stops[0].mid - 0.2).abs() < 1e-9);
    }

    /// OK changes only what was changed on every selected object, with
    /// Overprint fill in the same step (#361).
    #[test]
    fn edit_fill_applies_what_was_changed() {
        let mut app = App::headless();
        let r = |app: &mut App, x: f64| {
            app.new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(x, 10.0, x + 20.0, 30.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect")
        };
        let a = r(&mut app, 10.0);
        let b = r(&mut app, 40.0);
        app.select(vec![a, b]);
        app.apply_fill(Fill::Fountain(two()));
        app.open_edit_fill();
        let crate::ui::dialogs::Dialog::EditFill(mut st) =
            std::mem::replace(&mut app.dialog, crate::ui::dialogs::Dialog::None)
        else {
            panic!("the dialog");
        };
        let Fill::Fountain(f) = &mut st.fill else {
            panic!("a gradient");
        };
        f.kind = FountainKind::Radial;
        add_node(f, 0.5);
        st.overprint = Some(true);
        let steps = app.engine.history_labels().0.len();
        app.apply_edit_fill(&st);
        assert_eq!(app.engine.history_labels().0.len(), steps + 1);
        for id in [a, b] {
            let s = app.doc().find_shape(id).cloned().expect("shape");
            let Fill::Fountain(f) = &s.fill else {
                panic!("a gradient");
            };
            assert_eq!(f.kind, FountainKind::Radial);
            assert_eq!(f.stops.len(), 3);
            assert!(s.overprint_fill);
        }
        // A new type for both.
        app.open_edit_fill();
        let crate::ui::dialogs::Dialog::EditFill(mut st) =
            std::mem::replace(&mut app.dialog, crate::ui::dialogs::Dialog::None)
        else {
            panic!("the dialog");
        };
        st.fill = new_of_kind(&st.fill, 1);
        st.touched = vec![FillField::All];
        app.apply_edit_fill(&st);
        let s = app.doc().find_shape(b).cloned().expect("shape");
        assert!(matches!(s.fill, Fill::Solid(_)));
    }
}

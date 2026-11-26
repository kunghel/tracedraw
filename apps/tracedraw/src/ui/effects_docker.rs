//! The Effects docker: the effect stack of the selected object, in the
//! order the effects apply. Each row switches its effect on or off (an
//! effect switched off is kept, at its place, and not drawn), moves it up
//! or down the stack, or removes it. Every change is one undo step.
//! Under the stack, the Presets (#2160): saved looks with a picture of
//! the selected object wearing each, applied to the selection with a
//! click (see `style_presets.rs`).

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::live::Effect;
use tracedraw_core::{Command, Shape};

/// The name an effect goes by in the stack.
pub fn effect_key(e: &Effect) -> &'static str {
    match e {
        Effect::Contour { .. } => "effect.contour",
        Effect::Blend { .. } => "effect.blend",
        Effect::Distort(_) => "effect.distort",
        Effect::Envelope { .. } => "effect.envelope",
        Effect::Perspective { .. } => "effect.perspective",
        Effect::MeshEnvelope { .. } => "effect.mesh_envelope",
        Effect::Extrude { .. } => "effect.extrude",
        Effect::Bevel { .. } => "effect.bevel",
        Effect::BlockShadow { .. } => "effect.block_shadow",
        Effect::InnerShadow { .. } => "effect.inner_shadow",
        Effect::OuterShadow { .. } => "effect.outer_shadow",
        Effect::Glow { inner: true, .. } => "effect.inner_glow",
        Effect::Glow { .. } => "effect.outer_glow",
        Effect::ColorOverlay { .. } => "effect.color_overlay",
        Effect::GradientOverlay { .. } => "effect.gradient_overlay",
        Effect::GaussianBlur { .. } => "effect.gaussian_blur",
        Effect::Halftone { .. } => "effect.halftone",
        Effect::Grain { .. } => "effect.grain",
        Effect::FillOpacity { .. } => "effect.fill_opacity",
        Effect::BlendIf { .. } => "effect.blend_if",
        Effect::Displace { .. } => "effect.displace",
        Effect::Symmetry { .. } => "effect.symmetry",
        Effect::Transform { .. } => "effect.transform",
        Effect::Grid { .. } => "effect.grid",
        Effect::Spray { .. } => "media.sprayer",
        Effect::Tiles { .. } => "media.tiles",
        Effect::LivePaint { .. } => "effect.live_paint",
        Effect::Lens(_) => "effect.lens",
        Effect::Transparency { .. } => "effect.transparency",
        Effect::OffsetPath { merge: true, .. } => "effect.merge_shapes",
        Effect::OffsetPath { .. } => "effect.offset_path",
        Effect::Combine { .. } => "effect.combine",
        Effect::Roughen { .. } => "effect.roughen",
        Effect::JitterNodes { .. } => "effect.jitter_nodes",
        Effect::Scribble { .. } => "effect.scribble",
        Effect::ShapeFrame { .. } => "effect.shape_frame",
        Effect::TrimMarks { .. } => "effect.trim_marks",
        Effect::Feather { .. } => "effect.feather",
        Effect::RoundCorners { .. } => "effect.round_corners",
        Effect::Slice { .. } => "effect.slice",
    }
}

/// A change asked for in the stack.
enum Change {
    Toggle(usize),
    Up(usize),
    Down(usize),
    Remove(usize),
}

/// Apply `change` to the stack of `s`; `None` when it changes nothing.
fn changed_stack(s: &Shape, change: Change) -> Option<Vec<(Effect, bool)>> {
    let mut stack = s.effect_stack();
    let n = stack.len();
    match change {
        Change::Toggle(i) if i < n => stack[i].1 = !stack[i].1,
        Change::Up(i) if i > 0 && i < n => stack.swap(i - 1, i),
        Change::Down(i) if i + 1 < n => stack.swap(i, i + 1),
        Change::Remove(i) if i < n => {
            stack.remove(i);
        }
        _ => return None,
    }
    Some(stack)
}

pub fn effects_docker(app: &mut App, ui: &mut Ui) {
    // A bitmap effect being set here rather than in its dialog (#4253).
    if app.fx_panel.is_some() {
        crate::ui::fx_panel::panel(app, ui);
        ui.add_space(4.0);
    }
    // The stackable looks with their values (#1141), then the whole stack.
    crate::ui::layer_effects::layer_effects(app, ui);
    ui.separator();
    effect_stack(app, ui);
    ui.separator();
    presets(app, ui);
}

/// The side of a preset's picture, points.
const PICTURE: f32 = 40.0;

/// The Presets: Save from the selection (with the name typed), then a
/// row per preset: its picture on the selected object, its name, Apply
/// and delete.
fn presets(app: &mut App, ui: &mut Ui) {
    ui.strong(tr("effects.presets"));
    let has = !app.selection.is_empty();
    ui.horizontal(|ui| {
        let id = egui::Id::new("style_preset_name");
        let mut name = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .desired_width(120.0)
                .hint_text(tr("effects.preset_name")),
        )
        .rich_tip(tr("effects.preset_name"));
        if ui
            .add_enabled(has, crate::ui::btn_icons::Ib::new("effects.preset_save"))
            .rich_tip(crate::ui::tips::pair(
                "effects.preset_save",
                "effects.preset_save_desc",
            ))
            .rich_disabled_tip(crate::ui::tips::pair(
                "effects.preset_save",
                "effects.preset_save_desc",
            ))
            .clicked()
            && app.save_style_preset(&name)
        {
            name.clear();
        }
        ui.data_mut(|d| d.insert_temp(id, name));
    });
    builtin_looks(app, ui);
    let n = app.settings.style_presets.len();
    if n == 0 {
        ui.label(egui::RichText::new(tr("effects.presets_hint")).color(Tokens::text_dim()));
        return;
    }
    let px = (PICTURE * ui.ctx().pixels_per_point()).round() as u32;
    let mut apply = None;
    let mut remove = None;
    for i in 0..n {
        let name = app.settings.style_presets[i].name.clone();
        let pic = app.style_preset_picture(ui.ctx(), i, px);
        ui.horizontal(|ui| {
            let resp = preset_picture(ui, pic);
            if resp
                .rich_tip(crate::ui::tips::pair(
                    "effects.preset_apply",
                    "effects.preset_picture_desc",
                ))
                .clicked()
                && has
            {
                apply = Some(i);
            }
            ui.label(&name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("\u{00D7}")
                    .rich_tip(tr("effects.preset_delete"))
                    .clicked()
                {
                    remove = Some(i);
                }
                if ui
                    .add_enabled(has, crate::ui::btn_icons::Ib::new("effects.preset_apply").small())
                    .rich_tip(crate::ui::tips::pair(
                        "effects.preset_apply",
                        "effects.preset_apply_desc",
                    ))
                    .rich_disabled_tip(crate::ui::tips::pair(
                        "effects.preset_apply",
                        "effects.preset_apply_desc",
                    ))
                    .clicked()
                {
                    apply = Some(i);
                }
            });
        });
    }
    if let Some(i) = apply {
        app.apply_style_preset(i);
    }
    if let Some(i) = remove {
        app.delete_style_preset(i);
    }
}

/// A preset's picture in a square, clickable.
fn preset_picture(ui: &mut Ui, pic: Option<egui::TextureHandle>) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(PICTURE, PICTURE), egui::Sense::click());
    crate::ui::before_after::checkerboard(ui.painter(), r);
    if let Some(t) = &pic {
        let size = t.size_vec2() / ui.ctx().pixels_per_point();
        let fit = (PICTURE / size.x.max(size.y).max(1.0)).min(1.0);
        let place = egui::Rect::from_center_size(r.center(), size * fit);
        ui.painter().image(
            t.id(),
            place,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    ui.painter().rect_stroke(
        r,
        2.0,
        egui::Stroke::new(1.0, Tokens::control_border()),
        egui::StrokeKind::Inside,
    );
    resp
}

/// The built-in looks (metals and glass), a picture each in a wrapping
/// row: a click gives the selection the look.
fn builtin_looks(app: &mut App, ui: &mut Ui) {
    let has = !app.selection.is_empty();
    ui.label(egui::RichText::new(tr("effects.looks_builtin")).color(Tokens::text_dim()));
    let px = (PICTURE * ui.ctx().pixels_per_point()).round() as u32;
    let looks = crate::builtin_looks::looks();
    let mut apply = None;
    ui.horizontal_wrapped(|ui| {
        for (i, p) in looks.iter().enumerate() {
            let pic = app.preset_picture(ui.ctx(), p, px);
            let resp = preset_picture(ui, pic);
            let tip = format!("{}\n{}", p.name, tr("effects.looks_builtin_desc"));
            if resp.rich_tip(tip).clicked() && has {
                apply = Some(i);
            }
        }
    });
    if let Some(i) = apply {
        app.apply_preset(&looks[i]);
    }
}

/// The effect stack of the first selected object.
fn effect_stack(app: &mut App, ui: &mut Ui) {
    let Some(shape) = app.selected_shapes().into_iter().next() else {
        ui.label(egui::RichText::new(tr("effects.select_hint")).color(Tokens::text_dim()));
        return;
    };
    if shape.effect_stack().is_empty() {
        ui.label(egui::RichText::new(tr("effects.none")).color(Tokens::text_dim()));
        return;
    }
    stack_rows(app, ui, &shape);
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tr("effects.order_hint")).color(Tokens::text_dim()));
}

/// The side of an eye button.
const EYE: f32 = 18.0;

/// An eye that shows whether something is drawn, open when `on`; a
/// click on it is the response's click.
pub fn eye_button(ui: &mut Ui, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(EYE, EYE), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 2.0, Tokens::tool_hover());
    }
    crate::ui::objects_docker::paint_eye(ui, rect, on);
    resp
}

/// The panel whose settings edit an effect of the stack: the effect's own
/// docker where it has one, else the Effects docker (Layer Effects).
pub fn edit_docker(e: &Effect) -> crate::app::DockerTab {
    use crate::app::DockerTab as D;
    match e {
        Effect::Contour { .. } => D::Contour,
        Effect::Blend { .. } => D::Blend,
        Effect::Envelope { .. } | Effect::MeshEnvelope { .. } => D::Envelope,
        Effect::Extrude { .. } => D::Extrude,
        Effect::Bevel { .. } => D::Bevel,
        Effect::Lens(_) => D::Lens,
        _ => D::Effects,
    }
}

/// A row per effect of `shape`'s stack, in the order they apply (#4231):
/// the eye that draws it or keeps it switched off at its place, its name
/// (dragged onto another object, it gives that object a copy), Edit,
/// which opens the panel that sets it, up, down and remove. The
/// Properties docker shows the same rows as the Effects docker.
pub fn stack_rows(app: &mut App, ui: &mut Ui, shape: &Shape) {
    let stack = shape.effect_stack();
    let n = stack.len();
    let mut edit = None;
    let mut change = None;
    let mut dropped = None;
    for (i, (effect, on)) in stack.iter().enumerate() {
        ui.horizontal(|ui| {
            if eye_button(ui, *on)
                .rich_tip(crate::ui::tips::pair("docker.show_hide_effect", "effects.toggle"))
                .clicked()
            {
                change = Some(Change::Toggle(i));
            }
            let name = egui::RichText::new(tr(effect_key(effect)));
            // The name can be dragged onto another object on the page,
            // which gets a copy of the effect (#4243).
            let label = ui
                .add(
                    egui::Label::new(if *on {
                        name
                    } else {
                        name.color(Tokens::text_dim())
                    })
                    .sense(egui::Sense::drag()),
                )
                .rich_tip(crate::ui::tips::pair(effect_key(effect), "effects.drag_to_object"));
            if label.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                if let Some(at) = ui.ctx().pointer_latest_pos() {
                    egui::Area::new(egui::Id::new("effect_drag_label"))
                        .order(egui::Order::Tooltip)
                        .fixed_pos(at + egui::vec2(14.0, 6.0))
                        .interactable(false)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.label(tr(effect_key(effect)));
                            });
                        });
                }
            } else if label.drag_stopped() {
                let main = ui.ctx().viewport_id() == egui::ViewportId::ROOT;
                if let Some(at) = ui.ctx().pointer_latest_pos().filter(|_| main) {
                    dropped = Some((effect.clone(), *on, at));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("\u{00D7}")
                    .rich_tip(tr("effects.remove"))
                    .clicked()
                {
                    change = Some(Change::Remove(i));
                }
                if ui
                    .add_enabled(i + 1 < n, egui::Button::new("\u{23F7}").small())
                    .rich_tip(tr("effects.down"))
                    .clicked()
                {
                    change = Some(Change::Down(i));
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new("\u{23F6}").small())
                    .rich_tip(tr("effects.up"))
                    .clicked()
                {
                    change = Some(Change::Up(i));
                }
                if ui
                    .small_button("\u{270F}")
                    .rich_tip(crate::ui::tips::pair("docker.edit_effect", "effects.edit_desc"))
                    .clicked()
                {
                    edit = Some(edit_docker(effect));
                    // Set in Layer Effects: its entry lights up there (#4350).
                    if let Some(kind) = crate::ui::layer_effects::Kind::ALL
                        .into_iter()
                        .find(|k| k.matches(effect))
                    {
                        crate::ui::layer_effects::highlight(ui.ctx(), kind);
                    }
                }
            });
        });
    }
    if let Some(tab) = edit {
        app.open_docker(tab);
    }
    // An effect taken away keeps its values for Layer Effects (#4242).
    if let Some(Change::Remove(i)) = &change {
        if let Some((e, _)) = shape.effect_stack().get(*i) {
            if let Some(kind) = crate::ui::layer_effects::Kind::ALL
                .into_iter()
                .find(|k| k.matches(e))
            {
                app.layer_fx_last.insert(kind, e.clone());
            }
        }
    }
    if let Some(stack) = change.and_then(|c| changed_stack(&shape, c)) {
        let (effects, parked) = Shape::split_stack(stack);
        app.run(Command::SetEffectStack {
            shape: shape.id,
            effects,
            parked,
        });
    }
    if let Some((effect, on, at)) = dropped {
        app.drop_effect_on(&shape, &effect, on, at);
    }
}

impl App {
    /// An effect of `src` dragged from the Effects panel and let go at
    /// `at` on the screen: the object there, if another one, gets a copy
    /// of it, sized to it, on top of its own effects (#4243). False when
    /// nothing took it.
    pub fn drop_effect_on(&mut self, src: &Shape, effect: &Effect, on: bool, at: egui::Pos2) -> bool {
        if !self.canvas_rect.contains(at) || !self.has_document() || self.show_welcome {
            return false;
        }
        let Some(id) = self.hit_test_inside(self.view.to_page(at)) else {
            return false;
        };
        if id == src.id {
            return false;
        }
        let Some(target) = self.doc().find_shape(id).cloned() else {
            return false;
        };
        let mut stack = target.effect_stack();
        stack.push((crate::copy_effect::carried(effect, src, &target), on));
        let (effects, parked) = Shape::split_stack(stack);
        if let Err(e) = self.engine.run_batch(
            "Copy Effect",
            &[Command::SetEffectStack {
                shape: id,
                effects,
                parked,
            }],
        ) {
            self.status = e.to_string();
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Point, Rect, Vec2};
    use tracedraw_core::{Color, ShapeKind};

    /// An effect dragged from the panel and let go over another object
    /// gives it a copy, on top of its own effects; let go over the same
    /// object or over nothing, nothing changes (#4243).
    #[test]
    fn an_effect_dragged_onto_an_object_is_copied_to_it() {
        let mut app = App::headless();
        app.show_welcome = false;
        app.canvas_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        app.zoom_to_page();
        // Two squares in the window, at 100 and 200 px from its left.
        let rect = |app: &mut App, x: f32| {
            let p0 = app.view.to_page(egui::pos2(x, 100.0));
            let p1 = app.view.to_page(egui::pos2(x + 40.0, 140.0));
            app.new_shape(ShapeKind::Rect {
                rect: Rect::new(p0.x.min(p1.x), p0.y.min(p1.y), p0.x.max(p1.x), p0.y.max(p1.y)),
                radius: 0.0,
                corners: None,
            })
            .expect("shape")
        };
        let a = rect(&mut app, 100.0);
        let b = rect(&mut app, 200.0);
        let shadow = Effect::BlockShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::BLACK,
            gap: 0.0,
            expand: 0.0,
        };
        app.run(Command::SetEffects {
            shape: a,
            effects: vec![shadow.clone()],
        });
        app.select(vec![a]);
        let src = app.doc().find_shape(a).expect("a").clone();
        let centre = |app: &App, id| {
            use tracedraw_core::geometry::Shape as _;
            let s = app.doc().find_shape(id).expect("shape");
            app.view.to_screen(s.page_path().bounding_box().center())
        };
        // Onto itself or onto nothing: no change.
        assert!(!app.drop_effect_on(&src, &shadow, true, centre(&app, a)));
        let empty = egui::pos2(600.0, 500.0);
        assert!(!app.drop_effect_on(&src, &shadow, true, empty));
        assert!(app.doc().find_shape(b).expect("b").effects.is_empty());
        // Onto the other object: it gets the effect, one undo step.
        assert!(app.drop_effect_on(&src, &shadow, true, centre(&app, b)));
        let got = &app.doc().find_shape(b).expect("b").effects;
        assert_eq!(got.len(), 1);
        assert!(matches!(got[0], Effect::BlockShadow { .. }));
        assert_eq!(app.doc().find_shape(a).expect("a").effects.len(), 1);
        app.undo();
        assert!(app.doc().find_shape(b).expect("b").effects.is_empty());
    }

    #[test]
    fn the_stack_switches_moves_and_removes_effects() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        let shadow = Effect::BlockShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::BLACK,
            gap: 0.0,
            expand: 0.0,
        };
        let persp = Effect::Perspective {
            corners: [Point::ZERO; 4],
        };
        app.run(Command::SetEffects {
            shape: id,
            effects: vec![shadow.clone(), persp.clone()],
        });
        app.select(vec![id]);
        let shape = |app: &App| app.doc().find_shape(id).expect("shape").clone();
        let apply = |app: &mut App, c: Change| {
            let s = shape(app);
            let (effects, parked) = Shape::split_stack(changed_stack(&s, c).expect("changes"));
            app.run(Command::SetEffectStack {
                shape: id,
                effects,
                parked,
            });
        };
        apply(&mut app, Change::Toggle(0));
        assert_eq!(shape(&app).effects, vec![persp.clone()]);
        assert_eq!(shape(&app).effect_stack()[0], (shadow.clone(), false));
        apply(&mut app, Change::Down(0));
        assert_eq!(
            shape(&app).effect_stack(),
            vec![(persp.clone(), true), (shadow.clone(), false)]
        );
        apply(&mut app, Change::Toggle(1));
        assert_eq!(shape(&app).effects, vec![persp.clone(), shadow.clone()]);
        apply(&mut app, Change::Remove(0));
        assert_eq!(shape(&app).effects, vec![shadow]);
        assert!(changed_stack(&shape(&app), Change::Up(0)).is_none());
        assert!(changed_stack(&shape(&app), Change::Remove(5)).is_none());
        app.undo();
        assert_eq!(shape(&app).effects.len(), 2);
        // The docker draws with and without a selection.
        let ctx = crate::theme::ui_context();
        let mut out = ctx.run_ui(Default::default(), |ui| effects_docker(&mut app, ui));
        out.textures_delta.clear();
        app.select(Vec::new());
        let mut out = ctx.run_ui(Default::default(), |ui| effects_docker(&mut app, ui));
        out.textures_delta.clear();
    }
}

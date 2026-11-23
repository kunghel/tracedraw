//! Layer Effects (#1141): the looks that stack on one object, each
//! switched on with its box and set on the right of the list, drawn live
//! on the page as the values change. A box switched off keeps the
//! effect's values at its place in the stack (the Effects docker's
//! switch), the × takes it away. Fill Opacity, under the list, fades the
//! object itself and leaves what the effects draw as it is.
//!
//! Each entry is an effect of the stack: Bevel / Emboss a bevel, Outline
//! a one-step contour outside, 3D a parallel extrusion, the shadows,
//! glows, overlays and Gaussian blur effects of their own
//! (`tracedraw_core::live::Effect`). Every change is one undo step and
//! goes to every selected object; the values shown are the first
//! selected object's.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::field::NumField;
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::geometry::{Affine, Vec2};
use tracedraw_core::live::{Effect, MergeMode, SurfaceShade, BLEND_IF_ALL};
use tracedraw_core::{Color, Command, Shape};

/// An entry of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bevel,
    Outline,
    ThreeD,
    InnerShadow,
    OuterShadow,
    InnerGlow,
    OuterGlow,
    ColorOverlay,
    GradientOverlay,
    GaussianBlur,
    OffsetPath,
    Transform,
    Grid,
    BlendIf,
    Displace,
    Combine,
    Roughen,
    JitterNodes,
    Scribble,
    ShapeFrame,
    TrimMarks,
    Feather,
    Halftone,
}

impl Kind {
    pub const ALL: [Kind; 23] = [
        Kind::Bevel,
        Kind::Outline,
        Kind::ThreeD,
        Kind::InnerShadow,
        Kind::OuterShadow,
        Kind::InnerGlow,
        Kind::OuterGlow,
        Kind::ColorOverlay,
        Kind::GradientOverlay,
        Kind::GaussianBlur,
        Kind::OffsetPath,
        Kind::Transform,
        Kind::Grid,
        Kind::BlendIf,
        Kind::Displace,
        Kind::Combine,
        Kind::Roughen,
        Kind::JitterNodes,
        Kind::Scribble,
        Kind::ShapeFrame,
        Kind::TrimMarks,
        Kind::Feather,
        Kind::Halftone,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Kind::Bevel => "layer_fx.bevel",
            Kind::Outline => "layer_fx.outline",
            Kind::ThreeD => "layer_fx.three_d",
            Kind::InnerShadow => "effect.inner_shadow",
            Kind::OuterShadow => "effect.outer_shadow",
            Kind::InnerGlow => "effect.inner_glow",
            Kind::OuterGlow => "effect.outer_glow",
            Kind::ColorOverlay => "effect.color_overlay",
            Kind::GradientOverlay => "effect.gradient_overlay",
            Kind::GaussianBlur => "effect.gaussian_blur",
            Kind::OffsetPath => "effect.offset_path",
            Kind::Transform => "effect.transform",
            Kind::Grid => "effect.grid",
            Kind::BlendIf => "effect.blend_if",
            Kind::Displace => "effect.displace",
            Kind::Combine => "effect.combine",
            Kind::Roughen => "effect.roughen",
            Kind::JitterNodes => "effect.jitter_nodes",
            Kind::Scribble => "effect.scribble",
            Kind::ShapeFrame => "effect.shape_frame",
            Kind::TrimMarks => "effect.trim_marks",
            Kind::Feather => "effect.feather",
            Kind::Halftone => "effect.halftone",
        }
    }

    /// `e` is this entry's effect.
    pub fn matches(self, e: &Effect) -> bool {
        matches!(
            (self, e),
            (Kind::Bevel, Effect::Bevel { .. })
                | (
                    Kind::Outline,
                    Effect::Contour {
                        steps: 1,
                        outside: true,
                        to_center: false,
                        ..
                    },
                )
                | (Kind::ThreeD, Effect::Extrude { .. })
                | (Kind::InnerShadow, Effect::InnerShadow { .. })
                | (Kind::OuterShadow, Effect::OuterShadow { .. })
                | (Kind::InnerGlow, Effect::Glow { inner: true, .. })
                | (Kind::OuterGlow, Effect::Glow { inner: false, .. })
                | (Kind::ColorOverlay, Effect::ColorOverlay { .. })
                | (Kind::GradientOverlay, Effect::GradientOverlay { .. })
                | (Kind::GaussianBlur, Effect::GaussianBlur { .. })
                | (Kind::OffsetPath, Effect::OffsetPath { .. })
                | (Kind::Transform, Effect::Transform { .. })
                | (Kind::Grid, Effect::Grid { .. })
                | (Kind::BlendIf, Effect::BlendIf { .. })
                | (Kind::Displace, Effect::Displace { .. })
                | (Kind::Combine, Effect::Combine { .. })
                | (Kind::Roughen, Effect::Roughen { .. })
                | (Kind::JitterNodes, Effect::JitterNodes { .. })
                | (Kind::Scribble, Effect::Scribble { .. })
                | (Kind::ShapeFrame, Effect::ShapeFrame { .. })
                | (Kind::TrimMarks, Effect::TrimMarks { .. })
                | (Kind::Feather, Effect::Feather { .. })
                | (Kind::Halftone, Effect::Halftone { .. })
        )
    }

    /// The effect an entry starts with when its box is first ticked.
    pub fn default_effect(self) -> Effect {
        match self {
            Kind::Bevel => {
                let b = crate::effects_ui::BevelSettings::default();
                Effect::Bevel {
                    distance: b.distance,
                    light_angle: b.light_angle,
                    intensity: b.intensity,
                    style: b.style,
                    shadow_color: b.shadow_color,
                    light_color: b.light_color,
                    level: 4,
                    emboss_color: b.emboss_color,
                    directional: b.directional,
                }
            }
            Kind::Outline => {
                let c = crate::contour_tool::ContourSettings {
                    steps: 1,
                    offset: 1.0,
                    direction: crate::contour_tool::ContourDirection::Outside,
                    fill_to: Color::BLACK,
                    outline_to: Color::BLACK,
                    ..Default::default()
                };
                c.effect()
            }
            Kind::ThreeD => crate::extrude_tool::ExtrudeSettings {
                kind: tracedraw_core::extrude::ExtrudeKind::BackParallel,
                vector: Vec2::new(2.0, -2.0),
                ..Default::default()
            }
            .effect(Affine::IDENTITY, Affine::IDENTITY),
            Kind::InnerShadow => Effect::InnerShadow {
                offset: Vec2::new(1.0, -1.0),
                color: Color::BLACK,
                opacity: 0.5,
                blur: 1.0,
                merge: MergeMode::Multiply,
                spread: 0.0,
            },
            Kind::OuterShadow => Effect::OuterShadow {
                offset: Vec2::new(1.5, -1.5),
                color: Color::BLACK,
                opacity: 0.5,
                blur: 1.5,
                merge: MergeMode::Multiply,
            },
            Kind::InnerGlow => Effect::Glow {
                inner: true,
                color: Color::rgb8(255, 240, 160),
                opacity: 0.75,
                blur: 1.5,
                merge: MergeMode::Screen,
            },
            Kind::OuterGlow => Effect::Glow {
                inner: false,
                color: Color::rgb8(255, 240, 160),
                opacity: 0.75,
                blur: 2.0,
                merge: MergeMode::Screen,
            },
            Kind::ColorOverlay => Effect::ColorOverlay {
                color: Color::rgb8(220, 40, 40),
                opacity: 0.5,
            },
            Kind::GradientOverlay => Effect::GradientOverlay {
                from: Color::BLACK,
                to: Color::WHITE,
                angle: 90.0,
                opacity: 0.5,
            },
            Kind::GaussianBlur => Effect::GaussianBlur { radius: 1.0 },
            Kind::Combine => Effect::Combine {
                mode: tracedraw_core::shaping::CombineMode::Weld,
            },
            Kind::Roughen => Effect::Roughen {
                size: 1.0,
                detail: 5,
                smooth: true,
                seed: 1,
            },
            Kind::JitterNodes => Effect::JitterNodes {
                across: 2.0,
                down: 2.0,
                handles: false,
                seed: 1,
            },
            Kind::Scribble => Effect::Scribble {
                angle: 30.0,
                spacing: 1.5,
                width: 0.5,
                loose: 0.5,
                seed: 1,
                curvy: 0.0,
                overlap: 0.0,
                spacing_var: 0.0,
                width_var: 0.0,
            },
            Kind::ShapeFrame => Effect::ShapeFrame {
                form: tracedraw_core::rough::FrameForm::Rounded,
                pad: 3.0,
                radius: 2.0,
            },
            Kind::TrimMarks => Effect::TrimMarks {
                length: 5.0,
                gap: 2.0,
                width: 0.1,
            },
            Kind::Feather => Effect::Feather { radius: 3.0 },
            Kind::OffsetPath => Effect::OffsetPath {
                distance: 1.0,
                smooth: 0.0,
                join: tracedraw_core::shaping::Join::Round,
                merge: false,
            },
            // Rings closing in on the centre, a little turned each time.
            Kind::Transform => Effect::Transform {
                scale_x: 85.0,
                scale_y: 85.0,
                move_x: 0.0,
                move_y: 0.0,
                angle: 15.0,
                copies: 6,
                reflect_x: false,
                reflect_y: false,
                random: false,
                seed: 1,
                scale_strokes: false,
                pivot_x: 0.0,
                pivot_y: 0.0,
            },
            Kind::Grid => Effect::Grid {
                columns: 4,
                rows: 3,
                gap_x: 2.0,
                gap_y: 2.0,
                shift_x: 0.0,
                shift_y: 0.0,
            },
            // What lies beneath hides the object where it is dark, fading
            // in to a quarter of the way up: a light text laid on a photo
            // takes its grain.
            Kind::BlendIf => Effect::BlendIf {
                this: BLEND_IF_ALL,
                below: [0.0, 0.25, 1.0, 1.0],
            },
            // Pushed up to 2 mm by the photo beneath, its shadows taken
            // on at 60 % (#3595); the map is sampled when it is set.
            Kind::Displace => Effect::Displace {
                map: None,
                horizontal: 2.0,
                vertical: 2.0,
                shade: SurfaceShade::Multiply,
                strength: 0.6,
            },
            Kind::Halftone => Effect::Halftone {
                pattern: 0,
                mode: 0,
                size: 1.0,
                scale: 1.0,
                softness: 0.1,
                angle: 45.0,
                center_x: 0.5,
                center_y: 0.5,
                clip: true,
                color: Color::BLACK,
            },
        }
    }
}

/// Each Displace of `stack`, on the object `id`, given its photo map
/// (#3595): its own from `before` when the edit kept one (a map stays
/// with the object it was sampled for, though the values shown came
/// from another), a new one sampled from the photo beneath when it had
/// none or the edit cleared it.
pub fn fill_displace_maps(
    doc: &tracedraw_core::Document,
    id: tracedraw_core::ShapeId,
    before: &[(Effect, bool)],
    stack: &mut [(Effect, bool)],
) {
    let own = before.iter().find_map(|(e, _)| match e {
        Effect::Displace { map: Some(m), .. } => Some(m.clone()),
        _ => None,
    });
    for (e, _) in stack.iter_mut() {
        if let Effect::Displace {
            map,
            horizontal,
            vertical,
            ..
        } = e
        {
            *map = match (map.is_some(), &own) {
                (true, Some(m)) => Some(m.clone()),
                _ => tracedraw_core::surface_map::sample(
                    doc,
                    id,
                    horizontal.abs().max(vertical.abs()),
                ),
            };
        }
    }
}

/// Where the entry's effect is in `stack`, the first that is.
pub fn find(stack: &[(Effect, bool)], kind: Kind) -> Option<usize> {
    stack.iter().position(|(e, _)| kind.matches(e))
}

/// The fill opacity `shape` has (1 without the effect).
pub fn fill_opacity(shape: &Shape) -> f64 {
    shape
        .effects
        .iter()
        .find_map(|e| match e {
            Effect::FillOpacity { opacity } => Some(*opacity),
            _ => None,
        })
        .unwrap_or(1.0)
}

impl App {
    /// Change the effect stack of every selected object with `edit`, as
    /// one undo step; objects `edit` leaves as they were are not touched.
    fn edit_layer_effects(&mut self, edit: impl Fn(&mut Vec<(Effect, bool)>)) {
        let mut cmds = Vec::new();
        let doc = self.doc();
        for s in self.selected_shapes() {
            let before = s.effect_stack();
            let mut stack = before.clone();
            edit(&mut stack);
            fill_displace_maps(doc, s.id, &before, &mut stack);
            if stack != before {
                let (effects, parked) = Shape::split_stack(stack);
                cmds.push(Command::SetEffectStack {
                    shape: s.id,
                    effects,
                    parked,
                });
            }
        }
        if !cmds.is_empty() {
            let _ = self.engine.run_batch("Layer Effects", &cmds);
        }
    }

    /// Tick or clear an entry's box on the selection: a cleared effect
    /// keeps its values, switched off, and comes back with them; an
    /// object without it gets `fallback` (the values shown).
    pub fn set_layer_effect_on(&mut self, kind: Kind, on: bool, fallback: &Effect) {
        self.edit_layer_effects(|stack| match find(stack, kind) {
            Some(i) => stack[i].1 = on,
            None if on => stack.push((fallback.clone(), true)),
            None => {}
        });
    }

    /// The entry's effect on the selection becomes `effect`.
    pub fn set_layer_effect(&mut self, kind: Kind, effect: &Effect) {
        self.edit_layer_effects(|stack| {
            if let Some(i) = find(stack, kind) {
                stack[i].0 = effect.clone();
            }
        });
    }

    /// After a move with the Pick tool: each moved object with Displace
    /// takes the photo now beneath it, in the move's undo step, so a
    /// print slid over a cloth follows the folds where it lands (#3595).
    pub fn displace_after_move(&mut self) {
        let ids = self.selection.clone();
        for id in ids {
            let doc = self.doc();
            let Some(shape) = doc.find_shape(id) else {
                continue;
            };
            if !shape
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Displace { map: Some(_), .. }))
            {
                continue;
            }
            let mut effects = shape.effects.clone();
            for e in effects.iter_mut() {
                if let Effect::Displace {
                    map,
                    horizontal,
                    vertical,
                    ..
                } = e
                {
                    let margin = horizontal.abs().max(vertical.abs());
                    *map = tracedraw_core::surface_map::sample(doc, id, margin);
                }
            }
            if effects == shape.effects {
                continue;
            }
            let cmd = Command::SetEffects { shape: id, effects };
            if self.engine.amend(&cmd).is_err() {
                self.run(cmd);
            }
        }
    }

    /// Take the entry's effect off the selection.
    pub fn remove_layer_effect(&mut self, kind: Kind) {
        self.edit_layer_effects(|stack| stack.retain(|(e, _)| !kind.matches(e)));
    }

    /// The selection's fill opacity becomes `opacity` (0 to 1); at 1 the
    /// effect goes.
    pub fn set_fill_opacity(&mut self, opacity: f64) {
        let o = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.edit_layer_effects(|stack| {
            let at = stack
                .iter()
                .position(|(e, _)| matches!(e, Effect::FillOpacity { .. }));
            match at {
                Some(i) if o >= 1.0 => {
                    stack.remove(i);
                }
                Some(i) => stack[i] = (Effect::FillOpacity { opacity: o }, true),
                None if o < 1.0 => stack.push((Effect::FillOpacity { opacity: o }, true)),
                None => {}
            }
        });
    }
}

/// The list's highlighted entry, kept between frames.
fn highlighted_id() -> egui::Id {
    egui::Id::new("layer_effects_highlighted")
}

/// The Layer Effects section of the Effects docker.
pub fn layer_effects(app: &mut App, ui: &mut Ui) {
    ui.strong(tr("layer_fx.title"));
    let Some(shape) = app.selected_shapes().into_iter().next() else {
        ui.label(egui::RichText::new(tr("effects.select_hint")).color(Tokens::text_dim()));
        return;
    };
    let stack = shape.effect_stack();
    let mut current = ui
        .data(|d| d.get_temp::<usize>(highlighted_id()))
        .and_then(|i| Kind::ALL.get(i).copied())
        .unwrap_or(Kind::Bevel);
    for kind in Kind::ALL {
        let at = find(&stack, kind);
        ui.horizontal(|ui| {
            let mut on = at.is_some_and(|i| stack[i].1);
            if ui
                .checkbox(&mut on, "")
                .rich_tip(tr("layer_fx.toggle"))
                .changed()
            {
                let fallback = at
                    .map(|i| stack[i].0.clone())
                    .unwrap_or_else(|| kind.default_effect());
                app.set_layer_effect_on(kind, on, &fallback);
                current = kind;
            }
            if ui
                .selectable_label(current == kind, tr(kind.key()))
                .rich_tip(tr("layer_fx.pick"))
                .clicked()
            {
                current = kind;
            }
            if at.is_some() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("\u{00D7}")
                        .rich_tip(tr("effects.remove"))
                        .clicked()
                    {
                        app.remove_layer_effect(kind);
                    }
                });
            }
        });
    }
    let index = Kind::ALL.iter().position(|k| *k == current).unwrap_or(0);
    ui.data_mut(|d| d.insert_temp(highlighted_id(), index));
    ui.separator();
    // The highlighted entry's values: the object's, or those it would
    // start with.
    let at = find(&stack, current);
    let mut effect = at
        .map(|i| stack[i].0.clone())
        .unwrap_or_else(|| current.default_effect());
    ui.label(egui::RichText::new(tr(current.key())).color(Tokens::SELECTION));
    let changed = settings(app, ui, &mut effect);
    if changed {
        match at {
            Some(_) => app.set_layer_effect(current, &effect),
            // Setting a value switches the entry on with it.
            None => app.set_layer_effect_on(current, true, &effect),
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(tr("layer_fx.fill_opacity"))
            .rich_tip(tr("layer_fx.fill_opacity_desc"));
        let mut pct = (fill_opacity(&shape) * 100.0).round();
        if ui
            .add(NumField::new(&mut pct).range(0.0..=100.0).suffix(" %"))
            .rich_tip(tr("layer_fx.fill_opacity_desc"))
            .changed()
        {
            app.set_fill_opacity(pct / 100.0);
        }
    });
    ui.label(egui::RichText::new(tr("layer_fx.hint")).color(Tokens::text_dim()));
}

/// A length field in the document's units, the value in mm.
fn length(app: &App, ui: &mut Ui, label: &str, mm: &mut f64, max: f64) -> bool {
    ui.label(tr(label));
    let mut v = app.units.world_from_mm(*mm);
    let r = ui
        .add(
            NumField::new(&mut v)
                .range(0.0..=app.units.world_from_mm(max))
                .length_decimals(app.settings.precision.min(10) as usize)
                .unit_mm(app.units.world_to_mm(1.0))
                .suffix(format!(" {}", app.units.short()))
                .width(70.0),
        )
        .rich_tip(tr(label));
    if r.changed() {
        *mm = app.units.world_to_mm(v).clamp(0.0, max);
        true
    } else {
        false
    }
}

/// A whole number from `min` to `max`.
fn whole(ui: &mut Ui, label: &str, v: &mut u32, min: u32, max: u32) -> bool {
    ui.label(tr(label));
    let mut f = f64::from(*v);
    if ui
        .add(
            NumField::new(&mut f)
                .range(f64::from(min)..=f64::from(max))
                .width(50.0),
        )
        .rich_tip(tr(label))
        .changed()
    {
        *v = (f.round().clamp(f64::from(min), f64::from(max))) as u32;
        true
    } else {
        false
    }
}

/// A 0..1 value as a percentage.
fn percent(ui: &mut Ui, label: &str, v: &mut f64) -> bool {
    ui.label(tr(label));
    let mut pct = (*v * 100.0).round();
    if ui
        .add(NumField::new(&mut pct).range(0.0..=100.0).suffix(" %"))
        .rich_tip(tr(label))
        .changed()
    {
        *v = (pct / 100.0).clamp(0.0, 1.0);
        true
    } else {
        false
    }
}

/// Degrees, 0 to 360.
fn degrees(ui: &mut Ui, label: &str, v: &mut f64) -> bool {
    ui.label(tr(label));
    let mut d = v.rem_euclid(360.0);
    if ui
        .add(NumField::new(&mut d).range(0.0..=360.0).suffix("\u{00B0}"))
        .rich_tip(tr(label))
        .changed()
    {
        *v = d;
        true
    } else {
        false
    }
}

/// One of a few named values, kept as its index.
fn choice(ui: &mut Ui, label: &str, v: &mut u8, keys: &[&str]) -> bool {
    ui.label(tr(label));
    let mut changed = false;
    let shown = keys.get(usize::from(*v)).copied().unwrap_or(keys[0]);
    crate::ui::keynav::ComboBox::from_id_salt(("layer_fx_choice", label))
        .selected_text(tr(shown))
        .width(110.0)
        .show_ui(ui, |ui| {
            for (i, key) in keys.iter().enumerate() {
                if ui.selectable_label(usize::from(*v) == i, tr(key)).clicked() {
                    *v = i as u8;
                    changed = true;
                }
            }
        })
        .response
        .rich_tip(crate::ui::tips::keyed(label));
    changed
}

fn color(ui: &mut Ui, label: &str, c: &mut Color) -> bool {
    ui.label(tr(label));
    crate::ui::color_picker::color_button(ui, c)
}

fn merge(ui: &mut Ui, m: &mut MergeMode, salt: &str) -> bool {
    ui.label(tr("layer_fx.merge"));
    let mut changed = false;
    crate::ui::keynav::ComboBox::from_id_salt(("layer_fx_merge", salt))
        .selected_text(m.name())
        .width(110.0)
        .show_ui(ui, |ui| {
            for mode in MergeMode::ALL {
                if ui.selectable_label(*m == mode, mode.name()).clicked() {
                    *m = mode;
                    changed = true;
                }
            }
        })
        .response
        .rich_tip(tr("layer_fx.merge"));
    changed
}

/// An offset as a distance and a direction (degrees counterclockwise
/// from the right).
pub fn polar(v: Vec2) -> (f64, f64) {
    (v.hypot(), v.y.atan2(v.x).to_degrees().rem_euclid(360.0))
}

pub fn from_polar(distance: f64, angle: f64) -> Vec2 {
    let a = angle.to_radians();
    Vec2::new(distance * a.cos(), distance * a.sin())
}

/// Distance and direction of an offset.
fn offset(app: &App, ui: &mut Ui, v: &mut Vec2) -> bool {
    let (mut d, mut a) = polar(*v);
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= length(app, ui, "layer_fx.distance", &mut d, 200.0);
        changed |= degrees(ui, "layer_fx.angle", &mut a);
    });
    if changed {
        *v = from_polar(d, a);
    }
    changed
}

/// The highlighted effect's own controls; true when one changed.
fn settings(app: &App, ui: &mut Ui, e: &mut Effect) -> bool {
    let mut changed = false;
    match e {
        Effect::Bevel {
            distance,
            light_angle,
            intensity,
            style,
            light_color,
            shadow_color,
            level,
            ..
        } => {
            ui.horizontal(|ui| {
                ui.label(tr("layer_fx.type"));
                changed |= ui
                    .selectable_value(style, 0, tr("docker.soft_edge"))
                    .rich_tip(crate::ui::tips::pair(
                        "docker.soft_edge",
                        "docker.soft_edge_desc",
                    ))
                    .changed();
                changed |= ui
                    .selectable_value(style, 1, tr("docker.emboss"))
                    .rich_tip(crate::ui::tips::pair("docker.emboss", "docker.emboss_desc"))
                    .changed();
                changed |= ui
                    .selectable_value(style, 2, tr("docker.bevel_center"))
                    .rich_tip(crate::ui::tips::pair(
                        "docker.bevel_center",
                        "docker.bevel_center_desc",
                    ))
                    .changed();
            });
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.depth", distance, 100.0);
                ui.label(tr("docker.level"));
                let mut l = u32::from(*level);
                if ui
                    .add(NumField::new(&mut l).range(1..=16))
                    .rich_tip(crate::ui::tips::pair("docker.level", "docker.level_desc"))
                    .changed()
                {
                    *level = l.clamp(1, 16) as u8;
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                changed |= degrees(ui, "docker.light_angle", light_angle);
                ui.label(tr("docker.intensity"));
                if ui
                    .add(NumField::new(intensity).range(0.0..=100.0))
                    .rich_tip(tr("docker.intensity"))
                    .changed()
                {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                changed |= color(ui, "docker.light_color", light_color);
                changed |= color(ui, "layer_fx.shadow_color", shadow_color);
            });
        }
        Effect::Contour {
            offset: width,
            fill_to,
            outline_to,
            ..
        } => {
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.width", width, 100.0);
                if color(ui, "layer_fx.color", fill_to) {
                    *outline_to = Some(*fill_to);
                    changed = true;
                }
            });
        }
        Effect::Extrude {
            depth,
            light_angle,
            light_intensity,
            ..
        } => {
            changed |= offset(app, ui, depth);
            ui.horizontal(|ui| {
                changed |= degrees(ui, "docker.light_angle", light_angle);
                ui.label(tr("docker.light_intensity"));
                if ui
                    .add(NumField::new(light_intensity).range(0.0..=100.0))
                    .rich_tip(crate::ui::tips::pair(
                        "docker.light_intensity",
                        "docker.light_intensity_desc",
                    ))
                    .changed()
                {
                    changed = true;
                }
            });
        }
        Effect::InnerShadow {
            offset: off,
            color: c,
            opacity,
            blur,
            merge: m,
            spread,
        } => {
            changed |= offset(app, ui, off);
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.blur", blur, 100.0);
                changed |= percent(ui, "layer_fx.opacity", opacity);
            });
            ui.horizontal(|ui| {
                changed |= percent(ui, "layer_fx.spread", spread);
            });
            ui.horizontal(|ui| {
                changed |= color(ui, "layer_fx.color", c);
                changed |= merge(ui, m, "shadow");
            });
        }
        Effect::OuterShadow {
            offset: off,
            color: c,
            opacity,
            blur,
            merge: m,
        } => {
            changed |= offset(app, ui, off);
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.blur", blur, 100.0);
                changed |= percent(ui, "layer_fx.opacity", opacity);
            });
            ui.horizontal(|ui| {
                changed |= color(ui, "layer_fx.color", c);
                changed |= merge(ui, m, "shadow");
            });
        }
        Effect::Glow {
            color: c,
            opacity,
            blur,
            merge: m,
            ..
        } => {
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.radius", blur, 100.0);
                changed |= percent(ui, "layer_fx.opacity", opacity);
            });
            ui.horizontal(|ui| {
                changed |= color(ui, "layer_fx.color", c);
                changed |= merge(ui, m, "glow");
            });
        }
        Effect::ColorOverlay { color: c, opacity } => {
            ui.horizontal(|ui| {
                changed |= color(ui, "layer_fx.color", c);
                changed |= percent(ui, "layer_fx.opacity", opacity);
            });
        }
        Effect::GradientOverlay {
            from,
            to,
            angle,
            opacity,
        } => {
            ui.horizontal(|ui| {
                changed |= color(ui, "layer_fx.from", from);
                changed |= color(ui, "layer_fx.to", to);
            });
            ui.horizontal(|ui| {
                changed |= degrees(ui, "layer_fx.angle", angle);
                changed |= percent(ui, "layer_fx.opacity", opacity);
            });
        }
        Effect::GaussianBlur { radius } => {
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.radius", radius, 50.0);
            });
        }
        Effect::Combine { mode } => {
            use tracedraw_core::shaping::CombineMode;
            // Each mode as the Combine Shapes command of the same name.
            ui.horizontal_wrapped(|ui| {
                for (m, key, tip) in [
                    (CombineMode::Weld, "menu.object.weld", "layer_fx.combine_weld_desc"),
                    (
                        CombineMode::BackMinusFront,
                        "menu.object.back_minus_front",
                        "layer_fx.combine_cut_desc",
                    ),
                    (
                        CombineMode::Intersect,
                        "menu.object.intersect",
                        "layer_fx.combine_intersect_desc",
                    ),
                    (CombineMode::Exclude, "menu.object.exclude", "layer_fx.combine_exclude_desc"),
                ] {
                    changed |= ui
                        .selectable_value(mode, m, tr(key))
                        .rich_tip(tr(tip))
                        .changed();
                }
            });
        }
        Effect::Roughen {
            size,
            detail,
            smooth: round,
            seed,
        } => {
            use tracedraw_core::rough::{DETAIL_MAX, ROUGH_MAX};
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.rough_size", size, ROUGH_MAX);
                changed |= whole(ui, "layer_fx.rough_detail", detail, 1, DETAIL_MAX);
            });
            ui.horizontal(|ui| {
                changed |= ui
                    .checkbox(round, tr("layer_fx.rough_smooth"))
                    .rich_tip(tr("layer_fx.rough_smooth_desc"))
                    .changed();
                changed |= whole(ui, "layer_fx.seed", seed, 0, 9999);
            });
        }
        Effect::JitterNodes {
            across,
            down,
            handles,
            seed,
        } => {
            use tracedraw_core::rough::ROUGH_MAX;
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.jitter_across", across, ROUGH_MAX);
                changed |= length(app, ui, "layer_fx.jitter_down", down, ROUGH_MAX);
            });
            ui.horizontal(|ui| {
                changed |= ui
                    .checkbox(handles, tr("layer_fx.jitter_handles"))
                    .rich_tip(tr("layer_fx.jitter_handles_desc"))
                    .changed();
                changed |= whole(ui, "layer_fx.seed", seed, 0, 9999);
            });
        }
        Effect::Scribble {
            angle,
            spacing,
            width,
            loose,
            seed,
            curvy,
            overlap,
            spacing_var,
            width_var,
        } => {
            use tracedraw_core::rough::ROUGH_MAX;
            ui.horizontal(|ui| {
                changed |= degrees(ui, "layer_fx.angle", angle);
                changed |= length(app, ui, "layer_fx.scribble_spacing", spacing, ROUGH_MAX);
            });
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.width", width, 50.0);
                changed |= length(app, ui, "layer_fx.scribble_loose", loose, ROUGH_MAX);
            });
            ui.horizontal(|ui| {
                changed |= percent(ui, "layer_fx.scribble_curvy", curvy);
                changed |= signed_length(
                    app,
                    ui,
                    "layer_fx.scribble_overlap",
                    "layer_fx.scribble_overlap_desc",
                    overlap,
                );
            });
            ui.horizontal(|ui| {
                changed |= percent(ui, "layer_fx.scribble_spacing_var", spacing_var);
                changed |= percent(ui, "layer_fx.scribble_width_var", width_var);
            });
            ui.horizontal(|ui| {
                changed |= whole(ui, "layer_fx.seed", seed, 0, 9999);
            });
        }
        Effect::ShapeFrame { form, pad, radius } => {
            use tracedraw_core::rough::{FrameForm, ROUGH_MAX};
            ui.horizontal(|ui| {
                for (v, key) in [
                    (FrameForm::Rectangle, "layer_fx.frame_rectangle"),
                    (FrameForm::Rounded, "layer_fx.frame_rounded"),
                    (FrameForm::Ellipse, "layer_fx.frame_ellipse"),
                ] {
                    changed |= ui
                        .selectable_value(form, v, tr(key))
                        .rich_tip(tr(key))
                        .changed();
                }
            });
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.frame_pad", pad, ROUGH_MAX);
                if *form == FrameForm::Rounded {
                    changed |= length(app, ui, "layer_fx.radius", radius, ROUGH_MAX);
                }
            });
        }
        Effect::TrimMarks { length: len, gap, width } => {
            use tracedraw_core::rough::ROUGH_MAX;
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.marks_length", len, ROUGH_MAX);
                changed |= length(app, ui, "layer_fx.marks_gap", gap, ROUGH_MAX);
            });
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.width", width, 10.0);
            });
        }
        Effect::Feather { radius } => {
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.radius", radius, 100.0);
            });
        }
        Effect::OffsetPath {
            distance,
            smooth,
            join,
            merge,
        } => {
            use tracedraw_core::shaping::{Join, MAX_OFFSET};
            ui.horizontal(|ui| {
                // Out when positive, in when negative.
                ui.label(tr("layer_fx.offset"));
                let mut v = app.units.world_from_mm(*distance);
                let max = app.units.world_from_mm(MAX_OFFSET);
                if ui
                    .add(
                        NumField::new(&mut v)
                            .range(-max..=max)
                            .length_decimals(app.settings.precision.min(10) as usize)
                            .unit_mm(app.units.world_to_mm(1.0))
                            .suffix(format!(" {}", app.units.short()))
                            .width(70.0),
                    )
                    .rich_tip(tr("layer_fx.offset_desc"))
                    .changed()
                {
                    *distance = app.units.world_to_mm(v).clamp(-MAX_OFFSET, MAX_OFFSET);
                    changed = true;
                }
                changed |= length(app, ui, "layer_fx.smooth", smooth, MAX_OFFSET);
            });
            ui.horizontal(|ui| {
                ui.label(tr("layer_fx.joins"));
                for (v, key) in [
                    (Join::Miter, "layer_fx.join_miter"),
                    (Join::Round, "layer_fx.join_round"),
                    (Join::Bevel, "layer_fx.join_bevel"),
                ] {
                    changed |= ui
                        .selectable_value(join, v, tr(key))
                        .rich_tip(tr(key))
                        .changed();
                }
            });
            changed |= ui
                .checkbox(merge, tr("layer_fx.merge_group"))
                .rich_tip(tr("layer_fx.merge_group_desc"))
                .changed();
        }
        Effect::Transform { .. } => changed |= transform_settings(app, ui, e),
        Effect::Grid { .. } => changed |= grid_settings(app, ui, e),
        Effect::Halftone {
            pattern,
            mode,
            size,
            scale,
            softness,
            angle,
            center_x,
            center_y,
            clip,
            color: c,
        } => {
            ui.horizontal(|ui| {
                changed |= choice(
                    ui,
                    "layer_fx.halftone_pattern",
                    pattern,
                    &[
                        "layer_fx.halftone_round",
                        "layer_fx.halftone_square",
                        "layer_fx.halftone_diamond",
                        "layer_fx.halftone_line",
                    ],
                );
            });
            ui.horizontal(|ui| {
                changed |= choice(
                    ui,
                    "layer_fx.halftone_mode",
                    mode,
                    &[
                        "layer_fx.halftone_rgb",
                        "layer_fx.halftone_cmyk",
                        "layer_fx.halftone_mono",
                    ],
                );
                if *mode == 2 {
                    changed |= color(ui, "layer_fx.color", c);
                }
            });
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.halftone_size", size, 50.0);
                ui.label(tr("layer_fx.halftone_scale"));
                let mut pct = (*scale * 100.0).round();
                if ui
                    .add(NumField::new(&mut pct).range(0.0..=200.0).suffix(" %"))
                    .rich_tip(crate::ui::tips::keyed("layer_fx.halftone_scale"))
                    .changed()
                {
                    *scale = (pct / 100.0).clamp(0.0, 2.0);
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                changed |= percent(ui, "layer_fx.halftone_softness", softness);
                changed |= degrees(ui, "layer_fx.angle", angle);
            });
            ui.horizontal(|ui| {
                changed |= percent(ui, "layer_fx.halftone_center_x", center_x);
                changed |= percent(ui, "layer_fx.halftone_center_y", center_y);
            });
            changed |= ui
                .checkbox(clip, tr("layer_fx.halftone_clip"))
                .rich_tip(crate::ui::tips::keyed("layer_fx.halftone_clip"))
                .changed();
        }
        Effect::BlendIf { this, below } => {
            ui.label(tr("layer_fx.blend_if_this"))
                .rich_tip(tr("layer_fx.blend_if_desc"));
            changed |= range_bar(ui, "blend_if_this", this);
            ui.label(tr("layer_fx.blend_if_below"))
                .rich_tip(tr("layer_fx.blend_if_desc"));
            changed |= range_bar(ui, "blend_if_below", below);
            if ui
                .small_button(tr("layer_fx.blend_if_reset"))
                .rich_tip(tr("layer_fx.blend_if_reset"))
                .clicked()
            {
                *this = BLEND_IF_ALL;
                *below = BLEND_IF_ALL;
                changed = true;
            }
            ui.label(egui::RichText::new(tr("layer_fx.blend_if_hint")).color(Tokens::text_dim()));
        }
        Effect::Displace {
            map,
            horizontal,
            vertical,
            shade,
            strength,
        } => {
            ui.label(egui::RichText::new(tr("layer_fx.displace_desc")).color(Tokens::text_dim()));
            ui.horizontal(|ui| {
                changed |= length(app, ui, "layer_fx.displace_horizontal", horizontal, 50.0);
                changed |= length(app, ui, "layer_fx.displace_vertical", vertical, 50.0);
            });
            ui.horizontal(|ui| {
                ui.label(tr("layer_fx.displace_shade"))
                    .rich_tip(tr("layer_fx.displace_shade_tip"));
                let name = |m: SurfaceShade| {
                    tr(match m {
                        SurfaceShade::None => "layer_fx.displace_shade_none",
                        SurfaceShade::Multiply => "layer_fx.displace_shade_multiply",
                        SurfaceShade::Screen => "layer_fx.displace_shade_screen",
                        SurfaceShade::Both => "layer_fx.displace_shade_both",
                    })
                };
                crate::ui::keynav::ComboBox::from_id_salt("layer_fx_displace_shade")
                    .selected_text(name(*shade))
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for m in SurfaceShade::ALL {
                            if ui.selectable_label(*shade == m, name(m)).clicked() {
                                *shade = m;
                                changed = true;
                            }
                        }
                    })
                    .response
                    .rich_tip(tr("layer_fx.displace_shade_tip"));
                changed |= percent(ui, "layer_fx.displace_strength", strength);
            });
            ui.horizontal(|ui| {
                if ui
                    .small_button(tr("layer_fx.displace_resample"))
                    .rich_tip(tr("layer_fx.displace_resample_tip"))
                    .clicked()
                {
                    // Cleared, each object takes the photo beneath it now.
                    *map = None;
                    changed = true;
                }
                if map.is_none() {
                    ui.label(egui::RichText::new(tr("layer_fx.displace_no_photo")).color(Tokens::text_dim()));
                }
            });
        }
        _ => {}
    }
    changed
}

/// The scale lock of the Transform entry, kept between frames.
fn scale_lock_id() -> egui::Id {
    egui::Id::new("layer_fx_transform_lock")
}

/// A signed length field in the document's units, the value in mm.
fn signed_length(app: &App, ui: &mut Ui, label: &str, desc: &str, mm: &mut f64) -> bool {
    const MAX: f64 = 2000.0;
    ui.label(tr(label));
    let mut v = app.units.world_from_mm(*mm);
    let lim = app.units.world_from_mm(MAX);
    let r = ui
        .add(
            NumField::new(&mut v)
                .range(-lim..=lim)
                .length_decimals(app.settings.precision.min(10) as usize)
                .unit_mm(app.units.world_to_mm(1.0))
                .suffix(format!(" {}", app.units.short()))
                .width(70.0),
        )
        .rich_tip(crate::ui::tips::pair(label, desc));
    if r.changed() {
        *mm = app.units.world_to_mm(v).clamp(-MAX, MAX);
        true
    } else {
        false
    }
}

/// The Transform entry: scale, move, turn, how many copies, mirrors,
/// random steps and whether outlines scale too.
fn transform_settings(app: &App, ui: &mut Ui, e: &mut Effect) -> bool {
    let Effect::Transform {
        scale_x,
        scale_y,
        move_x,
        move_y,
        angle,
        copies,
        reflect_x,
        reflect_y,
        random,
        seed,
        scale_strokes,
        pivot_x,
        pivot_y,
    } = e
    else {
        return false;
    };
    let mut changed = false;
    let mut lock = ui
        .data(|d| d.get_temp::<bool>(scale_lock_id()))
        .unwrap_or(true);
    let pct = |ui: &mut Ui, v: &mut f64, label: &str| {
        ui.label(tr(label));
        ui.add(
            NumField::new(v)
                .range(-1000.0..=1000.0)
                .speed(1.0)
                .suffix(" %")
                .width(62.0),
        )
        .rich_tip(crate::ui::tips::pair(label, "layer_fx.scale_desc"))
        .changed()
    };
    ui.horizontal(|ui| {
        if pct(ui, scale_x, "layer_fx.scale_h") {
            if lock {
                *scale_y = *scale_x;
            }
            changed = true;
        }
        if pct(ui, scale_y, "layer_fx.scale_v") {
            if lock {
                *scale_x = *scale_y;
            }
            changed = true;
        }
        ui.checkbox(&mut lock, tr("docker.proportional"))
            .rich_tip(tr("layer_fx.scale_lock"));
    });
    ui.data_mut(|d| d.insert_temp(scale_lock_id(), lock));
    ui.horizontal(|ui| {
        changed |= signed_length(app, ui, "layer_fx.move_h", "layer_fx.move_desc", move_x);
        changed |= signed_length(app, ui, "layer_fx.move_v", "layer_fx.move_desc", move_y);
    });
    ui.horizontal(|ui| {
        ui.label(tr("layer_fx.angle"));
        changed |= ui
            .add(
                NumField::new(angle)
                    .range(-360.0..=360.0)
                    .speed(1.0)
                    .suffix("\u{00B0}")
                    .width(62.0),
            )
            .rich_tip(crate::ui::tips::pair(
                "layer_fx.angle",
                "layer_fx.turn_desc",
            ))
            .changed();
        ui.label(tr("layer_fx.copies"));
        changed |= ui
            .add(NumField::new(copies).range(0..=tracedraw_core::live::MAX_TRANSFORM_COPIES))
            .rich_tip(crate::ui::tips::pair(
                "layer_fx.copies",
                "layer_fx.copies_desc",
            ))
            .changed();
    });
    // The point the steps turn and scale about, from the box's centre:
    // copies round a centre of one's own (#3607).
    ui.horizontal(|ui| {
        changed |= signed_length(app, ui, "layer_fx.pivot_h", "layer_fx.pivot_desc", pivot_x);
        changed |= signed_length(app, ui, "layer_fx.pivot_v", "layer_fx.pivot_desc", pivot_y);
    });
    ui.horizontal(|ui| {
        changed |= ui
            .checkbox(reflect_x, tr("layer_fx.reflect_x"))
            .rich_tip(tr("layer_fx.reflect_x_desc"))
            .changed();
        changed |= ui
            .checkbox(reflect_y, tr("layer_fx.reflect_y"))
            .rich_tip(tr("layer_fx.reflect_y_desc"))
            .changed();
    });
    ui.horizontal(|ui| {
        changed |= ui
            .checkbox(random, tr("layer_fx.random"))
            .rich_tip(tr("layer_fx.random_desc"))
            .changed();
        if ui
            .add_enabled(*random, egui::Button::new(tr("layer_fx.reseed")))
            .rich_tip(tr("layer_fx.reseed_desc"))
            .clicked()
        {
            *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            changed = true;
        }
    });
    changed |= ui
        .checkbox(scale_strokes, tr("layer_fx.scale_strokes"))
        .rich_tip(tr("layer_fx.scale_strokes_desc"))
        .changed();
    changed
}

/// The Grid entry: columns and rows, the gaps between cells and the
/// shift of every other row and column.
fn grid_settings(app: &App, ui: &mut Ui, e: &mut Effect) -> bool {
    let Effect::Grid {
        columns,
        rows,
        gap_x,
        gap_y,
        shift_x,
        shift_y,
    } = e
    else {
        return false;
    };
    let mut changed = false;
    let max = tracedraw_core::live::MAX_GRID_SIDE;
    ui.horizontal(|ui| {
        ui.label(tr("layer_fx.columns"));
        changed |= ui
            .add(NumField::new(columns).range(1..=max))
            .rich_tip(crate::ui::tips::pair(
                "layer_fx.columns",
                "layer_fx.grid_size_desc",
            ))
            .changed();
        ui.label(tr("layer_fx.rows"));
        changed |= ui
            .add(NumField::new(rows).range(1..=max))
            .rich_tip(crate::ui::tips::pair(
                "layer_fx.rows",
                "layer_fx.grid_size_desc",
            ))
            .changed();
    });
    ui.horizontal(|ui| {
        changed |= signed_length(app, ui, "layer_fx.gap_h", "layer_fx.gap_desc", gap_x);
        changed |= signed_length(app, ui, "layer_fx.gap_v", "layer_fx.gap_desc", gap_y);
    });
    ui.horizontal(|ui| {
        changed |= signed_length(
            app,
            ui,
            "layer_fx.shift_rows",
            "layer_fx.shift_rows_desc",
            shift_x,
        );
        changed |= signed_length(
            app,
            ui,
            "layer_fx.shift_cols",
            "layer_fx.shift_cols_desc",
            shift_y,
        );
    });
    ui.label(tr("layer_fx.grid_hint"));
    changed
}

/// Which of a Blend If range's four markers a drag moves, and whether
/// its partner of the same end goes with it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RangeDrag {
    marker: usize,
    together: bool,
    /// Where the pointer was when the drag began, 0..1, until the first
    /// move tells which half of a joined end Alt splits off.
    start: Option<f64>,
}

/// The marker of `range` a press at `at` (0..1) takes: the nearest; of
/// two in one place, the dark end's inner one when the press is right of
/// it and its outer one left of it (the light end the other way round).
fn pick_marker(range: &[f64; 4], at: f64) -> usize {
    let mut best = 0;
    for i in 1..4 {
        if (range[i] - at).abs() < (range[best] - at).abs() {
            best = i;
        }
    }
    best
}

/// The marker a drag of a joined end with Alt moves: the one on the
/// side the pointer goes.
fn split_marker(marker: usize, right: bool) -> usize {
    match (marker < 2, right) {
        (true, true) => 1,
        (true, false) => 0,
        (false, true) => 3,
        (false, false) => 2,
    }
}

/// Move `marker` of `range` to `v`, keeping the four in order; with
/// `together`, its partner of the same end moves by as much.
fn move_marker(range: &mut [f64; 4], marker: usize, together: bool, v: f64) {
    let v = v.clamp(0.0, 1.0);
    let marker = marker.min(3);
    let partner = if marker < 2 { 1 - marker } else { 5 - marker };
    let d = v - range[marker];
    range[marker] = v;
    if together {
        range[partner] = (range[partner] + d).clamp(0.0, 1.0);
    }
    // Keep the order: the moved marker pushes the ones it passes.
    for i in (0..marker).rev() {
        if range[i] > range[i + 1] {
            range[i] = range[i + 1];
        }
    }
    for i in marker + 1..4 {
        if range[i] < range[i - 1] {
            range[i] = range[i - 1];
        }
    }
}

/// A lightness range: a strip from black to white with the range's four
/// markers under it. A marker drags its end; the two markers of an end
/// in one place move together, and Alt splits them apart so the end
/// fades in over the gap. The values show as 0 to 255.
fn range_bar(ui: &mut Ui, salt: &str, range: &mut [f64; 4]) -> bool {
    let id = ui.id().with(("blend_if_bar", salt));
    let width = ui.available_width().clamp(120.0, 240.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::click_and_drag());
    let strip = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 0.0), egui::vec2(width - 12.0, 10.0));
    let to_x = |v: f64| strip.left() + strip.width() * v as f32;
    let to_v = |x: f32| f64::from(((x - strip.left()) / strip.width()).clamp(0.0, 1.0));
    let mut changed = false;
    let mut drag = ui.data(|d| d.get_temp::<RangeDrag>(id));
    if let Some(p) = response.interact_pointer_pos() {
        let at = to_v(p.x);
        if response.drag_started() || response.clicked() || drag.is_none() {
            let marker = pick_marker(range, at);
            let partner = if marker < 2 { 1 - marker } else { 5 - marker };
            let joined = (range[marker] - range[partner]).abs() < 1e-9;
            let alt = ui.input(|i| i.modifiers.alt);
            drag = Some(RangeDrag {
                marker,
                together: joined && !alt,
                start: (joined && alt).then_some(at),
            });
        }
        if let Some(d) = drag.as_mut() {
            if let Some(s) = d.start {
                if (at - s).abs() > 1e-3 {
                    d.marker = split_marker(d.marker, at > s);
                    d.start = None;
                }
            }
            if d.start.is_none() && (response.dragged() || response.clicked()) {
                let before = *range;
                move_marker(range, d.marker, d.together, at);
                changed |= *range != before;
            }
        }
    }
    if response.drag_stopped() || !response.is_pointer_button_down_on() {
        drag = None;
    }
    ui.data_mut(|d| match drag {
        Some(v) => {
            d.insert_temp(id, v);
        }
        None => d.remove::<RangeDrag>(id),
    });
    let painter = ui.painter_at(rect.expand(2.0));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(strip.left_top(), egui::Color32::BLACK);
    mesh.colored_vertex(strip.right_top(), egui::Color32::WHITE);
    mesh.colored_vertex(strip.right_bottom(), egui::Color32::WHITE);
    mesh.colored_vertex(strip.left_bottom(), egui::Color32::BLACK);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
    painter.rect_stroke(
        strip,
        0.0,
        egui::Stroke::new(1.0, Tokens::text_dim()),
        egui::StrokeKind::Outside,
    );
    for (i, v) in range.iter().enumerate() {
        let x = to_x(*v);
        let top = strip.bottom() + 1.0;
        // The dark end's markers dark, the light end's light.
        let fill = if i < 2 {
            egui::Color32::from_gray(40)
        } else {
            egui::Color32::from_gray(235)
        };
        painter.add(egui::epaint::PathShape::convex_polygon(
            vec![
                egui::pos2(x, top),
                egui::pos2(x + 5.0, top + 8.0),
                egui::pos2(x - 5.0, top + 8.0),
            ],
            fill,
            egui::Stroke::new(1.0, Tokens::text_dim()),
        ));
    }
    let byte = |v: f64| (v * 255.0).round() as i32;
    let end = |a: f64, b: f64| {
        if byte(a) == byte(b) {
            byte(a).to_string()
        } else {
            format!("{} / {}", byte(a), byte(b))
        }
    };
    response.rich_tip(tr("layer_fx.blend_if_desc"));
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(end(range[0], range[1])).small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(end(range[2], range[3])).small());
        });
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;
    use tracedraw_core::ShapeKind;

    /// Displace (#3595): ticked on an object over a photo it takes the
    /// photo beneath as its map; setting values keeps each object's own
    /// map; Use Photo Beneath and a move with the Pick tool sample it
    /// again where the object is; the panel draws its controls.
    #[test]
    fn displace_takes_the_photo_beneath_and_samples_it_again_after_a_move() {
        let mut app = App::headless();
        let (w, h) = (8u32, 4u32);
        let mut px = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 0 } else { 255 };
                px.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let png = tracedraw_core::bitmap_warp::encode_rgba(w, h, &px).expect("png");
        app.new_shape(ShapeKind::Bitmap {
            rect: Rect::new(0.0, 0.0, 40.0, 20.0),
            width_px: w,
            height_px: h,
            png: png.into(),
            fx: None,
        })
        .expect("photo");
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(10.0, 5.0, 30.0, 15.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.select(vec![id]);
        let map = |app: &App| {
            app.doc()
                .find_shape(id)
                .expect("shape")
                .effects
                .iter()
                .find_map(|e| match e {
                    Effect::Displace { map, .. } => Some(map.clone()),
                    _ => None,
                })
                .expect("displace")
        };
        app.set_layer_effect_on(Kind::Displace, true, &Kind::Displace.default_effect());
        let first = map(&app).expect("map");
        assert!((first.mean - 0.5).abs() < 0.1, "{}", first.mean);
        // A value set from another object's effect keeps this one's map.
        let mut other = Kind::Displace.default_effect();
        if let Effect::Displace { map, vertical, .. } = &mut other {
            *map = Some(tracedraw_core::live::SurfaceMap {
                rect: Rect::new(0.0, 0.0, 1.0, 1.0),
                cols: 1,
                rows: 1,
                light: vec![0],
                mean: 0.0,
                to_local: [1.0, 0.0, 0.0, 1.0],
            });
            *vertical = 4.0;
        }
        app.set_layer_effect(Kind::Displace, &other);
        assert_eq!(map(&app).as_ref(), Some(&first));
        // Moved onto the white half: sampled again, in the move's step.
        app.run(Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::translate((15.0, 0.0)),
        });
        app.displace_after_move();
        assert!(map(&app).expect("map").mean > 0.6);
        app.undo();
        assert_eq!(map(&app).as_ref(), Some(&first));
        // Use Photo Beneath clears the map and each object samples anew.
        let mut again = other.clone();
        if let Effect::Displace { map, .. } = &mut again {
            *map = None;
        }
        app.set_layer_effect(Kind::Displace, &again);
        // Read anew, over the box grown by the new 4 mm.
        let anew = map(&app).expect("map");
        assert!((anew.rect.x0 - 6.0).abs() < 1e-9, "{:?}", anew.rect);
        assert!((anew.mean - first.mean).abs() < 1e-6);
        let ctx = crate::theme::ui_context();
        ctx.data_mut(|d| d.insert_temp(highlighted_id(), Kind::ALL.len() - 1));
        let mut out = ctx.run_ui(Default::default(), |ui| layer_effects(&mut app, ui));
        out.textures_delta.clear();
    }

    /// Ticking an entry adds its effect; clearing it keeps the values,
    /// switched off, and ticking it again brings them back; a value set
    /// goes to the effect; × takes it away; fill opacity comes and goes;
    /// each change is one undo step (#1141).
    #[test]
    fn entries_switch_set_and_remove_their_effects() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.select(vec![id]);
        let shape = |app: &App| app.doc().find_shape(id).expect("shape").clone();
        for kind in Kind::ALL {
            assert!(kind.matches(&kind.default_effect()), "{kind:?}");
            for other in Kind::ALL.into_iter().filter(|k| *k != kind) {
                assert!(!other.matches(&kind.default_effect()), "{other:?} {kind:?}");
            }
        }
        let glow = Kind::OuterGlow.default_effect();
        app.set_layer_effect_on(Kind::OuterGlow, true, &glow);
        assert_eq!(shape(&app).effects, vec![glow.clone()]);
        let brighter = Effect::Glow {
            inner: false,
            color: Color::WHITE,
            opacity: 1.0,
            blur: 4.0,
            merge: MergeMode::Normal,
        };
        app.set_layer_effect(Kind::OuterGlow, &brighter);
        assert_eq!(shape(&app).effects, vec![brighter.clone()]);
        app.set_layer_effect_on(Kind::OuterGlow, false, &glow);
        assert!(shape(&app).effects.is_empty());
        assert_eq!(shape(&app).effect_stack(), vec![(brighter.clone(), false)]);
        app.set_layer_effect_on(Kind::OuterGlow, true, &glow);
        assert_eq!(shape(&app).effects, vec![brighter.clone()]);
        app.set_fill_opacity(0.4);
        assert!((fill_opacity(&shape(&app)) - 0.4).abs() < 1e-9);
        app.set_fill_opacity(1.0);
        assert_eq!(fill_opacity(&shape(&app)), 1.0);
        assert_eq!(shape(&app).effects, vec![brighter.clone()]);
        app.remove_layer_effect(Kind::OuterGlow);
        assert!(shape(&app).effect_stack().is_empty());
        app.undo();
        assert_eq!(shape(&app).effects, vec![brighter]);
        // The offsets' distance and direction.
        let (d, a) = polar(Vec2::new(0.0, -2.0));
        assert!((d - 2.0).abs() < 1e-9 && (a - 270.0).abs() < 1e-9);
        let v = from_polar(d, a);
        assert!(v.x.abs() < 1e-9 && (v.y + 2.0).abs() < 1e-9);
        // Blend If's markers: a joined end moves together, a split one
        // alone, and the order holds.
        let mut r = BLEND_IF_ALL;
        move_marker(&mut r, 0, true, 0.2);
        assert_eq!(r, [0.2, 0.2, 1.0, 1.0]);
        move_marker(&mut r, split_marker(0, true), false, 0.5);
        assert_eq!(r, [0.2, 0.5, 1.0, 1.0]);
        move_marker(&mut r, 3, true, 0.3);
        assert_eq!(r, [0.2, 0.3, 0.3, 0.3]);
        assert_eq!(pick_marker(&[0.0, 0.25, 0.9, 1.0], 0.3), 1);
        assert_eq!(split_marker(2, false), 2);
        assert_eq!(split_marker(3, true), 3);
        // The panel draws with each entry highlighted, and with nothing
        // selected.
        let ctx = crate::theme::ui_context();
        for i in 0..Kind::ALL.len() {
            ctx.data_mut(|d| d.insert_temp(highlighted_id(), i));
            let mut out = ctx.run_ui(Default::default(), |ui| layer_effects(&mut app, ui));
            out.textures_delta.clear();
        }
        app.select(Vec::new());
        let mut out = ctx.run_ui(Default::default(), |ui| layer_effects(&mut app, ui));
        out.textures_delta.clear();
    }
}

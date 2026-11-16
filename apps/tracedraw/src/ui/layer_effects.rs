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
use tracedraw_core::live::{Effect, MergeMode};
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
}

impl Kind {
    pub const ALL: [Kind; 10] = [
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
        for s in self.selected_shapes() {
            let before = s.effect_stack();
            let mut stack = before.clone();
            edit(&mut stack);
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
        }
        | Effect::OuterShadow {
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
        _ => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;
    use tracedraw_core::ShapeKind;

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

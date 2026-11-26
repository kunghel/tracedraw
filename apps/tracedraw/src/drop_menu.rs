//! Dragging objects with the right mouse button (Pick tools): a blue
//! outline of the selection follows the pointer while the objects stay
//! put; on release a menu asks what to do there: Move Here, Copy Here,
//! Copy Fill Here, Copy Outline Here, Copy All Properties (onto the object
//! under the pointer), Place Inside (that object becomes the selection's
//! clip frame), Fit Text to Path, or Cancel. Ctrl keeps the drag
//! horizontal or vertical;
//! Esc cancels it. Let go over another object with Shift held, that
//! object takes the outline of the selection without the menu, with Alt
//! the fill and with Ctrl fill and outline. A move with
//! the left button let go over another object with Alt held copies the
//! fill there without a menu, with Alt+Shift the outline
//! (`App::copy_style_onto`, discussion #2066).

use crate::app::App;
use egui::{Painter, Response, Ui};
use tracedraw_core::{
    geometry::{Affine, Point, Vec2},
    Command, ShapeId, ShapeKind,
};

/// A right-button drag in progress (page space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RightDrag {
    pub start: Point,
    pub current: Point,
}

impl RightDrag {
    /// How far the selection would go; `constrain` keeps the larger of
    /// the two directions only.
    pub fn offset(&self, constrain: bool) -> Vec2 {
        let d = self.current - self.start;
        if constrain {
            if d.x.abs() >= d.y.abs() {
                Vec2::new(d.x, 0.0)
            } else {
                Vec2::new(0.0, d.y)
            }
        } else {
            d
        }
    }

    fn menu_offset(&self, zoom: f32, constrain: bool) -> Option<Vec2> {
        let offset = self.offset(constrain);
        (offset.hypot() * f64::from(zoom) >= 2.0).then_some(offset)
    }
}

/// The menu shown where a right-button drag ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropMenu {
    /// Where the menu opens (screen).
    pub screen: egui::Pos2,
    pub offset: Vec2,
    /// The object under the pointer that is not being dragged.
    pub target: Option<ShapeId>,
    /// The release point in page space when this drop can fit text to a path.
    fit_text_at: Option<Point>,
}

/// What the drop menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropAction {
    Move,
    Copy,
    CopyFill,
    CopyOutline,
    CopyProperties,
    /// The first selected object's Layer Effects go onto the object
    /// under the drop (#4243).
    CopyEffects,
    PlaceInside,
    FitTextToPath,
    Cancel,
}

impl DropAction {
    pub const ALL: [DropAction; 9] = [
        DropAction::Move,
        DropAction::Copy,
        DropAction::CopyFill,
        DropAction::CopyOutline,
        DropAction::CopyProperties,
        DropAction::CopyEffects,
        DropAction::PlaceInside,
        DropAction::FitTextToPath,
        DropAction::Cancel,
    ];

    pub fn key(self) -> &'static str {
        match self {
            DropAction::Move => "drop.move_here",
            DropAction::Copy => "drop.copy_here",
            DropAction::CopyFill => "drop.copy_fill_here",
            DropAction::CopyOutline => "drop.copy_outline_here",
            DropAction::CopyProperties => "drop.copy_all_properties",
            DropAction::CopyEffects => "drop.copy_effects_here",
            DropAction::PlaceInside => "drop.place_inside",
            DropAction::FitTextToPath => "menu.text.fit_text_to_path",
            DropAction::Cancel => "drop.cancel",
        }
    }

    /// What a right-button drag let go over another object does without
    /// the menu, by the keys held (discussion #2066): Shift copies the
    /// outline, Alt the fill, Ctrl (Cmd on a Mac) fill and outline.
    pub fn for_right_drop(mods: egui::Modifiers) -> Option<DropAction> {
        if mods.alt {
            Some(DropAction::CopyFill)
        } else if mods.shift {
            Some(DropAction::CopyOutline)
        } else if mods.command {
            Some(DropAction::CopyProperties)
        } else {
            None
        }
    }

    /// Whether the action needs an object under the pointer.
    pub fn needs_target(self) -> bool {
        matches!(
            self,
            DropAction::CopyFill
                | DropAction::CopyOutline
                | DropAction::CopyProperties
                | DropAction::CopyEffects
                | DropAction::PlaceInside
                | DropAction::FitTextToPath
        )
    }
}

impl App {
    /// Right-button drags with the Pick tools; true when the input was
    /// used here.
    pub fn right_drag_input(&mut self, response: &Response, p: Point) -> bool {
        if response.drag_started_by(egui::PointerButton::Secondary) {
            // A right drag from a selected object, also one covered by
            // another object there, moves the selection as it is (#1684).
            if !self.selection_under(p) {
                let Some(id) = self.hit_test(p) else {
                    return false;
                };
                if !self.selection.contains(&id) {
                    self.select(vec![id]);
                }
            }
            self.right_drag = Some(RightDrag {
                start: p,
                current: p,
            });
            self.right_drag_style = None;
            self.drop_menu = None;
            return true;
        }
        let Some(mut drag) = self.right_drag else {
            return false;
        };
        if response.ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.right_drag = None;
            self.right_drag_style = None;
            return true;
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            drag.current = p;
            self.right_drag = Some(drag);
            return true;
        }
        if response.drag_stopped() || !response.ctx.input(|i| i.pointer.secondary_down()) {
            drag.current = p;
            let mods = response.ctx.input(|i| i.modifiers);
            self.right_drag_released(drag, mods);
            return true;
        }
        true
    }

    /// The object under `p` that is not being dragged.
    fn right_drop_target(&self, p: Point) -> Option<ShapeId> {
        let selection = &self.selection;
        self.hit_test_where(p, |id| selection.contains(&id))
    }

    /// A single editable text dropped on a different vector outline.
    fn text_drop_pair(
        &self,
        target: impl FnOnce() -> Option<ShapeId>,
    ) -> Option<(ShapeId, ShapeId)> {
        if self.selection.len() != 1 {
            return None;
        }
        let selected = self.selected_rc();
        let [text] = selected.as_slice() else {
            return None;
        };
        if !matches!(text.kind, ShapeKind::Text { .. }) || text.locked {
            return None;
        }
        let path = self.doc().find_shape(target()?)?;
        matches!(
            path.kind,
            ShapeKind::Path { .. }
                | ShapeKind::Ellipse { .. }
                | ShapeKind::Rect { .. }
                | ShapeKind::Polygon { .. }
                | ShapeKind::ClipFrame { .. }
        )
        .then_some((text.id, path.id))
    }

    /// The path that would offer Fit Text to Path on release. Check the
    /// source before picking a target so other right drags stay as cheap.
    pub(crate) fn right_drag_text_target(&self, mods: egui::Modifiers) -> Option<ShapeId> {
        if self.right_drag_style.is_some() || DropAction::for_right_drop(mods).is_some() {
            return None;
        }
        let drag = self.right_drag?;
        drag.menu_offset(self.view.zoom, mods.command)?;
        self.text_drop_pair(|| self.right_drop_target(drag.current))
            .map(|(_, path)| path)
    }

    /// What a right-button drag that ended with the keys `mods` held does
    /// there, and the object it does it to (shown while dragging).
    pub fn right_drop_style(
        &self,
        p: Point,
        mods: egui::Modifiers,
    ) -> Option<(DropAction, ShapeId)> {
        let action = DropAction::for_right_drop(mods)?;
        Some((action, self.right_drop_target(p)?))
    }

    /// A right-button drag let go at `drag.current` with the keys `mods`
    /// held: over another object with Shift, Alt or Ctrl that object takes
    /// the outline, the fill or both at once (discussion #2066); otherwise
    /// the drop menu opens there.
    pub fn right_drag_released(&mut self, drag: RightDrag, mods: egui::Modifiers) {
        self.right_drag = None;
        let p = drag.current;
        // Carried onto another page by its tab (#2704): the object under
        // the pointer there takes the fill (the outline with Shift, both
        // with Ctrl); no menu, as the objects dragged are not on this page.
        if let Some(src) = self.right_drag_style.take() {
            let action = DropAction::for_right_drop(mods).unwrap_or(DropAction::CopyFill);
            if let Some(target) = self.hit_test(p) {
                self.copy_style_from(&src, target, action);
            }
            return;
        }
        if let Some((action, target)) = self.right_drop_style(p, mods) {
            self.copy_style_onto(target, action);
            return;
        }
        if let Some(offset) = drag.menu_offset(self.view.zoom, mods.command) {
            let target = self.right_drop_target(p);
            self.drop_menu = Some(DropMenu {
                screen: self.view.to_screen(p),
                offset,
                target,
                fit_text_at: self.text_drop_pair(|| target).map(|_| p),
            });
        }
    }

    /// The moved selection's outline, or the drop target with its action.
    pub fn draw_right_drag(
        &self,
        painter: &Painter,
        mods: egui::Modifiers,
        text_target: Option<ShapeId>,
    ) {
        let Some(drag) = self.right_drag else {
            return;
        };
        if let Some(target) = text_target {
            crate::canvas::draw_drop_target(self, painter, DropAction::FitTextToPath, target);
            return;
        }
        if self.right_drag_style.is_some() {
            let action = DropAction::for_right_drop(mods).unwrap_or(DropAction::CopyFill);
            if let Some(target) = self.hit_test(drag.current) {
                crate::canvas::draw_drop_target(self, painter, action, target);
            }
            return;
        }
        if let Some((action, target)) = self.right_drop_style(drag.current, mods) {
            crate::canvas::draw_drop_target(self, painter, action, target);
            return;
        }
        let t = Affine::translate(drag.offset(mods.command));
        crate::canvas::draw_transform_outline(self, painter, t);
    }

    /// Carry out a drop menu choice.
    pub fn drop_action(&mut self, menu: DropMenu, action: DropAction) {
        if self.selection.is_empty() {
            return;
        }
        match action {
            DropAction::Move => self.transform_selection(Affine::translate(menu.offset)),
            DropAction::Copy => self.duplicate_by(menu.offset),
            DropAction::CopyFill | DropAction::CopyOutline | DropAction::CopyProperties => {
                if let Some(target) = menu.target {
                    self.copy_style_onto(target, action);
                }
            }
            DropAction::CopyEffects => {
                if let Some(target) = menu.target {
                    self.copy_layer_effects_onto(target);
                }
            }
            DropAction::PlaceInside => {
                let Some(target) = menu.target else {
                    return;
                };
                let contents: Vec<ShapeId> = self
                    .selection
                    .iter()
                    .copied()
                    .filter(|id| *id != target)
                    .collect();
                if !contents.is_empty() {
                    self.place_inside(contents, target);
                    self.select(vec![target]);
                }
            }
            DropAction::FitTextToPath => {
                let (Some(at), Some((source, target))) =
                    (menu.fit_text_at, self.text_drop_pair(|| menu.target))
                else {
                    return;
                };
                let doc = self.doc();
                let (Some(text), Some(path)) =
                    (doc.shape_on_page(source), doc.shape_on_page(target))
                else {
                    return;
                };
                let Some((mut kind, transform)) = crate::text_path::fit(&text, &path, Some(at)) else {
                    return;
                };
                // The drop locates the text along the path, not away from it.
                if let ShapeKind::Text {
                    on_path: Some(on_path),
                    ..
                } = &mut kind
                {
                    on_path.set_distance(0.0);
                    on_path.source = path.uid.map(|uid| tracedraw_core::text_path::PathSource {
                        shape: target,
                        uid,
                    });
                }
                let mut commands = vec![Command::SetShapeKind {
                    shape: source,
                    kind,
                }];
                if transform != text.transform {
                    commands.push(Command::TransformShapes {
                        shapes: vec![source],
                        transform: transform * text.transform.inverse(),
                    });
                }
                if let Err(error) = self.engine.run_batch("Fit Text to Path", &commands) {
                    self.status = error.to_string();
                }
            }
            DropAction::Cancel => {}
        }
    }

    /// Copy Fill Here, Copy Outline Here or Copy All Properties: object
    /// `target` takes the fill, the outline or both and the transparency
    /// of the first selected object, one undo step. The right-button drop
    /// menu and a left-button move let go with Alt (Alt+Shift) held do
    /// this (discussion #2066). Other actions do nothing here.
    pub fn copy_style_onto(&mut self, target: ShapeId, action: DropAction) {
        let Some(first) = self.selected_shapes().first().cloned() else {
            return;
        };
        self.copy_style_from(&first, target, action);
    }

    /// `target` takes the fill, the outline or both (with the opacity)
    /// of `first`, by `action`, as one undo step.
    pub fn copy_style_from(
        &mut self,
        first: &tracedraw_core::document::Shape,
        target: ShapeId,
        action: DropAction,
    ) {
        if first.id == target
            || !matches!(
                action,
                DropAction::CopyFill | DropAction::CopyOutline | DropAction::CopyProperties
            )
        {
            return;
        }
        let Some(t) = self.doc().find_shape(target).cloned() else {
            return;
        };
        // Read from the object that shows the properties (a group's
        // frontmost object), as Copy Properties From does; a mesh fill,
        // whose nodes lie in its object's own space, is fitted to the
        // target's box, not left where the source is (#2896).
        let src = crate::copy_props::properties_source(first);
        let targets = [t];
        let mut cmds = Vec::new();
        if action != DropAction::CopyOutline {
            cmds.extend(crate::copy_props::fill_commands(
                &crate::copy_props::shown_fill(&src),
                &src,
                &targets,
            ));
        }
        if action != DropAction::CopyFill {
            cmds.extend(crate::copy_props::clear_character_outlines(&targets));
            cmds.push(Command::SetStroke {
                shapes: vec![target],
                stroke: crate::copy_props::shown_stroke(&src),
            });
        }
        if action == DropAction::CopyProperties {
            cmds.push(Command::SetOpacity {
                shapes: vec![target],
                opacity: first.opacity,
            });
        }
        let label = match action {
            DropAction::CopyFill => "Copy Fill",
            DropAction::CopyOutline => "Copy Outline",
            _ => "Copy Properties",
        };
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
        }
    }

    /// Copy Effects Here (#4243): `target` takes the Layer Effects of the
    /// first selected object, one undo step.
    pub fn copy_layer_effects_onto(&mut self, target: ShapeId) {
        let Some(first) = self.selected_shapes().first().cloned() else {
            return;
        };
        let Some(t) = self.doc().find_shape(target).cloned() else {
            return;
        };
        match crate::copy_effect::layer_effects_commands(&first, &[t]) {
            None => self.status = crate::i18n::tr("status.no_effect_to_copy"),
            Some(cmds) if !cmds.is_empty() => {
                if let Err(e) = self.engine.run_batch("Copy Effect", &cmds) {
                    self.status = e.to_string();
                }
            }
            Some(_) => {}
        }
    }

    /// Copies of the selection moved by `offset`, selected.
    pub fn duplicate_by(&mut self, offset: Vec2) {
        // The copy goes where the drag ended, also when the dragged
        // objects are the copies of a Duplicate still selected: the step
        // a repeated Duplicate takes put it far from the drop (#2640).
        self.last_duplicate.clear();
        let saved = self.duplicate_offset;
        self.duplicate_offset = offset;
        self.duplicate();
        self.duplicate_offset = saved;
    }
}

/// The drop menu, drawn like the context menu where the drag ended.
pub fn drop_menu_ui(app: &mut App, ui: &mut Ui) {
    let Some(menu) = app.drop_menu else {
        return;
    };
    let id = egui::Id::new("canvas_drop_menu");
    let ctx = ui.ctx().clone();
    let pos_id = id.with("pos");
    if !crate::ui::context_menus::has_visible_items(&app.settings.context_menus, "drop") {
        egui::Popup::close_id(&ctx, id);
        app.drop_menu = None;
        ctx.data_mut(|data| data.remove::<egui::Pos2>(pos_id));
        return;
    }
    if ctx.data(|d| d.get_temp::<egui::Pos2>(pos_id)) != Some(menu.screen) {
        ctx.data_mut(|d| d.insert_temp(pos_id, menu.screen));
        egui::Popup::open_id(&ctx, id);
    }
    let mut chosen: Option<DropAction> = None;
    let shown = egui::Popup::new(
        id,
        ctx.clone(),
        egui::PopupAnchor::Position(menu.screen),
        ui.layer_id(),
    )
    .kind(egui::PopupKind::Menu)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .style(crate::ui::menus::menu_popup_style)
    .open_memory(None)
    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
    .show(|ui| {
        let _scope = crate::ui::context_menus::Scope::new(&app.settings.context_menus, "drop");
        ui.set_min_width(190.0);
        crate::ui::menus::body(ui, |ui| {
            let keys = DropAction::ALL.map(DropAction::key);
            for key in crate::ui::context_menu_filter::ordered_keys(&keys)
                .iter()
                .copied()
            {
                let Some(action) = DropAction::ALL
                    .into_iter()
                    .find(|action| action.key() == key)
                else {
                    continue;
                };
                if action == DropAction::FitTextToPath && menu.fit_text_at.is_none() {
                    continue;
                }
                if matches!(action, DropAction::CopyFill | DropAction::Cancel) {
                    crate::ui::menus::sep(ui);
                }
                let enabled = !action.needs_target() || menu.target.is_some();
                if crate::ui::menus::item(ui, action.key(), "", enabled) {
                    chosen = Some(action);
                }
            }
        })
    });
    if let Some(action) = chosen {
        app.drop_action(menu, action);
    }
    if chosen.is_some() || shown.is_none() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        egui::Popup::close_id(&ctx, id);
        app.drop_menu = None;
        ctx.data_mut(|d| d.remove::<egui::Pos2>(pos_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{Color, Fill, Rect, ShapeKind, Stroke};

    fn two_boxes(app: &mut App) -> (ShapeId, ShapeId) {
        let a = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("a");
        let b = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(50.0, 0.0, 70.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("b");
        app.run(Command::SetFill {
            shapes: vec![a],
            fill: Fill::Solid(Color::rgb8(255, 0, 0)),
        });
        app.run(Command::SetStroke {
            shapes: vec![a],
            stroke: Some(Stroke::new(Color::rgb8(0, 0, 255), 1.5)),
        });
        (a, b)
    }

    fn bounds(app: &App, id: ShapeId) -> tracedraw_core::Rect {
        app.doc()
            .shape(id)
            .map(|(_, s)| s.bounds())
            .unwrap_or_default()
    }

    #[test]
    fn a_ctrl_drag_keeps_to_one_direction() {
        let d = RightDrag {
            start: Point::new(0.0, 0.0),
            current: Point::new(10.0, 3.0),
        };
        assert_eq!(d.offset(false), Vec2::new(10.0, 3.0));
        assert_eq!(d.offset(true), Vec2::new(10.0, 0.0));
    }

    #[test]
    fn move_here_and_copy_here() {
        let mut app = App::headless();
        let (a, _) = two_boxes(&mut app);
        app.select(vec![a]);
        let menu = DropMenu {
            screen: egui::Pos2::ZERO,
            offset: Vec2::new(20.0, 5.0),
            target: None,
            fit_text_at: None,
        };
        app.drop_action(menu, DropAction::Copy);
        // A copy, selected, where the drop was; the original stays.
        assert_eq!(app.selection.len(), 1);
        let copy = app.selection[0];
        assert_ne!(copy, a);
        assert!((bounds(&app, copy).x0 - 20.0).abs() < 1e-6);
        assert!((bounds(&app, a).x0).abs() < 1e-6);
        app.select(vec![a]);
        app.drop_action(menu, DropAction::Move);
        assert!((bounds(&app, a).x0 - 20.0).abs() < 1e-6);
        // Cancel changes nothing.
        let before = app.doc().clone();
        app.drop_action(menu, DropAction::Cancel);
        assert_eq!(*app.doc(), before);
    }

    #[test]
    fn copy_here_from_copies_still_selected_lands_where_the_drag_ended() {
        // #2640: a right drag copied from a copy still selected (after
        // Duplicate or an earlier Copy Here) landed by the earlier step,
        // not where the drag ended.
        let mut app = App::headless();
        let (a, _) = two_boxes(&mut app);
        app.select(vec![a]);
        let menu = |dx: f64, dy: f64| DropMenu {
            screen: egui::Pos2::ZERO,
            offset: Vec2::new(dx, dy),
            target: None,
            fit_text_at: None,
        };
        app.drop_action(menu(80.0, 0.0), DropAction::Copy);
        let first = app.selection[0];
        assert!((bounds(&app, first).x0 - 80.0).abs() < 1e-6);
        app.drop_action(menu(5.0, 30.0), DropAction::Copy);
        let second = app.selection[0];
        assert_ne!(second, first);
        let r = bounds(&app, second);
        assert!((r.x0 - 85.0).abs() < 1e-6, "copy at {r:?}");
        assert!((r.y0 - (bounds(&app, first).y0 + 30.0)).abs() < 1e-6);
        // The same after Edit > Duplicate.
        app.select(vec![a]);
        app.duplicate();
        let dup = app.selection[0];
        let at = bounds(&app, dup);
        app.drop_action(menu(0.0, 40.0), DropAction::Copy);
        let r = bounds(&app, app.selection[0]);
        assert!((r.x0 - at.x0).abs() < 1e-6 && (r.y0 - (at.y0 + 40.0)).abs() < 1e-6);
    }

    #[test]
    fn fill_outline_and_properties_go_onto_the_target() {
        let mut app = App::headless();
        let (a, b) = two_boxes(&mut app);
        app.select(vec![a]);
        let menu = DropMenu {
            screen: egui::Pos2::ZERO,
            offset: Vec2::new(55.0, 5.0),
            target: Some(b),
            fit_text_at: None,
        };
        app.drop_action(menu, DropAction::CopyFill);
        let shape = |app: &App, id| app.doc().shape(id).map(|(_, s)| s.clone()).ok();
        let (sa, sb) = (shape(&app, a).expect("a"), shape(&app, b).expect("b"));
        assert_eq!(sb.fill, sa.fill);
        assert_ne!(sb.stroke, sa.stroke);
        app.drop_action(menu, DropAction::CopyOutline);
        let sb = shape(&app, b).expect("b");
        assert_eq!(sb.stroke, sa.stroke);
        // The source did not move.
        assert!((bounds(&app, a).x0).abs() < 1e-6);
        // Without a target these do nothing.
        let none = DropMenu {
            target: None,
            ..menu
        };
        let before = app.doc().clone();
        app.drop_action(none, DropAction::CopyProperties);
        assert_eq!(*app.doc(), before);
    }

    /// Copy Effects Here gives the target the source's Layer Effects,
    /// switched off ones too, in place of its own, and leaves its other
    /// effects; Object > Copy Effect > Layer Effects From does the same
    /// (#4243).
    #[test]
    fn copy_effects_here_gives_the_target_the_layer_effects() {
        use crate::ui::layer_effects::Kind;
        let mut app = App::headless();
        let (a, b) = two_boxes(&mut app);
        let c = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 80.0, 20.0, 100.0),
                radius: 0.0,
                corners: None,
            })
            .expect("c");
        app.select(vec![a]);
        let mut ht = Kind::Halftone.default_effect();
        if let tracedraw_core::live::Effect::Halftone { size, .. } = &mut ht {
            *size = 2.25;
        }
        let glow = Kind::OuterGlow.default_effect();
        app.set_layer_effect_on(Kind::Halftone, true, &ht);
        app.set_layer_effect_on(Kind::OuterGlow, true, &glow);
        app.set_layer_effect_on(Kind::OuterGlow, false, &glow);
        // The target has a blur of its own, which goes.
        app.select(vec![b]);
        app.set_layer_effect_on(Kind::GaussianBlur, true, &Kind::GaussianBlur.default_effect());
        app.select(vec![a]);
        let menu = DropMenu {
            screen: egui::Pos2::ZERO,
            offset: Vec2::new(55.0, 5.0),
            target: Some(b),
            fit_text_at: None,
        };
        app.drop_action(menu, DropAction::CopyEffects);
        let stack = |app: &App, id| app.doc().find_shape(id).expect("shape").effect_stack();
        assert_eq!(stack(&app, b), stack(&app, a));
        assert_eq!(stack(&app, b), vec![(ht.clone(), true), (glow.clone(), false)]);
        // One undo step.
        app.undo();
        assert!(stack(&app, b).iter().all(|(e, _)| Kind::GaussianBlur.matches(e)));
        // The menu's Layer Effects From.
        app.select(vec![c, a]);
        app.copy_effect_from(a, crate::copy_effect::CopyKind::LayerEffects);
        assert_eq!(stack(&app, c), stack(&app, a));
    }

    #[test]
    fn place_inside_makes_the_target_a_clip_frame() {
        let mut app = App::headless();
        let (a, b) = two_boxes(&mut app);
        app.select(vec![a]);
        let menu = DropMenu {
            screen: egui::Pos2::ZERO,
            offset: Vec2::new(55.0, 5.0),
            target: Some(b),
            fit_text_at: None,
        };
        app.drop_action(menu, DropAction::PlaceInside);
        let frame = app.doc().shape(b).map(|(_, s)| s.clone()).expect("frame");
        assert!(matches!(frame.kind, ShapeKind::ClipFrame { .. }));
        assert_eq!(app.selection, vec![b]);
    }

    /// Discussion #2066: a right-button drag let go over another object
    /// with Shift held copies the outline there without the menu, with
    /// Alt the fill and with Ctrl both; without keys the
    /// menu opens.
    #[test]
    fn right_drop_with_shift_alt_or_ctrl_copies_without_the_menu() {
        let shape = |app: &App, id| app.doc().shape(id).map(|(_, s)| s.clone()).ok();
        let mods = |shift, alt, command| egui::Modifiers {
            shift,
            alt,
            command,
            ctrl: command,
            ..Default::default()
        };
        for (keys, fill, outline) in [
            (mods(true, false, false), false, true),
            (mods(false, true, false), true, false),
            (mods(false, false, true), true, true),
        ] {
            let mut app = App::headless();
            let (a, b) = two_boxes(&mut app);
            app.select(vec![a]);
            let over_b = bounds(&app, b).center();
            let drag = RightDrag {
                start: bounds(&app, a).center(),
                current: over_b,
            };
            app.right_drag_released(drag, keys);
            assert!(app.drop_menu.is_none(), "no menu with {keys:?}");
            let (sa, sb) = (shape(&app, a).expect("a"), shape(&app, b).expect("b"));
            assert_eq!(sb.fill == sa.fill, fill, "fill with {keys:?}");
            assert_eq!(sb.stroke == sa.stroke, outline, "outline with {keys:?}");
            assert!((bounds(&app, a).x0).abs() < 1e-6, "the source stays");
        }
        // No keys: the menu, nothing changed yet.
        let mut app = App::headless();
        let (a, b) = two_boxes(&mut app);
        app.select(vec![a]);
        let before = app.doc().clone();
        let drag = RightDrag {
            start: bounds(&app, a).center(),
            current: bounds(&app, b).center(),
        };
        app.right_drag_released(drag, egui::Modifiers::default());
        assert_eq!(app.drop_menu.map(|m| m.target), Some(Some(b)));
        assert_eq!(*app.doc(), before);
    }

    /// #2896: Copy All Properties (a right-button drag let go with Ctrl,
    /// or the drop menu) gives the target a mesh fill over its own box,
    /// as a mesh fill given to it directly lies, not one left where the
    /// source is; the same onto an ellipse.
    #[test]
    fn copy_all_properties_fits_a_mesh_fill_to_the_target() {
        use tracedraw_core::Mesh;
        let ctrl = egui::Modifiers {
            command: true,
            ctrl: true,
            ..Default::default()
        };
        let ellipse = ShapeKind::Ellipse {
            rect: Rect::new(100.0, 40.0, 130.0, 60.0),
            arc: None,
        };
        for target in [None, Some(ellipse)] {
            let mut app = App::headless();
            let (a, mut b) = two_boxes(&mut app);
            if let Some(kind) = target.clone() {
                b = app.new_shape(kind).expect("ellipse");
            }
            app.run(Command::SetFill {
                shapes: vec![a],
                fill: Fill::Mesh(Mesh::new(
                    Rect::new(0.0, 0.0, 10.0, 10.0),
                    2,
                    2,
                    Color::rgb8(200, 30, 30),
                )),
            });
            app.select(vec![a]);
            let drag = RightDrag {
                start: bounds(&app, a).center(),
                current: bounds(&app, b).center(),
            };
            app.right_drag_released(drag, ctrl);
            let s = app.doc().find_shape(b).expect("b").clone();
            let Fill::Mesh(m) = &s.fill else {
                panic!("no mesh: {:?}", s.fill);
            };
            let bb = s.bounds();
            for n in &m.nodes {
                let p = s.transform * n.pos;
                assert!(
                    p.x >= bb.x0 - 1e-6
                        && p.x <= bb.x1 + 1e-6
                        && p.y >= bb.y0 - 1e-6
                        && p.y <= bb.y1 + 1e-6,
                    "node {p:?} off the target {bb:?}"
                );
            }
            let has = |c: Point| {
                m.nodes
                    .iter()
                    .any(|n| (s.transform * n.pos - c).hypot() < 1e-6)
            };
            assert!(has(Point::new(bb.x0, bb.y0)) && has(Point::new(bb.x1, bb.y1)));
            // One undo step gives the target back its fill.
            app.undo();
            assert!(!matches!(
                app.doc().find_shape(b).expect("b").fill,
                Fill::Mesh(_)
            ));
            // The drop menu's Copy All Properties fits it too.
            let menu = DropMenu {
                screen: egui::Pos2::ZERO,
                offset: Vec2::new(55.0, 5.0),
                target: Some(b),
                fit_text_at: None,
            };
            app.drop_action(menu, DropAction::CopyProperties);
            let s = app.doc().find_shape(b).expect("b").clone();
            let Fill::Mesh(m) = &s.fill else {
                panic!("no mesh");
            };
            let bb = s.bounds();
            assert!(m
                .nodes
                .iter()
                .all(|n| bb.inflate(1e-6, 1e-6).contains(s.transform * n.pos)));
        }
    }
}

#[cfg(test)]
mod context_menu_settings_tests {
    use super::*;
    use tracedraw_core::{Color, Fill, ShapeKind};

    #[test]
    fn hiding_copy_here_does_not_disable_the_copy_action() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("object");
        app.select(vec![id]);
        app.settings
            .context_menus
            .set_visible("drop", "drop.copy_here", false);
        let menu = DropMenu {
            screen: egui::pos2(32.0, 32.0),
            offset: Vec2::new(20.0, 5.0),
            target: None,
            fit_text_at: None,
        };
        app.drop_menu = Some(menu);
        crate::ui::menus::ROWS.with(|rows| rows.borrow_mut().clear());
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| drop_menu_ui(&mut app, ui));
            out.textures_delta.clear();
        }
        let rows = crate::ui::menus::ROWS.with(|rows| rows.borrow().clone());
        assert!(!rows.iter().any(|(key, _)| key == "drop.copy_here"));
        assert!(rows.iter().any(|(key, _)| key == "drop.move_here"));
        app.drop_action(menu, DropAction::Copy);
        assert_eq!(app.selection.len(), 1);
        assert_ne!(app.selection[0], id);
        assert_eq!(app.doc().find_shape(id).expect("original").bounds().x0, 0.0);
    }

    #[test]
    fn hidden_drop_rows_leave_modifier_drag_gestures_unchanged() {
        let mut app = App::headless();
        let source = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("source");
        let target = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(50.0, 0.0, 70.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("target");
        let color = Color::rgb8(180, 25, 40);
        app.run(Command::SetFill {
            shapes: vec![source],
            fill: Fill::Solid(color),
        });
        app.select(vec![source]);
        app.settings
            .context_menus
            .set_visible("drop", "drop.copy_fill_here", false);
        app.right_drag_released(
            RightDrag {
                start: Point::new(5.0, 5.0),
                current: Point::new(60.0, 10.0),
            },
            egui::Modifiers {
                alt: true,
                ..Default::default()
            },
        );
        assert!(app.drop_menu.is_none());
        assert_eq!(
            app.doc().find_shape(target).expect("target").fill,
            Fill::Solid(color)
        );
    }
}

#[cfg(test)]
mod hidden_root_popup_tests {
    use super::*;

    #[test]
    fn hiding_every_operation_clears_the_popup_and_pending_menu() {
        let mut app = App::headless();
        let position = egui::pos2(30.0, 30.0);
        app.drop_menu = Some(DropMenu {
            screen: position,
            offset: Vec2::new(1.0, 2.0),
            target: None,
            fit_text_at: None,
        });
        let definition = crate::ui::context_menus::CATALOG
            .iter()
            .find(|menu| menu.id == "drop")
            .expect("menu");
        for key in definition.items {
            app.settings
                .context_menus
                .set_visible(definition.id, key, false);
        }
        let context = crate::theme::ui_context();
        let id = egui::Id::new("canvas_drop_menu");
        egui::Popup::open_id(&context, id);
        context.data_mut(|data| data.insert_temp(id.with("pos"), position));
        let mut output = context.run_ui(Default::default(), |ui| drop_menu_ui(&mut app, ui));
        output.textures_delta.clear();
        assert!(app.drop_menu.is_none());
        assert!(!egui::Popup::is_id_open(&context, id));
        assert!(context
            .data(|data| data.get_temp::<egui::Pos2>(id.with("pos")))
            .is_none());
    }
}

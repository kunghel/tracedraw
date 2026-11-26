//! Options > Customization > Context Menus: tree visibility and sibling order.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::context_menus::{MenuDefinition, CATALOG};
use crate::ui::tips::RichTip;
use egui::{Id, Ui};
use std::collections::BTreeSet;

#[derive(Clone, Default)]
struct EditorState {
    menu: String,
    search: String,
    collapsed: BTreeSet<String>,
}

#[derive(Clone)]
struct DragItem {
    menu: String,
    parent: String,
    item: String,
}

struct TreeRow {
    item: &'static str,
    parent: Vec<&'static str>,
    path: String,
    depth: usize,
    branch: bool,
    dynamic: bool,
    inherited_hidden: bool,
    index: usize,
    siblings: Vec<&'static str>,
}

fn collect_rows(
    menu: &str,
    preferences: &crate::settings::ContextMenuPreferences,
    state: &EditorState,
) -> Vec<TreeRow> {
    fn visit(
        menu: &str,
        preferences: &crate::settings::ContextMenuPreferences,
        state: &EditorState,
        parent: &[&'static str],
        inherited_hidden: bool,
        parent_matches: bool,
        rows: &mut Vec<TreeRow>,
    ) {
        let defaults = super::context_menu_branches::siblings(menu, parent);
        let siblings = preferences.ordered_items(
            menu,
            &super::context_menu_branches::path_key(parent),
            &defaults,
        );
        for (index, &item) in siblings.iter().enumerate() {
            let mut path = parent.to_vec();
            path.push(item);
            let path_key = path.join("/");
            let dynamic = super::context_menu_branches::is_dynamic_branch(menu, item);
            let branch = dynamic || !super::context_menu_branches::siblings(menu, &path).is_empty();
            let matched = parent_matches || matches_filter(&tr(item), item, &state.search);
            let mut descendants = Vec::new();
            if branch && (!state.collapsed.contains(&path_key) || !state.search.trim().is_empty()) {
                visit(
                    menu,
                    preferences,
                    state,
                    &path,
                    inherited_hidden || !preferences.is_visible(menu, item),
                    matched,
                    &mut descendants,
                );
            }
            if matched || !descendants.is_empty() {
                rows.push(TreeRow {
                    item,
                    parent: parent.to_vec(),
                    path: path_key,
                    depth: parent.len(),
                    branch,
                    dynamic,
                    inherited_hidden,
                    index,
                    siblings: siblings.clone(),
                });
                rows.extend(descendants);
            }
        }
    }
    let mut rows = Vec::new();
    visit(menu, preferences, state, &[], false, false, &mut rows);
    rows
}

/// A move changes only siblings. Parent keys carry their complete subtree.
fn move_item(
    preferences: &mut crate::settings::ContextMenuPreferences,
    menu: &str,
    parent: &[&str],
    item: &str,
    target: &str,
    after: bool,
) {
    let defaults = super::context_menu_branches::siblings(menu, parent);
    let parent_key = parent.join("/");
    let mut items = preferences.ordered_items(menu, &parent_key, &defaults);
    if item == target || !items.contains(&target) {
        return;
    }
    let Some(from) = items.iter().position(|key| *key == item) else {
        return;
    };
    let moved = items.remove(from);
    let Some(to) = items.iter().position(|key| *key == target) else {
        return;
    };
    items.insert(to + usize::from(after), moved);
    let only_known = preferences
        .order(menu, &parent_key)
        .iter()
        .all(|key| defaults.contains(&key.as_str()));
    if items == defaults && only_known {
        preferences.set_order(menu, &parent_key, &[]);
    } else {
        preferences.set_order(menu, &parent_key, &items);
    }
}

enum ControlIcon {
    MoveUp,
    MoveDown,
    Collapse,
    Expand,
    Clear,
}

fn control(ui: &mut Ui, icon: ControlIcon, label: &str, enabled: bool) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("").min_size(egui::vec2(22.0, 24.0)),
    );
    // These geometric controls must work with every script's font fallback.
    let center = response.rect.center();
    let stroke = if enabled {
        ui.visuals().widgets.inactive.fg_stroke
    } else {
        ui.visuals().widgets.noninteractive.fg_stroke
    };
    let segments: &[[(f32, f32); 2]] = match icon {
        ControlIcon::MoveUp => &[
            [(-4.0, 0.0), (0.0, -4.0)],
            [(0.0, -4.0), (4.0, 0.0)],
            [(0.0, -4.0), (0.0, 5.0)],
        ],
        ControlIcon::MoveDown => &[
            [(-4.0, 0.0), (0.0, 4.0)],
            [(0.0, 4.0), (4.0, 0.0)],
            [(0.0, 4.0), (0.0, -5.0)],
        ],
        ControlIcon::Collapse => &[[(-4.0, -2.0), (0.0, 2.0)], [(0.0, 2.0), (4.0, -2.0)]],
        ControlIcon::Expand => &[[(-2.0, -4.0), (2.0, 0.0)], [(2.0, 0.0), (-2.0, 4.0)]],
        ControlIcon::Clear => &[[(-3.0, -3.0), (3.0, 3.0)], [(-3.0, 3.0), (3.0, -3.0)]],
    };
    for &[(x1, y1), (x2, y2)] in segments {
        ui.painter().line_segment(
            [center + egui::vec2(x1, y1), center + egui::vec2(x2, y2)],
            stroke,
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response.rich_tip(label)
}

fn tree_row(
    app: &mut App,
    ui: &mut Ui,
    menu: &str,
    state: &mut EditorState,
    row: &TreeRow,
) -> egui::Rect {
    ui.push_id((menu, &row.path), |ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        ui.horizontal(|ui| {
            ui.add_space(row.depth as f32 * 14.0);
            if row.branch {
                let expanded =
                    !state.collapsed.contains(&row.path) || !state.search.trim().is_empty();
                let label = tr(if expanded {
                    "options.context_menu_collapse"
                } else {
                    "options.context_menu_expand"
                });
                let toggle = control(
                    ui,
                    if expanded {
                        ControlIcon::Collapse
                    } else {
                        ControlIcon::Expand
                    },
                    &label,
                    state.search.trim().is_empty(),
                );
                #[cfg(test)]
                remember_rect(ui, ("expand", menu, &row.path), &toggle);
                if toggle.clicked() {
                    if expanded {
                        state.collapsed.insert(row.path.clone());
                    } else {
                        state.collapsed.remove(&row.path);
                    }
                }
            } else {
                ui.allocate_space(egui::vec2(22.0, 24.0));
            }
            let handle = ui.allocate_response(egui::vec2(18.0, 24.0), egui::Sense::drag());
            let label = tr("options.context_menu_drag");
            handle
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
            let center = handle.rect.center();
            for x in [-2.0, 2.0] {
                for y in [-4.0, 0.0, 4.0] {
                    ui.painter()
                        .circle_filled(center + egui::vec2(x, y), 1.0, Tokens::text_dim());
                }
            }
            handle.dnd_set_drag_payload(DragItem {
                menu: menu.to_string(),
                parent: row.parent.join("/"),
                item: row.item.to_string(),
            });
            #[cfg(test)]
            remember_rect(ui, ("drag", menu, &row.path), &handle);
            handle.rich_tip(label);

            let mut visible = app.settings.context_menus.is_visible(menu, row.item);
            let name = tr(row.item);
            let mut text = egui::RichText::new(&name);
            if row.branch {
                text = text.strong();
            }
            if row.inherited_hidden {
                text = text.color(Tokens::text_dim());
            }
            let label_width = (ui.available_width() - 50.0).max(24.0);
            let response = ui
                .allocate_ui_with_layout(
                    egui::vec2(label_width, 24.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(label_width);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        ui.add(egui::Checkbox::new(&mut visible, text))
                    },
                )
                .inner;
            #[cfg(test)]
            remember_rect(ui, ("item", menu, row.item), &response);
            #[cfg(test)]
            remember_rect(ui, ("path", menu, &row.path), &response);
            if response.changed() {
                app.settings
                    .context_menus
                    .set_visible(menu, row.item, visible);
            }
            response.rich_tip(if row.inherited_hidden {
                format!("{name}\n{}", tr("options.context_menu_parent_hidden"))
            } else {
                name
            });
            let up = control(
                ui,
                ControlIcon::MoveUp,
                &tr("options.context_menu_move_up"),
                row.index > 0,
            );
            #[cfg(test)]
            remember_rect(ui, ("up", menu, &row.path), &up);
            if up.clicked() {
                if let Some(target) = row.index.checked_sub(1).and_then(|i| row.siblings.get(i)) {
                    move_item(
                        &mut app.settings.context_menus,
                        menu,
                        &row.parent,
                        row.item,
                        target,
                        false,
                    );
                }
            }
            let down = control(
                ui,
                ControlIcon::MoveDown,
                &tr("options.context_menu_move_down"),
                row.index + 1 < row.siblings.len(),
            );
            #[cfg(test)]
            remember_rect(ui, ("down", menu, &row.path), &down);
            if down.clicked() {
                if let Some(target) = row.siblings.get(row.index + 1) {
                    move_item(
                        &mut app.settings.context_menus,
                        menu,
                        &row.parent,
                        row.item,
                        target,
                        true,
                    );
                }
            }
        })
        .response
        .rect
    })
    .inner
}

fn tree(app: &mut App, ui: &mut Ui, menu: &str, state: &mut EditorState) {
    let rows = collect_rows(menu, &app.settings.context_menus, state);
    if rows.is_empty() {
        ui.label(
            egui::RichText::new(tr("options.context_menu_no_results")).color(Tokens::text_dim()),
        );
        return;
    }
    let rects: Vec<_> = rows
        .iter()
        .map(|row| {
            let rect = tree_row(app, ui, menu, state, row);
            if row.dynamic
                && (!state.collapsed.contains(&row.path) || !state.search.trim().is_empty())
            {
                let hint = ui
                    .horizontal(|ui| {
                        ui.add_space((row.depth + 1) as f32 * 14.0 + 25.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(tr("options.context_menu_dynamic"))
                                    .small()
                                    .color(Tokens::text_dim()),
                            )
                            .wrap(),
                        );
                    })
                    .response
                    .rect;
                rect.union(hint)
            } else {
                rect
            }
        })
        .collect();
    let Some(drag) = egui::DragAndDrop::payload::<DragItem>(ui.ctx()) else {
        return;
    };
    if drag.menu != menu {
        return;
    }
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };
    if !ui.clip_rect().contains(pointer) {
        return;
    }
    // Keep distant siblings reachable while dragging a long menu.
    let edge = 24.0;
    let clip = ui.clip_rect();
    let delta = if pointer.y < clip.top() + edge {
        6.0
    } else if pointer.y > clip.bottom() - edge {
        -6.0
    } else {
        0.0
    };
    if delta != 0.0 && ui.input(|input| input.pointer.primary_down()) {
        ui.scroll_with_delta(egui::vec2(0.0, delta));
        ui.ctx().request_repaint();
    }
    for (i, row) in rows.iter().enumerate() {
        if row.parent.join("/") != drag.parent || row.item == drag.item {
            continue;
        }
        let Some(&rect) = rects.get(i) else { continue };
        let mut group = rect;
        for (child, &child_rect) in rows.iter().zip(&rects).skip(i + 1) {
            if child.depth <= row.depth {
                break;
            }
            group = group.union(child_rect);
        }
        if group.contains(pointer) {
            let after = pointer.y >= group.center().y;
            let y = if after { group.bottom() } else { group.top() };
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left() + row.depth as f32 * 14.0, y),
                    egui::pos2(rect.right(), y),
                ],
                egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            if ui.input(|input| input.pointer.any_released()) {
                move_item(
                    &mut app.settings.context_menus,
                    menu,
                    &row.parent,
                    &drag.item,
                    row.item,
                    after,
                );
                egui::DragAndDrop::clear_payload(ui.ctx());
            }
            break;
        }
    }
}

fn state_id() -> Id {
    Id::new("context_menu_editor_state")
}

#[cfg(test)]
pub(crate) fn set_test_view(ctx: &egui::Context, menu: &str, search: &str) {
    ctx.data_mut(|data| {
        data.insert_temp(
            state_id(),
            EditorState {
                menu: menu.to_string(),
                search: search.to_string(),
                ..Default::default()
            },
        )
    });
}

fn menu_for(state: &EditorState) -> Option<&'static MenuDefinition> {
    CATALOG
        .iter()
        .find(|menu| menu.id == state.menu)
        .or_else(|| CATALOG.first())
}

/// Search the translated label and stable item key, requiring every word.
fn matches_filter(name: &str, item: &str, search: &str) -> bool {
    let name = name.to_lowercase();
    let item = item.to_lowercase();
    search.split_whitespace().all(|term| {
        let term = term.to_lowercase();
        name.contains(&term) || item.contains(&term)
    })
}

/// Edit settings in place; OptionsSnapshot restores them when cancelled.
pub fn show(app: &mut App, ui: &mut Ui) {
    let mut state = ui
        .ctx()
        .data_mut(|data| data.get_temp::<EditorState>(state_id()))
        .unwrap_or_default();
    let Some(initial) = menu_for(&state) else {
        return;
    };
    state.menu = initial.id.to_string();

    ui.add(
        egui::Label::new(
            egui::RichText::new(tr("options.context_menu_intro")).color(Tokens::text_dim()),
        )
        .wrap(),
    );
    ui.add_space(4.0);
    let scene_label = ui.label(tr("options.context_menu_scene"));
    let scene = crate::ui::keynav::ComboBox::from_id_salt("context_menu_location")
        .selected_text(tr(initial.key))
        .width(ui.available_width().min(400.0))
        .truncate()
        .show_ui(ui, |ui| {
            for menu in CATALOG {
                ui.selectable_value(&mut state.menu, menu.id.to_string(), tr(menu.key));
            }
        })
        .response
        .labelled_by(scene_label.id)
        .rich_tip(tr("options.context_menu_scene"));
    #[cfg(test)]
    remember_rect(ui, "scene", &scene);
    #[cfg(not(test))]
    let _ = scene;

    let Some(menu) = menu_for(&state) else {
        return;
    };
    let search_label = ui.label(tr("options.context_menu_search"));
    let search = ui
        .horizontal(|ui| {
            let has_search = !state.search.is_empty();
            let width = (ui.available_width() - if has_search { 29.0 } else { 0.0 }).max(40.0);
            let search = ui.add(
                egui::TextEdit::singleline(&mut state.search)
                    .id_salt("context_menu_search")
                    .desired_width(width),
            );
            if has_search
                && control(ui, ControlIcon::Clear, &tr("font_list.filter_clear"), true).clicked()
            {
                state.search.clear();
                search.request_focus();
            }
            search
        })
        .inner
        .labelled_by(search_label.id)
        .rich_tip(tr("options.context_menu_search"));
    #[cfg(test)]
    remember_rect(ui, "search", &search);
    #[cfg(not(test))]
    let _ = search;

    ui.horizontal_wrapped(|ui| {
        let customized = app.settings.context_menus.has_overrides(menu.id);
        let reset = ui
            .add_enabled(
                customized,
                crate::ui::btn_icons::Ib::new("options.context_menu_reset"),
            )
            .rich_tip(tr("options.context_menu_reset"));
        #[cfg(test)]
        remember_rect(ui, "reset_menu", &reset);
        if reset.clicked() {
            app.settings.context_menus.reset_menu(menu.id);
        }
        let reset_all = ui
            .add_enabled(
                !app.settings.context_menus.is_default(),
                crate::ui::btn_icons::Ib::new("options.context_menu_reset_all"),
            )
            .rich_tip(tr("options.context_menu_reset_all"));
        #[cfg(test)]
        remember_rect(ui, "reset_all", &reset_all);
        if reset_all.clicked() {
            app.settings.context_menus.reset_all();
        }
    });
    ui.separator();
    ui.add(
        egui::Label::new(
            egui::RichText::new(tr("options.context_menu_reorder_hint"))
                .small()
                .color(Tokens::text_dim()),
        )
        .wrap(),
    );

    egui::ScrollArea::vertical()
        .id_salt(("context_menu_items", menu.id))
        .auto_shrink([false, true])
        .max_height(330.0)
        .show(ui, |ui| {
            tree(app, ui, menu.id, &mut state);
        });

    ui.ctx()
        .data_mut(|data| data.insert_temp(state_id(), state));
}

#[cfg(test)]
fn remember_rect(ui: &Ui, key: impl std::hash::Hash + std::fmt::Debug, response: &egui::Response) {
    ui.ctx().data_mut(|data| {
        data.insert_temp(Id::new(("context_menu_editor_test", &key)), response.rect);
        data.insert_temp(
            Id::new(("context_menu_editor_test_clip", &key)),
            ui.clip_rect(),
        );
        data.insert_temp(Id::new(("context_menu_editor_test_id", key)), response.id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
        let _ = frame_at_width(app, ctx, events, 600.0);
    }

    fn frame_at_width(
        app: &mut App,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        width: f32,
    ) -> f32 {
        let mut overflow = 0.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                let right = ui.max_rect().right();
                show(app, ui);
                overflow = ui.min_rect().right() - right;
            },
        );
        output.textures_delta.clear();
        overflow
    }

    fn setup(menu: &str) -> (App, egui::Context) {
        let app = App::headless();
        let ctx = crate::theme::ui_context();
        ctx.data_mut(|data| {
            data.insert_temp(
                state_id(),
                EditorState {
                    menu: menu.to_string(),
                    search: String::new(),
                    ..Default::default()
                },
            );
        });
        (app, ctx)
    }

    fn rect(ctx: &egui::Context, key: impl std::hash::Hash + std::fmt::Debug) -> egui::Rect {
        ctx.data(|data| {
            data.get_temp(Id::new(("context_menu_editor_test", key)))
                .expect("visible editor control")
        })
    }

    fn click(app: &mut App, ctx: &egui::Context, rect: egui::Rect) {
        let pos = rect.center();
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    fn item(menu: &str) -> &'static str {
        CATALOG
            .iter()
            .find(|definition| definition.id == menu)
            .and_then(|definition| definition.items.first().copied())
            .expect("menu contains an item")
    }

    #[test]
    fn filter_matches_translated_labels_and_stable_keys() {
        assert!(matches_filter(
            "Copy objects",
            "menu.edit.copy",
            "OBJECT copy"
        ));
        assert!(matches_filter("复制对象", "menu.edit.copy", "复制"));
        assert!(matches_filter(
            "Copy objects",
            "menu.edit.copy",
            "edit.copy"
        ));
        assert!(matches_filter("Copy objects", "menu.edit.copy", "  "));
        assert!(!matches_filter(
            "Copy objects",
            "menu.edit.copy",
            "copy missing"
        ));
    }

    #[test]
    fn unknown_location_falls_back_to_first_catalog_menu() {
        let state = EditorState {
            menu: "unknown_menu".to_string(),
            ..Default::default()
        };
        assert_eq!(
            menu_for(&state).map(|menu| menu.id),
            CATALOG.first().map(|menu| menu.id)
        );
    }

    #[test]
    fn checkbox_changes_only_the_selected_menu() {
        let first = item("canvas_object");
        let second = item("canvas_page");
        let (mut app, ctx) = setup("canvas_object");
        frame(&mut app, &ctx, Vec::new());
        let checkbox = rect(&ctx, ("item", "canvas_object", first));
        click(&mut app, &ctx, checkbox);
        assert!(!app
            .settings
            .context_menus
            .is_visible("canvas_object", first));
        assert!(app.settings.context_menus.is_visible("canvas_page", second));
        click(&mut app, &ctx, checkbox);
        assert!(app.settings.context_menus.is_default());
    }

    #[test]
    fn keyboard_toggles_the_focused_checkbox_without_pointer_input() {
        let menu = "canvas_object";
        let first = item(menu);
        for key in [egui::Key::Space, egui::Key::Enter] {
            let (mut app, ctx) = setup(menu);
            frame(&mut app, &ctx, Vec::new());
            let id = ctx.data(|data| {
                data.get_temp::<egui::Id>(Id::new((
                    "context_menu_editor_test_id",
                    ("item", menu, first),
                )))
                .expect("visible checkbox has an identity")
            });
            ctx.memory_mut(|memory| memory.request_focus(id));
            for visible in [false, true] {
                for pressed in [true, false] {
                    frame(
                        &mut app,
                        &ctx,
                        vec![egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                    );
                }
                assert_eq!(
                    app.settings.context_menus.is_visible(menu, first),
                    visible,
                    "{key:?} toggles the focused checkbox"
                );
                assert_eq!(ctx.memory(|memory| memory.focused()), Some(id));
            }
        }
    }

    #[test]
    fn every_menu_fits_a_narrow_settings_panel_without_horizontal_overflow() {
        for width in [240.0, 320.0, 420.0] {
            for menu in CATALOG {
                let (mut app, ctx) = setup(menu.id);
                for _ in 0..3 {
                    let overflow = frame_at_width(&mut app, &ctx, Vec::new(), width);
                    assert!(
                        overflow < 0.5,
                        "{} exceeds its {width}-point panel by {overflow}",
                        menu.id
                    );
                }
                assert!(app.settings.context_menus.is_default());
            }
        }
    }

    #[test]
    fn hidden_items_remain_in_the_filtered_editor() {
        let first = item("canvas_object");
        let (mut app, ctx) = setup("canvas_object");
        app.settings
            .context_menus
            .set_visible("canvas_object", first, false);
        ctx.data_mut(|data| {
            data.insert_temp(
                state_id(),
                EditorState {
                    menu: "canvas_object".to_string(),
                    search: first.to_string(),
                    ..Default::default()
                },
            );
        });
        frame(&mut app, &ctx, Vec::new());
        let checkbox = rect(&ctx, ("item", "canvas_object", first));
        click(&mut app, &ctx, checkbox);
        assert!(app.settings.context_menus.is_default());
    }

    #[test]
    fn restore_current_menu_preserves_other_menu_preferences() {
        let first = item("canvas_object");
        let second = item("canvas_page");
        let (mut app, ctx) = setup("canvas_object");
        app.settings
            .context_menus
            .set_visible("canvas_object", first, false);
        app.settings
            .context_menus
            .set_visible("canvas_page", second, false);
        frame(&mut app, &ctx, Vec::new());
        let reset = rect(&ctx, "reset_menu");
        click(&mut app, &ctx, reset);
        assert!(app
            .settings
            .context_menus
            .is_visible("canvas_object", first));
        assert!(!app.settings.context_menus.is_visible("canvas_page", second));
    }

    #[test]
    fn restore_current_menu_clears_unknown_operations_and_preserves_other_locations() {
        let (mut app, ctx) = setup("canvas_page");
        app.settings.context_menus = serde_json::from_str(
            r#"{"canvas_page":["future.command"],"canvas_object":["future.other"]}"#,
        )
        .expect("preferences from a newer version");
        let other = item("canvas_object");
        app.settings
            .context_menus
            .set_visible("canvas_object", other, false);
        frame(&mut app, &ctx, Vec::new());
        let reset = rect(&ctx, "reset_menu");
        click(&mut app, &ctx, reset);
        assert!(!app.settings.context_menus.has_overrides("canvas_page"));
        assert!(app
            .settings
            .context_menus
            .is_visible("canvas_page", "future.command"));
        assert!(!app
            .settings
            .context_menus
            .is_visible("canvas_object", "future.other"));
        assert!(!app
            .settings
            .context_menus
            .is_visible("canvas_object", other));
    }

    #[test]
    fn restore_all_menus_returns_to_default_visibility() {
        let (mut app, ctx) = setup("canvas_object");
        app.settings
            .context_menus
            .set_visible("canvas_object", item("canvas_object"), false);
        app.settings
            .context_menus
            .set_visible("canvas_page", item("canvas_page"), false);
        frame(&mut app, &ctx, Vec::new());
        let reset = rect(&ctx, "reset_all");
        click(&mut app, &ctx, reset);
        assert!(app.settings.context_menus.is_default());
    }

    #[test]
    fn options_snapshot_restores_menu_preferences_after_cancel() {
        let mut app = App::headless();
        let first = item("canvas_object");
        app.settings
            .context_menus
            .set_visible("canvas_object", first, false);
        let snapshot = crate::ui::options::OptionsSnapshot::take(&app);
        app.settings.context_menus.reset_all();
        app.settings
            .context_menus
            .set_visible("canvas_page", item("canvas_page"), false);
        app.settings
            .context_menus
            .set_order("canvas_page", "", &["menu.edit.paste"]);
        snapshot.restore(&mut app);
        assert!(!app.settings.context_menus.has_order("canvas_page"));
        assert!(!app
            .settings
            .context_menus
            .is_visible("canvas_object", first));
        assert!(app
            .settings
            .context_menus
            .is_visible("canvas_page", item("canvas_page")));
    }

    #[test]
    fn nested_search_keeps_the_complete_parent_path() {
        let state = EditorState {
            menu: "canvas_object".to_string(),
            search: "menu.bitmaps.trace_logo".to_string(),
            ..Default::default()
        };
        let rows = collect_rows(&state.menu, &Default::default(), &state);
        let paths: Vec<_> = rows.iter().map(|row| (row.item, row.depth)).collect();
        assert_eq!(
            paths,
            [
                ("menu.bitmaps.trace", 0),
                ("menu.bitmaps.outline_trace", 1),
                ("menu.bitmaps.trace_logo", 2),
            ]
        );
    }

    #[test]
    fn collapsed_branches_expand_for_search_without_losing_their_state() {
        let mut state = EditorState::default();
        state.collapsed.insert("menu.bitmaps.trace".to_string());
        let prefs = Default::default();
        let rows = collect_rows("canvas_object", &prefs, &state);
        assert!(!rows.iter().any(|row| row.item == "menu.bitmaps.trace_logo"));
        state.search = "menu.bitmaps.trace_logo".to_string();
        let rows = collect_rows("canvas_object", &prefs, &state);
        assert!(rows.iter().any(|row| row.item == "menu.bitmaps.trace_logo"));
        assert!(state.collapsed.contains("menu.bitmaps.trace"));
    }

    #[test]
    fn moving_a_branch_preserves_children_and_sorts_only_its_siblings() {
        let mut prefs = crate::settings::ContextMenuPreferences::default();
        move_item(
            &mut prefs,
            "canvas_object",
            &[],
            "menu.object.order",
            "menu.edit.cut",
            false,
        );
        let rows = collect_rows("canvas_object", &prefs, &EditorState::default());
        let order = rows
            .iter()
            .position(|row| row.item == "menu.object.order")
            .expect("order branch");
        let cut = rows
            .iter()
            .position(|row| row.item == "menu.edit.cut")
            .expect("cut row");
        assert!(order < cut);
        assert!(rows
            .iter()
            .skip(order + 1)
            .take(cut - order - 1)
            .all(|row| row.depth == 1));
        let children =
            super::super::context_menu_branches::siblings("canvas_object", &["menu.object.order"]);
        assert_eq!(
            rows.iter()
                .skip(order + 1)
                .take(children.len())
                .map(|row| row.item)
                .collect::<Vec<_>>(),
            children
        );
        let before = prefs.clone();
        move_item(
            &mut prefs,
            "canvas_object",
            &["menu.object.order"],
            "menu.object.to_front_of_page",
            "menu.edit.copy",
            false,
        );
        assert_eq!(prefs, before, "a cross-parent target is rejected");
    }

    #[test]
    fn repeated_child_keys_have_distinct_paths_and_independent_order() {
        let mut prefs = crate::settings::ContextMenuPreferences::default();
        let parent = ["bars.customize", "bars.toolbox_toolbar", "bars.button_size"];
        move_item(
            &mut prefs,
            "toolbar",
            &parent,
            "bars.size_large",
            "bars.size_default",
            false,
        );
        let rows = collect_rows("toolbar", &prefs, &EditorState::default());
        let large: Vec<_> = rows
            .iter()
            .filter(|row| row.item == "bars.size_large")
            .collect();
        assert_eq!(large.len(), 2);
        assert_ne!(large[0].path, large[1].path);
        assert!(prefs
            .order("toolbar", "bars.customize/bars.default_button_size")
            .is_empty());
        assert_eq!(
            prefs
                .order("toolbar", &parent.join("/"))
                .first()
                .map(String::as_str),
            Some("bars.size_large")
        );
    }

    #[test]
    fn parent_visibility_does_not_erase_child_choices() {
        let mut prefs = crate::settings::ContextMenuPreferences::default();
        prefs.set_visible("canvas_object", "menu.bitmaps.trace", false);
        let rows = collect_rows("canvas_object", &prefs, &EditorState::default());
        let logo = rows
            .iter()
            .find(|row| row.item == "menu.bitmaps.trace_logo")
            .expect("child stays editable");
        assert!(logo.inherited_hidden);
        assert!(prefs.is_visible("canvas_object", logo.item));
    }

    #[test]
    fn movement_buttons_work_with_keyboard_and_restore_default_order() {
        let menu = "canvas_page";
        let (mut app, ctx) = setup(menu);
        let defaults = super::super::context_menu_branches::siblings(menu, &[]);
        let first = *defaults.first().expect("page has rows");
        let second = *defaults.get(1).expect("page has two rows");
        frame(&mut app, &ctx, Vec::new());
        let down = rect(&ctx, ("down", menu, first));
        click(&mut app, &ctx, down);
        assert_eq!(
            app.settings
                .context_menus
                .order(menu, "")
                .first()
                .map(String::as_str),
            Some(second)
        );
        frame(&mut app, &ctx, Vec::new());
        let up_id = ctx
            .data(|data| {
                data.get_temp::<Id>(Id::new((
                    "context_menu_editor_test_id",
                    ("up", menu, first),
                )))
            })
            .expect("up button");
        ctx.memory_mut(|memory| memory.request_focus(up_id));
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(app.settings.context_menus.is_default());
    }

    #[test]
    fn pointer_drag_changes_sibling_order_and_escape_cancels_a_drag() {
        let menu = "canvas_page";
        let (mut app, ctx) = setup(menu);
        let defaults = super::super::context_menu_branches::siblings(menu, &[]);
        let first = *defaults.first().expect("first page row");
        let second = *defaults.get(1).expect("second page row");
        frame(&mut app, &ctx, Vec::new());
        let start = rect(&ctx, ("drag", menu, first)).center();
        let end = rect(&ctx, ("drag", menu, second)).center() + egui::vec2(0.0, 7.0);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            app.settings
                .context_menus
                .order(menu, "")
                .first()
                .map(String::as_str),
            Some(second)
        );

        let before = app.settings.context_menus.clone();
        frame(&mut app, &ctx, Vec::new());
        let start = rect(&ctx, ("drag", menu, first)).center();
        let end = rect(&ctx, ("drag", menu, second)).center();
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.settings.context_menus, before);
    }
}

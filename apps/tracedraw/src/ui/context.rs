//! Right-click context menus: object menu (Pick tool), node menu (Shape
//! tool), page menu (empty area), text editing menu.

use crate::app::{App, DockerTab};
use crate::i18n::tr;
use crate::tools::Tool;
use crate::ui::dialogs::Dialog;
use crate::ui::menus::{
    body, check, item, item_label_keyed, menu_popup_style, radio, sep, sub, CLIP_EDIT_KEY,
};
use egui::{Response, Ui};
use tracedraw_core::{document::ShapeKind, Command, ShapeId};

fn ci(ui: &mut Ui, close: &mut bool, key: &str, shortcut: &str, enabled: bool) -> bool {
    let r = item(ui, key, shortcut, enabled);
    if r {
        *close = true;
    }
    r
}

/// [`ci`] with a hover tip for the greyed row.
fn ci_tip(ui: &mut Ui, close: &mut bool, key: &str, enabled: bool, tip: Option<&str>) -> bool {
    let r = crate::ui::menus::item_tip(ui, key, "", enabled, tip);
    if r {
        *close = true;
    }
    r
}

/// The canvas context menu, drawn like the menus of the menu bar, at the
/// position of the right click. Clicking an item or anywhere else closes
/// it; submenus open on hover.
pub fn context_menu(app: &mut App, ui: &mut Ui, response: &Response) {
    let Some((pos, point)) = app.context_menu else {
        return;
    };
    let id = egui::Id::new("canvas_context_menu");
    let ctx = ui.ctx().clone();
    // A new right click (a new position) opens the menu afresh.
    let pos_id = id.with("pos");
    let location = menu_location(app, point);
    let dynamic = [("docker.object_styles", !app.doc().object_styles.is_empty())];
    if ctx.data(|d| d.get_temp::<egui::Pos2>(pos_id)) != Some(pos) {
        ctx.data_mut(|d| d.insert_temp(pos_id, pos));
        egui::Popup::open_id(&ctx, id);
    }
    let mut close = false;
    let popup = egui::Popup::new(
        id,
        ctx.clone(),
        egui::PopupAnchor::Position(pos),
        ui.layer_id(),
    )
    .kind(egui::PopupKind::Menu)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .style(menu_popup_style)
    .open_memory(None)
    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let shown = crate::ui::context_menus::filter_popup_with(
        popup,
        &app.settings.context_menus,
        location,
        &dynamic,
    )
    .show(|ui| {
        ui.set_min_width(200.0);
        body(ui, |ui| menu_items(app, ui, &mut close))
    });
    if close || shown.is_none() {
        egui::Popup::close_id(&ctx, id);
        app.context_menu = None;
        app.context_locked = None;
        app.context_docker = None;
        app.tab_ruler_menu = None;
        ctx.data_mut(|d| d.remove::<egui::Pos2>(pos_id));
    }
    let _ = response;
}

fn menu_location(app: &App, p: tracedraw_core::Point) -> &'static str {
    let on_guide = app.selection.is_empty()
        && matches!(app.tool, Tool::Pick | Tool::FreeformPick)
        && app.guide_at(p).is_some();
    if app.tab_ruler_menu.is_some() {
        "tab_ruler"
    } else if app.context_locked.is_some() {
        "canvas_locked"
    } else if app.context_docker.is_some() && !app.selection.is_empty() {
        "objects_object"
    } else if app.editing_text() {
        "canvas_text"
    } else if app.tool == Tool::Shape {
        "canvas_node"
    } else if on_guide {
        "canvas_guide"
    } else if app.selection.is_empty() {
        "canvas_page"
    } else {
        "canvas_object"
    }
}

/// The rows of the context menu: which menu fits where it was opened
/// (the tab ruler, a locked object, an Objects docker row, text being
/// edited, the Shape tool's nodes, a guideline, the page, the selection).
pub(crate) fn menu_items(app: &mut App, ui: &mut Ui, close: &mut bool) {
    let Some((_, p)) = app.context_menu else {
        return;
    };
    let location = menu_location(app, p);
    let _menu = crate::ui::context_menus::Scope::new(&app.settings.context_menus, location);
    if let Some(hit) = app.tab_ruler_menu {
        tab_ruler_menu(app, ui, hit, close);
    } else if let Some(locked) = app.context_locked {
        locked_menu(app, ui, locked, close);
    } else if let Some(id) = app.context_docker.filter(|_| !app.selection.is_empty()) {
        // From the Objects docker: the docker's own commands,
        // then the same menu as on the drawing (#2601).
        object_menu_for(app, ui, close, Some(id));
    } else if app.editing_text() {
        text_edit_menu(app, ui, close);
    } else if app.tool == Tool::Shape {
        node_menu(app, ui, close);
    } else if location == "canvas_guide" {
        guide_menu(app, ui, close);
    } else if app.selection.is_empty() {
        page_menu(app, ui, close);
    } else {
        object_menu(app, ui, close);
    }
}

/// Opens Text > Change Case for the text under the right click: a text
/// being typed ends its editing and is selected first (#3826, #2059).
pub(crate) fn open_change_case(app: &mut App) {
    if let Some(shape) = app.text_edit.as_ref().map(|t| t.shape) {
        app.finish_text();
        app.select(vec![shape]);
    }
    app.dialog = crate::ui::dialogs::Dialog::ChangeCase(crate::ops2::CaseMode::Sentence);
}

/// Right-click on the tab ruler (#1852): the four kinds of stop, the one
/// under the pointer checked (a pick changes it, or with no stop there
/// adds one of that kind), and Remove tab on a stop.
fn tab_ruler_menu(app: &mut App, ui: &mut Ui, hit: crate::tab_ruler::RulerHit, close: &mut bool) {
    let current = hit
        .stop
        .and_then(|i| app.tab_ruler().and_then(|r| r.stops.get(i).map(|s| s.kind)));
    let kinds = tracedraw_core::TabKind::ALL;
    let [a, b, c, d] = kinds.map(crate::ui::dialogs::tab_kind_key);
    let keys = [a, b, c, d, "dialog.remove_tab"];
    for key in crate::ui::context_menu_filter::ordered_keys(&keys)
        .iter()
        .copied()
    {
        if key == "dialog.remove_tab" {
            if hit.stop.is_some() {
                sep(ui);
                if ci(ui, close, key, "", true) {
                    app.tab_ruler_pick(hit, None);
                }
            }
        } else if let Some(kind) = kinds
            .iter()
            .copied()
            .find(|kind| crate::ui::dialogs::tab_kind_key(*kind) == key)
        {
            if radio(ui, key, "", current == Some(kind)) {
                *close = true;
                app.tab_ruler_pick(hit, Some(kind));
            }
        }
    }
}

/// Right-click on a guideline: delete, lock or unlock, and the Guidelines
/// docker.
fn guide_menu(app: &mut App, ui: &mut Ui, close: &mut bool) {
    let guides = app.page_guides();
    let selected = app.selected_guides.clone();
    let locked = !selected.is_empty()
        && selected
            .iter()
            .all(|i| guides.get(*i).is_some_and(|g| g.locked));
    let guide_lock_key = if locked {
        "guides.unlock"
    } else {
        "guides.lock"
    };
    crate::ui::context_menu_filter::ordered_menu! { ui;
        "menu.edit.undo" => {
            if ci(
                ui,
                close,
                "menu.edit.undo",
                "Ctrl+Z",
                app.engine.undo_label().is_some(),
            ) {
                app.undo();
            }
        }
        "menu.edit.delete" => {
            sep(ui);
            if ci(ui, close, "menu.edit.delete", "Delete", !locked) && !app.delete_selected_guides()
            {
                app.status = tr("status.guideline_locked");
            }
        }
        guide_lock_key => {
            sep(ui);
            if locked {
                if ci(ui, close, "guides.unlock", "", true) {
                    app.set_guides_locked(&selected, false);
                }
            } else if ci(ui, close, "guides.lock", "", !selected.is_empty()) {
                app.set_guides_locked(&selected, true);
            }
        }
        "menu.object.properties" => {
            sep(ui);
            if ci(ui, close, "menu.object.properties", "", true) {
                app.show_dockers = true;
                app.docker_tab = DockerTab::Guidelines;
                if let Some(i) = selected.first().copied() {
                    app.open_guide_properties(i);
                }
            }
        }
    }
}

/// Right-click while a text is being edited: the editing commands act on
/// its characters (#1377), and the Properties docker.
pub(crate) fn text_edit_menu(app: &mut App, ui: &mut Ui, close: &mut bool) {
    let picked = app.text_selection_active();
    crate::ui::context_menu_filter::ordered_menu! { ui;
        "menu.edit.undo" => {
            if ci(
                ui,
                close,
                "menu.edit.undo",
                "Ctrl+Z",
                app.engine.undo_label().is_some(),
            ) {
                app.undo();
            }
        }
        "menu.edit.cut" => {
            sep(ui);
            if ci(ui, close, "menu.edit.cut", "Ctrl+X", picked) {
                app.text_cut();
            }
        }
        "menu.edit.copy" => {
            if ci(ui, close, "menu.edit.copy", "Ctrl+C", picked) {
                app.text_copy();
            }
        }
        "menu.edit.paste" => {
            if ci(ui, close, "menu.edit.paste", "Ctrl+V", true) {
                app.text_paste(None);
            }
        }
        "menu.edit.delete" => {
            if ci(ui, close, "menu.edit.delete", "Delete", picked) {
                app.text_delete_forward();
            }
        }
        "menu.edit.select_all" => {
            sep(ui);
            if ci(ui, close, "menu.edit.select_all", "Ctrl+A", true) {
                app.text_select_all();
            }
        }
        "menu.text.change_case" => {
            // Text > Change Case on the right click too (#3826): the editing
            // ends with the text selected, as Shift+F3 does while typing.
            if ci(ui, close, "menu.text.change_case", "Shift+F3", true) {
                open_change_case(app);
            }
        }
        "menu.table" => {
            // The table commands act on the cells being edited (#2976).
            if app.table_edit.is_some() && app.selected_table().is_some() {
                sep(ui);
                table_items(app, ui, close);
            }
        }
        "menu.text.insert_placeholder_text" => {
            // A paragraph frame being edited: filler text in place of its own.
            if app.placeholder_text_offered()
                && ci(ui, close, "menu.text.insert_placeholder_text", "", true)
            {
                app.insert_placeholder_text();
            }
        }
        "menu.object.properties" => {
            sep(ui);
            if ci(ui, close, "menu.object.properties", "Alt+Enter", true) {
                app.show_dockers = true;
                app.docker_tab = DockerTab::Properties;
            }
        }
    }
}

/// The Table submenu of Text > Table, for the context menu (#2976): add,
/// select and delete rows and columns, delete the table, merge, unmerge
/// and split the cells. They act on the chosen cells, as from the menu.
fn table_items(app: &mut App, ui: &mut Ui, close: &mut bool) {
    use crate::table::TableOp;
    use crate::ui::table_bar::TableCountKind;
    sub(ui, "menu.table", |ui| {
        sub(ui, "menu.table.insert", |ui| {
            for (key, op) in [
                ("menu.table.insert_row_above", TableOp::RowAbove),
                ("menu.table.insert_row_below", TableOp::RowBelow),
                ("menu.table.insert_column_left", TableOp::ColLeft),
                ("menu.table.insert_column_right", TableOp::ColRight),
            ] {
                if ci(ui, close, key, "", true) {
                    app.table_op(op);
                }
            }
            sep(ui);
            if ci(ui, close, "menu.table.insert_rows", "", true) {
                app.open_table_count(TableCountKind::InsertRows);
            }
            if ci(ui, close, "menu.table.insert_columns", "", true) {
                app.open_table_count(TableCountKind::InsertCols);
            }
        });
        sub(ui, "menu.table.select", |ui| {
            for (key, op) in [
                ("menu.table.select_cell", TableOp::SelectCell),
                ("menu.table.select_row", TableOp::SelectRow),
                ("menu.table.select_column", TableOp::SelectCol),
                ("menu.table.select_table", TableOp::SelectTable),
            ] {
                if ci(ui, close, key, "", true) {
                    app.table_op(op);
                }
            }
        });
        sub(ui, "menu.table.delete", |ui| {
            for (key, op) in [
                ("menu.table.delete_row", TableOp::DeleteRow),
                ("menu.table.delete_column", TableOp::DeleteCol),
                ("menu.table.delete_table", TableOp::DeleteTable),
            ] {
                if ci(ui, close, key, "", true) {
                    app.table_op(op);
                }
            }
        });
        sep(ui);
        if ci(ui, close, "menu.table.merge_cells", "Ctrl+M", true) {
            app.table_op(TableOp::Merge);
        }
        if ci(ui, close, "menu.table.unmerge_cells", "", true) {
            app.table_op(TableOp::Unmerge);
        }
        if ci(ui, close, "menu.table.split_into_rows", "", true) {
            app.open_table_count(TableCountKind::SplitRows);
        }
        if ci(ui, close, "menu.table.split_into_columns", "", true) {
            app.open_table_count(TableCountKind::SplitCols);
        }
    });
}

/// Right-click on a locked object: only unlocking it (it is selected
/// once unlocked) or every locked object of the page.
fn locked_menu(app: &mut App, ui: &mut Ui, id: ShapeId, close: &mut bool) {
    crate::ui::context_menu_filter::ordered_menu! { ui;
        "menu.object.unlock_object" => {
            if ci(ui, close, "menu.object.unlock_object", "", true) {
                app.unlock_object(id);
            }
        }
        "menu.object.unlock_all" => {
            if ci(ui, close, "menu.object.unlock_all", "", true) {
                app.unlock_all();
            }
        }
    }
}

pub(crate) fn object_menu(app: &mut App, ui: &mut Ui, close: &mut bool) {
    object_menu_for(app, ui, close, None);
}

fn object_menu_for(app: &mut App, ui: &mut Ui, close: &mut bool, source: Option<ShapeId>) {
    let shapes = app.selected_shapes();
    let is_text = shapes
        .iter()
        .any(|s| matches!(s.kind, ShapeKind::Text { .. }));
    // A clip frame whose frame is a group ungroups too (#1605).
    let is_group = shapes
        .iter()
        .any(|s| matches!(s.kind, ShapeKind::Group { .. }) || s.is_clip_frame_of_group());
    let is_curve = shapes
        .iter()
        .any(|s| matches!(s.kind, ShapeKind::Path { .. }));
    let is_bitmap = shapes
        .iter()
        .any(|s| matches!(s.kind, ShapeKind::Bitmap { .. }));
    let (ai, tip, vtip) = if is_bitmap {
        (
            app.ai_image_ready() && app.ai_source_bitmap().is_some() && !app.ai_image.busy(),
            app.ai_image_tip(),
            app.ai_vectorize_tip(),
        )
    } else {
        (false, None, None)
    };
    let many = shapes.len() > 1;
    // Clones and their masters.
    let has_clone = app.selection_has_clone();
    let has_clones = app.selection_has_clones();
    // Break Apart, or the drop shadow's or effect group's version of it.
    let label = app.break_apart_label();
    // Also a warped object (its warp is baked into curves) and artistic
    // text, as the Object menu and Ctrl+K do (#647).
    let can = is_curve
        || is_group
        || label != "menu.object.break_apart"
        || shapes.iter().any(App::breaks_apart);
    let locked = shapes.iter().any(|s| s.locked);
    let lock_key = if locked {
        "menu.object.unlock_object"
    } else {
        "menu.object.lock_object"
    };
    // A clip frame: into its contents, or its contents out (#1273).
    let clip = shapes
        .iter()
        .any(|s| matches!(s.kind, ShapeKind::ClipFrame { .. }));
    // Wrap Paragraph Text: on or off, the seven styles and the offset.
    let (wrap, current) = app.wrap_state();
    let of = shapes.iter().any(|s| s.overprint_fill);
    let oo = shapes.iter().any(|s| s.overprint_outline);
    let hint = shapes.iter().all(|s| app.object_hinted(s.id));
    crate::ui::context_menu_filter::ordered_menu! { ui;
        "docker.rename" => {
            if let Some(id) = source {
                crate::ui::objects_docker::row_menu_item(app, ui, id, close, "docker.rename");
            }
        }
        "docker.move_to_layer" => {
            if let Some(id) = source {
                crate::ui::objects_docker::row_menu_item(
                    app,
                    ui,
                    id,
                    close,
                    "docker.move_to_layer",
                );
            }
        }
        "docker.copy_to_layer" => {
            if let Some(id) = source {
                crate::ui::objects_docker::row_menu_item(app, ui, id, close, "docker.copy_to_layer");
                sep(ui);
            }
        }
        "menu.table" => {
            // Table commands are available for imported spreadsheets too (#2976).
            if app.selected_table().is_some() {
                table_items(app, ui, close);
                sep(ui);
            }
        }
        "menu.text.edit_text" => {
            if is_text && ci(ui, close, "menu.text.edit_text", "Ctrl+Shift+T", true) {
                app.open_edit_text_dialog();
            }
        }
        "menu.text.convert" => {
            if is_text && ci(ui, close, "menu.text.convert", "", true) {
                app.toggle_text_kind();
            }
        }
        "menu.text.change_case" => {
            if is_text && ci(ui, close, "menu.text.change_case", "Shift+F3", true) {
                open_change_case(app);
            }
        }
        "menu.text.insert_placeholder_text" => {
            if is_text {
                // Filler text for a paragraph frame, an empty one most of all.
                if app.placeholder_text_offered()
                    && ci(ui, close, "menu.text.insert_placeholder_text", "", true)
                {
                    app.insert_placeholder_text();
                }
            }
        }
        "menu.text.join_lines" => {
            if is_text
                && many
                && !app.joinable_text_lines().is_empty()
                && ci(ui, close, "menu.text.join_lines", "", true)
            {
                app.join_text_lines();
            }
        }
        "menu.text.link_content" => {
            if is_text {
                // Linked text content (#1568), offered when it can be used.
                if app.can_link_text_content() && ci(ui, close, "menu.text.link_content", "", true)
                {
                    app.link_text_content();
                }
            }
        }
        "menu.text.unlink_content" => {
            if is_text {
                if app.selection_has_content_link()
                    && ci(ui, close, "menu.text.unlink_content", "", true)
                {
                    app.unlink_text_content();
                }
                sep(ui);
            }
        }
        "menu.table.edit_in_spreadsheet" => {
            // A table alone: its cells in the spreadsheet program (#4145).
            if !crate::files::WEB && app.lone_selected_table().is_some() {
                let label = app.edit_in_spreadsheet_label();
                if crate::ui::menus::item_text(
                    ui,
                    &label,
                    "",
                    true,
                    None,
                    "menu.table.edit_in_spreadsheet",
                ) {
                    *close = true;
                    app.edit_table_externally();
                }
                sep(ui);
            }
        }
        "menu.bitmaps.edit_bitmap" => {
            if is_bitmap {
                // Edit Bitmap, then Remove Background (greyed until the AI
                // service makes pictures; the tip says why), then tracing,
                // cropping and remixing.

                if ci(
                    ui,
                    close,
                    "menu.bitmaps.edit_bitmap",
                    "",
                    app.selected_bitmap().is_some(),
                ) {
                    app.edit_bitmap_externally();
                }
            }
        }
        "menu.bitmaps.replace_bitmap" => {
            if is_bitmap {
                // Another picture in its place (#2598).
                if ci(
                    ui,
                    close,
                    "menu.bitmaps.replace_bitmap",
                    "",
                    app.selected_bitmap().is_some(),
                ) {
                    app.replace_image();
                }
            }
        }
        "menu.bitmaps.remove_background_local" => {
            if is_bitmap {
                // On this computer, no AI service needed (#231).
                if ci(
                    ui,
                    close,
                    "menu.bitmaps.remove_background_local",
                    "",
                    app.selected_bitmap().is_some(),
                ) {
                    crate::ui::bitmap_dialogs::open_remove_background(app);
                }
            }
        }
        "menu.bitmaps.remove_background" => {
            if is_bitmap
                && ci_tip(
                    ui,
                    close,
                    "menu.bitmaps.remove_background",
                    ai,
                    tip.as_deref(),
                )
            {
                app.ai_remove_background();
            }
        }
        "menu.bitmaps.ai_vectorize" => {
            if is_bitmap
                && ci_tip(
                    ui,
                    close,
                    "menu.bitmaps.ai_vectorize",
                    app.ai_vectorize_enabled(),
                    vtip.as_deref(),
                )
            {
                app.ai_vectorize();
            }
            if is_bitmap {
                crate::ai_photo::menu_items(app, ui, |ui, key, on, tip| {
                    ci_tip(ui, close, key, on, tip)
                });
            }
        }
        "menu.bitmaps.save_bitmap_as" => {
            if is_bitmap
                && ci(
                    ui,
                    close,
                    "menu.bitmaps.save_bitmap_as",
                    "",
                    app.selected_bitmap().is_some(),
                )
            {
                app.save_bitmap_as();
            }
        }
        "menu.bitmaps.trace" => {
            if is_bitmap {
                // The trace commands of the Image menu (#452), under Trace: the
                // automatic trace, the last trace again or its settings, and the
                // Centerline and Outline presets.
                sub(ui, "menu.bitmaps.trace", |ui| {
                    let last = app.last_trace.is_some();
                    crate::ui::context_menu_filter::ordered_menu! { ui;
                        "menu.bitmaps.quick_trace" => {
                            if ci(ui, close, "menu.bitmaps.quick_trace", "", true) {
                                app.quick_trace();
                            }
                        }
                        "menu.bitmaps.repeat_last_trace" => {
                            if ci(ui, close, "menu.bitmaps.repeat_last_trace", "", last) {
                                app.repeat_last_trace();
                            }
                        }
                        "menu.bitmaps.last_trace_settings" => {
                            if ci(ui, close, "menu.bitmaps.last_trace_settings", "", last) {
                                app.open_last_trace_settings();
                            }
                        }
                        "menu.bitmaps.centerline_trace" => {
                            sep(ui);
                            sub(ui, "menu.bitmaps.centerline_trace", |ui| {
                                let presets = crate::ui::menus::CENTERLINE_TRACES;
                                let keys = presets.map(|(key, _)| key);
                                for key in crate::ui::context_menu_filter::ordered_keys(&keys)
                                    .iter()
                                    .copied()
                                {
                                    let Some((_, preset)) =
                                        presets.iter().find(|(candidate, _)| *candidate == key)
                                    else {
                                        continue;
                                    };
                                    let preset = *preset;
                                    if ci(ui, close, key, "", true) {
                                        app.open_trace(preset);
                                    }
                                }
                            });
                        }
                        "menu.bitmaps.outline_trace" => {
                            sub(ui, "menu.bitmaps.outline_trace", |ui| {
                                let presets = crate::ui::menus::OUTLINE_TRACES;
                                let keys = presets.map(|(key, _)| key);
                                for key in crate::ui::context_menu_filter::ordered_keys(&keys)
                                    .iter()
                                    .copied()
                                {
                                    let Some((_, preset)) =
                                        presets.iter().find(|(candidate, _)| *candidate == key)
                                    else {
                                        continue;
                                    };
                                    let preset = *preset;
                                    if ci(ui, close, key, "", true) {
                                        app.open_trace(preset);
                                    }
                                }
                            });
                        }
                    }
                });
            }
        }
        "menu.bitmaps.crop_bitmap" => {
            if is_bitmap && ci(ui, close, "menu.bitmaps.crop_bitmap", "", true) {
                app.set_tool(Tool::Crop);
            }
        }
        "menu.bitmaps.scale_known" => {
            if is_bitmap && app.can_scale_known() && ci(ui, close, "menu.bitmaps.scale_known", "", true) {
                app.open_scale_known();
            }
        }
        "context.ai_remix" => {
            if is_bitmap {
                if ci_tip(ui, close, "context.ai_remix", ai, tip.as_deref()) {
                    app.ai_image.mode = crate::ai_image::Mode::Remix;
                    app.ai_image.info = false;
                    app.ai_source_from_selection();
                    app.open_ai_generate();
                }
                sep(ui);
            }
        }
        "context.select_master" => {
            if has_clone && ci(ui, close, "context.select_master", "", true) {
                app.select_clone_masters();
            }
        }
        "context.revert_to_master" => {
            if has_clone && ci(ui, close, "context.revert_to_master", "", true) {
                app.revert_clones_to_master();
            }
        }
        "context.unlink_clone" => {
            if has_clone && ci(ui, close, "context.unlink_clone", "", true) {
                app.unlink_clones();
            }
        }
        "context.select_clones" => {
            if has_clone || has_clones {
                if has_clones && ci(ui, close, "context.select_clones", "", true) {
                    app.select_clones_of_selection();
                }
                sep(ui);
            }
        }
        "context.replace" => {
            // Replace the selected objects with another one (#4220).
            sub(ui, "context.replace", |ui| {
                crate::ui::context_menu_filter::ordered_menu! { ui;
                    "context.replace_with_object" => {
                        if ci(ui, close, "context.replace_with_object", "", true) {
                            app.start_replace_objects(false);
                        }
                    }
                    "context.replace_with_clones" => {
                        if ci(ui, close, "context.replace_with_clones", "", true) {
                            app.start_replace_objects(true);
                        }
                    }
                }
            });
        }
        "context.bring_into_focus" => {
            // One object (a group most of all): edit it on its own.
            if ci(
                ui,
                close,
                "context.bring_into_focus",
                "",
                shapes.len() == 1
                    && shapes
                        .first()
                        .is_some_and(|shape| app.focus_target() != Some(shape.id)),
            ) {
                app.bring_into_focus();
            }
        }
        "menu.object.convert_to_curves" => {
            if ci(
                ui,
                close,
                "menu.object.convert_to_curves",
                "Ctrl+Q",
                app.can_convert_to_curves(),
            ) {
                app.convert_to_curves();
            }
        }
        "menu.object.convert_outline_to_object" => {
            // Under Convert to Curves, as on the Object menu (#3895).
            if ci(
                ui,
                close,
                "menu.object.convert_outline_to_object",
                "Ctrl+Shift+Q",
                !shapes.is_empty(),
            ) {
                app.convert_outline_to_object();
            }
        }
        "menu.object.convert_to_guidelines" => {
            // The objects become guidelines along their sides (#3954).
            if ci(
                ui,
                close,
                "menu.object.convert_to_guidelines",
                "",
                app.can_convert_to_guides(),
            ) {
                app.convert_to_guides();
            }
        }
        label => {
            if ci(ui, close, label, "Ctrl+K", can) {
                app.break_apart();
            }
        }
        "menu.object.combine" => {
            if ci(ui, close, "menu.object.combine", "Ctrl+L", many) {
                app.combine();
            }
        }
        "context.edit_shadow" => {
            // A drop shadow: the Drop Shadow tool on it.
            if app.can_edit_shadow() && ci(ui, close, "context.edit_shadow", "", true) {
                app.edit_shadow();
            }
        }
        "menu.edit.cut" => {
            sep(ui);
            if ci(ui, close, "menu.edit.cut", "Ctrl+X", true) {
                app.cut();
            }
        }
        "menu.edit.copy" => {
            if ci(ui, close, "menu.edit.copy", "Ctrl+C", true) {
                app.copy_with_system();
            }
        }
        "menu.edit.paste" => {
            // Paste on an object's menu too, landing where Ctrl+V puts it (#3993).
            if ci(ui, close, "menu.edit.paste", "Ctrl+V", true) {
                app.paste_any();
            }
        }
        "menu.edit.duplicate" => {
            if ci(ui, close, "menu.edit.duplicate", "Ctrl+D", true) {
                app.duplicate();
            }
        }
        "menu.edit.delete" => {
            if ci(ui, close, "menu.edit.delete", "Delete", true) {
                app.delete_selection();
            }
        }
        "menu.object.add_to_shelf" => {
            if ci(ui, close, "menu.object.add_to_shelf", "", true) && app.shelf_add_selection() {
                app.open_docker(DockerTab::Shelf);
            }
        }
        "menu.object.add_common_shape" => {
            if ci(ui, close, "menu.object.add_common_shape", "", true) {
                app.add_selection_as_common_shape();
            }
        }
        "menu.edit.select_similar" => {
            sub(ui, "menu.edit.select_similar", |ui| {
                use crate::select_similar::Similar;
                let keys = Similar::ALL.map(Similar::key);
                for key in crate::ui::context_menu_filter::ordered_keys(&keys).iter().copied() {
                    if let Some(how) = Similar::ALL.into_iter().find(|how| how.key() == key) {
                        if ci(ui, close, key, "", true) {
                            app.select_similar(how);
                        }
                    }
                }
                crate::select_similar::tolerance_row(ui, &mut app.settings.similar_tolerance);
            });
        }
        "menu.object.order" => {
            sep(ui);
            sub(ui, "menu.object.order", |ui| crate::ui::context_menu_filter::ordered_menu! { ui;
                "menu.object.to_front_of_page" => {
                    if ci(ui, close, "menu.object.to_front_of_page", "Ctrl+Home", true) {
                        app.order_page(true);
                    }
                }
                "menu.object.to_back_of_page" => {
                    if ci(ui, close, "menu.object.to_back_of_page", "Ctrl+End", true) {
                        app.order_page(false);
                    }
                }
                "menu.object.forward_one" => {
                    if ci(ui, close, "menu.object.forward_one", "Ctrl+PgUp", true) {
                        app.order(1);
                    }
                }
                "menu.object.back_one" => {
                    if ci(ui, close, "menu.object.back_one", "Ctrl+PgDn", true) {
                        app.order(2);
                    }
                }
                "menu.object.in_front_of" => {
                    if ci(ui, close, "menu.object.in_front_of", "", true) {
                        app.pending_order = Some(true);
                    }
                }
                "menu.object.behind" => {
                    if ci(ui, close, "menu.object.behind", "", true) {
                        app.pending_order = Some(false);
                    }
                }
                "menu.object.reverse_order" => {
                    if ci(ui, close, "menu.object.reverse_order", "", many) {
                        app.reverse_order();
                    }
                }
            });
        }
        "menu.object.group_group" => {
            if app.can_group() && ci(ui, close, "menu.object.group_group", "Ctrl+G", true) {
                app.group_selection();
            }
        }
        "menu.object.group_ungroup" => {
            if is_group && ci(ui, close, "menu.object.group_ungroup", "Ctrl+U", true) {
                app.ungroup_selection();
            }
        }
        "menu.object.group_ungroup_all" => {
            if is_group && ci(ui, close, "menu.object.group_ungroup_all", "", true) {
                app.ungroup_all();
            }
        }
        "menu.object.hide_object" => {
            sep(ui);
            if ci(ui, close, "menu.object.hide_object", "", true) {
                app.set_visible(false);
            }
        }
        lock_key => {
            if ci(ui, close, lock_key, "", true) {
                app.set_locked(!locked);
            }
        }
        "menu.object.clip_frame_place_inside" => {
            sep(ui);
            if ci(ui, close, "menu.object.clip_frame_place_inside", "", true) {
                app.pending_clip_frame = true;
                app.status = tr("status.click_frame");
            }
        }
        "menu.object.clip_frame_edit" => {
            // A clip frame among the contents being edited: into it, inside
            // the one round it (#4110: there was no way into a second level).
            let edited = app.clip_frame_edit.as_ref().map(|(f, _)| *f);
            let inner = app.editing_clip_contents()
                && shapes.iter().any(|s| {
                    matches!(s.kind, ShapeKind::ClipFrame { .. }) && Some(s.id) != edited
                });
            if clip
                && (app.clip_frame_edit.is_none() || inner)
                && ci(
                    ui,
                    close,
                    "menu.object.clip_frame_edit",
                    CLIP_EDIT_KEY,
                    true,
                )
            {
                app.edit_clip_frame();
            }
        }
        "menu.object.clip_frame_extract" => {
            if clip
                && app.clip_frame_edit.is_none()
                && ci(ui, close, "menu.object.clip_frame_extract", "", true)
            {
                app.extract_clip_contents();
            }
        }
        "menu.object.clip_frame_finish" => {
            finish_clip_edit_row(app, ui, close);
        }
        "context.frame_type" => {
            sub(ui, "context.frame_type", |ui| {
                let empty_frames = !app.empty_clip_frames().is_empty();
                let framable = !app.framable().is_empty();
                crate::ui::context_menu_filter::ordered_menu! { ui;
                    "context.frame_none" => {
                        if ci(ui, close, "context.frame_none", "", empty_frames) {
                            app.frames_to_objects();
                        }
                    }
                    "context.frame_empty_clip_frame" => {
                        if ci(ui, close, "context.frame_empty_clip_frame", "", framable) {
                            app.objects_to_clip_frames();
                        }
                    }
                    "context.frame_text" => {
                        if ci(
                            ui,
                            close,
                            "context.frame_text",
                            "",
                            app.selection.len() == 1,
                        ) {
                            app.object_to_text_frame();
                        }
                    }
                }
            });
        }
        "menu.object.symmetry_create" => {
            sep(ui);
            if ci(ui, close, "menu.object.symmetry_create", "Alt+S", true) {
                app.create_symmetry();
            }
        }
        "context.create_symbol" => {
            if ci(ui, close, "context.create_symbol", "", true) {
                app.create_symbol_from_selection();
            }
        }
        "menu.object.add_dimensions" => {
            if ci(ui, close, "menu.object.add_dimensions", "", true) {
                app.add_dimensions(false);
            }
        }
        "menu.object.add_dimensions_each" => {
            if shapes.len() > 1 && ci(ui, close, "menu.object.add_dimensions_each", "", true) {
                app.add_dimensions(true);
            }
        }
        "context.dimension_label_text" => {
            // A copy of a dimension's label as plain text (#4405).
            let one = match app.selection.as_slice() {
                [id] => app
                    .doc()
                    .find_shape(*id)
                    .filter(|s| crate::dimensions::is_dimension(s))
                    .map(|_| *id),
                _ => None,
            };
            if let Some(id) = one {
                if ci(ui, close, "context.dimension_label_text", "", true) {
                    let t = tracedraw_core::geometry::Affine::translate(app.duplicate_offset);
                    app.copy_dimension_label(id, t);
                }
            }
        }
        "menu.object.symbol_edit" => {
            // A symbol instance: its symbol's objects edited in place (#1980).
            if let Some(id) = app.selected_symbol_instance() {
                if ci(
                    ui,
                    close,
                    "menu.object.symbol_edit",
                    "",
                    app.clip_frame_edit.is_none(),
                ) {
                    app.edit_symbol_instance(id);
                }
            }
        }
        "context.internet_links" => {
            sub(
                ui,
                "context.internet_links",
                |ui| crate::ui::context_menu_filter::ordered_menu! { ui;
                    "docker.hyperlink" => {
                        if ci(ui, close, "docker.hyperlink", "", true) {
                            app.show_dockers = true;
                            app.docker_tab = DockerTab::Links;
                        }
                    }
                },
            );
        }
        "docker.object_styles" => {
            crate::ui::menus::sub_dynamic(
                ui,
                "docker.object_styles",
                !app.doc().object_styles.is_empty(),
                |ui| {
                    let styles = app.doc().object_styles.clone();
                    let linked = !app.styled_selection().is_empty();
                    crate::ui::context_menu_filter::ordered_menu! { ui; fixed ["docker.object_styles"];
                        "docker.new_style_from_selection" => {
                            if ci(ui, close, "docker.new_style_from_selection", "", true) {
                                app.new_style_from_selection();
                            }
                        }
                        "docker.object_styles" => {
                            if !styles.is_empty() {
                                sep(ui);
                            }
                            for (i, os) in styles.iter().enumerate() {
                                if item_label_keyed(ui, "docker.object_styles", &os.name, "", true)
                                {
                                    app.apply_style(i);
                                    *close = true;
                                }
                            }
                        }
                        "docker.break_style_link" => {
                            if linked {
                                sep(ui);
                            }
                            if ci(ui, close, "docker.break_style_link", "", linked) {
                                app.break_style_links();
                            }
                        }
                        "docker.open_object_styles" => {
                            if ci(ui, close, "docker.open_object_styles", "", true) {
                                app.show_dockers = true;
                                app.docker_tab = DockerTab::ObjectStyles;
                            }
                        }
                    }
                },
            );
        }
        "docker.color_styles" => {
            sub(
                ui,
                "docker.color_styles",
                |ui| crate::ui::context_menu_filter::ordered_menu! { ui;
                    "docker.new_from_selection" => {
                        if ci(ui, close, "docker.new_from_selection", "", true) {
                            app.show_dockers = true;
                            app.docker_tab = DockerTab::ColorStyles;
                        }
                    }
                },
            );
        }
        "context.wrap_paragraph_text" => {
            sep(ui);
            sub(ui, "context.wrap_paragraph_text", |ui| {
                let styles = tracedraw_core::TextWrap::STYLES;
                let [a, b, c, d, e, f, g] =
                    styles.map(|(mode, side)| crate::ui::propbar::wrap_style_key(mode, side));
                let keys = [
                    "context.wrap_text_on",
                    a,
                    b,
                    c,
                    d,
                    e,
                    f,
                    g,
                    "context.wrap_offset",
                ];
                for key in crate::ui::context_menu_filter::ordered_keys(&keys)
                    .iter()
                    .copied()
                {
                    if key == "context.wrap_text_on" {
                        if check(ui, key, "", wrap) {
                            let ids = app.selection.clone();
                            app.run(Command::SetWrapText {
                                shapes: ids,
                                wrap: !wrap,
                            });
                            *close = true;
                        }
                        sep(ui);
                    } else if key == "context.wrap_offset" {
                        sep(ui);
                        if crate::ui::context_menus::show_item(ui, key) {
                            crate::ui::propbar::wrap_offset_field(app, ui, current.offset);
                        }
                    } else if let Some((mode, side)) =
                        styles.iter().copied().find(|(mode, side)| {
                            crate::ui::propbar::wrap_style_key(*mode, *side) == key
                        })
                    {
                        let here = wrap && current.mode == mode && current.side == side;
                        if radio(ui, key, "", here) {
                            app.set_wrap_style(mode, side);
                            *close = true;
                        }
                    }
                }
            });
        }
        "menu.object.overprint_fill" => {
            sep(ui);
            if check(ui, "menu.object.overprint_fill", "", of) {
                app.toggle_overprint(true);
                *close = true;
            }
        }
        "menu.object.overprint_outline" => {
            if check(ui, "menu.object.overprint_outline", "", oo) {
                app.toggle_overprint(false);
                *close = true;
            }
        }
        "menu.object.object_hinting" => {
            if check(ui, "menu.object.object_hinting", "", hint) {
                app.toggle_object_hinting();
                *close = true;
            }
        }
        "context.align_pixel_grid" => {
            if ci(ui, close, "context.align_pixel_grid", "", true) {
                app.align_to_pixel_grid();
            }
        }
        "menu.object.properties" => {
            sep(ui);
            if ci(ui, close, "menu.object.properties", "Alt+Enter", true) {
                app.show_dockers = true;
                app.docker_tab = DockerTab::Properties;
            }
        }
    }
}

pub(crate) fn node_menu(app: &mut App, ui: &mut Ui, close: &mut bool) {
    use tracedraw_core::nodes::NodeType;
    // A B-spline's chosen control points: fixed (the curve passes
    // through them) or floating (#1758).
    let spline_fixed = app.chosen_spline_points();
    let has_nodes = !app.node_selection.is_empty();
    crate::ui::context_menu_filter::ordered_menu! { ui;
        "context.spline_fixed" => {
            if let Some(all_fixed) = spline_fixed {
                if check(ui, "context.spline_fixed", "C", all_fixed) {
                    app.set_selected_spline_fixed(true);
                    *close = true;
                }
            }
        }
        "context.spline_floating" => {
            if let Some(all_fixed) = spline_fixed {
                if check(ui, "context.spline_floating", "S", !all_fixed) {
                    app.set_selected_spline_fixed(false);
                    *close = true;
                }
                sep(ui);
            }
        }
        "context.node_add" => {
            if ci(ui, close, "context.node_add", "", has_nodes) {
                app.add_node_midpoints();
            }
        }
        "context.node_delete" => {
            // The chosen nodes go, the curve stays; with no node chosen the
            // selected object goes, as with the Delete key (#1982).
            if ci(
                ui,
                close,
                "context.node_delete",
                "Delete",
                has_nodes || !app.selection.is_empty(),
            ) {
                app.delete_nodes_or_selection();
            }
        }
        "context.node_join" => {
            sep(ui);
            if ci(
                ui,
                close,
                "context.node_join",
                "",
                app.node_selection.len() == 2,
            ) {
                app.join_selected_nodes();
            }
        }
        "context.node_break" => {
            if ci(ui, close, "context.node_break", "", has_nodes) {
                app.break_selected_nodes();
            }
        }
        "context.node_to_line" => {
            sep(ui);
            if ci(ui, close, "context.node_to_line", "", has_nodes) {
                app.selected_segments_to_line();
            }
        }
        "context.node_to_curve" => {
            if ci(ui, close, "context.node_to_curve", "", has_nodes) {
                app.selected_segments_to_curve();
            }
        }
        "context.node_cusp" => {
            sep(ui);
            if ci(ui, close, "context.node_cusp", "C", has_nodes) {
                app.set_selected_node_type(NodeType::Cusp);
            }
        }
        "context.node_smooth" => {
            if ci(ui, close, "context.node_smooth", "S", has_nodes) {
                app.set_selected_node_type(NodeType::Smooth);
            }
        }
        "context.node_symmetrical" => {
            if ci(ui, close, "context.node_symmetrical", "Y", has_nodes) {
                app.set_selected_node_type(NodeType::Symmetrical);
            }
        }
        "context.node_reverse" => {
            sep(ui);
            if ci(ui, close, "context.node_reverse", "", true) {
                app.reverse_selected_curves();
            }
        }
        "context.node_extract" => {
            if ci(ui, close, "context.node_extract", "", has_nodes) {
                app.extract_subpath();
            }
        }
        "context.node_close" => {
            sep(ui);
            if ci(ui, close, "context.node_close", "", true) {
                app.close_selected_curves();
            }
        }
        "context.elastic_mode" => {
            if check(ui, "context.elastic_mode", "", app.elastic_mode) {
                app.elastic_mode = !app.elastic_mode;
                *close = true;
            }
        }
        "menu.object.properties" => {
            sep(ui);
            if ci(ui, close, "menu.object.properties", "Alt+Enter", true) {
                app.show_dockers = true;
                app.docker_tab = DockerTab::Properties;
            }
        }
    }
}

/// Finish Editing while a clip frame's contents are edited (#1273).
fn finish_clip_edit_row(app: &mut App, ui: &mut Ui, close: &mut bool) {
    if app.editing_clip_contents()
        && ci(
            ui,
            close,
            "menu.object.clip_frame_finish",
            CLIP_EDIT_KEY,
            true,
        )
    {
        app.finish_clip_frame_edit();
    }
}

fn page_menu(app: &mut App, ui: &mut Ui, close: &mut bool) {
    crate::ui::context_menu_filter::ordered_menu! { ui;
        "menu.object.clip_frame_finish" => {
            // Inside a clip frame, a click on the empty page finishes too.
            if app.editing_clip_contents() {
                finish_clip_edit_row(app, ui, close);
                sep(ui);
            }
        }
        "menu.edit.paste" => {
            // Offered like Edit > Paste: a copy made in another program pastes
            // too, not only one made here, and in its own place as Ctrl+V puts
            // it, not at the pointer (#3993; Paste in View centres it).
            if ci(ui, close, "menu.edit.paste", "Ctrl+V", true) {
                app.paste_any();
            }
        }
        "menu.edit.paste_at_pointer" => {
            // The same, centred where the menu was opened.
            if let Some((_, at)) = app.context_menu {
                if ci(ui, close, "menu.edit.paste_at_pointer", "", true) {
                    app.paste_at_pointer(at);
                }
            }
        }
        "menu.edit.select_all_objects" => {
            if ci(ui, close, "menu.edit.select_all_objects", "Ctrl+A", true) {
                app.select_all();
            }
        }
        "menu.file.import" => {
            // A file into the drawing, placed with the import cursor (#1931).
            if ci(ui, close, "menu.file.import", "Ctrl+I", true) {
                app.import();
            }
        }
        "menu.layout.insert_page" => {
            sep(ui);
            if ci(ui, close, "menu.layout.insert_page", "", true) {
                app.dialog = Dialog::InsertPage {
                    count: 1,
                    after: true,
                };
            }
        }
        "menu.layout.rename_page" => {
            if ci(ui, close, "menu.layout.rename_page", "", true) {
                let name = app
                    .doc()
                    .page(app.page)
                    .map(|pg| pg.name.clone())
                    .unwrap_or_default();
                app.dialog = Dialog::RenamePage { name };
            }
        }
        "menu.layout.page_size" => {
            if ci(ui, close, "menu.layout.page_size", "", true) {
                app.dialog = crate::ui::dialogs::page_size_dialog(app);
            }
        }
        "menu.layout.page_background" => {
            if ci(ui, close, "menu.layout.page_background", "", true) {
                app.dialog = Dialog::Options;
                app.options_page = crate::ui::dialogs::OptionsPage::Background;
            }
        }
        "menu.view.zoom_to_page" => {
            sep(ui);
            if ci(ui, close, "menu.view.zoom_to_page", "Shift+F4", true) {
                app.zoom_to_page();
            }
        }
        "menu.view.zoom_to_fit" => {
            if ci(ui, close, "menu.view.zoom_to_fit", "F4", true) {
                app.zoom_to_fit();
            }
        }
        "menu.tools.options_app" => {
            sep(ui);
            if ci(ui, close, "menu.tools.options_app", "Ctrl+J", true) {
                app.dialog = Dialog::Options;
            }
        }
    }
}

/// The menu `menu` drawn for a fresh app from `fresh`, walked row by row
/// (each row on an app of its own) until the row `key` is hit: that app,
/// with the row run; `None` when the menu has no such row or it is
/// greyed.
#[cfg(test)]
pub(crate) fn run_menu_row(
    fresh: &dyn Fn() -> App,
    menu: fn(&mut App, &mut Ui, &mut bool),
    key: &str,
) -> Option<App> {
    let ctx = egui::Context::default();
    let run = |app: &mut App, target: usize| {
        crate::ui::menus::replay::start(target);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let mut close = false;
            menu(app, ui, &mut close);
        });
        out.textures_delta.clear();
        crate::ui::menus::replay::finish()
    };
    let (_, rows) = run(&mut fresh(), usize::MAX);
    (0..rows).find_map(|t| {
        let mut app = fresh();
        let (hit, _) = run(&mut app, t);
        (hit.as_deref() == Some(key)).then_some(app)
    })
}

#[cfg(test)]
mod context_menu_settings_tests {
    use super::*;

    fn page_app() -> App {
        let mut app = App::headless();
        app.show_welcome = false;
        app.set_tool(Tool::Pick);
        app.context_menu = Some((
            egui::pos2(30.0, 30.0),
            tracedraw_core::geometry::Point::new(5.0, 5.0),
        ));
        app
    }

    fn object_app() -> App {
        let mut app = page_app();
        let object = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rectangle");
        app.select(vec![object]);
        app
    }

    fn drawn_rows(app: &mut App) -> Vec<String> {
        crate::ui::menus::ROWS.with(|rows| rows.borrow_mut().clear());
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1000.0),
                )),
                ..Default::default()
            },
            |ui| {
                let mut close = false;
                menu_items(app, ui, &mut close);
            },
        );
        output.textures_delta.clear();
        crate::ui::menus::ROWS
            .with(|rows| rows.borrow().iter().map(|(key, _)| key.clone()).collect())
    }

    fn has_row(rows: &[String], key: &str) -> bool {
        rows.iter().any(|row| row == key)
    }

    #[test]
    fn context_menu_order_moves_page_actions_and_preserves_hidden_rows() {
        let mut app = page_app();
        app.settings.context_menus.set_order(
            "canvas_page",
            "",
            &[
                "menu.tools.options_app",
                "menu.layout.rename_page",
                "menu.edit.paste",
            ],
        );
        app.settings
            .context_menus
            .set_visible("canvas_page", "menu.edit.paste", false);
        let rows = drawn_rows(&mut app);
        assert_eq!(
            rows.first().map(String::as_str),
            Some("menu.tools.options_app")
        );
        assert_eq!(
            rows.get(1).map(String::as_str),
            Some("menu.layout.rename_page")
        );
        assert!(!has_row(&rows, "menu.edit.paste"));
    }

    #[test]
    fn context_menu_order_click_uses_the_reordered_row_rectangle() {
        let mut app = page_app();
        app.settings.context_menus.set_order(
            "canvas_page",
            "",
            &["menu.layout.rename_page", "menu.edit.paste"],
        );
        let ctx = crate::theme::ui_context();
        let frame = |app: &mut App, events| {
            crate::ui::menus::ROWS.with(|rows| rows.borrow_mut().clear());
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| menu_items(app, ui, &mut false),
            );
            output.textures_delta.clear();
        };
        frame(&mut app, Vec::new());
        let position = crate::ui::menus::ROWS.with(|rows| {
            let rows = rows.borrow();
            let (key, rect) = rows.first().expect("first visible action");
            assert_eq!(key, "menu.layout.rename_page");
            rect.center()
        });
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(matches!(app.dialog, Dialog::RenamePage { .. }));
    }

    #[test]
    fn context_menu_order_merges_object_panel_extras_with_drawing_actions() {
        let mut app = object_app();
        app.context_docker = app.selection.first().copied();
        app.settings.context_menus.set_order(
            "objects_object",
            "",
            &["menu.edit.copy", "docker.move_to_layer", "docker.rename"],
        );
        let rows = drawn_rows(&mut app);
        assert_eq!(
            rows.iter().take(3).map(String::as_str).collect::<Vec<_>>(),
            ["menu.edit.copy", "docker.move_to_layer", "docker.rename"]
        );
    }

    #[test]
    fn hiding_a_page_row_preserves_the_other_page_commands() {
        let mut app = page_app();
        assert!(has_row(&drawn_rows(&mut app), "menu.layout.rename_page"));
        app.settings
            .context_menus
            .set_visible("canvas_page", "menu.layout.rename_page", false);
        let rows = drawn_rows(&mut app);
        assert!(!has_row(&rows, "menu.layout.rename_page"));
        assert!(has_row(&rows, "menu.layout.insert_page"));
        assert!(has_row(&rows, "menu.tools.options_app"));
    }

    #[test]
    fn object_menu_visibility_does_not_hide_the_same_node_menu_command() {
        let mut app = object_app();
        app.settings
            .context_menus
            .set_visible("canvas_object", "menu.object.properties", false);
        assert!(!has_row(&drawn_rows(&mut app), "menu.object.properties"));
        app.set_tool(Tool::Shape);
        assert!(has_row(&drawn_rows(&mut app), "menu.object.properties"));
    }

    #[test]
    fn objects_panel_and_canvas_keep_independent_object_menu_choices() {
        let mut app = object_app();
        app.settings
            .context_menus
            .set_visible("canvas_object", "menu.edit.duplicate", false);
        assert!(!has_row(&drawn_rows(&mut app), "menu.edit.duplicate"));
        app.context_docker = app.selection.first().copied();
        assert!(has_row(&drawn_rows(&mut app), "menu.edit.duplicate"));
        app.settings
            .context_menus
            .set_visible("objects_object", "menu.edit.duplicate", false);
        assert!(!has_row(&drawn_rows(&mut app), "menu.edit.duplicate"));
    }

    #[test]
    fn a_hidden_context_row_remains_available_as_a_command() {
        let mut app = object_app();
        app.settings
            .context_menus
            .set_visible("canvas_object", "menu.edit.duplicate", false);
        assert!(!has_row(&drawn_rows(&mut app), "menu.edit.duplicate"));
        let count = |app: &App| {
            app.doc()
                .pages
                .iter()
                .flat_map(|page| &page.layers)
                .map(|layer| layer.shapes.len())
                .sum::<usize>()
        };
        let before = count(&app);
        let ctx = crate::theme::ui_context();
        let mut initialized = ctx.run_ui(Default::default(), |_| ());
        initialized.textures_delta.clear();
        let _menu =
            crate::ui::context_menus::Scope::new(&app.settings.context_menus, "canvas_object");
        assert!(crate::ui::menus::run_command(
            &mut app,
            &ctx,
            "menu.edit.duplicate",
        ));
        assert!(count(&app) > before);
    }
}

#[cfg(test)]
mod hidden_canvas_popup_tests {
    use super::*;

    #[test]
    fn unavailable_conditional_actions_do_not_leave_empty_canvas_popups() {
        for (scene, remaining, tool, selected) in [
            (
                "canvas_page",
                "menu.object.clip_frame_finish",
                Tool::Pick,
                false,
            ),
            ("canvas_object", "menu.text.edit_text", Tool::Pick, true),
            ("canvas_node", "context.spline_fixed", Tool::Shape, true),
        ] {
            let context = crate::theme::ui_context();
            let mut app = App::headless();
            let object = app
                .new_shape(ShapeKind::Rect {
                    rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: 0.0,
                    corners: None,
                })
                .expect("rectangle");
            app.select(if selected { vec![object] } else { Vec::new() });
            app.set_tool(tool);
            let position = egui::pos2(30.0, 30.0);
            let point = tracedraw_core::Point::new(5.0, 5.0);
            let id = egui::Id::new("canvas_context_menu");
            let draw = |app: &mut App, empty: bool| {
                let mut output = context.run_ui(Default::default(), |ui| {
                    let response = ui.response();
                    context_menu(app, ui, &response);
                    if empty {
                        let layer = egui::LayerId::new(egui::Order::Foreground, id);
                        assert!(context.graphics(|graphics| {
                            graphics.get(layer).is_none_or(|paint| paint.is_empty())
                        }));
                    }
                });
                output.textures_delta.clear();
            };
            app.context_menu = Some((position, point));
            for _ in 0..2 {
                draw(&mut app, false);
                assert!(app.context_menu.is_some(), "default {scene} menu");
            }
            let menu = crate::ui::context_menus::CATALOG
                .iter()
                .find(|menu| menu.id == scene)
                .expect("menu");
            for key in menu.items {
                app.settings
                    .context_menus
                    .set_visible(scene, key, *key == remaining);
            }
            assert!(crate::ui::context_menus::has_visible_items(
                &app.settings.context_menus,
                scene,
            ));
            // Both the already open popup and a new right click close cleanly.
            for _ in 0..2 {
                app.context_menu = Some((position, point));
                draw(&mut app, true);
                assert!(app.context_menu.is_none(), "{scene}: {remaining}");
                assert!(!egui::Popup::is_id_open(&context, id));
                assert!(context
                    .data(|data| data.get_temp::<egui::Pos2>(id.with("pos")))
                    .is_none());
            }
        }
    }

    #[test]
    fn a_dynamic_style_parent_needs_real_document_styles_to_keep_the_root_open() {
        let mut app = App::headless();
        app.set_tool(Tool::Pick);
        let object = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("object");
        app.select(vec![object]);
        let definition = crate::ui::context_menus::CATALOG
            .iter()
            .find(|menu| menu.id == "canvas_object")
            .expect("object menu");
        for key in definition.items {
            app.settings
                .context_menus
                .set_visible(definition.id, key, false);
        }
        app.settings
            .context_menus
            .set_visible(definition.id, "docker.object_styles", true);
        assert!(app.doc().object_styles.is_empty());
        let context = crate::theme::ui_context();
        let position = egui::pos2(30.0, 30.0);
        let draw = |app: &mut App| {
            let mut output = context.run_ui(Default::default(), |ui| {
                let response = ui.response();
                context_menu(app, ui, &response);
            });
            output.textures_delta.clear();
        };
        app.context_menu = Some((position, tracedraw_core::Point::new(5.0, 5.0)));
        draw(&mut app);
        assert!(
            app.context_menu.is_none(),
            "the empty branch cannot leave a root popup"
        );
        app.new_style_from_selection();
        assert!(!app.doc().object_styles.is_empty());
        app.context_menu = Some((position, tracedraw_core::Point::new(5.0, 5.0)));
        draw(&mut app);
        assert!(
            app.context_menu.is_some(),
            "real style targets keep the menu"
        );
        assert!(egui::Popup::is_id_open(
            &context,
            egui::Id::new("canvas_context_menu")
        ));
    }

    #[test]
    fn hidden_canvas_menu_clears_existing_popup_and_click_position() {
        let mut app = App::headless();
        app.select(Vec::new());
        app.set_tool(Tool::Pick);
        let position = egui::pos2(30.0, 30.0);
        app.context_menu = Some((position, tracedraw_core::Point::new(5.0, 5.0)));
        let menu = crate::ui::context_menus::CATALOG
            .iter()
            .find(|menu| menu.id == "canvas_page")
            .expect("page menu");
        for key in menu.items {
            app.settings.context_menus.set_visible(menu.id, key, false);
        }
        let context = crate::theme::ui_context();
        let id = egui::Id::new("canvas_context_menu");
        egui::Popup::open_id(&context, id);
        context.data_mut(|data| data.insert_temp(id.with("pos"), position));
        let mut output = context.run_ui(Default::default(), |ui| {
            let response = ui.response();
            context_menu(&mut app, ui, &response);
        });
        output.textures_delta.clear();
        assert!(app.context_menu.is_none());
        assert!(app.context_locked.is_none());
        assert!(app.context_docker.is_none());
        assert!(app.tab_ruler_menu.is_none());
        assert!(!egui::Popup::is_id_open(&context, id));
        assert!(context
            .data(|data| data.get_temp::<egui::Pos2>(id.with("pos")))
            .is_none());
        app.settings.context_menus.reset_menu("canvas_page");
        assert!(!egui::Popup::is_id_open(&context, id));
    }
}

#[cfg(test)]
mod select_same_tests {
    use super::*;

    fn selected_object() -> App {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(10.0, 10.0, 30.0, 30.0),
                radius: 0.0,
                corners: None,
            })
            .expect("reference rectangle");
        app.select(vec![id]);
        app
    }

    #[test]
    fn object_context_does_not_show_select_same() {
        crate::i18n::with_thread_language("en", || {
            let ctx = crate::theme::ui_context();
            let mut app = selected_object();
            let selection = app.selection.clone();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let mut close = false;
                    object_menu(&mut app, ui, &mut close);
                    assert!(!close);
                },
            );
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect();
            assert!(!labels.is_empty());
            assert!(!labels
                .iter()
                .any(|label| label.contains(&tr("docker.select_same"))));
            assert_eq!(app.selection, selection);
            assert!(!app.docker_shown(DockerTab::SelectSame));
            output.textures_delta.clear();
        });
    }
}

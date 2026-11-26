//! Stable operation identifiers for independently configurable context menus.
//!
//! Keep target names out of this directory: a dynamic list is one operation,
//! regardless of its current files, layers, palettes or collections.

pub(crate) use super::context_menu_filter::{show_item, Preferences, Scope};

/// Skip a known menu only when all of its configured operations are hidden.
/// New and unknown locations keep their default behavior, as do command walks.
pub(crate) fn has_visible_items(preferences: &Preferences, scene: &str) -> bool {
    has_visible_items_with(preferences, scene, &[])
}

/// Includes only dynamic branches whose callers report actual content.
pub(crate) fn has_visible_items_with(
    preferences: &Preferences,
    scene: &str,
    dynamic: &[(&str, bool)],
) -> bool {
    if !preferences.has_overrides(scene) || super::menus::invoke::active() {
        return true;
    }
    CATALOG
        .iter()
        .find(|menu| menu.id == scene)
        .is_none_or(|menu| {
            menu.items.iter().any(|item| {
                super::context_menu_branches::is_root(scene, item)
                    && super::context_menu_branches::root_has_visible_action(
                        scene,
                        item,
                        preferences,
                        dynamic,
                    )
            })
        })
}

/// Filters before showing the popup and clears any previously open instance.
/// The returned popup does not retain a borrow of the preferences.
pub(crate) fn filter_popup<'a>(
    popup: egui::Popup<'a>,
    preferences: &Preferences,
    scene: &str,
) -> FilteredPopup<'a> {
    filter_popup_with(popup, preferences, scene, &[])
}

/// Dynamic presence is queried before drawing and never retains an app borrow.
pub(crate) fn filter_popup_with<'a>(
    popup: egui::Popup<'a>,
    preferences: &Preferences,
    scene: &str,
    dynamic: &[(&str, bool)],
) -> FilteredPopup<'a> {
    let popup = if has_visible_items_with(preferences, scene, dynamic) {
        popup
    } else {
        egui::Popup::close_id(popup.ctx(), popup.get_id());
        popup.open(false)
    };
    FilteredPopup {
        popup,
        check_content: preferences.has_overrides(scene) && !super::menus::invoke::active(),
    }
}

/// The catalog covers every document state; only actual rows can keep a
/// customized popup open when its remaining operations are conditional.
pub(crate) struct FilteredPopup<'a> {
    popup: egui::Popup<'a>,
    check_content: bool,
}

impl FilteredPopup<'_> {
    pub(crate) fn show<R>(
        self,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<egui::InnerResponse<R>> {
        let Self {
            popup,
            check_content,
        } = self;
        if !check_content {
            return popup.show(add);
        }
        let id = popup.get_id();
        let mut empty = false;
        let shown = popup.show(|ui| {
            let content = super::context_menu_filter::PopupContent::new();
            let result = add(ui);
            empty = !content.has_item();
            if empty {
                ui.close();
            }
            result
        });
        if empty {
            if let Some(response) = &shown {
                let context = &response.response.ctx;
                egui::Popup::close_id(context, id);
                // The popup owns this layer. Remove its frame and menu gutter
                // too, so a newly empty menu does not flash for one frame.
                context.graphics_mut(|graphics| {
                    if let Some(paint) = graphics.get_mut(response.response.layer_id) {
                        *paint = Default::default();
                    }
                });
            }
            None
        } else {
            shown
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct MenuDefinition {
    pub id: &'static str,
    pub key: &'static str,
    pub items: &'static [&'static str],
}

macro_rules! object_items {
    ($($extra:literal),* $(,)?) => {
        &[
            "menu.table",
            "menu.text.edit_text",
            "menu.text.convert",
            "menu.text.change_case",
            "menu.text.insert_placeholder_text",
            "menu.text.join_lines",
            "menu.text.link_content",
            "menu.text.unlink_content",
            "menu.table.edit_in_spreadsheet",
            "menu.bitmaps.edit_bitmap",
            "menu.bitmaps.replace_bitmap",
            "menu.bitmaps.remove_background_local",
            "menu.bitmaps.remove_background",
            "menu.bitmaps.ai_vectorize",
            "menu.bitmaps.save_bitmap_as",
            "menu.bitmaps.trace",
            "menu.bitmaps.quick_trace",
            "menu.bitmaps.repeat_last_trace",
            "menu.bitmaps.last_trace_settings",
            "menu.bitmaps.centerline_trace",
            "menu.bitmaps.outline_trace",
            "menu.bitmaps.trace_technical",
            "menu.bitmaps.trace_line_drawing",
            "menu.bitmaps.trace_line_art",
            "menu.bitmaps.trace_logo",
            "menu.bitmaps.trace_detailed_logo",
            "menu.bitmaps.trace_clipart",
            "menu.bitmaps.trace_low_quality",
            "menu.bitmaps.trace_high_quality",
            "menu.bitmaps.trace_optimized",
            "menu.bitmaps.crop_bitmap",
            "menu.bitmaps.scale_known",
            "context.ai_remix",
            "context.select_master",
            "context.revert_to_master",
            "context.unlink_clone",
            "context.select_clones",
            "context.replace",
            "context.replace_with_object",
            "context.replace_with_clones",
            "context.bring_into_focus",
            "menu.object.convert_to_curves",
            "menu.object.convert_outline_to_object",
            "menu.object.convert_to_guidelines",
            "menu.object.break_apart",
            "menu.object.break_contour_apart",
            "menu.object.break_blend_apart",
            "menu.object.break_extrude_apart",
            "menu.object.break_shadows_apart",
            "menu.object.break_shadow_apart",
            "menu.object.combine",
            "context.edit_shadow",
            "menu.edit.cut",
            "menu.edit.copy",
            "menu.edit.paste",
            "menu.edit.duplicate",
            "menu.edit.delete",
            "menu.object.add_to_shelf",
            "menu.object.add_common_shape",
            "menu.edit.select_similar",
            "menu.edit.similar_fill",
            "menu.edit.similar_outline",
            "menu.edit.similar_text_format",
            "menu.edit.similar_kind",
            "menu.edit.similar_size",
            "menu.edit.similar_look",
            "menu.object.order",
            "menu.object.to_front_of_page",
            "menu.object.to_back_of_page",
            "menu.object.forward_one",
            "menu.object.back_one",
            "menu.object.in_front_of",
            "menu.object.behind",
            "menu.object.reverse_order",
            "menu.object.group_group",
            "menu.object.group_ungroup",
            "menu.object.group_ungroup_all",
            "menu.object.hide_object",
            "menu.object.unlock_object",
            "menu.object.lock_object",
            "menu.object.clip_frame_place_inside",
            "menu.object.clip_frame_edit",
            "menu.object.clip_frame_extract",
            "menu.object.clip_frame_finish",
            "context.frame_type",
            "context.frame_none",
            "context.frame_empty_clip_frame",
            "context.frame_text",
            "menu.object.symmetry_create",
            "context.create_symbol",
            "menu.object.add_dimensions",
            "menu.object.add_dimensions_each",
            "context.dimension_label_text",
            "menu.object.symbol_edit",
            "context.internet_links",
            "docker.hyperlink",
            "docker.object_styles",
            "docker.new_style_from_selection",
            "docker.break_style_link",
            "docker.open_object_styles",
            "docker.color_styles",
            "docker.new_from_selection",
            "context.wrap_paragraph_text",
            "context.wrap_text_on",
            "context.wrap_contour_left",
            "context.wrap_contour_right",
            "context.wrap_contour_straddle",
            "context.wrap_square_left",
            "context.wrap_square_right",
            "context.wrap_square_straddle",
            "context.wrap_square_above_below",
            "context.wrap_offset",
            "menu.object.overprint_fill",
            "menu.object.overprint_outline",
            "menu.object.object_hinting",
            "context.align_pixel_grid",
            "menu.object.properties",
            $($extra),*
        ]
    };
}

macro_rules! palette_items {
    ($($extra:literal),* $(,)?) => {
        &[
            "palette.size",
            "palette.size_xs",
            "palette.size_s",
            "palette.size_m",
            "palette.size_l",
            "palette.size_xl",
            "palette.rows",
            "palette.rows_1",
            "palette.rows_2",
            "palette.rows_3",
            "palette.rows_4",
            "palette.rows_5",
            "palette.rows_6",
            "palette.rows_7",
            "palette.rows_8",
            "palette.rows_fit",
            "palette.position",
            "bars.top",
            "bars.bottom",
            "bars.left",
            "bars.right",
            "palette.float",
            "palette.among_toolbars",
            $($extra),*
        ]
    };
}

pub(crate) const CATALOG: &[MenuDefinition] = &[
    MenuDefinition {
        id: "canvas_object",
        key: "context_menu.canvas_object",
        items: object_items!(),
    },
    MenuDefinition {
        id: "canvas_page",
        key: "context_menu.canvas_page",
        items: &[
            "menu.object.clip_frame_finish",
            "menu.edit.paste",
            "menu.edit.paste_at_pointer",
            "menu.edit.select_all_objects",
            "menu.file.import",
            "menu.layout.insert_page",
            "menu.layout.rename_page",
            "menu.layout.page_size",
            "menu.layout.page_background",
            "menu.view.zoom_to_page",
            "menu.view.zoom_to_fit",
            "menu.tools.options_app",
        ],
    },
    MenuDefinition {
        id: "canvas_node",
        key: "context_menu.canvas_node",
        items: &[
            "context.spline_fixed",
            "context.spline_floating",
            "context.node_add",
            "context.node_delete",
            "context.node_join",
            "context.node_break",
            "context.node_to_line",
            "context.node_to_curve",
            "context.node_cusp",
            "context.node_smooth",
            "context.node_symmetrical",
            "context.node_reverse",
            "context.node_extract",
            "context.node_close",
            "context.elastic_mode",
            "menu.object.properties",
        ],
    },
    MenuDefinition {
        id: "canvas_text",
        key: "context_menu.canvas_text",
        items: &[
            "menu.edit.undo",
            "menu.edit.cut",
            "menu.edit.copy",
            "menu.edit.paste",
            "menu.edit.delete",
            "menu.edit.select_all",
            "menu.text.change_case",
            "menu.table",
            "menu.text.insert_placeholder_text",
            "menu.object.properties",
        ],
    },
    MenuDefinition {
        id: "canvas_locked",
        key: "context_menu.canvas_locked",
        items: &["menu.object.unlock_object", "menu.object.unlock_all"],
    },
    MenuDefinition {
        id: "canvas_guide",
        key: "context_menu.canvas_guide",
        items: &[
            "menu.edit.undo",
            "menu.edit.delete",
            "guides.unlock",
            "guides.lock",
            "menu.object.properties",
        ],
    },
    MenuDefinition {
        id: "tab_ruler",
        key: "context_menu.tab_ruler",
        items: &[
            "text.tab_left",
            "text.tab_center",
            "text.tab_right",
            "text.tab_decimal",
            "dialog.remove_tab",
        ],
    },
    MenuDefinition {
        id: "rulers",
        key: "context_menu.rulers",
        items: &[
            "rulers.ruler_setup",
            "rulers.grid_setup",
            "rulers.guidelines_setup",
            "menu.view.rulers",
        ],
    },
    MenuDefinition {
        id: "toolbar",
        key: "context_menu.toolbar",
        items: &[
            "bars.button_properties",
            "bars.top",
            "bars.palette_top_edge",
            "bars.bottom",
            "bars.palette_bottom_edge",
            "bars.left",
            "bars.palette_left_between",
            "bars.palette_left_beside",
            "bars.right",
            "bars.palette_right_between",
            "bars.palette_right_beside",
            "bars.floating",
            "menu.window.toolbar_menu_bar",
            "menu.window.toolbar_standard",
            "menu.window.toolbar_property_bar",
            "menu.window.toolbar_toolbox",
            "bars.palette",
            "bars.ai_prompt",
            "menu.window.toolbar_status_bar",
            "menu.window.toolbar_text",
            "menu.window.toolbar_zoom",
            "menu.window.toolbar_transform",
            "menu.window.toolbar_time_tracker",
            "bars.customize",
            "bars.default_button_size",
            "bars.size_extra_small",
            "bars.size_small",
            "bars.size_medium",
            "bars.size_large",
            "bars.size_extra_large",
            "bars.toolbox_toolbar",
            "bars.toolbox_reset",
            "bars.button_size",
            "bars.size_default",
            "bars.image_only",
            "bars.caption_only",
            "bars.caption_right",
            "menu.window.workspace",
            "menu.window.workspace_export",
            "menu.window.workspace_import",
            "bars.customize_options",
            "bars.lock",
            "bars.reset",
        ],
    },
    MenuDefinition {
        id: "docker_tabs",
        key: "context_menu.docker_tabs",
        items: DOCKER_ITEMS,
    },
    MenuDefinition {
        id: "docker_title",
        key: "context_menu.docker_title",
        items: DOCKER_ITEMS,
    },
    MenuDefinition {
        id: "page_tabs",
        key: "context_menu.page_tabs",
        items: &[
            "menu.layout.rename_page",
            "page_tab.insert_after",
            "page_tab.insert_before",
            "menu.layout.duplicate_page",
            "page_tab.duplicate_to_new_document",
            "page_tab.delete",
            "menu.layout.switch_orientation",
        ],
    },
    MenuDefinition {
        id: "objects_page",
        key: "context_menu.objects_page",
        items: &["docker.go_to_page", "docker.rename"],
    },
    MenuDefinition {
        id: "objects_guide",
        key: "context_menu.objects_guide",
        items: &[
            "menu.edit.delete",
            "menu.edit.cut",
            "menu.edit.copy",
            "menu.edit.paste",
            "guides.unlock",
            "guides.lock",
            "guides.show",
            "menu.object.hide",
            "docker.rename",
        ],
    },
    MenuDefinition {
        id: "objects_layer",
        key: "context_menu.objects_layer",
        items: &[
            "docker.rename",
            "docker.new_master_layer_short",
            "docker.new_range_layer",
            "docker.new_layer",
            "docker.select_layer_objects",
            "menu.edit.cut",
            "menu.edit.copy",
            "menu.edit.paste",
            "docker.show_hide",
            "docker.print_layer",
            "docker.lock_layer",
            "docker.edit_across_layers",
            "docker.move_up",
            "docker.move_down",
            "docker.show_on",
            "options.scope_all",
            "options.scope_odd",
            "options.scope_even",
            "docker.range_pages",
            "docker.master_in_front",
            "docker.desktop_takes_beside_page",
            "docker.master_to_page",
            "docker.layer_to_master",
            "docker.move_to_page",
            "docker.layer_properties",
            "docker.delete_layer",
        ],
    },
    MenuDefinition {
        id: "objects_object",
        key: "context_menu.objects_object",
        items: object_items!(
            "docker.rename",
            "docker.move_to_layer",
            "docker.copy_to_layer"
        ),
    },
    MenuDefinition {
        id: "objects_new_master",
        key: "context_menu.objects_new_master",
        items: &[
            "options.new_master_all",
            "options.new_master_odd",
            "options.new_master_even",
            "docker.new_range_layer",
            "docker.master_in_front",
            "options.scope_all",
            "options.scope_odd",
            "options.scope_even",
        ],
    },
    MenuDefinition {
        id: "palette",
        key: "context_menu.palette",
        items: palette_items!(
            "menu.window.palette_open",
            "palette.edit",
            "menu.window.palette_manager",
            "palette.show_document",
            "palette.show_names",
            "palette.open_bar",
            "palette.open_as_bar",
            "palette.new",
            "palette.fit_selection",
            "palette.add_selection_to_palette",
            "palette.add_from_bitmap",
            "palette.rename",
            "palette.close",
        ),
    },
    MenuDefinition {
        id: "palette_swatch",
        key: "context_menu.palette_swatch",
        items: palette_items!(
            "menu.window.palette_open",
            "palette.edit",
            "menu.window.palette_manager",
            "palette.show_document",
            "palette.show_names",
            "palette.open_bar",
            "palette.open_as_bar",
            "palette.new",
            "palette.fit_selection",
            "palette.add_selection_to_palette",
            "palette.add_from_bitmap",
            "palette.rename",
            "palette.close",
            "palette.delete_color",
        ),
    },
    MenuDefinition {
        id: "palette_manager",
        key: "context_menu.palette_manager",
        items: &[
            "palette.open_as_bar",
            "palette.close_bar",
            "docker.edit",
            "palette.rename",
            "palette.reset_name",
            "palette.duplicate",
            "docker.palette_unlock",
            "docker.palette_lock",
            "palette.delete",
            "palette.move_up",
            "palette.move_down",
        ],
    },
    MenuDefinition {
        id: "palette_document",
        key: "context_menu.palette_document",
        items: palette_items!(
            "palette.edit",
            "palette.auto_update",
            "palette.add_from_selection",
            "palette.add_from_document",
            "palette.add_from_bitmap",
            "palette.delete_color",
            "palette.palette",
            "palette.show_names",
            "palette.reset",
            "palette.rename",
            "palette.open_bar",
            "palette.open_as_bar",
            "palette.new",
            "palette.close",
        ),
    },
    MenuDefinition {
        id: "palette_document_swatch",
        key: "context_menu.palette_document_swatch",
        items: palette_items!(
            "palette.edit",
            "palette.auto_update",
            "palette.add_from_selection",
            "palette.add_from_document",
            "palette.add_from_bitmap",
            "palette.delete_color",
            "palette.palette",
            "palette.show_names",
            "palette.reset",
            "palette.rename",
            "palette.open_bar",
            "palette.open_as_bar",
            "palette.new",
            "palette.close",
        ),
    },
    MenuDefinition {
        id: "palette_document_manager",
        key: "context_menu.palette_document_manager",
        items: &[
            "palette.open_as_bar",
            "palette.close_bar",
            "palette.rename",
            "palette.reset_name",
        ],
    },
    MenuDefinition {
        id: "font_folder",
        key: "context_menu.font_folder",
        items: &["font_manager.remove_folder"],
    },
    MenuDefinition {
        id: "font_collection",
        key: "context_menu.font_collection",
        items: &["font_manager.rename", "font_manager.delete_collection"],
    },
    MenuDefinition {
        id: "font",
        key: "context_menu.font",
        items: &[
            "font_manager.install",
            "font_manager.uninstall",
            "font_manager.use_in_drawing",
            "font_manager.remove_favorite",
            "font_manager.add_favorite",
            "font_manager.add_to_collection",
            "font_manager.remove_from_collection",
            "font_manager.show_details",
        ],
    },
    MenuDefinition {
        id: "save_bookmark",
        key: "context_menu.save_bookmark",
        items: &["dialog.remove_bookmark"],
    },
    MenuDefinition {
        id: "save_entry",
        key: "context_menu.save_entry",
        items: &[
            "dialog.open_entry",
            "dialog.rename_entry",
            "dialog.add_bookmark",
        ],
    },
    MenuDefinition {
        id: "shelf",
        key: "context_menu.shelf",
        items: &["shelf.place", "shelf.rename", "shelf.move_to_folder", "shelf.no_folder", "shelf.remove"],
    },
    MenuDefinition {
        id: "my_shapes",
        key: "context_menu.my_shapes",
        items: &["shapes.mine_rename", "shapes.mine_remove"],
    },
    MenuDefinition {
        id: "recent_file",
        key: "context_menu.recent_file",
        items: &["welcome.remove_recent"],
    },
    MenuDefinition {
        id: "drop",
        key: "context_menu.drop",
        items: &[
            "drop.move_here",
            "drop.copy_here",
            "drop.copy_fill_here",
            "drop.copy_outline_here",
            "drop.copy_all_properties",
            "drop.place_inside",
            "drop.cancel",
        ],
    },
];

const DOCKER_ITEMS: &[&str] = &[
    "bars.right",
    "docker.right_inner",
    "bars.left",
    "docker.left_inner",
    "bars.floating",
    "docker.stack_below",
    "docker.lock_docker",
    "docker.close_docker",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn context_menu_catalog_has_unique_locations_and_operations() {
        let mut locations = BTreeSet::new();
        for menu in CATALOG {
            assert!(locations.insert(menu.id), "duplicate location: {}", menu.id);
            assert_eq!(menu.key, format!("context_menu.{}", menu.id));
            assert!(!menu.items.is_empty(), "empty location: {}", menu.id);
            let mut operations = BTreeSet::new();
            for key in menu.items {
                assert!(
                    operations.insert(key),
                    "duplicate operation {key} in {}",
                    menu.id
                );
                assert!(
                    key.contains('.'),
                    "operation must be a stable translation key: {key}"
                );
            }
        }
        assert_eq!(locations.len(), 32);
    }

    #[test]
    fn objects_panel_reuses_canvas_operations_with_its_own_location() {
        let canvas = CATALOG
            .iter()
            .find(|menu| menu.id == "canvas_object")
            .expect("canvas menu");
        let panel = CATALOG
            .iter()
            .find(|menu| menu.id == "objects_object")
            .expect("object panel menu");
        assert!(canvas.items.iter().all(|key| panel.items.contains(key)));
        assert_eq!(panel.items.len(), canvas.items.len() + 3);
        assert!(panel.items.contains(&"docker.move_to_layer"));
        assert!(panel.items.contains(&"docker.copy_to_layer"));
    }

    #[test]
    fn catalog_operations_have_translations_in_every_language() {
        for (language, source) in [
            ("en", include_str!("../../lang/en.toml")),
            ("zh-CN", include_str!("../../lang/zh-CN.toml")),
            ("hi", include_str!("../../lang/hi.toml")),
            ("es", include_str!("../../lang/es.toml")),
            ("fr", include_str!("../../lang/fr.toml")),
            ("ar", include_str!("../../lang/ar.toml")),
            ("bn", include_str!("../../lang/bn.toml")),
            ("ru", include_str!("../../lang/ru.toml")),
            ("pt-BR", include_str!("../../lang/pt-BR.toml")),
            ("id", include_str!("../../lang/id.toml")),
            ("de", include_str!("../../lang/de.toml")),
            ("ja", include_str!("../../lang/ja.toml")),
            ("zh-TW", include_str!("../../lang/zh-TW.toml")),
            ("it", include_str!("../../lang/it.toml")),
            ("nl", include_str!("../../lang/nl.toml")),
            ("pl", include_str!("../../lang/pl.toml")),
            ("tr", include_str!("../../lang/tr.toml")),
            ("cs", include_str!("../../lang/cs.toml")),
            ("lo", include_str!("../../lang/lo.toml")),
        ] {
            let translations = crate::i18n::parse(source);
            for key in [
                "options.context_menus",
                "options.context_menu_intro",
                "options.context_menu_scene",
                "options.context_menu_search",
                "options.context_menu_no_results",
                "options.context_menu_reset",
                "options.context_menu_reset_all",
            ] {
                assert!(translations.contains_key(key), "{language}: missing {key}");
            }
            for menu in CATALOG {
                assert!(
                    translations.contains_key(menu.key),
                    "{language}: missing {}",
                    menu.key
                );
                for key in menu.items {
                    assert!(translations.contains_key(*key), "{language}: missing {key}");
                }
            }
        }
    }

    #[test]
    fn dynamic_targets_are_configured_by_operation() {
        for (location, key) in [
            ("font", "font_manager.add_to_collection"),
            ("objects_layer", "docker.move_to_page"),
            ("objects_object", "docker.move_to_layer"),
            ("objects_object", "docker.copy_to_layer"),
            ("palette", "palette.open_as_bar"),
            ("palette_swatch", "palette.add_from_bitmap"),
        ] {
            let menu = CATALOG
                .iter()
                .find(|menu| menu.id == location)
                .expect("location");
            assert!(menu.items.contains(&key), "missing dynamic operation {key}");
        }
    }
}

#[cfg(test)]
mod empty_root_tests {
    use super::*;

    fn keep_only(scene: &str, visible: &str) -> Preferences {
        let menu = CATALOG.iter().find(|menu| menu.id == scene).expect("menu");
        let mut preferences = Preferences::default();
        for key in menu.items {
            preferences.set_visible(scene, key, *key == visible);
        }
        preferences
    }

    #[test]
    fn conditional_layer_rows_close_empty_popups_without_running_actions_twice() {
        let context = crate::theme::ui_context();
        let id = egui::Id::new("conditional_layer_popup");
        let preferences = keep_only("objects_layer", "docker.new_master_layer_short");
        for master in [false, true, false] {
            let mut calls = 0;
            let mut output = context.run_ui(Default::default(), |ui| {
                let popup = egui::Popup::new(
                    id,
                    context.clone(),
                    egui::PopupAnchor::Position(egui::pos2(20.0, 20.0)),
                    ui.layer_id(),
                )
                .open_memory(egui::SetOpenCommand::Bool(true));
                let shown = filter_popup(popup, &preferences, "objects_layer")
                    .show(|ui| {
                        calls += 1;
                        let _scope = Scope::new(&preferences, "objects_layer");
                        let key = if master {
                            "docker.new_master_layer_short"
                        } else {
                            "docker.new_layer"
                        };
                        if show_item(ui, key) {
                            let _ = ui.button("New layer");
                        }
                    })
                    .is_some();
                assert_eq!(shown, master);
                assert_eq!(egui::Popup::is_id_open(&context, id), master);
                if !master {
                    let layer = egui::LayerId::new(egui::Order::Foreground, id);
                    assert!(context.graphics(|graphics| {
                        graphics.get(layer).is_none_or(|paint| paint.is_empty())
                    }));
                }
            });
            output.textures_delta.clear();
            assert_eq!(calls, 1, "the menu body runs once per frame");
        }
    }

    #[test]
    fn visible_disabled_and_dynamic_rows_keep_customized_popups_open() {
        let context = crate::theme::ui_context();
        let preferences = keep_only("objects_object", "docker.move_to_layer");
        for enabled in [false, true] {
            let id = egui::Id::new(("visible_dynamic_popup", enabled));
            let mut output = context.run_ui(Default::default(), |ui| {
                let popup = egui::Popup::new(
                    id,
                    context.clone(),
                    egui::PopupAnchor::Position(egui::pos2(20.0, 20.0)),
                    ui.layer_id(),
                )
                .open_memory(egui::SetOpenCommand::Bool(true));
                assert!(filter_popup(popup, &preferences, "objects_object")
                    .show(|ui| {
                        let _scope = Scope::new(&preferences, "objects_object");
                        assert!(!super::super::menus::item_label_keyed(
                            ui,
                            "docker.move_to_layer",
                            "Destination layer",
                            "",
                            enabled,
                        ));
                    })
                    .is_some());
                assert!(egui::Popup::is_id_open(&context, id));
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn an_empty_nested_popup_does_not_erase_its_visible_parent_layer() {
        let context = crate::theme::ui_context();
        let outer_id = egui::Id::new("visible_outer_popup");
        let inner_id = egui::Id::new("empty_inner_popup");
        let preferences = keep_only("objects_layer", "docker.new_master_layer_short");
        for _ in 0..2 {
            let mut output = context.run_ui(Default::default(), |ui| {
                let popup = egui::Popup::new(
                    outer_id,
                    context.clone(),
                    egui::PopupAnchor::Position(egui::pos2(20.0, 20.0)),
                    ui.layer_id(),
                );
                assert!(filter_popup(popup, &preferences, "objects_layer")
                    .show(|ui| {
                        let _scope = Scope::new(&preferences, "objects_layer");
                        assert!(show_item(ui, "docker.new_master_layer_short"));
                        ui.label("Visible parent");
                        let nested = egui::Popup::new(
                            inner_id,
                            context.clone(),
                            egui::PopupAnchor::Position(egui::pos2(240.0, 20.0)),
                            ui.layer_id(),
                        );
                        assert!(filter_popup(nested, &preferences, "objects_layer")
                            .show(|ui| {
                                let _scope = Scope::new(&preferences, "objects_layer");
                                assert!(!show_item(ui, "docker.new_layer"));
                            })
                            .is_none());
                    })
                    .is_some());
                let outer = egui::LayerId::new(egui::Order::Foreground, outer_id);
                let inner = egui::LayerId::new(egui::Order::Foreground, inner_id);
                assert!(context.graphics(|graphics| {
                    graphics.get(outer).is_some_and(|paint| !paint.is_empty())
                        && graphics.get(inner).is_none_or(|paint| paint.is_empty())
                }));
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn every_catalog_menu_can_be_hidden_and_restored_independently() {
        let mut preferences = Preferences::default();
        for menu in CATALOG {
            assert!(has_visible_items(&preferences, menu.id));
            for key in menu.items {
                preferences.set_visible(menu.id, key, false);
            }
            assert!(!has_visible_items(&preferences, menu.id), "{}", menu.id);
            let key = match menu.id {
                "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch" => {
                    "palette.edit"
                }
                _ => menu.items.first().copied().expect("operation"),
            };
            preferences.set_visible(menu.id, key, true);
            assert!(has_visible_items(&preferences, menu.id), "{}", menu.id);
            preferences.reset_menu(menu.id);
        }
        preferences.set_visible("future_menu", "future.operation", false);
        assert!(has_visible_items(&preferences, "future_menu"));
        assert!(has_visible_items(&preferences, "recent_file"));
    }

    #[test]
    fn a_fully_hidden_popup_never_runs_its_body_and_clears_open_memory() {
        let context = crate::theme::ui_context();
        let id = egui::Id::new("hidden_recent_popup");
        let mut preferences = Preferences::default();
        preferences.set_visible("recent_file", "welcome.remove_recent", false);
        egui::Popup::open_id(&context, id);
        let mut output = context.run_ui(Default::default(), |ui| {
            let popup = egui::Popup::new(
                id,
                context.clone(),
                egui::PopupAnchor::Position(egui::pos2(20.0, 20.0)),
                ui.layer_id(),
            )
            .open_memory(None);
            assert!(filter_popup(popup, &preferences, "recent_file")
                .show(|_| panic!("hidden popup body ran"))
                .is_none());
            assert!(!egui::Popup::is_id_open(&context, id));
        });
        output.textures_delta.clear();
        preferences.reset_menu("recent_file");
        assert!(!egui::Popup::is_id_open(&context, id));
    }

    #[test]
    fn a_pending_open_request_cannot_bypass_hidden_menu_preferences() {
        let context = crate::theme::ui_context();
        let id = egui::Id::new("pending_recent_popup");
        let mut preferences = Preferences::default();
        for visible in [false, true, false] {
            preferences.set_visible("recent_file", "welcome.remove_recent", visible);
            let mut output = context.run_ui(Default::default(), |ui| {
                let popup = egui::Popup::new(
                    id,
                    context.clone(),
                    egui::PopupAnchor::Position(egui::pos2(20.0, 20.0)),
                    ui.layer_id(),
                )
                .open_memory(egui::SetOpenCommand::Bool(true));
                let mut body_ran = false;
                let shown = filter_popup(popup, &preferences, "recent_file")
                    .show(|ui| {
                        body_ran = true;
                        ui.label("Recent file");
                    })
                    .is_some();
                assert_eq!(shown, visible);
                assert_eq!(body_ran, visible);
                assert_eq!(egui::Popup::is_id_open(&context, id), visible);
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn checked_descendants_cannot_reopen_a_menu_with_every_root_hidden() {
        for scene in [
            "canvas_object",
            "objects_object",
            "toolbar",
            "palette",
            "palette_swatch",
            "palette_document",
            "objects_layer",
            "objects_new_master",
        ] {
            let mut preferences = Preferences::default();
            let menu = CATALOG.iter().find(|menu| menu.id == scene).expect("menu");
            for key in menu.items {
                if super::super::context_menu_branches::is_root(scene, key) {
                    preferences.set_visible(scene, key, false);
                }
            }
            assert!(!has_visible_items(&preferences, scene), "{scene}");
            if let Some(child) = menu
                .items
                .iter()
                .find(|key| !super::super::context_menu_branches::is_root(scene, key))
            {
                assert!(
                    preferences.is_visible(scene, child),
                    "descendant stays checked"
                );
            }
        }
    }

    #[test]
    fn dynamic_rows_only_keep_a_root_when_the_caller_reports_real_content() {
        for (scene, parent) in [
            ("canvas_object", "docker.object_styles"),
            ("objects_object", "docker.object_styles"),
            ("palette", "palette.size"),
            ("palette_swatch", "palette.position"),
        ] {
            let mut preferences = Preferences::default();
            let menu = CATALOG.iter().find(|menu| menu.id == scene).expect("menu");
            for key in menu.items {
                preferences.set_visible(scene, key, false);
            }
            preferences.set_visible(scene, parent, true);
            assert!(!has_visible_items_with(
                &preferences,
                scene,
                &[(parent, false)]
            ));
            assert!(has_visible_items_with(
                &preferences,
                scene,
                &[(parent, true)]
            ));
            preferences.set_visible(scene, parent, false);
            assert!(!has_visible_items_with(
                &preferences,
                scene,
                &[(parent, true)]
            ));
        }
    }
}

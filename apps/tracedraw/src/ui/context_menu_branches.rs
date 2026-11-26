//! Context-menu hierarchy shared by the editor and runtime visibility checks.
//!
//! This table only answers whether a branch has a visible operation. It never
//! runs a menu closure, so dynamic rows can opt in explicitly at the call site.

use std::collections::BTreeSet;

pub(crate) fn children(scene: &str, parent: &str) -> &'static [&'static str] {
    match (scene, parent) {
        ("canvas_object" | "objects_object", "menu.bitmaps.trace") => &[
            "menu.bitmaps.quick_trace",
            "menu.bitmaps.repeat_last_trace",
            "menu.bitmaps.last_trace_settings",
            "menu.bitmaps.centerline_trace",
            "menu.bitmaps.outline_trace",
        ],
        ("canvas_object" | "objects_object", "menu.bitmaps.centerline_trace") => &[
            "menu.bitmaps.trace_technical",
            "menu.bitmaps.trace_line_drawing",
        ],
        ("canvas_object" | "objects_object", "menu.bitmaps.outline_trace") => &[
            "menu.bitmaps.trace_line_art",
            "menu.bitmaps.trace_logo",
            "menu.bitmaps.trace_detailed_logo",
            "menu.bitmaps.trace_clipart",
            "menu.bitmaps.trace_low_quality",
            "menu.bitmaps.trace_high_quality",
            "menu.bitmaps.trace_optimized",
        ],
        ("canvas_object" | "objects_object", "menu.edit.select_similar") => &[
            "menu.edit.similar_fill",
            "menu.edit.similar_outline",
            "menu.edit.similar_text_format",
            "menu.edit.similar_kind",
            "menu.edit.similar_size",
            "menu.edit.similar_look",
        ],
        ("canvas_object" | "objects_object", "menu.object.order") => &[
            "menu.object.to_front_of_page",
            "menu.object.to_back_of_page",
            "menu.object.forward_one",
            "menu.object.back_one",
            "menu.object.in_front_of",
            "menu.object.behind",
            "menu.object.reverse_order",
        ],
        ("canvas_object" | "objects_object", "context.frame_type") => &[
            "context.frame_none",
            "context.frame_empty_clip_frame",
            "context.frame_text",
        ],
        ("canvas_object" | "objects_object", "context.internet_links") => &["docker.hyperlink"],
        ("canvas_object" | "objects_object", "docker.object_styles") => &[
            "docker.new_style_from_selection",
            "docker.break_style_link",
            "docker.open_object_styles",
        ],
        ("canvas_object" | "objects_object", "docker.color_styles") => {
            &["docker.new_from_selection"]
        }
        ("canvas_object" | "objects_object", "context.wrap_paragraph_text") => &[
            "context.wrap_text_on",
            "context.wrap_contour_left",
            "context.wrap_contour_right",
            "context.wrap_contour_straddle",
            "context.wrap_square_left",
            "context.wrap_square_right",
            "context.wrap_square_straddle",
            "context.wrap_square_above_below",
            "context.wrap_offset",
        ],
        ("toolbar", "bars.customize") => &[
            "bars.default_button_size",
            "bars.toolbox_toolbar",
            "menu.window.workspace",
            "bars.customize_options",
        ],
        ("toolbar", "bars.default_button_size") => &[
            "bars.size_extra_small",
            "bars.size_small",
            "bars.size_medium",
            "bars.size_large",
            "bars.size_extra_large",
        ],
        ("toolbar", "bars.toolbox_toolbar") => &[
            "bars.toolbox_reset",
            "bars.button_size",
            "bars.image_only",
            "bars.caption_only",
            "bars.caption_right",
        ],
        ("toolbar", "bars.button_size") => &[
            "bars.size_default",
            "bars.size_extra_small",
            "bars.size_small",
            "bars.size_medium",
            "bars.size_large",
            "bars.size_extra_large",
        ],
        ("toolbar", "menu.window.workspace") => &[
            "menu.window.workspace_export",
            "menu.window.workspace_import",
        ],
        (
            "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch",
            "palette.size",
        ) => &[
            "palette.size_xs",
            "palette.size_s",
            "palette.size_m",
            "palette.size_l",
            "palette.size_xl",
        ],
        (
            "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch",
            "palette.rows",
        ) => &[
            "palette.rows_1",
            "palette.rows_2",
            "palette.rows_3",
            "palette.rows_4",
            "palette.rows_5",
            "palette.rows_6",
            "palette.rows_7",
            "palette.rows_8",
            "palette.rows_fit",
        ],
        (
            "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch",
            "palette.position",
        ) => &[
            "bars.top",
            "bars.bottom",
            "bars.left",
            "bars.right",
            "palette.float",
        ],
        (
            "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch",
            "palette.open_bar",
        ) => &["palette.open_as_bar"],
        (
            "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch",
            "palette.palette",
        ) => &["palette.reset", "palette.rename"],
        ("objects_layer", "docker.show_on") | ("objects_new_master", "docker.master_in_front") => {
            &[
                "options.scope_all",
                "options.scope_odd",
                "options.scope_even",
            ]
        }
        _ => &[],
    }
}

/// Stable path for a sibling group's preferences. The root uses an empty key.
pub(crate) fn path_key(parents: &[&str]) -> String {
    parents.join("/")
}

/// Direct editor rows, including conditional operations known to this version.
/// A complete parent path distinguishes repeated operations in separate branches.
/// Invalid paths do not turn into root menus or expose unrelated children.
pub(crate) fn siblings(scene: &str, parents: &[&str]) -> Vec<&'static str> {
    let Some(menu) = super::context_menus::CATALOG
        .iter()
        .find(|menu| menu.id == scene)
    else {
        return Vec::new();
    };
    let mut items: Vec<_> = menu
        .items
        .iter()
        .copied()
        .filter(|key| is_root(scene, key))
        .collect();
    order_roots(scene, &mut items);
    for parent in parents {
        if !items.contains(parent) {
            return Vec::new();
        }
        items = children(scene, parent).to_vec();
        if matches!(
            scene,
            "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch"
        ) && *parent == "palette.position"
        {
            // This row is conditional at runtime. Including it in the editor
            // must not make an otherwise empty Position popup reachable.
            items.push("palette.among_toolbars");
        }
    }
    items
}

fn order_roots(scene: &str, items: &mut [&'static str]) {
    let document = matches!(scene, "palette_document" | "palette_document_swatch");
    let swatch = matches!(scene, "palette_swatch" | "palette_document_swatch");
    let palette = [
        if swatch { "palette.delete_color" } else { "" },
        "menu.window.palette_open",
        "palette.edit",
        "menu.window.palette_manager",
        "palette.auto_update",
        "palette.add_from_selection",
        "palette.add_from_document",
        if document {
            "palette.add_from_bitmap"
        } else {
            ""
        },
        if document && !swatch {
            "palette.delete_color"
        } else {
            ""
        },
        "palette.palette",
        "palette.show_document",
        "palette.show_names",
        "palette.size",
        "palette.rows",
        "palette.position",
        "palette.open_bar",
        "palette.new",
        if document { "" } else { "palette.fit_selection" },
        "palette.add_selection_to_palette",
        if !document {
            "palette.add_from_bitmap"
        } else {
            ""
        },
        "palette.rename",
        "palette.close",
        "palette.close_bar",
        "palette.reset_name",
    ];
    let first: &[&str] = match scene {
        "objects_object" => &[
            "docker.rename",
            "docker.move_to_layer",
            "docker.copy_to_layer",
        ],
        "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch" => &palette,
        _ => return,
    };
    items.sort_by_key(|key| {
        first
            .iter()
            .position(|item| item == key)
            .unwrap_or(usize::MAX)
    });
}

/// Target names remain managed by their owning document or library. The editor
/// marks these parents as branches without inventing persistent target keys.
pub(crate) fn is_dynamic_branch(scene: &str, key: &str) -> bool {
    matches!(
        (scene, key),
        ("canvas_object" | "objects_object", "docker.object_styles")
            | (
                "objects_object",
                "docker.move_to_layer" | "docker.copy_to_layer"
            )
            | ("objects_layer", "docker.move_to_page")
            | ("font", "font_manager.add_to_collection")
            | (
                "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch",
                "palette.size" | "palette.open_bar" | "palette.add_from_bitmap"
            )
    )
}

/// Only top-level operations can make a root popup reachable.
pub(crate) fn is_root(scene: &str, key: &str) -> bool {
    let parents: &[&str] = match scene {
        "canvas_object" | "objects_object" => &[
            "menu.bitmaps.trace",
            "menu.bitmaps.centerline_trace",
            "menu.bitmaps.outline_trace",
            "menu.edit.select_similar",
            "menu.object.order",
            "context.frame_type",
            "context.internet_links",
            "docker.object_styles",
            "docker.color_styles",
            "context.wrap_paragraph_text",
        ],
        "toolbar" => &[
            "bars.customize",
            "bars.default_button_size",
            "bars.toolbox_toolbar",
            "bars.button_size",
            "menu.window.workspace",
        ],
        "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch" => &[
            "palette.size",
            "palette.rows",
            "palette.position",
            "palette.open_bar",
            "palette.palette",
        ],
        "objects_layer" => &["docker.show_on"],
        "objects_new_master" => &["docker.master_in_front"],
        _ => &[],
    };
    // Rename is also a direct operation on palette bars. Among Toolbars is
    // drawn only inside Position and is supplied dynamically for that branch.
    if matches!(
        scene,
        "palette" | "palette_swatch" | "palette_document" | "palette_document_swatch"
    ) {
        if key == "palette.rename" {
            return true;
        }
        if key == "palette.among_toolbars" {
            return false;
        }
    }
    !parents
        .iter()
        .any(|parent| children(scene, parent).contains(&key))
}

pub(crate) fn root_has_visible_action(
    scene: &str,
    key: &str,
    preferences: &crate::settings::ContextMenuPreferences,
    dynamic: &[(&str, bool)],
) -> bool {
    if !preferences.is_visible(scene, key) {
        return false;
    }
    if dynamic
        .iter()
        .any(|(operation, present)| *operation == key && *present)
    {
        return true;
    }
    let items = children(scene, key);
    items.is_empty()
        || items
            .iter()
            .any(|item| root_has_visible_action(scene, item, preferences, dynamic))
}

/// Static trees are acyclic and require no allocation while drawing.
pub(crate) fn has_visible_children(
    scene: &str,
    parent: &str,
    hidden: &BTreeSet<String>,
    dynamic: bool,
) -> bool {
    if dynamic {
        return true;
    }
    let items = children(scene, parent);
    items.is_empty()
        || items
            .iter()
            .any(|item| !hidden.contains(*item) && has_visible_children(scene, item, hidden, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hidden(keys: &[&str]) -> BTreeSet<String> {
        keys.iter().map(|key| (*key).to_owned()).collect()
    }

    #[test]
    fn nested_static_branches_disappear_when_every_leaf_is_hidden() {
        let keys = hidden(&[
            "menu.bitmaps.quick_trace",
            "menu.bitmaps.repeat_last_trace",
            "menu.bitmaps.last_trace_settings",
            "menu.bitmaps.trace_technical",
            "menu.bitmaps.trace_line_drawing",
            "menu.bitmaps.trace_line_art",
            "menu.bitmaps.trace_logo",
            "menu.bitmaps.trace_detailed_logo",
            "menu.bitmaps.trace_clipart",
            "menu.bitmaps.trace_low_quality",
            "menu.bitmaps.trace_high_quality",
            "menu.bitmaps.trace_optimized",
        ]);
        assert!(!has_visible_children(
            "canvas_object",
            "menu.bitmaps.trace",
            &keys,
            false
        ));
    }

    #[test]
    fn one_visible_leaf_keeps_each_enclosing_branch() {
        let keys = hidden(&[
            "menu.bitmaps.quick_trace",
            "menu.bitmaps.repeat_last_trace",
            "menu.bitmaps.last_trace_settings",
            "menu.bitmaps.trace_technical",
            "menu.bitmaps.trace_line_drawing",
            "menu.bitmaps.trace_logo",
            "menu.bitmaps.trace_detailed_logo",
            "menu.bitmaps.trace_clipart",
            "menu.bitmaps.trace_low_quality",
            "menu.bitmaps.trace_high_quality",
        ]);
        assert!(has_visible_children(
            "canvas_object",
            "menu.bitmaps.trace",
            &keys,
            false
        ));
    }

    #[test]
    fn dynamic_content_keeps_a_branch_without_executing_its_closure() {
        assert!(has_visible_children(
            "canvas_object",
            "docker.object_styles",
            &hidden(&[
                "docker.new_style_from_selection",
                "docker.break_style_link",
                "docker.open_object_styles",
            ]),
            true
        ));
    }

    #[test]
    fn editor_tree_exposes_all_catalog_operations_without_cycles() {
        fn visit(scene: &str, path: &mut Vec<&'static str>, found: &mut BTreeSet<&'static str>) {
            let items = siblings(scene, path);
            let mut unique = BTreeSet::new();
            for key in items {
                assert!(unique.insert(key), "duplicate sibling in {scene}: {key}");
                assert!(!path.contains(&key), "cycle in {scene}: {key}");
                found.insert(key);
                path.push(key);
                visit(scene, path, found);
                path.pop();
            }
        }
        for menu in super::super::context_menus::CATALOG {
            let mut found = BTreeSet::new();
            visit(menu.id, &mut Vec::new(), &mut found);
            assert_eq!(found, menu.items.iter().copied().collect(), "{}", menu.id);
        }
    }

    #[test]
    fn nested_trace_items_are_children_of_their_actual_submenu() {
        let scene = "canvas_object";
        let roots = siblings(scene, &[]);
        assert!(roots.contains(&"menu.bitmaps.trace"));
        assert!(!roots.contains(&"menu.bitmaps.outline_trace"));
        assert!(!roots.contains(&"menu.bitmaps.trace_logo"));
        assert!(siblings(scene, &["menu.bitmaps.trace"]).contains(&"menu.bitmaps.outline_trace"));
        assert!(
            siblings(scene, &["menu.bitmaps.trace", "menu.bitmaps.outline_trace"])
                .contains(&"menu.bitmaps.trace_logo")
        );
        assert!(siblings(scene, &["menu.bitmaps.outline_trace"]).is_empty());
        assert!(siblings(scene, &["missing"]).is_empty());
        assert!(siblings("missing", &[]).is_empty());
    }

    #[test]
    fn repeated_toolbar_actions_keep_distinct_paths() {
        let first = ["bars.customize", "bars.default_button_size"];
        let second = ["bars.customize", "bars.toolbox_toolbar", "bars.button_size"];
        assert!(siblings("toolbar", &first).contains(&"bars.size_small"));
        assert!(siblings("toolbar", &second).contains(&"bars.size_small"));
        assert_ne!(path_key(&first), path_key(&second));
        assert_eq!(path_key(&[]), "");
        assert_eq!(
            path_key(&second),
            "bars.customize/bars.toolbox_toolbar/bars.button_size"
        );
    }

    #[test]
    fn conditional_palette_position_row_is_nested_only_in_the_editor() {
        for scene in ["palette", "palette_swatch", "palette_document"] {
            assert!(!siblings(scene, &[]).contains(&"palette.among_toolbars"));
            assert!(siblings(scene, &["palette.position"]).contains(&"palette.among_toolbars"));
            assert!(!children(scene, "palette.position").contains(&"palette.among_toolbars"));
            assert!(!has_visible_children(
                scene,
                "palette.position",
                &hidden(children(scene, "palette.position")),
                false
            ));
        }
    }

    #[test]
    fn object_panel_own_actions_precede_its_shared_canvas_actions() {
        let roots = siblings("objects_object", &[]);
        assert_eq!(roots.first(), Some(&"docker.rename"));
        assert_eq!(roots.get(1), Some(&"docker.move_to_layer"));
        assert_eq!(roots.get(2), Some(&"docker.copy_to_layer"));
        assert!(is_dynamic_branch("objects_object", "docker.move_to_layer"));
        assert!(!is_dynamic_branch("canvas_object", "docker.move_to_layer"));
        assert!(siblings("objects_object", &["docker.move_to_layer"]).is_empty());
    }

    #[test]
    fn the_select_similar_branch_lists_the_real_actions() {
        let keys = crate::select_similar::Similar::ALL.map(crate::select_similar::Similar::key);
        for scene in ["canvas_object", "objects_object"] {
            assert!(siblings(scene, &[]).contains(&"menu.edit.select_similar"));
            assert_eq!(siblings(scene, &["menu.edit.select_similar"]), keys);
        }
    }
}

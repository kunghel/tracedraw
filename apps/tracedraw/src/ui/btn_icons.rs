//! Pictures on the text buttons of the bars, panels and dialogs: every
//! command button shows a small picture before its name, so a button
//! is recognised at a glance and not only read.
//!
//! [`IconUi`] gives `Ui` the icon versions of egui's text buttons
//! (`ibutton`, `iselectable`, `itoggle`, `imenu_button`) and [`Ib`] the
//! icon version of `egui::Button`; each takes the language key of the
//! button's name and draws the picture [`source`] finds for that key.
//! The pictures come from the ones the program already has (the tools',
//! the menus', the property bar's) and, for the commands that had none,
//! from the small set drawn here ([`B`]) in the same 16 by 16 style.

use crate::i18n::tr;
use crate::theme::Tokens;
use crate::tools::Tool;
use crate::ui::icons::Action;
use crate::ui::menu_glyphs::Glyph;
use crate::ui::propbar::Pic;
use egui::{epaint, Atom, Color32, Id, Painter, Pos2, Rect, Response, Stroke, Ui, Vec2, Widget};

/// The pictures drawn here for buttons that had none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum B {
    Plus,
    Minus,
    Reset,
    Refresh,
    Dice,
    Shuffle,
    Check,
    Cross,
    Clear,
    Pencil,
    Rename,
    Play,
    Stop,
    Swap,
    FindNext,
    FindPrev,
    Eye,
    Magnet,
    ArrowUp,
    ArrowDown,
    Star,
    StarOff,
    Flask,
    Gauge,
    Download,
    Upload,
    Wand,
    Picture,
    Folder,
    FolderPlus,
    Chat,
    Plug,
    Bookmark,
    List,
    Book,
    Hyphen,
    Spell,
    Pin,
    Feather,
    Border,
    Target,
    Screen,
    Field,
    ColumnAdd,
    Parallel,
    Radial,
    Cells,
    Log,
    Bulge,
    Hairline,
    Anchor,
    Page,
    PagePlus,
    MoveTo,
    Clockwise,
    CounterClockwise,
    FlipArc,
    Smooth,
    Grow,
    Invert,
    SelectNone,
    Mask,
    Install,
    Uninstall,
    Layer,
    Merge,
    Palette,
    PaletteAdd,
    Swatches,
    Data,
    None,
    Custom,
    Csv,
    Text,
    Light,
    Gap,
    Split,
    Code,
    Pause,
    /// A colour harmony: 0 shades of one colour, 1 analogous, 2
    /// complementary, 3 split complementary, 4 triadic, 5 tetradic.
    Harmony(u8),
    /// Selecting by tone: 0 shadows, 1 midtones, 2 highlights.
    Tone(u8),
    /// Selecting by channel: 0 red, 1 green, 2 blue, 3 alpha.
    Channel(u8),
    Zigzag,
    Arrow,
    ContourIn,
    ContourOut,
    ContourCenter,
    ContourBoth,
}

/// Where a button's picture comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Src {
    Tool(Tool),
    Pic(Pic),
    Action(Action),
    Glyph(Glyph),
    /// The fill kinds' pictures of the Properties docker (0 = none ...
    /// 6 = procedural).
    Fill(usize),
    B(B),
    /// The picture of the menu row of the same key.
    Menu,
}

/// Keys whose buttons stay text only: the answers that close a dialog
/// (OK, Cancel, Yes, No ...), which read best as words alone, and
/// labels that are not commands.
pub const TEXT_ONLY: &[&str] = &[
    "dialog.ok",
    "dialog.cancel",
    "dialog.close",
    "dialog.yes",
    "dialog.no",
    "dialog.skip",
    "dialog.help",
    "menu.file.no_recent",
    "dialog.export_preset_none",
    "font_manager.menu_file",
    "font_manager.menu_edit",
    "font_manager.menu_view",
    "font_manager.menu_font",
    "font_manager.menu_help",
];

/// The picture of the button named by `key`.
pub fn source(key: &str) -> Option<Src> {
    if TEXT_ONLY.contains(&key) {
        return None;
    }
    if let Some(s) = exact(key) {
        return Some(s);
    }
    if crate::ui::menus::has_own_picture(key) {
        return Some(Src::Menu);
    }
    by_words(key)
}

fn exact(key: &str) -> Option<Src> {
    Some(match key {
        "guides.lock" => Src::Pic(Pic::Lock(true)),
        "guides.unlock" => Src::Pic(Pic::Lock(false)),
        "pixel.cancel" => Src::B(B::Cross),
        "harmony.analogous" => Src::B(B::Harmony(1)),
        "harmony.complementary" => Src::B(B::Harmony(2)),
        "harmony.split_complementary" => Src::B(B::Harmony(3)),
        "harmony.triadic" => Src::B(B::Harmony(4)),
        "harmony.tetradic" => Src::B(B::Harmony(5)),
        "harmony.monochromatic" => Src::B(B::Harmony(0)),
        "pixel.sel.shadows" => Src::B(B::Tone(0)),
        "pixel.sel.midtones" => Src::B(B::Tone(1)),
        "pixel.sel.highlights" => Src::B(B::Tone(2)),
        "pixel.sel.red" => Src::B(B::Channel(0)),
        "pixel.sel.green" => Src::B(B::Channel(1)),
        "pixel.sel.blue" => Src::B(B::Channel(2)),
        "pixel.sel.alpha" => Src::B(B::Channel(3)),
        "shapegen.growth_run" => Src::B(B::Play),
        "shapegen.growth_pause" | "tracker.pause" => Src::B(B::Pause),
        "tracker.record" => Src::Glyph(Glyph::Record),
        "dialog.trace_optimize" => Src::B(B::Wand),
        "options.download" => Src::B(B::Download),
        "merge.view_all" => Src::B(B::Eye),
        "preflight.show_settings" => Src::Action(Action::Options),
        "table.edit_in_spreadsheet" => Src::B(B::Data),
        "export_docker.export" => Src::Action(Action::Export),
        "dimension.options" => Src::Action(Action::Options),
        "docker.use_sampled" => Src::Tool(Tool::ColorEyedropper),
        "docker.cancel_symbol_edit" => Src::B(B::Cross),
        "clip.edit" | "clip.select_contents" => Src::Glyph(Glyph::ClipEdit),
        "clip.extract" => Src::Glyph(Glyph::ClipExtract),
        "clip.lock" => Src::Pic(Pic::Lock(true)),
        "clip.fit_contents" => Src::Pic(Pic::ZoomAll),
        "benchmark.test_with_card" | "benchmark.test_without_card" => Src::B(B::Gauge),
        "shapegen.create" | "shapegen.make_pattern" => Src::Tool(Tool::ShapeGenerator),
        "shapegen.slice_apply" => Src::B(B::Split),
        "math.mode_graph" => Src::Tool(Tool::GraphPaper),
        "math.mode_figure" => Src::Tool(Tool::Polygon),
        "shapegen.wrap" => Src::Pic(Pic::Combine),
        "docker.apply" => Src::B(B::Check),
        "hint.drop_ask_import" => Src::Action(Action::Import),
        "hint.drop_ask_open" => Src::Action(Action::Open),
        "dialog.table_shapes_file" => Src::B(B::Csv),
        "toolbar.area_fill_all" => Src::Tool(Tool::AreaFill),
        "preflight.locate" => Src::Pic(Pic::ZoomSelected),
        "recolor.pick_all" => Src::Glyph(Glyph::SelectAll),
        "recolor.pick_none" => Src::B(B::SelectNone),
        "recolor.pick_similar" => Src::Tool(Tool::ColorEyedropper),
        "recolor.replace_picked" => Src::B(B::Swap),
        "shelf.new_folder" => Src::B(B::FolderPlus),
        "shelf.rename_folder" => Src::B(B::Rename),
        "shelf.delete_folder" => Src::Action(Action::Delete),
        // Lists shown as buttons on the bars.
        "toolbar.push_pull" => Src::B(B::Smooth),
        "toolbar.zipper" => Src::B(B::Zigzag),
        "toolbar.twister" => Src::Tool(Tool::Spiral),
        "toolbar.ft_rotation" => Src::Pic(Pic::Rotate),
        "toolbar.ft_reflection" => Src::Pic(Pic::MirrorH),
        "toolbar.ft_scale" => Src::Pic(Pic::ScaleMirror),
        "toolbar.ft_skew" => Src::Pic(Pic::SkewBox),
        "knife.freehand" | "segment_delete.freehand" => Src::Tool(Tool::Freehand),
        "knife.two_point" => Src::Tool(Tool::TwoPointLine),
        "segment_delete.marquee" => Src::Glyph(Glyph::SelectAll),
        "docker.inside" => Src::B(B::ContourIn),
        "docker.outside" => Src::B(B::ContourOut),
        "docker.to_center" => Src::B(B::ContourCenter),
        "docker.contour_both" => Src::B(B::ContourBoth),
        "balloon.speech" | "balloon.thought" | "balloon.whisper" | "balloon.shout"
        | "balloon.caption" => Src::Tool(Tool::Balloon),
        "comments.pin" => Src::B(B::Pin),
        "comments.rect" => Src::B(B::Border),
        "comments.arrow" => Src::B(B::Arrow),
        "docker.iso_left" | "docker.iso_left_up" | "docker.iso_right" | "docker.iso_right_up"
        | "docker.iso_top" | "docker.iso_top_turned" => Src::Tool(Tool::Extrude),
        "writing.auto_replace" => Src::B(B::Swap),
        "writing.skip_all" | "writing.skip_once" | "writing.skip_rule" => Src::B(B::FindNext),
        "toolbar.zoom_to_fit" => Src::Pic(Pic::ZoomAll),
        "toolbar.zoom_to_page" => Src::Pic(Pic::ZoomPage),
        "toolbar.zoom_to_selected" => Src::Pic(Pic::ZoomSelected),
        "toolbar.zoom_to_width" => Src::Pic(Pic::ZoomWidth),
        "toolbar.zoom_to_height" => Src::Pic(Pic::ZoomHeight),
        "tracecut.zoom_in" => Src::Pic(Pic::ZoomIn),
        "tracecut.zoom_out" => Src::Pic(Pic::ZoomOut),
        "tracecut.zoom_fit" | "tracecut.zoom_actual" => Src::Pic(Pic::ZoomAll),
        "tracecut.zoom_selection" => Src::Pic(Pic::ZoomSelected),
        // The picture property bar.
        "toolbar.edit_bitmap" => Src::Pic(Pic::EditBitmap),
        "toolbar.replace_bitmap" => Src::B(B::Picture),
        "toolbar.crop_bitmap" => Src::Pic(Pic::CropBitmap),
        "toolbar.trace_bitmap" | "menu.bitmaps.quick_trace" => {
            Src::Pic(Pic::QuickTrace)
        }
        "menu.bitmaps.straighten_image" => Src::Pic(Pic::StraightenImage),
        "menu.bitmaps.resample" => Src::Pic(Pic::Resample),
        "menu.bitmaps.remove_background_local" | "menu.bitmaps.remove_background" => Src::B(B::Mask),
        "toolbar.remove_background" => Src::Glyph(Glyph::Ai),
        // Drawing inside or behind a shape.
        "toolbar.draw_inside" => Src::Glyph(Glyph::ClipEdit),
        "toolbar.draw_behind" => Src::Glyph(Glyph::Behind),
        // The connector kinds.
        "connector.straight" => Src::Tool(Tool::Connector),
        "connector.right_angle" => Src::Tool(Tool::RightAngleConnector),
        "connector.rounded" => Src::Tool(Tool::RoundedConnector),
        // The fill kinds.
        "docker.fill_none" => Src::Fill(0),
        "docker.fill_uniform" => Src::Fill(1),
        "docker.fill_fountain" => Src::Fill(2),
        "docker.fill_pattern" => Src::Fill(3),
        "docker.fill_texture" => Src::Fill(4),
        "docker.fill_mesh" => Src::Fill(5),
        "docker.fill_procedural" => Src::Fill(6),
        // Shapes cut into parts (#4209, #4210).
        "toolbar.slices" => Src::Pic(Pic::Pie),
        "toolbar.triangles" => Src::Tool(Tool::Polygon),
        "toolbar.copy_fill" | "docker.data_copy_to_selection" => Src::Glyph(Glyph::CopyProperties),
        // Effects and tools.
        "toolbar.clear_shadow" | "shadow.break_apart" | "context.edit_shadow" => Src::Tool(Tool::DropShadow),
        "toolbar.clear_inner_shadow" => Src::Tool(Tool::InnerShadow),
        "toolbar.clear_block_shadow" => Src::Tool(Tool::BlockShadow),
        "toolbar.clear_mesh" | "envelope.add_mesh" => Src::Tool(Tool::MeshFill),
        "toolbar.clear_transparency" | "transparency.edit" => Src::Tool(Tool::Transparency),
        "contour.clear" | "contour.copy_from" => Src::Tool(Tool::Contour),
        "extrude.clear" | "extrude.space_menu" => Src::Tool(Tool::Extrude),
        "distort.clear" => Src::Tool(Tool::Distort),
        "distort.center" => Src::B(B::Target),
        "distort.convert" => Src::Pic(Pic::ToCurves),
        "docker.clear_blend" | "docker.blend_straighten" => Src::Tool(Tool::Blend),
        "vo.clear" | "vo.delete_node" => Src::Tool(Tool::VariableOutline),
        "crop.apply" | "crop.clear" | "pixel.sel.crop" => Src::Tool(Tool::Crop),
        "knife.split" => Src::B(B::Split),
        "knife.gap" => Src::B(B::Gap),
        "toolbar.sample_screen" => Src::B(B::Screen),
        "toolbar.select_color" | "toolbar.apply_color" => Src::Tool(Tool::ColorEyedropper),
        "toolbar.select_attrs" | "toolbar.apply_attrs" => Src::Tool(Tool::AttributesEyedropper),
        "toolbar.attr_properties" | "docker.layer_properties" => {
            Src::Glyph(Glyph::Properties)
        }
        "toolbar.attr_transformations" => Src::Pic(Pic::Position),
        "toolbar.rotate_90" => Src::Pic(Pic::Rotate),
        "toolbar.mirror_h_short" | "pixel.flip_h" => Src::Pic(Pic::MirrorH),
        "toolbar.mirror_v_short" | "pixel.flip_v" => Src::Pic(Pic::MirrorV),
        "toolbar.clockwise" => Src::B(B::Clockwise),
        "toolbar.counterclockwise" => Src::B(B::CounterClockwise),
        "toolbar.line_arc" => Src::Pic(Pic::Arc),
        "toolbar.line_arc_flip" => Src::B(B::FlipArc),
        "toolbar.parallel" | "toolbar.parallel_lines" | "toolbar.parallel_both" => Src::B(B::Parallel),
        "toolbar.radial" | "toolbar.graph_radial" => Src::B(B::Radial),
        "toolbar.graph_cells" => Src::B(B::Cells),
        "toolbar.logarithmic" => Src::B(B::Log),
        "toolbar.attract" | "toolbar.repel" => Src::Tool(Tool::AttractRepel),
        "toolbar.apply_bulge" => Src::B(B::Bulge),
        "toolbar.hairline" | "docker.hairline" => Src::B(B::Hairline),
        "toolbar.anchor_delete" | "toolbar.anchor_clear" => Src::B(B::Anchor),
        "toolbar.symmetrical" => Src::Pic(Pic::NodeSymmetrical),
        "toolbar.path_follow" | "docker.apply_to_curve" | "docker.new_path" => {
            Src::Pic(Pic::FitTextToPath)
        }
        "docker.detach_path" => Src::Pic(Pic::BreakLink),
        "toolbar.path_tick_snap" | "guides.snap" => Src::B(B::Magnet),
        "toolbar.autofit_page" | "options.fit_page_drawing" | "options.fit_page_selection" => {
            Src::Pic(Pic::FitPage)
        }
        "toolbar.custom_page" | "toolbar.edit_page_sizes" | "dimension.drawing_scale_custom" => {
            Src::B(B::Custom)
        }
        "toolbar.delete_color_point" => Src::Pic(Pic::DeleteNode),
        "toolbar.mesh_shuffle" | "docker.scatter_shuffle" => Src::B(B::Shuffle),
        "toolbar.action_reseed" | "layer_fx.reseed" | "layer_fx.new_grain" | "shapegen.new_seed"
        | "merge.random_draw" => {
            Src::B(B::Dice)
        }
        "toolbar.builder_add" => Src::B(B::Plus),
        "toolbar.builder_delete" => Src::B(B::Minus),
        "toolbar.builder_accept" | "perspective.finish" => Src::B(B::Check),
        "toolbar.builder_cancel" => Src::B(B::Cross),
        "toolbar.shape_gen_options" => Src::Tool(Tool::ShapeGenerator),
        "toolbar.none" | "outline_pen.none" | "context.wrap_none" | "pixel.sel.none" => Src::B(B::None),
        "docker.no_fill" => Src::Fill(0),
        "docker.no_outline_action" => Src::B(B::None),
        "docker.fill" | "docker.add_fill" => Src::Tool(Tool::InteractiveFill),
        "docker.outline" | "docker.add_outline" | "docker.outline_pen_dialog"
        | "docker.outline_from_fill" | "outline_pen.default" => Src::Tool(Tool::OutlinePen),
        "docker.edit_fill_dialog" => Src::Tool(Tool::InteractiveFill),
        "docker.edit_line_style" | "line_style.add" | "line_style.replace" => Src::Tool(Tool::OutlinePen),
        "docker.edit_with_text_tool" | "menu.text.edit_text" => Src::Pic(Pic::EditText),
        "docker.swap_fills" | "docker.swap_colors" | "transparency.swap" | "dialog.swap" => Src::B(B::Swap),
        "docker.break_apart" => Src::Pic(Pic::BreakApart),
        "docker.break_style_link" => Src::Pic(Pic::BreakLink),
        "docker.revert_to_objects" => Src::Pic(Pic::Ungroup),
        "docker.registration" => Src::B(B::Target),
        "docker.new_ink" | "docker.add_to_spot_colors" => Src::B(B::PaletteAdd),
        "docker.add_to_document_palette" | "docker.add_to_color_styles" | "docker.new_color_style"
        | "docker.new_style_from_selection" | "docker.cs_from_document" | "palette.add_selection_colors"
        | "palette.add_from_document" | "shelf.add_colors" | "dialog.add_color" => Src::B(B::PaletteAdd),
        "docker.cs_new_gradient" | "docker.import_gradient" => Src::Fill(2),
        "palette.new" | "dialog.new_palette" | "dialog.save_palette" => Src::B(B::Palette),
        "palette.clear" | "palette.reset_name" => Src::B(B::Swatches),
        "docker.new_symbol" | "docker.edit_symbol" | "docker.duplicate_symbol" => {
            Src::Glyph(Glyph::Symbol)
        }
        "docker.symbolize_similar" => Src::Glyph(Glyph::Symbol),
        "docker.pattern_new" => Src::Fill(3),
        "docker.pattern_import" | "docker.pattern_folder" | "docker.picture_mosaic_browse" => {
            Src::B(B::Picture)
        }
        "docker.vector_mosaic_select" => Src::Glyph(Glyph::Mosaic),
        "docker.shape3d_release" | "docker.shape3d_fit" | "docker.shape3d_halftone_create" => {
            Src::Tool(Tool::Extrude)
        }
        "docker.iso_reset" => Src::B(B::Reset),
        "docker.font_axes_reset" => Src::Glyph(Glyph::FontManager),
        "docker.select_layer_objects" => Src::Glyph(Glyph::SelectAll),
        "docker.delete_layer" => Src::B(B::Layer),
        "docker.layer_to_master" | "docker.master_to_page" | "docker.move_to_page" => Src::B(B::MoveTo),
        "docker.go_to_page" => Src::Glyph(Glyph::PageGoto),
        "docker.show_on" => Src::B(B::Eye),
        "docker.add_field" | "merge.insert_field" | "merge.update_field"
        | "merge.configure" => Src::B(B::Field),
        "merge.add_column" => Src::B(B::ColumnAdd),
        "docker.add_effect" => Src::Glyph(Glyph::Lens),
        "docker.data_sheet" | "merge.save_data" => Src::B(B::Data),
        "docker.evaluate" | "docker.debug" => Src::B(B::Code),
        "docker.install_plugin" | "docker.install_filter" | "menu.tools.plugin_install" => {
            Src::B(B::Install)
        }
        "menu.tools.plugin_new" => Src::B(B::Plug),
        "docker.record" | "macros.stop" => Src::Glyph(Glyph::Record),
        "docker.scatter_use_selection" | "docker.scatter_picked" | "shapegen.use_selection"
        | "shapegen.split_pick" | "docker.from_selection" | "docker.update_from_selection"
        | "docker.new_from_selection" | "find.from_selection" | "export_docker.add_selection"
        | "shelf.add_selection" | "coords.replace" => Src::Glyph(Glyph::SelectAll),
        "docker.step" => Src::B(B::FindNext),
        "docker.reset_panel" | "toolbox.reset" => Src::B(B::Reset),
        // Pages, guides and the grid.
        "menu.layout.insert_page" | "export_docker.add_pages" => Src::B(B::PagePlus),
        "options.add_page_frame" => Src::B(B::Border),
        "guides.add" | "guides.apply_presets" => Src::Action(Action::ShowGuidelines),
        "guides.show" | "print.view" => Src::B(B::Eye),
        "guides.move_to" => Src::B(B::MoveTo),
        "guides.modify" => Src::B(B::Pencil),
        "guides.duplicate" => Src::Glyph(Glyph::Duplicate),
        "guides.clear" => Src::B(B::Clear),
        "grid_cut.object_size" | "tracecut.tray_take_size" | "sizes.copy" => {
            Src::Pic(Pic::SizeBox)
        }
        // Printing.
        "print.fit" | "print.fit_width" | "pixel.zoom_fit" | "resample.fit" => {
            Src::Pic(Pic::ZoomAll)
        }
        "print.print_sheet" | "print.print_and_close" | "merge.print_document" | "dialog.print" => {
            Src::Action(Action::Print)
        }
        "print.use_bleed_title" => Src::B(B::Page),
        "print.sheet_save" => Src::Action(Action::Save),
        "preview.back_to_page" => Src::B(B::Page),
        "preflight.refresh" => Src::B(B::Refresh),
        "preflight.open" => Src::Glyph(Glyph::Preflight),
        // Text.
        "edit_text.spell" | "thesaurus.look_up" => Src::B(B::Spell),
        "edit_text.import" => Src::Action(Action::Import),
        "hyphens.add" | "hyphens.remove" | "hyphens.apply_selection" => Src::B(B::Hyphen),
        "autocorrect.add" | "autocorrect.delete" | "autocorrect.replace" | "autocorrect.apply_selection" => {
            Src::B(B::Spell)
        }
        "thesaurus.pick_file" | "options.add_dictionary_files" | "options.refresh_dictionaries" => Src::B(B::Book),
        "balloon.add_text" | "dialog.add_tab" => Src::B(B::Text),
        "find.special_character" => Src::Glyph(Glyph::FontManager),
        "find.previous" => Src::B(B::FindPrev),
        "dialog.find_next" | "macros.find_next" => Src::B(B::FindNext),
        "dialog.find_all" => Src::Glyph(Glyph::Find),
        "dialog.replace" | "dialog.replace_all" => Src::B(B::Swap),
        "font_list.filter_clear" | "font_manager.clear_filters" => Src::B(B::Clear),
        "font_manager.favorites" | "font_manager.add_favorite" => Src::B(B::Star),
        "font_manager.remove_favorite" => Src::B(B::StarOff),
        "font_manager.add_font" | "missing_fonts.add_font" | "font_manager.install" => {
            Src::Glyph(Glyph::FontManager)
        }
        "font_manager.uninstall" => Src::B(B::Uninstall),
        "font_manager.add_folder" => Src::B(B::FolderPlus),
        "font_manager.remove_folder" => Src::B(B::Folder),
        "font_manager.new_collection" | "font_manager.add_to_collection" => Src::B(B::Bookmark),
        "font_manager.remove_from_collection" | "font_manager.delete_collection" => Src::B(B::Bookmark),
        "font_manager.use_in_drawing" => Src::Tool(Tool::Text),
        "font_manager.sample_family" => Src::B(B::Text),
        "font_manager.show_details" => Src::Glyph(Glyph::Info),
        "font_manager.help" => Src::Glyph(Glyph::Help),
        "font_manager.select_none" | "options.deselect_all" => Src::B(B::SelectNone),
        // Pictures in the image mode.
        "pixel.sel.all" | "options.select_all" | "menu.edit.select_all" => {
            Src::Glyph(Glyph::SelectAll)
        }
        "pixel.sel.invert" | "dialog.invert" => Src::B(B::Invert),
        "pixel.sel.grow" => Src::B(B::Grow),
        "pixel.sel.smooth" | "dialog.smooth" => Src::B(B::Smooth),
        "pixel.sel.feather" => Src::B(B::Feather),
        "pixel.sel.border" => Src::B(B::Border),
        "pixel.sel.expand" => Src::B(B::Grow),
        "pixel.sel.contract" => Src::B(B::Minus),
        "pixel.sel.stroke" => Src::Tool(Tool::OutlinePen),
        "pixel.sel.modify" => Src::B(B::Border),
        "pixel.text.to_pixels" => Src::Tool(Tool::Text),
        "pixel.opt.clone_layer" => Src::B(B::Plus),
        "pixel.opt.clone_source" => Src::Pic(Pic::MirrorH),
        "pixel.sel.fill" => Src::Tool(Tool::InteractiveFill),
        "pixel.sel.save" => Src::Action(Action::Save),
        "pixel.sel.quick_mask" => Src::B(B::Mask),
        "pixel.red_eye" => Src::B(B::Eye),
        "pixel.content_fill" => Src::B(B::Wand),
        "pixel.light.add" | "pixel.light.remove" | "pixel.light.clear" | "pixel.light.fill" => {
            Src::B(B::Light)
        }
        "pixel.opt.tip" => Src::Tool(Tool::BrushStrokes),
        "pixel.opt.surface" => Src::Fill(4),
        "pixel.warp.start" => Src::B(B::Play),
        "dialog.lab_auto_adjust" | "dialog.auto_balance_tone" => Src::B(B::Wand),
        "dialog.lab_snapshot" => Src::B(B::Picture),
        "dialog.fx_show_original" => Src::B(B::Eye),
        "dialog.bg_brush_clear" | "painterly.clear" => Src::Tool(Tool::BrushStrokes),
        "dialog.trace_with_ai" | "dialog.trace_clean_sketch_ai" | "ai.draw"
        | "menu.object.ai_draw_words" | "menu.tools.ai_generate" | "aiui.set_up"
        | "menu.tools.options_ai" => Src::Glyph(Glyph::Ai),
        "dialog.trace_enlarge_download" => Src::B(B::Download),
        "dialog.trace_merge_colors" => Src::B(B::Merge),
        "dialog.trace_options" | "dialog.export_pdf_settings" => Src::Action(Action::Options),
        "dialog.add_bookmark" | "dialog.remove_bookmark" => Src::B(B::Bookmark),
        "dialog.new_folder" => Src::B(B::FolderPlus),
        "dialog.open_entry" | "tpl.open" | "github.open_version" | "welcome.open_template" => {
            Src::Action(Action::Open)
        }
        "tpl.more" => Src::Action(Action::Template),
        "welcome.remove_recent" => Src::B(B::Minus),
        "dialog.rename_entry" => Src::B(B::Rename),
        // AI, scripts, macros.
        "ai.refresh_models" | "aiui.test" => Src::B(B::Refresh),
        "ai.new_conversation" | "comments.reply" => Src::B(B::Chat),
        "ai.open_in_scripts" | "convert.to_scripts" => Src::Glyph(Glyph::Script),
        "convert.run_js" | "macros.run" | "macros.run_sub" | "flow.run" | "docker.run" => Src::B(B::Play),
        "flow.run_file" => Src::B(B::Picture),
        "flow.preview" => Src::B(B::Eye),
        "convert.save_js" => Src::Action(Action::Save),
        "convert.fix" => Src::B(B::Wand),
        "convert.check" | "qr.validate" | "flow.choose" => Src::B(B::Check),
        "convert.convert_again" => Src::B(B::Refresh),
        "macros.unload" => Src::B(B::Upload),
        "macros.delete" => Src::Action(Action::Delete),
        // Time tracking.
        "tracker.new_task" | "tracker.tasks" => Src::B(B::List),
        "tracker.make_active" => Src::B(B::Play),
        "tracker.reset" => Src::Glyph(Glyph::Clock),
        "tracker.export_csv" => Src::B(B::Csv),
        "tracker.export_text" => Src::Action(Action::Export),
        // Cutting.
        "tracecut.test_cut" => Src::B(B::Flask),
        "tracecut.laser_frame" => Src::B(B::Border),
        "tracecut.laser_test" => Src::B(B::Cells),
        "tracecut.laser_dxf" => Src::Action(Action::Export),
        "menu.tools.tracecut" => Src::Glyph(Glyph::TraceCut),
        // Merge.
        "merge.place_inside" => Src::Glyph(Glyph::ClipInside),
        "merge.go_to" => Src::B(B::FindNext),
        "merge.create" | "merge.create_document" => Src::B(B::Merge),
        "merge.import_file" => Src::Action(Action::Import),
        // Options.
        "options.calibrate_zoom" => Src::Pic(Pic::ZoomPage),
        "options.reset_origin" => Src::B(B::Target),
        "options.dynamic_add_angle" => Src::B(B::Plus),
        "options.page_look_reset" => Src::B(B::Reset),
        "options.open_folder" => Src::B(B::Folder),
        "options.clear_folder" => Src::B(B::Clear),
        "options.choose_bitmap" => Src::B(B::Picture),
        "options.command_icon_choose" => Src::B(B::Picture),
        "options.apply_labels" => Src::B(B::Check),
        "options.workspace_import" => Src::Action(Action::Import),
        "options.workspace_export" => Src::Action(Action::Export),
        "options.save_as_default" => Src::Glyph(Glyph::SaveDefaults),
        "color_mgmt.load_profiles" => Src::Glyph(Glyph::ColorManagement),
        "bars.customize_options" | "writing.options" | "table.options" => {
            Src::Action(Action::Options)
        }
        "barcode.advanced" => Src::Glyph(Glyph::Barcode),
        "benchmark.quick" | "benchmark.advanced" | "menu.help.benchmark" => Src::B(B::Gauge),
        "benchmark.copy" => Src::Action(Action::Copy),
        "shapes.all" | "shapes.mine" => Src::Tool(Tool::CommonShapes),
        "shapes.mine_remove" | "shelf.remove" => Src::B(B::Minus),
        "shapes.mine_rename" => Src::B(B::Rename),
        "shelf.place" | "writing.insert" => Src::B(B::MoveTo),
        "pdf_import.size_as_file" => Src::Pic(Pic::SizeBox),
        "newdoc.preset_custom" => Src::B(B::Custom),
        "coords.create" => Src::B(B::Plus),
        "coords.auto_close" => Src::Pic(Pic::CloseCurve),
        "edit_fill.reverse" => Src::B(B::Swap),
        "edit_fill.reset_nodes" => Src::B(B::Reset),
        "effects.preset_apply" => Src::B(B::Check),
        "shapegen.warp_clear" => Src::B(B::Pin),
        "shapegen.relax_once" => Src::B(B::Smooth),
        "shapegen.growth_stop" | "docker.stop" => Src::B(B::Stop),
        "shapegen.frac_edit" => Src::B(B::Pencil),
        "shapegen.reset_points" | "docker.reset_defaults" => Src::B(B::Reset),
        "shapegen.update" => Src::B(B::Refresh),
        "perspective.put_on_plane" => Src::Glyph(Glyph::Perspective),
        "recolor.reset" => Src::B(B::Reset),
        "menu.window.welcome_screen" => Src::B(B::Page),
        "menu.view.page_sorter" => Src::Glyph(Glyph::PageSorter),
        "menu.object.lock_object" => Src::Pic(Pic::Lock(true)),
        "menu.object.add_dimensions_each" => Src::Tool(Tool::HorizontalVerticalDimension),
        "menu.object.add_common_shape" => Src::Tool(Tool::CommonShapes),
        "menu.text.straighten_text" => Src::Pic(Pic::StraightenText),
        "menu.layout.switch_orientation" => Src::Glyph(Glyph::PageOrientation),
        "menu.layout.duplicate_page" => Src::Glyph(Glyph::PageDuplicate),
        "menu.layout.delete_page" => Src::Glyph(Glyph::PageDelete),
        "menu.file.print_preview" => Src::Glyph(Glyph::PrintPreview),
        _ => return Option::None,
    })
}

/// The picture of a key no table names, from the words of its last part
/// ("delete", "reset", "save" ...).
fn by_words(key: &str) -> Option<Src> {
    let last = key.rsplit('.').next().unwrap_or(key);
    let has = |w: &str| last.split('_').any(|p| p == w);
    let starts = |w: &str| last.starts_with(w);
    Some(if has("delete") || has("remove") || has("uninstall") {
        Src::Action(Action::Delete)
    } else if has("reset") || has("defaults") || has("default") || has("revert") {
        Src::B(B::Reset)
    } else if has("clear") {
        Src::B(B::Clear)
    } else if starts("save") || has("save") {
        Src::Action(Action::Save)
    } else if has("load") || has("open") || has("browse") || has("choose") || has("pick") {
        Src::Action(Action::Open)
    } else if has("export") {
        Src::Action(Action::Export)
    } else if has("import") {
        Src::Action(Action::Import)
    } else if has("copy") {
        Src::Action(Action::Copy)
    } else if has("paste") {
        Src::Action(Action::Paste)
    } else if has("cut") {
        Src::Action(Action::Cut)
    } else if has("undo") {
        Src::Action(Action::Undo)
    } else if has("redo") {
        Src::Action(Action::Redo)
    } else if has("rename") {
        Src::B(B::Rename)
    } else if has("edit") || has("modify") {
        Src::B(B::Pencil)
    } else if has("add") || has("new") || has("create") || has("insert") {
        Src::B(B::Plus)
    } else if has("run") || has("start") || has("play") {
        Src::B(B::Play)
    } else if has("stop") {
        Src::B(B::Stop)
    } else if has("apply") || has("accept") || has("finish") || has("continue") || has("ok") {
        Src::B(B::Check)
    } else if has("cancel") {
        Src::B(B::Cross)
    } else if has("swap") {
        Src::B(B::Swap)
    } else if has("refresh") || has("update") || has("again") {
        Src::B(B::Refresh)
    } else if has("options") || has("settings") || has("configure") || has("customize") {
        Src::Action(Action::Options)
    } else if has("find") || has("search") {
        Src::Glyph(Glyph::Find)
    } else if has("up") {
        Src::B(B::ArrowUp)
    } else if has("down") {
        Src::B(B::ArrowDown)
    } else if has("print") {
        Src::Action(Action::Print)
    } else if has("help") {
        Src::Glyph(Glyph::Help)
    } else if has("show") || has("view") || has("preview") {
        Src::B(B::Eye)
    } else if has("select") || has("selection") {
        Src::Glyph(Glyph::SelectAll)
    } else if has("duplicate") {
        Src::Glyph(Glyph::Duplicate)
    } else if has("fit") {
        Src::Pic(Pic::ZoomAll)
    } else if has("lock") {
        Src::Pic(Pic::Lock(true))
    } else {
        return Option::None;
    })
}

/// Whether the button named by `key` has a picture.
pub fn has(key: &str) -> bool {
    source(key).is_some()
}

/// Draw the picture of `key` into `r`; false when it has none.
pub fn draw(painter: &Painter, r: Rect, key: &str, color: Color32) -> bool {
    let Some(src) = source(key) else {
        return false;
    };
    match src {
        Src::Menu => crate::ui::menus::draw_own_picture(painter, r, key, color),
        Src::Tool(t) => {
            let c = if color == Tokens::icon() {
                crate::ui::icons::tool_color(t, color)
            } else {
                color
            };
            crate::ui::icons::draw(painter, r, t, c);
            true
        }
        Src::Pic(p) => {
            crate::ui::propbar::draw_pic(painter, r, p, color);
            true
        }
        Src::Action(a) => {
            crate::ui::icons::draw_action(painter, r, a, color);
            true
        }
        Src::Glyph(g) => {
            crate::ui::menu_glyphs::draw(painter, r, g, color);
            true
        }
        Src::Fill(i) => {
            crate::ui::dockers::fill_kind_picture(painter, r.shrink(1.0), i);
            true
        }
        Src::B(b) => {
            draw_b(painter, r, b, color);
            true
        }
    }
}

/// The side of a button's picture: the text's height, so a button
/// with a picture is no taller than one without.
fn icon_side(ui: &Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Button).clamp(12.0, 16.0)
}

/// The width the picture of `key` adds to its button: the picture and
/// the gap after it (none for a key without a picture).
pub fn extra_width(ui: &Ui, key: &str) -> f32 {
    if has(key) {
        icon_side(ui) + ui.spacing().icon_spacing
    } else {
        0.0
    }
}

fn icon_id(key: &str) -> Id {
    Id::new(("btn_icon", key))
}

/// An `egui::Button` with the picture of its key before its text.
pub struct Ib {
    key: String,
    text: egui::WidgetText,
    selectable: bool,
    selected: bool,
    small: bool,
    frame: Option<bool>,
    min_size: Option<Vec2>,
}

impl Ib {
    /// The button named by the translation of `key`.
    pub fn new(key: &str) -> Self {
        Self::with_text(key, tr(key))
    }

    /// The button named `text`, with the picture of `key`.
    pub fn with_text(key: &str, text: impl Into<egui::WidgetText>) -> Self {
        Self {
            key: key.to_owned(),
            text: text.into(),
            selectable: false,
            selected: false,
            small: false,
            frame: Option::None,
            min_size: Option::None,
        }
    }

    /// A selectable button (as `egui::Button::selectable`): framed only
    /// when on or hovered.
    pub fn selectable(on: bool, key: &str) -> Self {
        let mut b = Self::new(key);
        b.selectable = true;
        b.selected = on;
        b
    }

    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn frame(mut self, on: bool) -> Self {
        self.frame = Some(on);
        self
    }

    pub fn min_size(mut self, s: Vec2) -> Self {
        self.min_size = Some(s);
        self
    }
}

fn b_atoms(key: &str, side: f32, text: egui::WidgetText) -> egui::Atoms<'static> {
    if has(key) {
        egui::Atoms::new((Atom::custom(icon_id(key), Vec2::splat(side)), text))
    } else {
        egui::Atoms::new(text)
    }
}

impl Widget for Ib {
    fn ui(self, ui: &mut Ui) -> Response {
        let side = icon_side(ui) - if self.small { 2.0 } else { 0.0 };
        let atoms = b_atoms(&self.key, side, self.text);
        let mut b = if self.selectable {
            egui::Button::selectable(self.selected, atoms)
        } else {
            egui::Button::new(atoms).selected(self.selected)
        };
        if self.small {
            b = b.small();
        }
        if let Some(f) = self.frame {
            b = b.frame(f);
        }
        if let Some(s) = self.min_size {
            b = b.min_size(s);
        }
        let out = b.atom_ui(ui);
        if let Some(r) = out.rect(icon_id(&self.key)) {
            paint(ui, r, &self.key, out.response.enabled());
        }
        out.response
    }
}

fn paint(ui: &Ui, r: Rect, key: &str, enabled: bool) {
    if !ui.is_rect_visible(r) {
        return;
    }
    let color = crate::ui::propbar::icon_color(enabled);
    draw(ui.painter(), r, key, color);
}

/// The icon versions of egui's text buttons, by language key.
pub trait IconUi {
    /// `ui.button(tr(key))` with the key's picture.
    fn ibutton(&mut self, key: &str) -> Response;
    /// `ui.selectable_label(on, tr(key))` with the key's picture.
    fn iselectable(&mut self, on: bool, key: &str) -> Response;
    /// `ui.toggle_value(value, tr(key))` with the key's picture.
    fn itoggle(&mut self, value: &mut bool, key: &str) -> Response;
    /// `ui.menu_button(tr(key), add)` with the key's picture.
    fn imenu_button<R>(
        &mut self,
        key: &str,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> egui::InnerResponse<Option<R>>;
}

impl IconUi for Ui {
    fn ibutton(&mut self, key: &str) -> Response {
        self.add(Ib::new(key))
    }

    fn iselectable(&mut self, on: bool, key: &str) -> Response {
        self.add(Ib::selectable(on, key))
    }

    fn itoggle(&mut self, value: &mut bool, key: &str) -> Response {
        let mut r = self.add(Ib::selectable(*value, key));
        if r.clicked() {
            *value = !*value;
            r.mark_changed();
        }
        r
    }

    fn imenu_button<R>(
        &mut self,
        key: &str,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> egui::InnerResponse<Option<R>> {
        let side = icon_side(self);
        let atoms = b_atoms(key, side, tr(key).into());
        let (response, inner) = if egui::containers::menu::is_in_menu(self) {
            egui::containers::menu::SubMenuButton::new(atoms).ui(self, add)
        } else {
            egui::containers::menu::MenuButton::new(atoms).ui(self, add)
        };
        if has(key) {
            // The picture sits first, after the button's padding.
            let pad = self.spacing().button_padding;
            let r = Rect::from_min_size(
                Pos2::new(
                    response.rect.min.x + pad.x,
                    response.rect.center().y - side / 2.0,
                ),
                Vec2::splat(side),
            );
            paint(self, r, key, response.enabled());
        }
        egui::InnerResponse::new(inner.map(|i| i.inner), response)
    }
}

fn q(r: Rect, x: f32, y: f32) -> Pos2 {
    r.min + Vec2::new(x, y) * (r.width() / 16.0)
}

const RED: Color32 = Color32::from_rgb(0xD0, 0x2B, 0x2B);
const GREEN: Color32 = Color32::from_rgb(0x1E, 0x9E, 0x3C);
const GOLD: Color32 = Color32::from_rgb(0xE8, 0xA3, 0x17);

/// Draw one of the pictures of [`B`] on its 16 by 16 grid.
pub fn draw_b(painter: &Painter, r: Rect, b: B, color: Color32) {
    use self::B::*;
    let k = r.width() / 16.0;
    let s = Stroke::new(1.2 * k, color);
    let thin = Stroke::new(0.9 * k, color);
    let bold = Stroke::new(1.7 * k, color);
    let enabled = color == Tokens::icon();
    let accent = |c: Color32| if enabled { c } else { color };
    let blue = accent(crate::ui::icons::accent());
    let paper = Tokens::field();
    let line = |a: (f32, f32), b: (f32, f32), st: Stroke| {
        painter.line_segment([q(r, a.0, a.1), q(r, b.0, b.1)], st);
    };
    let poly = |pts: &[(f32, f32)], st: Stroke| {
        let _ = painter.add(epaint::PathShape::line(
            pts.iter().map(|(x, y)| q(r, *x, *y)).collect(),
            st,
        ));
    };
    let closed = |pts: &[(f32, f32)], fill: Color32, st: Stroke| {
        let _ = painter.add(epaint::PathShape::convex_polygon(
            pts.iter().map(|(x, y)| q(r, *x, *y)).collect(),
            fill,
            st,
        ));
    };
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32, fill: Color32, st: Stroke| {
        painter.rect(
            Rect::from_min_max(q(r, x0, y0), q(r, x1, y1)),
            0.0,
            fill,
            st,
            egui::StrokeKind::Middle,
        );
    };
    let circle = |x: f32, y: f32, rad: f32, fill: Color32, st: Stroke| {
        painter.circle(q(r, x, y), rad * k, fill, st);
    };
    let head = |b: (f32, f32), d: (f32, f32), c: Color32| {
        let v = Vec2::new(d.0, d.1).normalized() * 3.0;
        let n = Vec2::new(-v.y, v.x) * 0.75;
        closed(
            &[
                b,
                (b.0 - v.x + n.x, b.1 - v.y + n.y),
                (b.0 - v.x - n.x, b.1 - v.y - n.y),
            ],
            c,
            Stroke::NONE,
        );
    };
    let arrow = |a: (f32, f32), b: (f32, f32), st: Stroke| {
        let v = Vec2::new(b.0 - a.0, b.1 - a.1).normalized() * 2.0;
        line(a, (b.0 - v.x, b.1 - v.y), st);
        head(b, (b.0 - a.0, b.1 - a.1), st.color);
    };
    // An arc around (cx, cy) from angle a0 to a1 (radians, clockwise on
    // screen), with an arrow head at its end when `tip`.
    let arc = |cx: f32, cy: f32, rad: f32, a0: f32, a1: f32, st: Stroke, tip: bool| {
        let n = 16;
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                (cx + rad * a.cos(), cy + rad * a.sin())
            })
            .collect();
        poly(&pts, st);
        if tip {
            let e = pts[n];
            let p = pts[n - 2];
            head((e.0 + (e.0 - p.0) * 0.6, e.1 + (e.1 - p.1) * 0.6), (e.0 - p.0, e.1 - p.1), st.color);
        }
    };
    let page = |x0: f32, y0: f32, x1: f32, y1: f32| {
        closed(
            &[(x0, y0), (x1 - 3.0, y0), (x1, y0 + 3.0), (x1, y1), (x0, y1)],
            paper,
            s,
        );
        poly(&[(x1 - 3.0, y0), (x1 - 3.0, y0 + 3.0), (x1, y0 + 3.0)], thin);
    };
    let plus = |cx: f32, cy: f32, h: f32, c: Color32| {
        let st = Stroke::new(1.7 * k, c);
        line((cx - h, cy), (cx + h, cy), st);
        line((cx, cy - h), (cx, cy + h), st);
    };
    let minus = |cx: f32, cy: f32, h: f32, c: Color32| {
        line((cx - h, cy), (cx + h, cy), Stroke::new(1.7 * k, c));
    };
    let cross = |cx: f32, cy: f32, h: f32, c: Color32| {
        let st = Stroke::new(1.7 * k, c);
        line((cx - h, cy - h), (cx + h, cy + h), st);
        line((cx - h, cy + h), (cx + h, cy - h), st);
    };
    let folder = |fill: Color32| {
        closed(
            &[(1.5, 4.0), (6.0, 4.0), (7.5, 5.5), (14.5, 5.5), (14.5, 13.0), (1.5, 13.0)],
            fill,
            s,
        );
    };
    let star = |cx: f32, cy: f32, ro: f32, fill: Color32| {
        let pts: Vec<Pos2> = (0..10)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                let rr = if i % 2 == 0 { ro } else { ro * 0.45 };
                q(r, cx + rr * a.cos(), cy + rr * a.sin())
            })
            .collect();
        painter.add(egui::Shape::Path(epaint::PathShape::closed_line(pts.clone(), s)));
        // Fill as a fan of triangles, the star not being convex.
        for i in 0..10 {
            painter.add(egui::Shape::convex_polygon(
                vec![q(r, cx, cy), pts[i], pts[(i + 1) % 10]],
                fill,
                Stroke::NONE,
            ));
        }
    };
    let eye = |cx: f32, cy: f32| {
        let pts: Vec<(f32, f32)> = (0..=12)
            .map(|i| {
                let t = i as f32 / 12.0 * std::f32::consts::PI;
                (cx - 6.5 * t.cos(), cy - 4.0 * t.sin())
            })
            .chain((0..=12).map(|i| {
                let t = i as f32 / 12.0 * std::f32::consts::PI;
                (cx + 6.5 * t.cos(), cy + 4.0 * t.sin())
            }))
            .collect();
        poly(&pts, s);
        circle(cx, cy, 2.2, blue, Stroke::NONE);
    };
    let pencil = |x0: f32, y0: f32, len: f32| {
        // Drawn from its tip at (x0, y0) up to the right.
        let d = len / std::f32::consts::SQRT_2;
        closed(
            &[
                (x0, y0),
                (x0 + 1.0, y0 - 3.0),
                (x0 + d + 1.0, y0 - d - 3.0),
                (x0 + d + 3.0, y0 - d - 1.0),
                (x0 + 3.0, y0 - 1.0),
            ],
            accent(GOLD),
            s,
        );
        line((x0 + 1.0, y0 - 3.0), (x0 + 3.0, y0 - 1.0), thin);
    };
    match b {
        Plus => plus(8.0, 8.0, 5.5, accent(GREEN)),
        Minus => minus(8.0, 8.0, 5.5, accent(RED)),
        Reset => {
            arc(8.0, 8.5, 5.0, -2.3, 3.0, s, false);
            head((2.4, 4.2), (-1.0, -1.6), color);
            circle(8.0, 8.5, 1.2, color, Stroke::NONE);
        }
        Refresh => {
            arc(8.0, 8.0, 5.5, -2.6, 0.3, s, true);
            arc(8.0, 8.0, 5.5, 0.55, 3.45, s, true);
        }
        Dice => {
            painter.rect(
                Rect::from_min_max(q(r, 2.0, 2.0), q(r, 14.0, 14.0)),
                2.0 * k,
                paper,
                s,
                egui::StrokeKind::Middle,
            );
            for (x, y) in [(5.0, 5.0), (11.0, 5.0), (8.0, 8.0), (5.0, 11.0), (11.0, 11.0)] {
                circle(x, y, 1.2, color, Stroke::NONE);
            }
        }
        Shuffle => {
            poly(&[(1.5, 4.5), (5.0, 4.5), (10.0, 11.5), (13.0, 11.5)], s);
            poly(&[(1.5, 11.5), (5.0, 11.5), (10.0, 4.5), (13.0, 4.5)], s);
            head((15.0, 4.5), (1.0, 0.0), color);
            head((15.0, 11.5), (1.0, 0.0), color);
        }
        Check => poly(
            &[(2.5, 8.5), (6.5, 12.5), (13.5, 3.5)],
            Stroke::new(2.0 * k, accent(GREEN)),
        ),
        Cross => cross(8.0, 8.0, 4.5, accent(RED)),
        Clear => {
            // An eraser on a line it rubs out.
            closed(
                &[(3.0, 10.0), (9.0, 3.5), (13.5, 7.5), (7.5, 14.0)],
                paper,
                s,
            );
            closed(
                &[(3.0, 10.0), (5.5, 7.3), (10.0, 11.4), (7.5, 14.0)],
                accent(Color32::from_rgb(0xE0, 0x7A, 0x9A)),
                s,
            );
            line((9.0, 14.5), (15.0, 14.5), thin);
        }
        Pencil => {
            pencil(2.5, 13.5, 10.0);
        }
        Rename => {
            rect(1.5, 4.0, 14.5, 12.0, paper, s);
            line((5.0, 6.0), (5.0, 10.0), bold);
            line((4.0, 6.0), (6.0, 6.0), thin);
            line((4.0, 10.0), (6.0, 10.0), thin);
            line((8.0, 8.0), (12.5, 8.0), Stroke::new(1.2 * k, blue));
        }
        Play => closed(
            &[(4.5, 2.5), (13.5, 8.0), (4.5, 13.5)],
            accent(GREEN),
            Stroke::NONE,
        ),
        Stop => {
            painter.rect_filled(
                Rect::from_min_max(q(r, 3.5, 3.5), q(r, 12.5, 12.5)),
                1.0 * k,
                accent(RED),
            );
        }
        Swap => {
            arrow((2.0, 5.0), (14.0, 5.0), s);
            arrow((14.0, 11.0), (2.0, 11.0), Stroke::new(1.2 * k, blue));
        }
        FindNext | FindPrev => {
            circle(6.5, 6.5, 4.0, paper, s);
            line((9.5, 9.5), (12.5, 12.5), bold);
            if b == FindNext {
                head((15.5, 6.5), (1.0, 0.0), blue);
                line((11.5, 6.5), (13.5, 6.5), Stroke::new(1.2 * k, blue));
            } else {
                head((0.5, 13.5), (-1.0, 0.0), blue);
                line((4.5, 13.5), (2.5, 13.5), Stroke::new(1.2 * k, blue));
            }
        }
        Eye => eye(8.0, 8.0),
        Magnet => {
            arc(8.0, 7.0, 5.0, std::f32::consts::PI, std::f32::consts::TAU, Stroke::new(3.0 * k, accent(RED)), false);
            line((3.0, 7.0), (3.0, 11.0), Stroke::new(3.0 * k, accent(RED)));
            line((13.0, 7.0), (13.0, 11.0), Stroke::new(3.0 * k, accent(RED)));
            line((1.5, 12.5), (4.5, 12.5), bold);
            line((11.5, 12.5), (14.5, 12.5), bold);
        }
        ArrowUp => arrow((8.0, 14.0), (8.0, 2.0), bold),
        ArrowDown => arrow((8.0, 2.0), (8.0, 14.0), bold),
        Star => star(8.0, 8.5, 6.5, accent(GOLD)),
        StarOff => {
            star(8.0, 8.5, 6.5, paper);
            line((2.0, 14.0), (14.0, 2.0), Stroke::new(1.5 * k, accent(RED)));
        }
        Flask => {
            closed(
                &[(6.0, 2.0), (10.0, 2.0), (10.0, 6.5), (14.0, 14.0), (2.0, 14.0), (6.0, 6.5)],
                paper,
                s,
            );
            closed(
                &[(4.2, 10.0), (11.8, 10.0), (14.0, 14.0), (2.0, 14.0)],
                blue,
                Stroke::NONE,
            );
        }
        Gauge => {
            arc(8.0, 10.0, 6.5, std::f32::consts::PI, std::f32::consts::TAU, s, false);
            line((1.5, 10.0), (14.5, 10.0), s);
            line((8.0, 10.0), (12.0, 5.5), Stroke::new(1.6 * k, accent(RED)));
            circle(8.0, 10.0, 1.4, color, Stroke::NONE);
        }
        Download | Upload | Install | Uninstall => {
            poly(&[(2.0, 10.0), (2.0, 14.0), (14.0, 14.0), (14.0, 10.0)], s);
            let c = match b {
                Uninstall => accent(RED),
                Install | Download => accent(GREEN),
                _ => blue,
            };
            let st = Stroke::new(1.7 * k, c);
            if matches!(b, Download | Install) {
                arrow((8.0, 1.5), (8.0, 11.5), st);
            } else {
                arrow((8.0, 11.5), (8.0, 1.5), st);
            }
        }
        Wand => {
            line((2.0, 14.0), (10.0, 6.0), Stroke::new(2.0 * k, color));
            for (a, bb) in [
                ((12.0, 1.0), (12.0, 4.0)),
                ((10.5, 2.5), (13.5, 2.5)),
                ((14.0, 6.0), (14.0, 8.0)),
                ((13.0, 7.0), (15.0, 7.0)),
                ((7.0, 2.0), (7.0, 4.0)),
                ((6.0, 3.0), (8.0, 3.0)),
            ] {
                line(a, bb, Stroke::new(1.1 * k, accent(GOLD)));
            }
        }
        Light => {
            circle(8.0, 6.5, 4.5, accent(Color32::from_rgb(0xFF, 0xD8, 0x4A)), s);
            rect(6.0, 11.0, 10.0, 14.0, paper, s);
            line((6.0, 12.5), (10.0, 12.5), thin);
        }
        Picture => {
            rect(1.5, 2.5, 14.5, 13.5, paper, s);
            closed(
                &[(2.5, 12.5), (6.5, 7.0), (9.5, 10.5)],
                accent(GREEN),
                Stroke::NONE,
            );
            closed(
                &[(7.0, 12.5), (10.5, 8.0), (13.5, 12.5)],
                accent(Color32::from_rgb(0x2E, 0x7D, 0x32)),
                Stroke::NONE,
            );
            circle(11.0, 5.5, 1.5, accent(GOLD), Stroke::NONE);
        }
        Folder => folder(accent(Color32::from_rgb(0xF2, 0xC9, 0x4C))),
        FolderPlus => {
            folder(accent(Color32::from_rgb(0xF2, 0xC9, 0x4C)));
            plus(11.0, 9.5, 2.5, accent(GREEN));
        }
        Chat => {
            painter.rect(
                Rect::from_min_max(q(r, 1.5, 2.0), q(r, 14.5, 11.0)),
                2.5 * k,
                paper,
                s,
                egui::StrokeKind::Middle,
            );
            closed(&[(4.0, 10.5), (7.0, 10.5), (3.5, 14.5)], color, Stroke::NONE);
            for x in [5.0, 8.0, 11.0] {
                circle(x, 6.5, 0.9, blue, Stroke::NONE);
            }
        }
        Plug => {
            rect(4.0, 5.0, 12.0, 10.0, paper, s);
            line((6.0, 1.5), (6.0, 5.0), bold);
            line((10.0, 1.5), (10.0, 5.0), bold);
            poly(&[(8.0, 10.0), (8.0, 12.5), (11.0, 14.5)], s);
        }
        Bookmark => closed(
            &[(4.0, 1.5), (12.0, 1.5), (12.0, 14.5), (8.0, 11.0), (4.0, 14.5)],
            blue,
            s,
        ),
        List => {
            for y in [4.0, 8.0, 12.0] {
                circle(3.0, y, 1.1, blue, Stroke::NONE);
                line((6.0, y), (14.0, y), s);
            }
        }
        Book => {
            closed(&[(2.5, 2.0), (12.5, 2.0), (12.5, 14.0), (2.5, 14.0)], paper, s);
            line((4.5, 2.0), (4.5, 14.0), thin);
            line((6.5, 5.5), (10.5, 5.5), thin);
            line((6.5, 7.5), (10.5, 7.5), thin);
        }
        Hyphen => {
            painter.text(
                q(r, 8.0, 8.0),
                egui::Align2::CENTER_CENTER,
                "a-b",
                egui::FontId::proportional(8.5 * k),
                color,
            );
        }
        Spell => {
            painter.text(
                q(r, 7.0, 5.5),
                egui::Align2::CENTER_CENTER,
                "abc",
                egui::FontId::proportional(7.5 * k),
                color,
            );
            poly(&[(5.0, 11.0), (8.0, 14.0), (14.0, 8.5)], Stroke::new(1.7 * k, accent(GREEN)));
        }
        Pin => {
            circle(8.0, 5.0, 3.5, accent(RED), s);
            line((8.0, 8.5), (8.0, 15.0), s);
        }
        Feather => {
            closed(&[(13.5, 2.0), (9.0, 4.0), (5.0, 9.0), (4.5, 11.5), (7.0, 11.0), (12.0, 7.0)], paper, s);
            line((2.0, 14.0), (11.0, 5.0), thin);
        }
        Border => {
            rect(2.0, 2.0, 14.0, 14.0, Color32::TRANSPARENT, s);
            rect(5.0, 5.0, 11.0, 11.0, Color32::TRANSPARENT, Stroke::new(1.0 * k, blue));
        }
        Target => {
            circle(8.0, 8.0, 5.5, Color32::TRANSPARENT, s);
            circle(8.0, 8.0, 2.0, accent(RED), Stroke::NONE);
            line((8.0, 0.5), (8.0, 4.0), thin);
            line((8.0, 12.0), (8.0, 15.5), thin);
            line((0.5, 8.0), (4.0, 8.0), thin);
            line((12.0, 8.0), (15.5, 8.0), thin);
        }
        Screen => {
            rect(1.5, 2.0, 14.5, 11.0, paper, s);
            line((8.0, 11.0), (8.0, 14.0), s);
            line((5.0, 14.0), (11.0, 14.0), s);
            circle(8.0, 6.5, 2.0, blue, Stroke::NONE);
        }
        Field => {
            painter.text(
                q(r, 8.0, 8.0),
                egui::Align2::CENTER_CENTER,
                "{ }",
                egui::FontId::monospace(9.0 * k),
                blue,
            );
        }
        ColumnAdd => {
            rect(2.0, 2.0, 10.0, 14.0, paper, s);
            line((6.0, 2.0), (6.0, 14.0), thin);
            plus(12.5, 8.0, 2.5, accent(GREEN));
        }
        Parallel => {
            poly(&[(2.0, 12.0), (8.0, 6.0), (14.0, 6.0)], s);
            poly(&[(2.0, 15.0), (9.0, 9.0), (14.0, 9.0)], Stroke::new(1.2 * k, blue));
        }
        Radial => {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                line((8.0, 8.0), (8.0 + 6.5 * a.cos(), 8.0 + 6.5 * a.sin()), thin);
            }
            circle(8.0, 8.0, 3.5, Color32::TRANSPARENT, s);
            circle(8.0, 8.0, 6.5, Color32::TRANSPARENT, s);
        }
        Cells => {
            rect(2.0, 2.0, 14.0, 14.0, paper, s);
            for t in [6.0, 10.0] {
                line((t, 2.0), (t, 14.0), thin);
                line((2.0, t), (14.0, t), thin);
            }
        }
        Log => {
            rect(2.0, 2.0, 14.0, 14.0, paper, thin);
            for t in [4.0, 7.0, 9.0, 10.5, 11.5, 12.3, 13.0] {
                line((t, 2.0), (t, 14.0), thin);
            }
        }
        Bulge => {
            rect(2.0, 3.0, 14.0, 13.0, Color32::TRANSPARENT, thin);
            circle(8.0, 8.0, 4.5, blue, s);
        }
        Hairline => {
            line((2.0, 8.0), (14.0, 8.0), Stroke::new(0.6 * k, color));
            line((2.0, 4.0), (14.0, 4.0), Stroke::new(0.6 * k, color.gamma_multiply(0.5)));
            line((2.0, 12.0), (14.0, 12.0), Stroke::new(0.6 * k, color.gamma_multiply(0.5)));
        }
        Anchor => {
            circle(8.0, 3.5, 1.8, Color32::TRANSPARENT, s);
            line((8.0, 5.3), (8.0, 14.0), s);
            line((5.0, 7.5), (11.0, 7.5), s);
            arc(8.0, 9.0, 5.5, 0.3, std::f32::consts::PI - 0.3, s, false);
        }
        Page => page(3.0, 1.5, 13.0, 14.5),
        PagePlus => {
            page(2.0, 1.5, 11.5, 14.5);
            plus(12.0, 11.0, 3.0, accent(GREEN));
        }
        MoveTo => {
            page(7.0, 1.5, 15.0, 14.5);
            arrow((1.0, 8.0), (10.0, 8.0), Stroke::new(1.6 * k, blue));
        }
        Clockwise => arc(8.0, 8.0, 5.5, -2.8, 1.6, s, true),
        CounterClockwise => arc(8.0, 8.0, 5.5, -0.34, -4.74, s, true),
        FlipArc => {
            arc(8.0, 10.0, 5.5, std::f32::consts::PI, std::f32::consts::TAU, s, false);
            arc(8.0, 6.0, 5.5, 0.0, std::f32::consts::PI, Stroke::new(1.2 * k, blue), false);
        }
        Smooth => {
            let pts: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let x = 1.5 + i as f32 * 13.0 / 16.0;
                    (x, 8.0 + 4.0 * ((x - 1.5) / 13.0 * std::f32::consts::TAU).sin())
                })
                .collect();
            poly(&pts, s);
        }
        Grow => {
            rect(5.0, 5.0, 11.0, 11.0, paper, s);
            for (a, bb) in [
                ((4.0, 4.0), (1.0, 1.0)),
                ((12.0, 4.0), (15.0, 1.0)),
                ((4.0, 12.0), (1.0, 15.0)),
                ((12.0, 12.0), (15.0, 15.0)),
            ] {
                arrow(a, bb, Stroke::new(1.1 * k, blue));
            }
        }
        Invert => {
            rect(2.0, 2.0, 14.0, 14.0, Color32::TRANSPARENT, Stroke::new(1.0 * k, color));
            painter.rect_filled(Rect::from_min_max(q(r, 2.0, 2.0), q(r, 8.0, 14.0)), 0.0, color);
            circle(8.0, 8.0, 3.5, Color32::TRANSPARENT, Stroke::new(1.2 * k, blue));
        }
        SelectNone => {
            for i in 0..6 {
                let t = 2.0 + i as f32 * 2.0;
                line((t, 2.0), (t + 1.0, 2.0), s);
                line((t, 14.0), (t + 1.0, 14.0), s);
                line((2.0, t), (2.0, t + 1.0), s);
                line((14.0, t), (14.0, t + 1.0), s);
            }
            cross(8.0, 8.0, 3.0, accent(RED));
        }
        Mask => {
            rect(1.5, 2.5, 14.5, 13.5, paper, s);
            for (i, y) in [2.5_f32, 5.25, 8.0, 10.75].into_iter().enumerate() {
                for (j, x) in [1.5_f32, 4.75, 8.0, 11.25].into_iter().enumerate() {
                    if (i + j) % 2 == 0 {
                        painter.rect_filled(
                            Rect::from_min_max(q(r, x, y), q(r, x + 3.25, (y + 2.75).min(13.5))),
                            0.0,
                            color.gamma_multiply(0.35),
                        );
                    }
                }
            }
            circle(8.0, 8.0, 3.5, blue, s);
        }
        Layer => {
            closed(&[(8.0, 2.0), (15.0, 6.0), (8.0, 10.0), (1.0, 6.0)], paper, s);
            poly(&[(1.0, 9.0), (8.0, 13.0), (15.0, 9.0)], s);
        }
        Merge => {
            poly(&[(2.0, 2.0), (8.0, 8.0), (8.0, 14.0)], s);
            poly(&[(14.0, 2.0), (8.0, 8.0)], s);
            head((8.0, 15.0), (0.0, 1.0), color);
        }
        Palette | PaletteAdd | Swatches => {
            let cs = [
                Color32::from_rgb(0xE5, 0x39, 0x35),
                Color32::from_rgb(0xFD, 0xD8, 0x35),
                Color32::from_rgb(0x43, 0xA0, 0x47),
                Color32::from_rgb(0x1E, 0x88, 0xE5),
            ];
            for (i, c) in cs.iter().enumerate() {
                let x = 1.5 + (i % 2) as f32 * 6.0;
                let y = 1.5 + (i / 2) as f32 * 6.0;
                rect(x, y, x + 5.0, y + 5.0, accent(*c), thin);
            }
            match b {
                PaletteAdd => plus(12.5, 12.5, 3.0, accent(GREEN)),
                Swatches => cross(12.5, 12.5, 2.3, accent(RED)),
                _ => {}
            }
        }
        Data => {
            rect(1.5, 2.0, 14.5, 14.0, paper, s);
            line((1.5, 6.0), (14.5, 6.0), s);
            line((1.5, 10.0), (14.5, 10.0), thin);
            line((6.0, 2.0), (6.0, 14.0), thin);
        }
        None => {
            circle(8.0, 8.0, 5.5, Color32::TRANSPARENT, s);
            line((4.2, 11.8), (11.8, 4.2), Stroke::new(1.5 * k, accent(RED)));
        }
        Custom => {
            rect(2.0, 3.0, 12.0, 13.0, paper, Stroke::new(1.0 * k, color));
            pencil(8.0, 14.5, 7.0);
        }
        Csv => {
            page(2.0, 1.5, 14.0, 14.5);
            painter.text(
                q(r, 8.0, 9.5),
                egui::Align2::CENTER_CENTER,
                "csv",
                egui::FontId::proportional(5.5 * k),
                blue,
            );
        }
        Text => {
            painter.text(
                q(r, 8.0, 8.5),
                egui::Align2::CENTER_CENTER,
                "T",
                egui::FontId::proportional(14.0 * k),
                color,
            );
        }
        Gap => {
            line((1.0, 8.0), (6.0, 8.0), bold);
            line((10.0, 8.0), (15.0, 8.0), bold);
            line((6.0, 4.0), (6.0, 12.0), Stroke::new(1.0 * k, blue));
            line((10.0, 4.0), (10.0, 12.0), Stroke::new(1.0 * k, blue));
        }
        Split => {
            rect(2.0, 3.0, 14.0, 13.0, paper, s);
            line((9.5, 1.0), (6.5, 15.0), Stroke::new(1.4 * k, accent(RED)));
        }
        Harmony(kind) => {
            circle(8.0, 8.0, 6.5, Color32::TRANSPARENT, thin);
            let top = -std::f32::consts::FRAC_PI_2;
            let pi = std::f32::consts::PI;
            let angles: &[f32] = match kind {
                1 => &[top - 0.75, top, top + 0.75],
                2 => &[top, top + pi],
                3 => &[top, top + pi - 0.6, top + pi + 0.6],
                4 => &[top, top + pi * 2.0 / 3.0, top + pi * 4.0 / 3.0],
                5 => &[top, top + pi / 2.0, top + pi, top + pi * 1.5],
                _ => &[],
            };
            for &a in angles {
                let h = (a / std::f32::consts::TAU).rem_euclid(1.0);
                let c = Color32::from(egui::ecolor::Hsva::new(h, 0.85, 0.95, 1.0));
                circle(8.0 + 5.0 * a.cos(), 8.0 + 5.0 * a.sin(), 2.0, accent(c), Stroke::new(0.6 * k, color));
            }
            if kind == 0 {
                for (i, rad) in [2.2_f32, 1.8, 1.4].into_iter().enumerate() {
                    let c = Color32::from(egui::ecolor::Hsva::new(0.6, 0.85, 0.4 + 0.25 * i as f32, 1.0));
                    circle(4.5 + i as f32 * 3.5, 8.0, rad, accent(c), Stroke::NONE);
                }
            }
        }
        Tone(t) => {
            let shades = [0.15_f32, 0.5, 0.85];
            for (i, v) in shades.iter().enumerate() {
                let g = (v * 255.0) as u8;
                let x = 1.5 + i as f32 * 4.5;
                rect(x, 4.0, x + 4.0, 12.0, accent(Color32::from_gray(g)), thin);
            }
            let x = 1.5 + t as f32 * 4.5;
            rect(x - 0.5, 2.5, x + 4.5, 13.5, Color32::TRANSPARENT, Stroke::new(1.4 * k, blue));
        }
        Channel(c) => {
            let col = match c {
                0 => Color32::from_rgb(0xE5, 0x39, 0x35),
                1 => Color32::from_rgb(0x43, 0xA0, 0x47),
                2 => Color32::from_rgb(0x1E, 0x88, 0xE5),
                _ => Color32::from_gray(0xB0),
            };
            if c == 3 {
                for (i, y) in [3.0_f32, 6.5, 10.0].into_iter().enumerate() {
                    for (j, x) in [3.0_f32, 6.5, 10.0].into_iter().enumerate() {
                        if (i + j) % 2 == 0 {
                            rect(x, y, x + 3.5, y + 3.5, accent(col), Stroke::NONE);
                        }
                    }
                }
                rect(3.0, 3.0, 13.5, 13.5, Color32::TRANSPARENT, thin);
            } else {
                circle(8.0, 8.0, 5.5, accent(col), thin);
            }
        }
        Pause => {
            painter.rect_filled(Rect::from_min_max(q(r, 3.5, 3.0), q(r, 6.5, 13.0)), 0.5 * k, blue);
            painter.rect_filled(Rect::from_min_max(q(r, 9.5, 3.0), q(r, 12.5, 13.0)), 0.5 * k, blue);
        }
        Zigzag => poly(
            &[(1.5, 8.0), (3.5, 4.0), (5.5, 12.0), (7.5, 4.0), (9.5, 12.0), (11.5, 4.0), (13.5, 12.0), (14.5, 8.0)],
            s,
        ),
        Arrow => arrow((2.5, 13.5), (13.5, 2.5), Stroke::new(1.6 * k, blue)),
        ContourIn | ContourOut | ContourCenter | ContourBoth => {
            rect(5.5, 5.5, 10.5, 10.5, paper, s);
            let ring = Stroke::new(1.0 * k, blue);
            if matches!(b, ContourOut | ContourBoth) {
                rect(2.0, 2.0, 14.0, 14.0, Color32::TRANSPARENT, ring);
            }
            if matches!(b, ContourIn | ContourBoth | ContourCenter) {
                rect(7.5, 7.5, 8.5, 8.5, Color32::TRANSPARENT, ring);
            }
            if b == ContourCenter {
                circle(8.0, 8.0, 0.9, accent(RED), Stroke::NONE);
                rect(3.5, 3.5, 12.5, 12.5, Color32::TRANSPARENT, Stroke::new(0.8 * k, color));
            }
        }
        Code => {
            poly(&[(5.0, 4.0), (1.5, 8.0), (5.0, 12.0)], s);
            poly(&[(11.0, 4.0), (14.5, 8.0), (11.0, 12.0)], s);
            line((9.5, 3.0), (6.5, 13.0), Stroke::new(1.2 * k, blue));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keys of the bars' buttons whose names come from lists, which
    /// the source scan below cannot read.
    const LISTED: &[&str] = &[
        "connector.straight",
        "connector.right_angle",
        "connector.rounded",
        "docker.fill_none",
        "docker.fill_uniform",
        "docker.fill_fountain",
        "docker.fill_pattern",
        "docker.fill_texture",
        "docker.fill_mesh",
        "docker.fill_procedural",
        "toolbar.draw_inside",
        "toolbar.draw_behind",
        "toolbar.copy_fill",
        "toolbar.push_pull",
        "toolbar.zipper",
        "toolbar.twister",
        "toolbar.ft_rotation",
        "toolbar.ft_reflection",
        "toolbar.ft_scale",
        "toolbar.ft_skew",
        "knife.freehand",
        "knife.two_point",
        "segment_delete.marquee",
        "segment_delete.freehand",
        "docker.inside",
        "docker.outside",
        "docker.to_center",
        "docker.contour_both",
        "balloon.speech",
        "comments.pin",
        "comments.rect",
        "comments.arrow",
        "toolbar.zoom_to_fit",
    ];

    #[test]
    fn listed_bar_buttons_have_pictures() {
        for k in LISTED {
            assert!(has(k), "{k} has no picture");
        }
    }

    /// Every command button that names its key in the source has a
    /// picture, and no text-only button form is left outside the
    /// answers of a dialog.
    #[test]
    fn no_text_only_command_buttons() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut files = Vec::new();
        let mut dirs = vec![std::path::PathBuf::from(root)];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    files.push(p);
                }
            }
        }
        let plain = [
            ".button(tr(\"",
            "Button::new(tr(\"",
            "menu_button(tr(\"",
        ];
        let mut missing = Vec::new();
        let mut plain_left = Vec::new();
        for f in files {
            if f.ends_with("btn_icons.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&f).unwrap();
            for key in icon_keys(&src) {
                if !has(&key) && !TEXT_ONLY.contains(&key.as_str()) {
                    missing.push(format!("{}: {key}", f.display()));
                }
            }
            for p in plain {
                let mut rest = src.as_str();
                while let Some(i) = rest.find(p) {
                    rest = &rest[i + p.len()..];
                    let key: String = rest.chars().take_while(|c| *c != '"').collect();
                    if !TEXT_ONLY.contains(&key.as_str()) {
                        plain_left.push(format!("{}: {key}", f.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "buttons without a picture: {missing:#?}");
        assert!(plain_left.is_empty(), "text-only buttons: {plain_left:#?}");
    }

    /// The literal keys passed to the icon buttons in `src`: the last
    /// argument of each call, when it is a string.
    fn icon_keys(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for call in [
            ".ibutton(",
            ".iselectable(",
            ".itoggle(",
            ".imenu_button(",
            "Ib::new(",
            "Ib::selectable(",
            "Ib::with_text(",
        ] {
            let mut rest = src;
            while let Some(i) = rest.find(call) {
                rest = &rest[i + call.len()..];
                // The arguments up to the matching parenthesis.
                let mut depth = 1;
                let mut end = 0;
                for (j, c) in rest.char_indices() {
                    match c {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = j;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let args = &rest[..end];
                // The key: the menu button's first argument, the
                // selectable's second, else the first.
                let parts: Vec<&str> = split_top(args);
                let at = match call {
                    ".iselectable(" | ".itoggle(" | "Ib::selectable(" => 1,
                    _ => 0,
                };
                if let Some(a) = parts.get(at) {
                    let a = a.trim();
                    if let Some(k) = a.strip_prefix('"').and_then(|a| a.strip_suffix('"')) {
                        out.push(k.to_owned());
                    }
                }
            }
        }
        out
    }

    fn split_top(args: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let (mut depth, mut start) = (0, 0);
        for (j, c) in args.char_indices() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(&args[start..j]);
                    start = j + 1;
                }
                _ => {}
            }
        }
        out.push(&args[start..]);
        out
    }

    #[test]
    fn icon_keys_reads_each_form() {
        let src = r#"ui.ibutton("a.b"); ui.iselectable(x == f(1, 2), "c.d"); ui.imenu_button("e.f", |ui| {});"#;
        assert_eq!(icon_keys(src), ["a.b", "c.d", "e.f"]);
    }

    #[test]
    fn every_picture_draws() {
        let ctx = egui::Context::default();
        let painter = Painter::new(ctx, egui::LayerId::background(), Rect::EVERYTHING);
        let r = Rect::from_min_size(Pos2::ZERO, Vec2::splat(16.0));
        for k in LISTED.iter().chain(TEXT_ONLY) {
            assert_eq!(draw(&painter, r, k, Tokens::icon()), has(k), "{k}");
        }
    }
}

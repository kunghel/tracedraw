//! The image editing mode: a double click on a picture (Bitmaps > Edit
//! Inside) gives the whole window over to its pixels, as the print
//! preview does to the page. The window keeps its bars, palettes and
//! panels, moved and sized as ever, with the picture's content in them:
//! the image menus, a toolbox of the image tools with their icons, the
//! standard bar with Finish, Cancel, Undo, Redo and the zoom, the tool's
//! options in the property bar, the image panels (Layers, Effects,
//! Selection, Retouch, Distort, Picture, Histogram) and a status bar of
//! the picture. The picture shows alone, upright, at a zoom of its own.
//! See docs/behavior/edit-inside-picture.md.

use crate::app::{App, DockerTab};
use crate::i18n::tr;
use crate::pixel_edit::PixelTool;
use crate::theme::Tokens;
use crate::ui::bars::{Bar, Dock};
use crate::ui::menus::{check, check_pic, item, item_label, sep, sub};
use crate::ui::pixel_cmds::PixelCmd;
use crate::ui::tips::RichTip;
use egui::{Color32, Rect, Sense, Stroke, Ui, Vec2};

/// The toolbox buttons: each a group of tools, the first shown until
/// another is picked from its flyout. Pick first, then the selections,
/// the view, the retouching, the distortions, the paint, the
/// transparency and the colours.
pub const GROUPS: [&[PixelTool]; 13] = [
    &[PixelTool::Move],
    &[
        PixelTool::RectSelect,
        PixelTool::EllipseSelect,
        PixelTool::FreehandMask,
        PixelTool::Lasso,
        PixelTool::EdgeMask,
        PixelTool::MagicWand,
        PixelTool::MaskBrush,
        PixelTool::BandMask,
    ],
    &[PixelTool::Zoom, PixelTool::Pan],
    &[
        PixelTool::Clone,
        PixelTool::Heal,
        PixelTool::Remove,
        PixelTool::RedEye,
    ],
    &[
        PixelTool::Dodge,
        PixelTool::Burn,
        PixelTool::Sponge,
        PixelTool::Blur,
        PixelTool::Sharpen,
        PixelTool::Smudge,
        PixelTool::Tint,
    ],
    &[
        PixelTool::Smear,
        PixelTool::Twirl,
        PixelTool::Attract,
        PixelTool::Repel,
        PixelTool::Liquify,
        PixelTool::Warp,
    ],
    &[
        PixelTool::Brush,
        PixelTool::Sprayer,
        PixelTool::Restore,
        PixelTool::ReplaceColor,
    ],
    &[PixelTool::Eraser],
    &[PixelTool::Shape],
    &[
        PixelTool::LayerFade,
        PixelTool::ColorToClear,
        PixelTool::TransparencyBrush,
    ],
    &[PixelTool::Light],
    &[PixelTool::Eyedropper],
    &[PixelTool::Fill, PixelTool::Gradient],
];

/// A line after these groups.
const BREAKS: [usize; 4] = [1, 2, 5, 8];

/// Each tool's key (with Shift when the flag is on), shown in its
/// flyout and its tip.
pub const SHORTCUTS: [(PixelTool, egui::Key, bool); 41] = {
    use egui::Key as K;
    use PixelTool as T;
    [
        (T::Move, K::V, false),
        (T::RectSelect, K::R, false),
        (T::EllipseSelect, K::J, false),
        (T::FreehandMask, K::K, false),
        (T::Lasso, K::A, false),
        (T::EdgeMask, K::Num4, false),
        (T::MagicWand, K::W, false),
        (T::MaskBrush, K::B, false),
        (T::BandMask, K::Num8, false),
        (T::Zoom, K::Z, false),
        (T::Pan, K::H, false),
        (T::Clone, K::C, false),
        (T::Heal, K::C, true),
        (T::Remove, K::X, true),
        (T::RedEye, K::Y, false),
        (T::Dodge, K::O, false),
        (T::Burn, K::O, true),
        (T::Sponge, K::P, true),
        (T::Blur, K::B, true),
        (T::Sharpen, K::H, true),
        (T::Smudge, K::U, true),
        (T::Tint, K::T, true),
        (T::Smear, K::S, true),
        (T::Twirl, K::W, true),
        (T::Attract, K::A, true),
        (T::Repel, K::R, true),
        (T::Liquify, K::L, true),
        (T::Warp, K::D, false),
        (T::Brush, K::P, false),
        (T::Sprayer, K::I, false),
        (T::Restore, K::U, false),
        (T::ReplaceColor, K::Q, false),
        (T::Eraser, K::X, false),
        (T::Shape, K::F6, false),
        (T::LayerFade, K::Num1, false),
        (T::ColorToClear, K::Num2, false),
        (T::TransparencyBrush, K::Num3, false),
        (T::Light, K::L, false),
        (T::Eyedropper, K::E, false),
        (T::Fill, K::F, false),
        (T::Gradient, K::G, false),
    ]
};

/// A tool's key as shown ("Shift+S"), empty when it has none.
pub fn shortcut(t: PixelTool) -> String {
    SHORTCUTS
        .iter()
        .find(|s| s.0 == t)
        .map(|(_, k, shift)| {
            let name = k.symbol_or_name();
            if *shift {
                format!("Shift+{name}")
            } else {
                name.to_string()
            }
        })
        .unwrap_or_default()
}

/// A tool's name and its key, for tips.
pub fn named(t: PixelTool) -> String {
    let k = shortcut(t);
    if k.is_empty() {
        tr(t.key())
    } else {
        format!("{} ({k})", tr(t.key()))
    }
}

/// The bars the image editing mode shows; the others wait for the
/// drawing.
pub fn shows_bar(bar: Bar) -> bool {
    matches!(
        bar,
        Bar::Toolbox | Bar::Palette | Bar::Standard | Bar::Property | Bar::Status
    )
}

/// The image panels shown the first time.
pub fn default_dockers() -> crate::settings::DockerSetup {
    let key = |t: DockerTab| t.key().to_string();
    crate::settings::DockerSetup {
        open: DockerTab::PIXEL.iter().map(|t| key(*t)).collect(),
        front: key(DockerTab::PixelLayers),
        ..Default::default()
    }
}

/// Into the image editing mode: the drawing's panels are kept and the
/// image panels shown as they were left.
pub fn enter(app: &mut App) {
    let main = (app.docker_setup(), app.show_dockers);
    let mut setup = app
        .settings
        .pixel_dockers
        .clone()
        .unwrap_or_else(default_dockers);
    // The Hints panel came after the panels were first left: it joins
    // them once.
    if !app.settings.pixel_hints_added {
        let key = DockerTab::PixelHints.key().to_string();
        let lists = [&setup.open, &setup.left, &setup.inner, &setup.left_inner];
        if !lists.iter().any(|l| l.contains(&key)) {
            setup.open.insert(0, key);
        }
        app.settings.pixel_hints_added = true;
    }
    app.set_docker_setup(&setup);
    app.show_dockers = !app.open_dockers.is_empty();
    if let Some(pe) = app.pixel_edit.as_mut() {
        pe.main_dockers = Some(main);
    }
}

/// Out of it: the image panels are remembered and the drawing's put
/// back.
pub fn leave(app: &mut App, pe: &crate::pixel_edit_app::PixelEdit) {
    if let Some((main, shown)) = &pe.main_dockers {
        app.settings.pixel_dockers = Some(app.docker_setup());
        app.set_docker_setup(main);
        app.show_dockers = *shown;
    }
}

/// Pick `t` in the toolbox: a warp under way is put in the layer first.
pub fn choose_tool(pe: &mut crate::pixel_edit_app::PixelEdit, t: PixelTool) {
    if t != PixelTool::Warp && pe.warp.is_some() {
        pe.apply_warp();
    }
    pe.tool = t;
    pe.drag_from = None;
    pe.drag_to = None;
    pe.lasso.clear();
    if let Some(g) = GROUPS.iter().position(|g| g.contains(&t)) {
        if pe.group_pick.len() < GROUPS.len() {
            pe.group_pick = GROUPS.iter().map(|g| g[0]).collect();
        }
        pe.group_pick[g] = t;
    }
}

/// The tool a group's button shows.
fn shown_tool(pe: &crate::pixel_edit_app::PixelEdit, g: usize) -> PixelTool {
    let group = GROUPS[g];
    if group.contains(&pe.tool) {
        return pe.tool;
    }
    pe.group_pick
        .get(g)
        .copied()
        .filter(|t| group.contains(t))
        .unwrap_or(group[0])
}

// The menus.

fn menu_bar_style(style: &mut egui::Style) {
    crate::ui::menus::menu_bar_style(style);
}

/// The image menus in place of the drawing's.
pub fn menu_bar(app: &mut App, ui: &mut Ui) {
    let mut titles: Vec<Rect> = Vec::new();
    egui::MenuBar::new()
        .style(menu_bar_style)
        .config(egui::containers::menu::MenuConfig::new().style(crate::ui::menus::menu_popup_style))
        .ui(ui, |ui| {
            let mut menu = |ui: &mut Ui, key: &str, f: &mut dyn FnMut(&mut Ui)| {
                let r = ui.menu_button(tr(key), |ui| {
                    ui.set_min_width(250.0);
                    crate::ui::menus::body(ui, |ui| f(ui));
                });
                titles.push(r.response.rect);
            };
            menu(ui, "menu.file", &mut |ui| {
                cmd(app, ui, PixelCmd::Finish);
                cmd(app, ui, PixelCmd::Cancel);
            });
            menu(ui, "menu.edit", &mut |ui| {
                cmd(app, ui, PixelCmd::Undo);
                cmd(app, ui, PixelCmd::Redo);
                sep(ui);
                cmd(app, ui, PixelCmd::CopyToLayer);
                cmd(app, ui, PixelCmd::CutToLayer);
                cmd(app, ui, PixelCmd::PasteAsLayer);
                sep(ui);
                cmd(app, ui, PixelCmd::Delete);
                cmd(app, ui, PixelCmd::Fill);
                cmd(app, ui, PixelCmd::ContentFill);
            });
            menu(ui, "menu.view", &mut |ui| {
                cmd(app, ui, PixelCmd::ZoomIn);
                cmd(app, ui, PixelCmd::ZoomOut);
                cmd(app, ui, PixelCmd::ZoomFit);
                cmd(app, ui, PixelCmd::Zoom100);
            });
            menu(ui, "pixel.menu.image", &mut |ui| image_menu(app, ui));
            menu(ui, "menu.effects.adjust", &mut |ui| adjust_menu(app, ui));
            menu(ui, "menu.effects", &mut |ui| {
                for (group, list) in crate::fx::groups() {
                    if group == "menu.effects.adjust" {
                        continue;
                    }
                    sub(ui, group, |ui| effect_items(app, ui, list));
                }
            });
            menu(ui, "pixel.section.selection", &mut |ui| {
                cmd(app, ui, PixelCmd::SelectAll);
                cmd(app, ui, PixelCmd::SelectNone);
                cmd(app, ui, PixelCmd::InvertSelection);
                sep(ui);
                cmd(app, ui, PixelCmd::Feather);
                cmd(app, ui, PixelCmd::Grow);
                cmd(app, ui, PixelCmd::Border);
                cmd(app, ui, PixelCmd::Smooth);
                sep(ui);
                sub(ui, "pixel.sel.by_tone", |ui| {
                    cmd(app, ui, PixelCmd::Shadows);
                    cmd(app, ui, PixelCmd::Midtones);
                    cmd(app, ui, PixelCmd::Highlights);
                });
                cmd(app, ui, PixelCmd::QuickMask);
                cmd(app, ui, PixelCmd::SaveSelection);
            });
            menu(ui, "pixel.section.layers", &mut |ui| {
                cmd(app, ui, PixelCmd::LayerNew);
                cmd(app, ui, PixelCmd::LayerDuplicate);
                cmd(app, ui, PixelCmd::LayerDelete);
                sep(ui);
                cmd(app, ui, PixelCmd::LayerUp);
                cmd(app, ui, PixelCmd::LayerDown);
                sep(ui);
                cmd(app, ui, PixelCmd::MergeDown);
                cmd(app, ui, PixelCmd::Flatten);
            });
            menu(ui, "menu.window", &mut |ui| window_menu(app, ui));
            menu(ui, "menu.help", &mut |ui| {
                if item(ui, "menu.help.product_help", "F1", true) {
                    app.open_help("bitmaps/resample-and-straighten#other");
                }
            });
        });
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::new(crate::ui::menus::MENU_BAR_LAYOUT),
            (titles, 0usize),
        )
    });
}

/// A command as a menu item (ticked when it is a toggle), with its
/// picture.
fn cmd(app: &mut App, ui: &mut Ui, c: PixelCmd) {
    let hit = match (c.checked(app), c.picture(), c.action()) {
        (Some(on), _, _) => check(ui, c.key(), c.shortcut(), on),
        (None, Some(pic), _) => crate::ui::menus::item_drawn(
            ui,
            &tr(c.key()),
            c.shortcut(),
            c.enabled(app),
            c.key(),
            &|p, r, ink| crate::ui::pixel_menu_icons::image(p, r, pic, ink),
        ),
        (None, None, Some(a)) => crate::ui::menus::item_text(
            ui,
            &tr(c.key()),
            c.shortcut(),
            c.enabled(app),
            Some(a),
            c.key(),
        ),
        (None, None, None) => item(ui, c.key(), c.shortcut(), c.enabled(app)),
    };
    if hit {
        c.run(app);
    }
}

/// The effects of a group: one chosen shows in the Effects panel with
/// its settings and a preview, for Apply.
fn effect_items(app: &mut App, ui: &mut Ui, list: &[crate::fx::EffectSpec]) {
    for spec in list {
        if item_label(ui, &tr(&format!("fx.{}", spec.id)), "", true) {
            if let Some(pe) = app.pixel_edit.as_mut() {
                pe.effect = Some(crate::fx::new_effect(spec.id));
                pe.refresh_preview();
            }
            app.open_docker(DockerTab::PixelEffects);
        }
    }
}

/// The adjustments with keys of their own: the key, with Shift, and the
/// effect.
pub const ADJUST_KEYS: [(egui::Key, bool, &str); 4] = [
    (egui::Key::T, false, "tone_curve"),
    (egui::Key::B, false, "brightness_contrast_intensity"),
    (egui::Key::U, true, "hue_saturation_lightness"),
    (egui::Key::B, true, "color_balance"),
];

fn adjust_shortcut(id: &str) -> String {
    ADJUST_KEYS
        .iter()
        .find(|k| k.2 == id)
        .map(|(k, shift, _)| {
            let s = if *shift { "Shift+" } else { "" };
            format!("Ctrl+{s}{}", k.symbol_or_name())
        })
        .unwrap_or_default()
}

/// The Adjust menu's rows, in groups with a line between them; the
/// adjustments not named here follow at the end.
const ADJUST_ORDER: [&[&str]; 6] = [
    &["auto_adjust", "image_adjustments"],
    &["contrast_enhancement", "local_equalization"],
    &["target_balance", "tone_curve"],
    &["brightness_contrast_intensity", "gamma"],
    &["hue_saturation_lightness", "desaturate", "selective_color", "replace_colors"],
    &["channel_mixer", "color_balance"],
];

/// Show the effect `id` in the Effects panel with its settings and a
/// preview, for Apply.
pub fn open_effect(app: &mut App, id: &str) {
    if let Some(pe) = app.pixel_edit.as_mut() {
        pe.effect = Some(crate::fx::new_effect(id));
        pe.refresh_preview();
    }
    app.open_docker(DockerTab::PixelEffects);
}

/// Adjust: the colour and tone corrections with their pictures and keys,
/// then greys and black and white.
fn adjust_menu(app: &mut App, ui: &mut Ui) {
    let list = crate::fx::adjust::EFFECTS;
    let row = |app: &mut App, ui: &mut Ui, id: &str| {
        let key = format!("fx.{id}");
        if crate::ui::menus::item_drawn(
            ui,
            &format!("{}...", tr(&key)),
            &adjust_shortcut(id),
            true,
            &key,
            &|p, r, ink| crate::ui::pixel_menu_icons::adjust(p, r, id, ink),
        ) {
            open_effect(app, id);
        }
    };
    for group in ADJUST_ORDER {
        for id in group {
            if list.iter().any(|e| e.id == *id) {
                row(app, ui, id);
            }
        }
        if group.contains(&"desaturate") {
            cmd(app, ui, PixelCmd::Grayscale);
        }
        sep(ui);
    }
    for e in list {
        if !ADJUST_ORDER.iter().any(|g| g.contains(&e.id)) {
            row(app, ui, e.id);
        }
    }
}

/// Image: crop, the conversions, mirror and turn, the sizes and the
/// histogram.
fn image_menu(app: &mut App, ui: &mut Ui) {
    cmd(app, ui, PixelCmd::Crop);
    cmd(app, ui, PixelCmd::AutoCrop);
    sep(ui);
    cmd(app, ui, PixelCmd::Grayscale);
    cmd(app, ui, PixelCmd::BlackWhite);
    sep(ui);
    sub(ui, "pixel.menu.mirror", |ui| {
        cmd(app, ui, PixelCmd::FlipH);
        cmd(app, ui, PixelCmd::FlipV);
    });
    sub(ui, "pixel.menu.rotate", |ui| {
        cmd(app, ui, PixelCmd::TurnRight);
        cmd(app, ui, PixelCmd::TurnLeft);
        cmd(app, ui, PixelCmd::Turn180);
    });
    sep(ui);
    cmd(app, ui, PixelCmd::CanvasSize);
    cmd(app, ui, PixelCmd::SmartResize);
    sep(ui);
    cmd(app, ui, PixelCmd::RedEye);
    cmd(app, ui, PixelCmd::Histogram);
}

fn window_menu(app: &mut App, ui: &mut Ui) {
    sub(ui, "menu.window.toolbars", |ui| {
        for (key, bar) in [
            ("menu.window.toolbar_standard", Bar::Standard),
            ("menu.window.toolbar_property_bar", Bar::Property),
            ("menu.window.toolbar_toolbox", Bar::Toolbox),
            ("bars.palette", Bar::Palette),
            ("menu.window.toolbar_status_bar", Bar::Status),
        ] {
            let on = bar.visible(app);
            if check(ui, key, "", on) {
                bar.set_visible(app, !on);
            }
        }
        sep(ui);
        sub(ui, "pixel.customize_bar", |ui| bar_buttons_menu(app, ui));
    });
    sub(ui, "menu.window.dockers", |ui| {
        for tab in DockerTab::PIXEL {
            let pic = crate::ui::dockers::tab_icon(tab);
            let pic = pic
                .as_ref()
                .map(|f| f as &dyn Fn(&egui::Painter, egui::Rect));
            let on = crate::ui::menus::effect_docker_open(app, tab);
            if check_pic(ui, tab.key(), "", on, pic) {
                app.open_docker(tab);
            }
        }
    });
    cmd(app, ui, PixelCmd::Hints);
    sep(ui);
    if item(ui, "pixel.reset_panels", "", true) {
        app.set_docker_setup(&default_dockers());
        app.show_dockers = true;
    }
}

// The toolbox.

/// The width of the toolbox docked at a side.
pub fn column_width() -> f32 {
    Tokens::tool_button() + 6.0
}

/// The image tools in the toolbox, docked at `dock`.
pub fn toolbox(app: &mut App, ui: &mut Ui, dock: Dock) {
    crate::ui::bars::background(app, ui, Bar::Toolbox);
    let horizontal = dock.horizontal();
    let side = Tokens::tool_button();
    let mut body = |ui: &mut Ui| {
        ui.spacing_mut().item_spacing = if horizontal {
            Vec2::new(1.0, 0.0)
        } else {
            Vec2::new(0.0, 1.0)
        };
        if dock != Dock::Floating {
            crate::ui::bars::grip(app, ui, Bar::Toolbox, horizontal);
        }
        for g in 0..GROUPS.len() {
            tool_button(app, ui, g, side, horizontal);
            if BREAKS.contains(&g) && dock != Dock::Floating {
                let (r, _) = if horizontal {
                    ui.allocate_exact_size(Vec2::new(5.0, side), Sense::hover())
                } else {
                    ui.allocate_exact_size(Vec2::new(side, 5.0), Sense::hover())
                };
                let st = Stroke::new(1.0, Tokens::border());
                if horizontal {
                    ui.painter()
                        .vline(r.center().x, r.y_range().shrink(4.0), st);
                } else {
                    ui.painter()
                        .hline(r.x_range().shrink(4.0), r.center().y, st);
                }
            }
        }
    };
    match dock {
        Dock::Top | Dock::Bottom => {
            crate::ui::overflow::row(ui, "pixel_toolbox_row", |ui| {
                ui.horizontal(|ui| body(ui));
            });
        }
        Dock::Floating => {
            ui.horizontal_wrapped(|ui| body(ui));
        }
        Dock::Left | Dock::Right => {
            crate::ui::overflow::column(ui, "pixel_toolbox_column", |ui| {
                ui.vertical(|ui| body(ui));
            });
        }
    }
}

fn flyout_id() -> egui::Id {
    egui::Id::new("pixel_toolbox_flyout")
}

fn tool_button(app: &mut App, ui: &mut Ui, g: usize, side: f32, horizontal: bool) {
    let Some(pe) = app.pixel_edit.as_mut() else {
        return;
    };
    let group = GROUPS[g];
    let shown = shown_tool(pe, g);
    let active = group.contains(&pe.tool);
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
    crate::theme::paint_icon_button(ui.painter(), rect, resp.hovered(), active);
    let icon = crate::theme::icon_in(side, Tokens::tool_button(), Tokens::tool_icon());
    crate::ui::pixel_icons::draw(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(icon)),
        shown,
        Tokens::icon(),
    );
    let flyout = group.len() > 1;
    if flyout {
        let p = rect.right_bottom() - Vec2::new(2.0, 2.0);
        ui.painter().add(egui::epaint::PathShape::convex_polygon(
            vec![p, p - Vec2::new(5.0, 0.0), p - Vec2::new(0.0, 5.0)],
            Tokens::icon(),
            Stroke::NONE,
        ));
    }
    let open: Option<usize> = ui.ctx().data(|d| d.get_temp(flyout_id())).flatten();
    let key = shown.key();
    let resp = if open.is_none() {
        resp.rich_tip(format!("{}\n{}", named(shown), tr(&format!("{key}_tip"))))
    } else {
        resp
    };
    let arrow = Rect::from_min_max(rect.right_bottom() - Vec2::splat(10.0), rect.max);
    let on_arrow = resp.interact_pointer_pos().is_some_and(|p| arrow.contains(p));
    let held = resp.is_pointer_button_down_on()
        && ui.input(|i| i.time - i.pointer.press_start_time().unwrap_or(i.time) >= 0.4);
    if resp.is_pointer_button_down_on() && !held {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    if flyout && ((resp.clicked() && on_arrow) || held || resp.secondary_clicked()) {
        ui.ctx()
            .data_mut(|d| d.insert_temp(flyout_id(), Some(g)));
    } else if resp.clicked() {
        ui.ctx()
            .data_mut(|d| d.insert_temp::<Option<usize>>(flyout_id(), None));
        choose_tool(pe, shown);
    }
    if open == Some(g) {
        flyout_list(app, ui, g, rect, horizontal);
    }
}

/// A group's tools in a list beside its button: a click picks one.
fn flyout_list(app: &mut App, ui: &mut Ui, g: usize, button: Rect, horizontal: bool) {
    let at = if horizontal {
        button.left_bottom() + Vec2::new(0.0, 2.0)
    } else {
        button.right_top() + Vec2::new(2.0, 0.0)
    };
    let mut chosen: Option<PixelTool> = None;
    let area = egui::Area::new(egui::Id::new(("pixel_flyout", g)))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                for t in GROUPS[g] {
                    let (r, resp) =
                        ui.allocate_exact_size(Vec2::new(240.0, 28.0), Sense::click());
                    let on = app.pixel_edit.as_ref().is_some_and(|pe| pe.tool == *t);
                    crate::theme::paint_icon_button(ui.painter(), r, resp.hovered(), on);
                    crate::ui::pixel_icons::draw(
                        ui.painter(),
                        Rect::from_center_size(
                            egui::pos2(r.left() + 16.0, r.center().y),
                            Vec2::splat(20.0),
                        ),
                        *t,
                        Tokens::icon(),
                    );
                    ui.painter().text(
                        egui::pos2(r.left() + 34.0, r.center().y),
                        egui::Align2::LEFT_CENTER,
                        tr(t.key()),
                        egui::FontId::proportional(13.0),
                        Tokens::text(),
                    );
                    ui.painter().text(
                        egui::pos2(r.right() - 10.0, r.center().y),
                        egui::Align2::RIGHT_CENTER,
                        shortcut(*t),
                        egui::FontId::proportional(12.0),
                        Tokens::text_dim(),
                    );
                    let resp = resp.rich_tip(tr(&format!("{}_tip", t.key())));
                    if resp.clicked() {
                        chosen = Some(*t);
                    }
                }
            });
        });
    let pressed_away = ui.input(|i| i.pointer.any_pressed())
        && ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !area.response.rect.contains(p) && !button.contains(p));
    if chosen.is_some() || pressed_away || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.ctx()
            .data_mut(|d| d.insert_temp::<Option<usize>>(flyout_id(), None));
    }
    if let (Some(t), Some(pe)) = (chosen, app.pixel_edit.as_mut()) {
        choose_tool(pe, t);
    }
}

// The standard and property bars.

/// The image standard bar's first buttons: Finish, Cancel, Undo, Redo,
/// the layer and selection commands most used, the zoom and the panels.
/// Alt and a drag moves a button or takes it off; Window > Toolbars >
/// Image Bar Buttons adds or takes off any command.
pub const IMAGE_BAR: [&str; 14] = [
    "pixel:Finish",
    "pixel:Cancel",
    crate::ui::bar_items::SEP,
    "pixel:Undo",
    "pixel:Redo",
    crate::ui::bar_items::SEP,
    "pixel:CopyToLayer",
    "pixel:CutToLayer",
    "pixel:PasteAsLayer",
    "pixel:Delete",
    crate::ui::bar_items::SEP,
    ZOOM_ITEM,
    crate::ui::bar_items::SEP,
    PANELS_ITEM,
];

/// The zoom of the image bar (out, the steps, in, Fit, 1:1).
const ZOOM_ITEM: &str = "pixel_zoom";
/// The panels' show or hide on the image bar.
const PANELS_ITEM: &str = "pixel_panels";

/// The standard bar in the image editing mode.
pub fn standard_bar(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        ui.set_min_height(Tokens::bar_button());
        crate::ui::bar_items::draw_bar(app, ui, crate::ui::bar_items::ItemBar::Image, bar_item);
    });
}

/// One button of the image bar, by its id.
fn bar_item(app: &mut App, ui: &mut Ui, _bar: &str, id: &str) {
    use crate::ui::icons::Action;
    use crate::ui::toolbar::{std_button, std_sep};
    match id {
        crate::ui::bar_items::SEP => std_sep(ui),
        ZOOM_ITEM => zoom_controls(app, ui),
        PANELS_ITEM => {
            let shown = app.show_dockers;
            if std_button(ui, Action::Dockers, &tr("toolbar.dockers"), true, shown).clicked() {
                app.show_dockers = !shown;
            }
        }
        _ => {
            let Some(c) = PixelCmd::from_id(id) else {
                return;
            };
            let enabled = c.enabled(app);
            let clicked = match (c, c.action()) {
                (PixelCmd::Finish, _) => ui
                    .add(egui::Button::new(
                        egui::RichText::new(format!("\u{2714} {}", tr("pixel.finish")))
                            .color(Color32::from_rgb(0x1E, 0x9E, 0x3C)),
                    ))
                    .rich_tip(tr("pixel.finish_tip"))
                    .clicked(),
                (PixelCmd::Cancel, _) => ui
                    .button(tr("pixel.cancel"))
                    .rich_tip(tr("pixel.cancel_tip"))
                    .clicked(),
                (_, Some(a)) => std_button(ui, a, &tip_of(c), enabled, false).clicked(),
                (_, None) => match c.checked(app) {
                    Some(on) => ui
                        .selectable_label(on, tr(c.key()))
                        .rich_tip(tip_of(c))
                        .clicked(),
                    None => ui
                        .add_enabled(enabled, egui::Button::new(tr(c.key())))
                        .rich_tip(tip_of(c))
                        .clicked(),
                },
            };
            if clicked {
                c.run(app);
            }
        }
    }
}

/// Window > Toolbars > Image Bar Buttons: every command, ticked while on
/// the bar; a click puts it at the end or takes it off.
fn bar_buttons_menu(app: &mut App, ui: &mut Ui) {
    use crate::ui::bar_items::{items, set_items, ItemBar};
    let mut list = items(app, ItemBar::Image);
    let mut changed = false;
    let mut row = |ui: &mut Ui, label: String, id: String| {
        let on = list.contains(&id);
        if crate::ui::menus::check_label(ui, &label, &format!("image_bar.{id}"), on) {
            if on {
                list.retain(|x| *x != id);
            } else {
                list.push(id);
            }
            changed = true;
        }
    };
    for c in PixelCmd::ALL {
        row(ui, tr(c.key()), c.id());
    }
    row(ui, tr("menu.window.toolbar_zoom"), ZOOM_ITEM.to_string());
    row(ui, tr("toolbar.dockers"), PANELS_ITEM.to_string());
    if changed {
        set_items(app, ItemBar::Image, list);
    }
    sep(ui);
    if item(ui, "pixel.reset_bar", "", true) {
        let first = IMAGE_BAR.iter().map(|s| s.to_string()).collect();
        set_items(app, ItemBar::Image, first);
    }
}

fn tip_of(c: PixelCmd) -> String {
    let s = c.shortcut();
    if s.is_empty() {
        tr(c.key())
    } else {
        format!("{} ({s})", tr(c.key()))
    }
}

/// Zoom out, the zoom in percent (a list of steps), zoom in, fit.
fn zoom_controls(app: &mut App, ui: &mut Ui) {
    let Some(pe) = app.pixel_edit.as_ref() else {
        return;
    };
    let percent = pe.ws.percent();
    if ui
        .button("-")
        .rich_tip(tip_of(PixelCmd::ZoomOut))
        .clicked()
    {
        PixelCmd::ZoomOut.run(app);
    }
    let mut pick: Option<f32> = None;
    egui::ComboBox::from_id_salt("pixel_zoom")
        .width(70.0)
        .selected_text(format!("{percent} %"))
        .show_ui(ui, |ui| {
            for z in crate::pixel_view::STEPS.iter().rev() {
                let p = (z * 100.0).round() as u32;
                if ui.selectable_label(p == percent, format!("{p} %")).clicked() {
                    pick = Some(*z);
                }
            }
        });
    if let (Some(z), Some(pe)) = (pick, app.pixel_edit.as_mut()) {
        let area = pe.ws_area;
        if area.is_positive() {
            pe.ws.zoom_to(area, area.center(), z);
        }
    }
    if ui.button("+").rich_tip(tip_of(PixelCmd::ZoomIn)).clicked() {
        PixelCmd::ZoomIn.run(app);
    }
    if ui
        .button(tr("pixel.zoom_fit"))
        .rich_tip(tip_of(PixelCmd::ZoomFit))
        .clicked()
    {
        PixelCmd::ZoomFit.run(app);
    }
    if ui
        .button("1:1")
        .rich_tip(tip_of(PixelCmd::Zoom100))
        .clicked()
    {
        PixelCmd::Zoom100.run(app);
    }
}

/// The property bar: the tool in use and its options.
pub fn property_bar(app: &mut App, ui: &mut Ui) {
    let Some(pe) = app.pixel_edit.as_mut() else {
        return;
    };
    ui.horizontal(|ui| {
        let side = Tokens::prop_button();
        let (r, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
        crate::ui::pixel_icons::draw(
            ui.painter(),
            Rect::from_center_size(r.center(), Vec2::splat(Tokens::prop_icon())),
            pe.tool,
            Tokens::icon(),
        );
        ui.label(egui::RichText::new(tr(pe.tool.key())).strong());
        ui.separator();
        crate::ui::pixel_bar::tool_options(ui, pe);
    });
}

// The picture's window.

/// A checkerboard under the picture's clear parts.
fn checker(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("pixel_checker");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    let (a, b) = (Color32::from_gray(255), Color32::from_gray(204));
    let img = egui::ColorImage::new([2, 2], vec![a, b, b, a]);
    let opts = egui::TextureOptions {
        wrap_mode: egui::TextureWrapMode::Repeat,
        ..egui::TextureOptions::NEAREST
    };
    let t = ctx.load_texture("pixel_checker", img, opts);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// The picture alone in the window's middle: zoomed with the wheel about
/// the pointer (or Ctrl and the wheel, or a pinch), moved with the middle
/// button, Space and a drag, or the Pan tool, and the image tools on it.
pub fn canvas(app: &mut App, ui: &mut Ui, full: Rect) {
    // Scrollbars along the right and the bottom, the zoom to fit in the
    // corner between them.
    let sb = crate::ui::window_bars::BAR;
    let area = Rect::from_min_max(full.min, full.max - Vec2::splat(sb));
    scrollbars(app, ui, full, area);
    let resp = ui.allocate_rect(area, Sense::click_and_drag());
    let painter = ui.painter_at(area);
    // A neutral grey round the picture, the same in every theme, so its
    // colours are judged against no tint.
    painter.rect_filled(area, 0.0, Color32::from_gray(0x78));
    let Some(pe) = app.pixel_edit.as_mut() else {
        return;
    };
    let (w, h) = pe.pi.img.dimensions();
    if pe.ws.fitted && (pe.ws_area != area || pe.ws.center != (w as f32 / 2.0, h as f32 / 2.0)) {
        pe.ws.fit(area, w, h, false);
    }
    pe.ws_area = area;
    let (mods, space, scroll, pinch) = ui.input(|i| {
        (
            i.modifiers,
            i.key_down(egui::Key::Space),
            i.smooth_scroll_delta,
            i.zoom_delta(),
        )
    });
    if let Some(p) = resp.hover_pos() {
        if (pinch - 1.0).abs() > 1e-4 {
            pe.ws.zoom_about(area, p, pinch);
        } else if scroll.y != 0.0 && !mods.shift {
            pe.ws.zoom_about(area, p, (scroll.y * 0.003).exp());
        } else if scroll.x != 0.0 || (mods.shift && scroll.y != 0.0) {
            pe.ws.pan(Vec2::new(scroll.x + scroll.y, 0.0));
        }
    }
    // Moving the view: the middle button, Space and a drag, the Pan tool.
    let pan_tool = pe.tool == PixelTool::Pan;
    let panning = resp.dragged_by(egui::PointerButton::Middle)
        || ((space || pan_tool) && resp.dragged_by(egui::PointerButton::Primary));
    if space || pan_tool {
        ui.ctx().set_cursor_icon(if panning {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
    }
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let ws = pe.ws;
    pe.pointer_px = resp.hover_pos().map(|p| ws.to_px(area, p));
    if panning {
        pe.ws.pan(resp.drag_delta());
    } else if pe.tool == PixelTool::Zoom {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
        if let Some(p) = resp.interact_pointer_pos() {
            let out = mods.alt || resp.secondary_clicked();
            if resp.clicked() || resp.secondary_clicked() {
                let z = pe.ws.step(if out { -1 } else { 1 });
                pe.ws.zoom_to(area, p, z);
            }
        }
    } else if !space && !pan_tool && !resp.dragged_by(egui::PointerButton::Middle) {
        let active = resp.dragged() || resp.drag_stopped() || resp.clicked();
        let px = if active || resp.hovered() {
            pointer.map(|p| ws.to_px(area, p))
        } else {
            None
        };
        app.pixel_px_input(&resp, px, mods, ws.zoom);
    }
    let Some(pe) = app.pixel_edit.as_ref() else {
        return;
    };
    let ws = pe.ws;
    // The checkerboard: squares of 8 screen points under the picture.
    let (w, h) = pe.pi.img.dimensions();
    let pic = Rect::from_two_pos(ws.to_screen(area, 0.0, 0.0), ws.to_screen(area, w as f32, h as f32));
    let shown = pic.intersect(area);
    if shown.is_positive() {
        let t = checker(ui.ctx());
        let uv = Rect::from_min_max(
            ((shown.min - pic.min) / 16.0).to_pos2(),
            ((shown.max - pic.min) / 16.0).to_pos2(),
        );
        painter.image(t.id(), shown, uv, Color32::WHITE);
    }
    painter.rect_stroke(
        pic.expand(0.5),
        0.0,
        Stroke::new(1.0, Color32::from_black_alpha(90)),
        egui::StrokeKind::Outside,
    );
    let map = |x: f32, y: f32| ws.to_screen(area, x, y);
    crate::pixel_edit_app::draw_mapped(pe, &painter, &map, pe.pointer_px);
}

/// The part of `[a0, a1]` (screen points along one side) the
/// scrollbar covers: the picture with half a window round it, and the
/// window itself.
pub fn scroll_extent(pic: (f32, f32), view: (f32, f32)) -> (f32, f32) {
    let half = (view.1 - view.0) * 0.5;
    ((pic.0 - half).min(view.0), (pic.1 + half).max(view.1))
}

/// The image window's scrollbars: arrows, a track that pages with a
/// click and a thumb that drags, like the drawing's.
fn scrollbars(app: &mut App, ui: &mut Ui, full: Rect, area: Rect) {
    use crate::ui::window_bars::{arrow_button, Dir, BAR, LINE};
    let Some(pe) = app.pixel_edit.as_ref() else {
        return;
    };
    let ws = pe.ws;
    let (w, h) = pe.pi.img.dimensions();
    let pic = Rect::from_two_pos(ws.to_screen(area, 0.0, 0.0), ws.to_screen(area, w as f32, h as f32));
    let p = ui.painter_at(full);
    let hbar = Rect::from_min_max(egui::pos2(full.min.x, area.max.y), egui::pos2(area.max.x, full.max.y));
    let vbar = Rect::from_min_max(egui::pos2(area.max.x, full.min.y), egui::pos2(full.max.x, area.max.y));
    let corner = Rect::from_min_max(area.max, full.max);
    for r in [hbar, vbar, corner] {
        p.rect_filled(r, 0.0, Tokens::panel());
    }
    let mut pan = Vec2::ZERO;
    for horizontal in [true, false] {
        let bar = if horizontal { hbar } else { vbar };
        let len = if horizontal { bar.width() } else { bar.height() };
        if len < BAR * 3.0 {
            continue;
        }
        let (a, b, dir_a, dir_b) = if horizontal {
            (
                Rect::from_min_size(bar.min, Vec2::new(BAR, bar.height())),
                Rect::from_min_size(egui::pos2(bar.max.x - BAR, bar.min.y), Vec2::new(BAR, bar.height())),
                Dir::Left,
                Dir::Right,
            )
        } else {
            (
                Rect::from_min_size(bar.min, Vec2::new(bar.width(), BAR)),
                Rect::from_min_size(egui::pos2(bar.min.x, bar.max.y - BAR), Vec2::new(bar.width(), BAR)),
                Dir::Up,
                Dir::Down,
            )
        };
        let along = |v: Vec2| if horizontal { v.x } else { v.y };
        let unit = if horizontal { Vec2::X } else { Vec2::Y };
        let id = if horizontal { "pixel_hscroll" } else { "pixel_vscroll" };
        if arrow_button(ui, a, &format!("{id}_a"), dir_a) {
            pan += unit * LINE;
        }
        if arrow_button(ui, b, &format!("{id}_b"), dir_b) {
            pan -= unit * LINE;
        }
        let track = Rect::from_min_max(
            if horizontal { egui::pos2(a.max.x, bar.min.y) } else { egui::pos2(bar.min.x, a.max.y) },
            if horizontal { egui::pos2(b.min.x, bar.max.y) } else { egui::pos2(bar.max.x, b.min.y) },
        );
        let (p0, p1, v0, v1) = if horizontal {
            (pic.min.x, pic.max.x, area.min.x, area.max.x)
        } else {
            (pic.min.y, pic.max.y, area.min.y, area.max.y)
        };
        let (e0, e1) = scroll_extent((p0, p1), (v0, v1));
        let span = (e1 - e0).max(1e-3);
        let tlen = along(track.size());
        let t0 = (v0 - e0) / span;
        let t1 = ((v1 - e0) / span).max(t0 + 0.03);
        let start = along(track.min.to_vec2());
        let thumb = if horizontal {
            Rect::from_min_max(
                egui::pos2(start + tlen * t0, track.min.y + 2.0),
                egui::pos2(start + tlen * t1, track.max.y - 2.0),
            )
        } else {
            Rect::from_min_max(
                egui::pos2(track.min.x + 2.0, start + tlen * t0),
                egui::pos2(track.max.x - 2.0, start + tlen * t1),
            )
        };
        let resp = ui.interact(track, egui::Id::new(id), Sense::click_and_drag());
        let hot = resp.dragged() || resp.hover_pos().is_some_and(|q| thumb.contains(q));
        p.rect_filled(
            thumb,
            0.0,
            if hot {
                Tokens::control_border()
            } else {
                Tokens::slider_rail()
            },
        );
        if resp.dragged() {
            pan -= unit * (along(resp.drag_delta()) / tlen.max(1.0) * span);
        } else if resp.clicked() {
            if let Some(q) = resp.interact_pointer_pos() {
                let page = (v1 - v0) * 0.9;
                let at = along(q.to_vec2());
                if at < along(thumb.min.to_vec2()) {
                    pan += unit * page;
                } else if at > along(thumb.max.to_vec2()) {
                    pan -= unit * page;
                }
            }
        }
    }
    // The corner: fit the picture in the window.
    let resp = ui
        .interact(corner, egui::Id::new("pixel_scroll_corner"), Sense::click())
        .rich_tip(tip_of(PixelCmd::ZoomFit));
    if resp.hovered() {
        p.rect_filled(corner, 0.0, Tokens::control_border());
    }
    crate::ui::icons::draw(
        &p,
        corner.shrink(2.0),
        crate::tools::Tool::Zoom,
        Tokens::icon(),
    );
    if resp.clicked() {
        PixelCmd::ZoomFit.run(app);
    }
    if pan != Vec2::ZERO {
        if let Some(pe) = app.pixel_edit.as_mut() {
            pe.ws.pan(pan);
        }
    }
}

// The status bar.

/// The status bar: the tool's hint, the pointer's pixel, the picture's
/// size, the layer in use and the zoom.
pub fn status_bar(app: &mut App, ui: &mut Ui) {
    let Some(pe) = app.pixel_edit.as_ref() else {
        return;
    };
    let (w, h) = pe.pi.img.dimensions();
    let dim = |s: String| egui::RichText::new(s).color(Tokens::text_dim());
    ui.horizontal_centered(|ui| {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(tr(pe.tool.key())).strong());
        ui.label(dim(tr(&format!("{}_hint", pe.tool.key()))));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(6.0);
            ui.label(format!("{} %", pe.ws.percent()));
            ui.separator();
            if let Some(l) = pe.pi.layers.get(pe.pi.active) {
                ui.label(crate::ui::pixel_bar::layer_label(pe.pi.active, &l.name));
                ui.separator();
            }
            ui.label(format!("{w} × {h} px"));
            if let Some((x, y)) = pe.pointer_px {
                if x >= 0.0 && y >= 0.0 && x < w as f32 && y < h as f32 {
                    ui.separator();
                    ui.label(format!("{}, {} px", x.floor(), y.floor()));
                }
            }
        });
    });
}

// The panels.

/// The body of an image panel.
pub fn docker_body(app: &mut App, ui: &mut Ui, tab: DockerTab) {
    let Some(pe) = app.pixel_edit.as_mut() else {
        ui.label(dim_text(tr("pixel.panel_idle")));
        return;
    };
    use crate::ui::pixel_bar as b;
    match tab {
        DockerTab::PixelLayers => b::layers_section(ui, pe),
        DockerTab::PixelSelection => b::selection_section(ui, pe),
        DockerTab::PixelRetouch => b::retouch_section(ui, pe),
        DockerTab::PixelDistort => b::distort_section(ui, pe),
        DockerTab::PixelEffects => b::effects_section(ui, pe),
        DockerTab::PixelImage => b::picture_section(ui, pe),
        DockerTab::PixelHistogram => b::histogram_section(ui, pe),
        DockerTab::PixelHints => hints(ui, pe),
        _ => {}
    }
}

/// The Hints panel: the tool in use, what it does, how to use it, its
/// key and the keys that change it, and the keys of the window.
fn hints(ui: &mut Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    let t = pe.tool;
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
        crate::ui::pixel_icons::draw(ui.painter(), r, t, Tokens::icon());
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(tr(t.key())).strong().size(16.0));
            let k = shortcut(t);
            if !k.is_empty() {
                ui.label(dim_text(format!("{} {k}", tr("pixel.hints.key"))));
            }
        });
    });
    ui.add_space(4.0);
    ui.label(tr(&format!("{}_tip", t.key())));
    ui.add_space(4.0);
    let bullet = |ui: &mut Ui, s: String| {
        ui.horizontal_wrapped(|ui| {
            ui.label("\u{2022}");
            ui.label(s);
        });
    };
    bullet(ui, tr(&format!("{}_hint", t.key())));
    if t.is_selection() {
        bullet(ui, tr("pixel.hints.sel_keys"));
    }
    if t.is_brush() || t.is_liquify() || matches!(t, PixelTool::MaskBrush | PixelTool::Sprayer) {
        bullet(ui, tr("pixel.hints.brush_keys"));
    }
    if matches!(t, PixelTool::Clone | PixelTool::Heal) {
        bullet(ui, tr("pixel.status.need_source"));
    }
    ui.add_space(8.0);
    ui.separator();
    ui.label(egui::RichText::new(tr("pixel.hints.window")).strong());
    for key in [
        "pixel.hints.view_keys",
        "pixel.hints.zoom_keys",
        "pixel.hints.undo_keys",
        "pixel.hints.finish_keys",
    ] {
        bullet(ui, tr(key));
    }
}

fn dim_text(s: String) -> egui::RichText {
    egui::RichText::new(s).color(Tokens::text_dim())
}

/// The picture on an image panel's tab.
pub fn docker_icon(tab: DockerTab) -> Option<crate::ui::dockers::TabIcon> {
    Some(match tab {
        DockerTab::PixelHints => return crate::ui::dockers::tab_icon(DockerTab::Hints),
        DockerTab::PixelLayers => |p, r| {
            let s = Stroke::new(1.2, Tokens::icon());
            for k in 0..3 {
                let y = r.top() + r.height() * (0.3 + 0.2 * k as f32);
                let (x0, x1) = (r.left() + r.width() * 0.15, r.right() - r.width() * 0.15);
                let d = r.width() * 0.12;
                p.add(egui::Shape::closed_line(
                    vec![
                        egui::pos2(x0 + d, y - d),
                        egui::pos2(x1 + d, y - d),
                        egui::pos2(x1 - d, y + d),
                        egui::pos2(x0 - d, y + d),
                    ],
                    s,
                ));
            }
        },
        DockerTab::PixelSelection => {
            |p, r| crate::ui::pixel_icons::draw(p, r, PixelTool::RectSelect, Tokens::icon())
        }
        DockerTab::PixelRetouch => {
            |p, r| crate::ui::pixel_icons::draw(p, r, PixelTool::Heal, Tokens::icon())
        }
        DockerTab::PixelDistort => {
            |p, r| crate::ui::pixel_icons::draw(p, r, PixelTool::Warp, Tokens::icon())
        }
        DockerTab::PixelEffects => |p, r| {
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "fx",
                egui::FontId::proportional(12.0),
                Tokens::icon(),
            );
        },
        DockerTab::PixelImage => {
            |p, r| crate::ui::icons::draw(p, r, crate::tools::Tool::Crop, Tokens::icon())
        }
        DockerTab::PixelHistogram => |p, r| {
            let s = Stroke::new(1.0, Tokens::icon());
            for (k, f) in [0.3_f32, 0.6, 0.9, 0.7, 0.45, 0.25].iter().enumerate() {
                let x = r.left() + r.width() * (0.15 + 0.14 * k as f32);
                p.line_segment(
                    [
                        egui::pos2(x, r.bottom() - r.height() * 0.15),
                        egui::pos2(x, r.bottom() - r.height() * (0.15 + 0.7 * f)),
                    ],
                    s,
                );
            }
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_picture() -> (App, tracedraw_core::ShapeId) {
        let mut app = App::headless();
        let img = image::RgbaImage::from_pixel(40, 30, image::Rgba([200, 60, 60, 255]));
        let png = crate::bitmap_fx::encode(&img).expect("png");
        let id = app
            .new_shape(tracedraw_core::ShapeKind::Bitmap {
                rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 50.0, 40.0),
                width_px: 40,
                height_px: 30,
                png: png.into(),
                fx: None,
            })
            .expect("a layer");
        (app, id)
    }

    #[test]
    fn every_image_tool_is_on_a_toolbox_button() {
        for t in PixelTool::ALL {
            assert_eq!(
                GROUPS.iter().filter(|g| g.contains(&t)).count(),
                1,
                "{t:?}"
            );
        }
    }

    #[test]
    fn the_image_mode_swaps_the_panels_and_puts_the_drawings_back() {
        let (mut app, id) = app_with_picture();
        let before = app.docker_setup();
        assert!(app.edit_bitmap_inside(id));
        assert!(app.open_dockers.contains(&DockerTab::PixelLayers));
        assert!(!app.open_dockers.iter().any(|t| DockerTab::ALL.contains(t)));
        // The preferences keep the drawing's panels meanwhile.
        assert_eq!(app.main_docker_setup(), before);
        // A panel closed in the image mode stays closed the next time.
        app.close_docker(DockerTab::PixelHistogram);
        app.finish_pixel_edit();
        assert_eq!(app.docker_setup(), before);
        assert!(app.edit_bitmap_inside(id));
        assert!(!app.open_dockers.contains(&DockerTab::PixelHistogram));
        app.cancel_pixel_edit();
        assert_eq!(app.docker_setup(), before);
    }

    #[test]
    fn only_the_main_bars_show_in_the_image_mode() {
        let (mut app, id) = app_with_picture();
        app.show_text_toolbar = true;
        assert!(Bar::Text.visible(&app));
        assert!(app.edit_bitmap_inside(id));
        assert!(!Bar::Text.visible(&app));
        assert!(Bar::Standard.visible(&app) == app.show_standard_toolbar);
        app.finish_pixel_edit();
        assert!(Bar::Text.visible(&app));
    }

    #[test]
    fn the_image_bar_buttons_are_commands_and_can_be_changed() {
        use crate::ui::bar_items::{items, set_items, ItemBar};
        for id in IMAGE_BAR {
            assert!(
                PixelCmd::from_id(id).is_some()
                    || [crate::ui::bar_items::SEP, ZOOM_ITEM, PANELS_ITEM].contains(&id),
                "{id}"
            );
        }
        for c in PixelCmd::ALL {
            assert_eq!(PixelCmd::from_id(&c.id()), Some(c));
        }
        let (mut app, _) = app_with_picture();
        let mut list = items(&app, ItemBar::Image);
        list.push(PixelCmd::Crop.id());
        set_items(&mut app, ItemBar::Image, list);
        assert!(app.settings.bar_items.contains_key("image_standard"));
        set_items(&mut app, ItemBar::Image, IMAGE_BAR.iter().map(|s| s.to_string()).collect());
        assert!(!app.settings.bar_items.contains_key("image_standard"));
    }

    #[test]
    fn toolbox_flyout_picks_are_remembered_per_button() {
        let (mut app, id) = app_with_picture();
        assert!(app.edit_bitmap_inside(id));
        let pe = app.pixel_edit.as_mut().expect("editing");
        choose_tool(pe, PixelTool::Heal);
        choose_tool(pe, PixelTool::Brush);
        let g = |t: PixelTool| GROUPS.iter().position(|g| g.contains(&t)).unwrap();
        assert_eq!(shown_tool(pe, g(PixelTool::Clone)), PixelTool::Heal);
        assert_eq!(shown_tool(pe, g(PixelTool::Sprayer)), PixelTool::Brush);
    }

    #[test]
    fn commands_run_from_the_menus_change_the_picture() {
        let (mut app, id) = app_with_picture();
        assert!(app.edit_bitmap_inside(id));
        assert!(!PixelCmd::SelectNone.enabled(&app));
        PixelCmd::SelectAll.run(&mut app);
        assert!(PixelCmd::Crop.enabled(&app));
        PixelCmd::LayerNew.run(&mut app);
        assert!(PixelCmd::Flatten.enabled(&app));
        PixelCmd::Undo.run(&mut app);
        assert!(!PixelCmd::Flatten.enabled(&app));
        PixelCmd::FlipH.run(&mut app);
        assert!(app.pixel_edit.as_ref().is_some_and(|pe| pe.changed));
        PixelCmd::Finish.run(&mut app);
        assert!(app.pixel_edit.is_none());
    }

    #[test]
    fn pick_and_the_view_tools_lead_the_toolbox_and_every_tool_has_a_key() {
        assert_eq!(GROUPS[0], &[PixelTool::Move]);
        assert!(GROUPS[2].contains(&PixelTool::Pan));
        for t in PixelTool::ALL {
            assert!(!shortcut(t).is_empty(), "{t:?}");
        }
        for (i, a) in SHORTCUTS.iter().enumerate() {
            for b in &SHORTCUTS[i + 1..] {
                assert!(a.0 != b.0, "{:?} twice", a.0);
                assert!((a.1, a.2) != (b.1, b.2), "{:?} and {:?} share a key", a.0, b.0);
            }
        }
        assert_eq!(shortcut(PixelTool::Smear), "Shift+S");
        assert_eq!(shortcut(PixelTool::LayerFade), "1");
    }

    #[test]
    fn the_scrollbars_cover_the_picture_and_half_a_window_round_it() {
        // A picture wider than the window: the bar covers it and half a
        // window each side.
        assert_eq!(scroll_extent((-500.0, 1500.0), (0.0, 1000.0)), (-1000.0, 2000.0));
        // A small one in the middle: the window itself.
        assert_eq!(scroll_extent((400.0, 600.0), (0.0, 1000.0)), (-100.0, 1100.0));
    }

    #[test]
    fn the_hints_panel_joins_panels_left_from_before_once() {
        let (mut app, id) = app_with_picture();
        let mut old = default_dockers();
        old.open.retain(|k| k != DockerTab::PixelHints.key());
        app.settings.pixel_dockers = Some(old);
        assert!(app.edit_bitmap_inside(id));
        assert!(app.open_dockers.contains(&DockerTab::PixelHints));
        app.close_docker(DockerTab::PixelHints);
        app.finish_pixel_edit();
        assert!(app.edit_bitmap_inside(id));
        assert!(!app.open_dockers.contains(&DockerTab::PixelHints));
        PixelCmd::Hints.run(&mut app);
        assert!(app.open_dockers.contains(&DockerTab::PixelHints));
    }

    #[test]
    fn the_image_menu_turns_and_greys_the_picture() {
        let (mut app, id) = app_with_picture();
        assert!(app.edit_bitmap_inside(id));
        PixelCmd::TurnRight.run(&mut app);
        let dims = |app: &App| app.pixel_edit.as_ref().unwrap().pi.img.dimensions();
        assert_eq!(dims(&app), (30, 40));
        PixelCmd::Turn180.run(&mut app);
        PixelCmd::TurnLeft.run(&mut app);
        assert_eq!(dims(&app), (40, 30));
        PixelCmd::Grayscale.run(&mut app);
        let q = *app.pixel_edit.as_ref().unwrap().pi.img.get_pixel(3, 3);
        assert!(q[0] == q[1] && q[1] == q[2]);
        PixelCmd::Undo.run(&mut app);
        let q = *app.pixel_edit.as_ref().unwrap().pi.img.get_pixel(3, 3);
        assert_eq!(q[0], 200);
        open_effect(&mut app, "tone_curve");
        assert!(app.pixel_edit.as_ref().unwrap().effect.is_some());
        assert!(app.open_dockers.contains(&DockerTab::PixelEffects));
        for (_, _, id) in ADJUST_KEYS {
            assert!(crate::fx::adjust::EFFECTS.iter().any(|e| e.id == id), "{id}");
        }
    }
}

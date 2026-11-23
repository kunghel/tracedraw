//! Dockers added with the full workspace: Align and Distribute, Shaping,
//! Repeat Copies, Text, Glyphs, Color, Color Styles, Object Styles,
//! Find and Replace, Scripts, Palettes, Lens, Blend, Contour, Envelope,
//! Extrude, Bevel, Brush Strokes, Color Mask, Object Data, Links,
//! Symbols, Pages, Guidelines, Fonts.

use crate::app::{App, DockerTab};
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::{document::ShapeKind, geometry::Affine, live::Effect, Color, Command, Fill, ShapeId};

/// The colour picker every colour of the drawing uses (#442).
fn color_button(ui: &mut Ui, c: &mut Color) -> bool {
    crate::ui::color_picker::color_button(ui, c)
}

fn dim(ui: &mut Ui, text: String) {
    ui.label(
        egui::RichText::new(text)
            .color(Tokens::text_dim())
            .size(11.0),
    );
}

pub fn show(app: &mut App, ui: &mut Ui, tab: DockerTab) {
    match tab {
        DockerTab::AlignDistribute => crate::ui::align_docker::align_distribute(app, ui),
        DockerTab::Shaping => crate::ui::shaping_docker::shaping(app, ui),
        DockerTab::StepAndRepeat => step_and_repeat(app, ui),
        DockerTab::Text => crate::ui::text_docker::text_docker(app, ui),
        DockerTab::Glyphs => crate::ui::glyphs_docker::glyphs(app, ui),
        DockerTab::Color => crate::ui::color_docker::color(app, ui),
        DockerTab::ColorStyles => color_styles(app, ui),
        DockerTab::ObjectStyles => object_styles(app, ui),
        DockerTab::FindReplace => find_replace(app, ui),
        DockerTab::Scripts => scripts(app, ui),
        DockerTab::Palettes => palettes(app, ui),
        DockerTab::Lens => lens(app, ui),
        DockerTab::Blend => blend(app, ui),
        DockerTab::Contour => contour(app, ui),
        DockerTab::Envelope => crate::ui::envelope_ui::envelope_docker(app, ui),
        DockerTab::Extrude => extrude(app, ui),
        DockerTab::Bevel => bevel(app, ui),
        DockerTab::BrushStrokes => brush_strokes(app, ui),
        DockerTab::BitmapMask => bitmap_mask(app, ui),
        DockerTab::ObjectData => object_data(app, ui),
        DockerTab::Links => links(app, ui),
        DockerTab::Symbols => symbols(app, ui),
        DockerTab::Pages => pages(app, ui),
        DockerTab::Guidelines => guidelines(app, ui),
        DockerTab::Fonts => fonts(app, ui),
        DockerTab::FontSampler => crate::ui::font_sampler::font_sampler(app, ui),
        DockerTab::Corners => corners(app, ui),
        DockerTab::JoinCurves => join_curves(app, ui),
        DockerTab::FitToPath => fit_to_path(app, ui),
        DockerTab::Coordinates => crate::ui::coords_docker::coordinates(app, ui),
        DockerTab::VectorMosaic => crate::ui::vector_mosaic_docker::vector_mosaic(app, ui),
        DockerTab::PictureMosaic => crate::ui::picture_mosaic_docker::picture_mosaic(app, ui),
        DockerTab::ShapeGenerator => crate::ui::shape_gen_docker::shape_generator(app, ui),
        DockerTab::SymbolScatter => crate::ui::symbol_scatter_docker::symbol_scatter(app, ui),
        DockerTab::FrameGenerator => crate::ui::frame_gen_docker::frame_generator(app, ui),
        DockerTab::Shape3d => crate::ui::shape3d_docker::shape_3d(app, ui),
        DockerTab::Shelf => crate::ui::shelf_docker::shelf(app, ui),
        DockerTab::Export => crate::ui::export_docker::export_docker(app, ui),
        DockerTab::Comments => crate::ui::comments_docker::comments_docker(app, ui),
        DockerTab::AiGenerate => crate::ui::ai_generate_docker::ai_generate(app, ui),
        DockerTab::Effects => crate::ui::effects_docker::effects_docker(app, ui),
        DockerTab::Macros => crate::ui::macros_ui::manager(app, ui),
        DockerTab::PrintMerge => crate::ui::print_merge_docker::print_merge(app, ui),
        DockerTab::PixelLayers
        | DockerTab::PixelSelection
        | DockerTab::PixelRetouch
        | DockerTab::PixelDistort
        | DockerTab::PixelEffects
        | DockerTab::PixelImage
        | DockerTab::PixelHistogram
        | DockerTab::PixelHints => crate::ui::pixel_mode::docker_body(app, ui, tab),
        _ => {}
    }
}

fn step_and_repeat(app: &mut App, ui: &mut Ui) {
    let u = app.units;
    let has = !app.selection.is_empty();
    // The panel is worked in while the pointer is on it, one of its fields
    // has the keys, or the last click (or tap) was in it; a click
    // anywhere else ends that (#1197).
    let over = ui.rect_contains_pointer(ui.max_rect());
    if ui.input(|i| i.pointer.any_pressed()) {
        app.step_repeat_working = over;
    }
    // Copies one after another, or the page filled with them (#1255).
    ui.horizontal(|ui| {
        ui.radio_value(
            &mut app.step_repeat.fill,
            false,
            tr("docker.sr_mode_copies"),
        )
        .rich_tip(crate::ui::tips::pair(
            "docker.sr_mode_copies",
            "docker.sr_mode_copies_desc",
        ));
        ui.radio_value(&mut app.step_repeat.fill, true, tr("docker.sr_mode_fill"))
            .rich_tip(crate::ui::tips::pair(
                "docker.sr_mode_fill",
                "docker.sr_mode_fill_desc",
            ));
    });
    // With nothing selected the fields wait, keeping their values (#1198).
    let (focused, can_apply) = if app.step_repeat.fill {
        let page = app.page_rect();
        let layout = app.fill_page_layout();
        let focused = ui
            .add_enabled_ui(has, |ui| {
                fill_page_fields(&mut app.step_repeat, u, page, layout.as_ref(), ui)
            })
            .inner;
        let ok = layout.as_ref().is_some_and(|(_, _, g)| !g.cells.is_empty());
        (focused, ok)
    } else {
        let focused = ui
            .add_enabled_ui(has, |ui| {
                step_and_repeat_fields(&mut app.step_repeat, u, ui)
            })
            .inner;
        (focused, true)
    };
    let desc = if app.step_repeat.fill {
        "docker.sr_fill_apply_desc"
    } else {
        "docker.sr_apply_desc"
    };
    if ui
        .add_enabled(has && can_apply, egui::Button::new(tr("docker.apply")))
        .rich_tip(crate::ui::tips::pair("docker.apply", desc))
        .clicked()
    {
        app.step_and_repeat_apply();
    }
    // The copies Apply would make, outlined on the canvas (#1197).
    if has && (over || focused || app.step_repeat_working) {
        app.step_preview_shown = app.step_and_repeat_preview().is_some();
    }
}

/// The panel's fields; true when one of them has the keyboard focus.
fn step_and_repeat_fields(
    sr: &mut crate::ops2::StepRepeat,
    u: crate::app::Units,
    ui: &mut Ui,
) -> bool {
    let mut focused = false;
    ui.horizontal(|ui| {
        ui.label(tr("docker.copies"));
        focused |= ui
            .add(crate::ui::field::NumField::new(&mut sr.copies).range(1..=999))
            .rich_tip(crate::ui::tips::keyed("docker.sr_copies"))
            .has_focus();
    });
    // Each axis can be turned off, so the copies go across only or up
    // and down only, and each goes one way or the other (#2572).
    ui.checkbox(
        &mut sr.use_x,
        egui::RichText::new(tr("docker.horizontal_settings")).strong(),
    )
    .rich_tip(crate::ui::tips::pair(
        "docker.horizontal_settings",
        "docker.sr_use_x_desc",
    ));
    let on = sr.use_x;
    ui.add_enabled_ui(on, |ui| {
        direction(ui, &mut sr.left, ("docker.sr_right", "docker.sr_left"));
    });
    ui.add_enabled_ui(on, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut sr.mode_x, 0, tr("docker.offset"))
                .rich_tip(crate::ui::tips::pair(
                    "docker.offset",
                    "docker.sr_offset_desc",
                ));
            ui.radio_value(&mut sr.mode_x, 1, tr("docker.spacing"))
                .rich_tip(crate::ui::tips::pair(
                    "docker.spacing",
                    "docker.sr_spacing_desc",
                ));
        });
        let mut dx = u.world_from_mm(sr.dx);
        let r = ui
            .add(
                crate::ui::field::NumField::new(&mut dx)
                    .speed(0.5)
                    .suffix(format!(" {}", u.short())),
            )
            .rich_tip(crate::ui::tips::keyed("docker.sr_dx"));
        focused |= r.has_focus();
        if r.changed() {
            sr.dx = u.world_to_mm(dx);
        }
    });
    ui.checkbox(
        &mut sr.use_y,
        egui::RichText::new(tr("docker.vertical_settings")).strong(),
    )
    .rich_tip(crate::ui::tips::pair(
        "docker.vertical_settings",
        "docker.sr_use_y_desc",
    ));
    let on = sr.use_y;
    ui.add_enabled_ui(on, |ui| {
        direction(ui, &mut sr.up, ("docker.sr_down", "docker.sr_up"));
    });
    ui.add_enabled_ui(on, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut sr.mode_y, 0, tr("docker.offset"))
                .rich_tip(crate::ui::tips::pair(
                    "docker.offset",
                    "docker.sr_offset_desc",
                ));
            ui.radio_value(&mut sr.mode_y, 1, tr("docker.spacing"))
                .rich_tip(crate::ui::tips::pair(
                    "docker.spacing",
                    "docker.sr_spacing_desc",
                ));
        });
        let mut dy = u.world_from_mm(sr.dy);
        let r = ui
            .add(
                crate::ui::field::NumField::new(&mut dy)
                    .speed(0.5)
                    .suffix(format!(" {}", u.short())),
            )
            .rich_tip(crate::ui::tips::keyed("docker.sr_dy"));
        focused |= r.has_focus();
        if r.changed() {
            sr.dy = u.world_to_mm(dy);
        }
    });
    focused
}

/// The way the copies go along one axis: `keys` names the forward way
/// (right, down) and the back one (left, up); `back` holds the choice.
fn direction(ui: &mut Ui, back: &mut bool, keys: (&str, &str)) {
    ui.horizontal(|ui| {
        for (value, key) in [(false, keys.0), (true, keys.1)] {
            ui.radio_value(back, value, tr(key))
                .rich_tip(crate::ui::tips::pair(key, "docker.sr_direction_desc"));
        }
    });
}

/// A length field in the document's unit holding millimetres, at least
/// `min`; true when it has the keyboard focus.
fn fill_length(ui: &mut Ui, u: crate::app::Units, mm: &mut f64, min: f64, key: &str) -> bool {
    let mut v = u.world_from_mm(*mm);
    let r = ui
        .add(
            crate::ui::field::NumField::new(&mut v)
                .speed(0.5)
                .suffix(format!(" {}", u.short())),
        )
        .rich_tip(crate::ui::tips::keyed(key));
    if r.changed() {
        let x = u.world_to_mm(v);
        *mm = if x.is_finite() { x.max(min) } else { min };
    }
    r.has_focus()
}

/// Fill page's fields (#1255): the page size, the area, the margins, the
/// gaps, the shift of every second row, which way the copies stand with
/// the count of each way, the trim marks and the count of the copies
/// Apply makes; true when one of them has the keyboard focus.
fn fill_page_fields(
    sr: &mut crate::ops2::StepRepeat,
    u: crate::app::Units,
    page: tracedraw_core::geometry::Rect,
    layout: Option<&(
        crate::fill_page::FillGrid,
        crate::fill_page::FillGrid,
        crate::fill_page::FillGrid,
    )>,
    ui: &mut Ui,
) -> bool {
    let mut focused = false;
    let size = |v: f64| {
        format!("{:.2}", u.world_from_mm(v))
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };
    ui.label(
        egui::RichText::new(trf(
            "docker.fillpage_page_size",
            &[
                ("w", &size(page.width())),
                ("h", &size(page.height())),
                ("unit", u.short()),
            ],
        ))
        .color(crate::theme::Tokens::text_dim()),
    );
    ui.horizontal(|ui| {
        ui.label(tr("docker.fillpage_area"));
        ui.radio_value(&mut sr.area_custom, false, tr("docker.fillpage_area_page"))
            .rich_tip(crate::ui::tips::keyed("docker.fillpage_area_page"));
        if ui
            .radio(sr.area_custom, tr("docker.fillpage_area_custom"))
            .rich_tip(crate::ui::tips::keyed("docker.fillpage_area_custom"))
            .clicked()
            && !sr.area_custom
        {
            sr.area_custom = true;
            sr.area = page;
        }
    });
    egui::Grid::new("fill_page_grid")
        .num_columns(2)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            if sr.area_custom {
                let a = sr.area;
                let (mut x, mut y, mut w, mut h) = (a.x0, a.y0, a.width(), a.height());
                ui.label(tr("docker.fillpage_area_x"));
                focused |= fill_length(ui, u, &mut x, f64::MIN, "docker.fillpage_area_x");
                ui.end_row();
                ui.label(tr("docker.fillpage_area_y"));
                focused |= fill_length(ui, u, &mut y, f64::MIN, "docker.fillpage_area_y");
                ui.end_row();
                ui.label(tr("docker.fillpage_area_w"));
                focused |= fill_length(ui, u, &mut w, 1.0, "docker.fillpage_area_w");
                ui.end_row();
                ui.label(tr("docker.fillpage_area_h"));
                focused |= fill_length(ui, u, &mut h, 1.0, "docker.fillpage_area_h");
                ui.end_row();
                sr.area = tracedraw_core::geometry::Rect::new(x, y, x + w, y + h);
            }
            ui.label(tr("docker.fillpage_margins"));
            ui.checkbox(&mut sr.margins_same, tr("docker.fillpage_same"))
                .rich_tip(crate::ui::tips::keyed("docker.fillpage_margins_same"));
            ui.end_row();
            if sr.margins_same {
                ui.label("");
                focused |= fill_length(ui, u, &mut sr.margins[0], 0.0, "docker.fillpage_margins");
                ui.end_row();
            } else {
                for (i, key) in [
                    "print.margin_left",
                    "print.margin_right",
                    "print.margin_top",
                    "print.margin_bottom",
                ]
                .iter()
                .enumerate()
                {
                    ui.label(tr(key));
                    focused |=
                        fill_length(ui, u, &mut sr.margins[i], 0.0, "docker.fillpage_margins");
                    ui.end_row();
                }
            }
            ui.label(tr("docker.fillpage_gap"));
            ui.checkbox(&mut sr.gap_same, tr("docker.fillpage_same"))
                .rich_tip(crate::ui::tips::keyed("docker.fillpage_gap_same"));
            ui.end_row();
            if sr.gap_same {
                ui.label("");
                focused |= fill_length(ui, u, &mut sr.gap.0, 0.0, "docker.fillpage_gap");
                ui.end_row();
            } else {
                ui.label(tr("docker.fillpage_gap_x"));
                focused |= fill_length(ui, u, &mut sr.gap.0, 0.0, "docker.fillpage_gap_x");
                ui.end_row();
                ui.label(tr("docker.fillpage_gap_y"));
                focused |= fill_length(ui, u, &mut sr.gap.1, 0.0, "docker.fillpage_gap_y");
                ui.end_row();
            }
            ui.label(tr("docker.fillpage_row_shift"));
            focused |= fill_length(ui, u, &mut sr.row_shift, 0.0, "docker.fillpage_row_shift");
            ui.end_row();
        });
    // Which way the copies stand, with how many fit each way.
    ui.strong(tr("docker.fillpage_turn"));
    let count = |g: Option<&crate::fill_page::FillGrid>| {
        g.map(|g| {
            if crate::fill_page::count(g) == 0 {
                tr("docker.fillpage_none")
            } else {
                App::fill_count_text(g)
            }
        })
        .unwrap_or_default()
    };
    for (i, key, grid) in [
        (0u8, "docker.fillpage_as_is", layout.map(|l| &l.0)),
        (1, "docker.fillpage_turned", layout.map(|l| &l.1)),
        (2, "docker.fillpage_best", None),
    ] {
        ui.horizontal(|ui| {
            ui.radio_value(&mut sr.turn, i, tr(key))
                .rich_tip(crate::ui::tips::keyed(key));
            if grid.is_some() {
                ui.label(egui::RichText::new(count(grid)).color(crate::theme::Tokens::text_dim()));
            }
        });
    }
    ui.checkbox(&mut sr.marks, tr("docker.fillpage_marks"))
        .rich_tip(crate::ui::tips::keyed("docker.fillpage_marks"));
    if sr.marks {
        egui::Grid::new("fill_page_marks")
            .num_columns(2)
            .spacing([6.0, 4.0])
            .show(ui, |ui| {
                ui.label(tr("docker.fillpage_mark_length"));
                focused |= fill_length(
                    ui,
                    u,
                    &mut sr.mark_length,
                    0.0,
                    "docker.fillpage_mark_length",
                );
                ui.end_row();
                ui.label(tr("docker.fillpage_mark_offset"));
                focused |= fill_length(
                    ui,
                    u,
                    &mut sr.mark_offset,
                    0.0,
                    "docker.fillpage_mark_offset",
                );
                ui.end_row();
            });
    }
    // The copies Apply makes.
    if let Some((_, _, g)) = layout {
        let n = crate::fill_page::count(g);
        if n == 0 {
            ui.colored_label(ui.visuals().warn_fg_color, tr("docker.fillpage_none"));
        } else if g.cells.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                trf(
                    "status.fill_page_too_many",
                    &[("max", &crate::fill_page::MAX_COPIES.to_string())],
                ),
            );
        } else {
            ui.strong(App::fill_count_text(g));
        }
    }
    focused
}

/// A colour style of `c`, named after its place in the list (New color
/// style, and the Colour panel's Add to Color Styles Palette).
pub(crate) fn add_color_style(app: &mut App, c: Color) {
    let mut st = app.doc().color_styles.clone();
    st.push(tracedraw_core::ColorStyle {
        name: format!("Color {}", st.len() + 1),
        color: c,
        harmony: None,
    });
    app.run(Command::SetColorStyles { styles: st });
}

fn color_styles(app: &mut App, ui: &mut Ui) {
    crate::ui::color_styles_docker::show(app, ui);
}

/// Object Styles: the default properties of new objects, then the styles
/// with their parts and the objects linked to each.
fn object_styles(app: &mut App, ui: &mut Ui) {
    default_properties(app, ui);
    ui.separator();
    styles_list(app, ui);
}

/// A fill swatch and an outline swatch (a red stroke across for none).
fn style_swatches(ui: &mut Ui, fill: &Fill, stroke: &Option<tracedraw_core::Stroke>) {
    let none = |ui: &mut Ui, r: egui::Rect| {
        ui.painter().line_segment(
            [r.left_bottom(), r.right_top()],
            egui::Stroke::new(1.5, egui::Color32::from_rgb(0xD0, 0x30, 0x30)),
        );
    };
    let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 2.0, egui::Color32::WHITE);
    match crate::app::fill_preview_color(fill) {
        Some(c) => {
            ui.painter().rect_filled(r, 2.0, c);
        }
        None => none(ui, r),
    }
    ui.painter().rect_stroke(
        r,
        2.0,
        egui::Stroke::new(1.0, Tokens::control_border()),
        egui::StrokeKind::Inside,
    );
    let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 2.0, egui::Color32::WHITE);
    match stroke {
        Some(s) => {
            let c = crate::canvas::to_color32(s.color);
            let w = (s.width as f32 * 4.0).clamp(1.0, 4.0);
            ui.painter().rect_stroke(
                r.shrink(2.0),
                1.0,
                egui::Stroke::new(w, c),
                egui::StrokeKind::Inside,
            );
        }
        None => none(ui, r),
    }
}

/// Default Properties: what each kind of new object starts with.
fn default_properties(app: &mut App, ui: &mut Ui) {
    use tracedraw_core::{DefaultKind, ObjectDefaults};
    ui.strong(tr("defaults.title"));
    let first = app.selected_shapes().first().cloned();
    for kind in DefaultKind::ALL {
        let d = app.doc().defaults.get(kind).clone();
        ui.horizontal(|ui| {
            style_swatches(ui, &d.fill, &d.stroke);
            ui.label(tr(crate::defaults::kind_key(kind)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(tr("docker.reset"))
                    .rich_tip(tr("defaults.reset_hint"))
                    .clicked()
                {
                    // The defaults of the drawing's mode, their black
                    // following the colour mode again.
                    let mode = app.doc().metadata.primary_color_mode;
                    let fresh = ObjectDefaults::for_mode(mode).get(kind).clone();
                    app.set_default_properties(kind, |p| *p = fresh);
                }
                let from = ui
                    .add_enabled(
                        first.is_some(),
                        egui::Button::new(tr("docker.from_selection")).small(),
                    )
                    .rich_tip(tr("defaults.from_selection_hint"));
                if from.clicked() {
                    if let Some(s) = &first {
                        let (fill, stroke) = (s.fill.clone(), s.stroke.clone());
                        // Colours taken from an object are the user's
                        // own, even a black.
                        app.set_default_properties(kind, |p| {
                            p.fill = fill;
                            p.stroke = stroke;
                            p.choose_fill(kind);
                            p.choose_outline(kind);
                        });
                    }
                }
            });
        });
    }
    ui.checkbox(&mut app.settings.defaults.ask, tr("defaults.ask"))
        .rich_tip(crate::ui::tips::pair(
            "defaults.ask_tip",
            "defaults.ask_desc",
        ));
}

/// The styles: a row each (swatches, name, objects linked, Apply, Update,
/// delete) and the parts it sets.
fn styles_list(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.strong(tr("docker.object_styles"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    !app.selection.is_empty(),
                    egui::Button::new(tr("docker.new_style_from_selection")).small(),
                )
                .rich_tip(crate::ui::tips::keyed("docker.new_style_from_selection"))
                .clicked()
            {
                app.new_style_from_selection();
            }
        });
    });
    let styles = app.doc().object_styles.clone();
    if styles.is_empty() {
        dim(ui, tr("docker.object_styles_hint"));
    }
    let counts = crate::styles::linked_counts(app.doc());
    let first = app.selected_shapes().first().cloned();
    let mut st = styles.clone();
    let mut changed = false;
    let mut remove = None;
    let mut apply = None;
    for (i, os) in st.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            style_swatches(ui, &os.fill, &os.stroke);
            let field = egui::Id::new(("object_style_name", i));
            let (_, typed) = kept_text(ui, field, os.id, &os.name.clone(), |ui, text| {
                ui.add(egui::TextEdit::singleline(text).desired_width(96.0))
                    .rich_tip(crate::ui::tips::keyed("docker.style_name"))
            });
            if let Some(name) = typed {
                os.name = name;
                changed = true;
            }
            let n = counts.get(&os.id).copied().unwrap_or(0);
            dim(ui, trf("docker.style_objects", &[("n", &n.to_string())]));
        });
        ui.horizontal(|ui| {
            ui.add_space(38.0);
            changed |= ui
                .checkbox(&mut os.parts.fill, tr("docker.style_fill"))
                .rich_tip(crate::ui::tips::keyed("docker.style_fill"))
                .changed();
            changed |= ui
                .checkbox(&mut os.parts.outline, tr("docker.style_outline"))
                .rich_tip(crate::ui::tips::keyed("docker.style_outline"))
                .changed();
            changed |= ui
                .checkbox(&mut os.parts.transparency, tr("docker.style_transparency"))
                .rich_tip(crate::ui::tips::keyed("docker.style_transparency"))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.add_space(38.0);
            if ui
                .add_enabled(
                    !app.selection.is_empty(),
                    egui::Button::new(tr("docker.apply")).small(),
                )
                .rich_tip(tr("docker.style_apply_hint"))
                .clicked()
            {
                apply = Some(i);
            }
            if ui
                .add_enabled(
                    first.is_some(),
                    egui::Button::new(tr("docker.update_from_selection")).small(),
                )
                .rich_tip(tr("docker.style_update_hint"))
                .clicked()
            {
                if let Some(s) = &first {
                    os.fill = s.fill.clone();
                    os.stroke = s.stroke.clone();
                    os.opacity = s.opacity;
                    changed = true;
                }
            }
            if ui
                .small_button(tr("docker.delete"))
                .rich_tip(tr("docker.style_delete_hint"))
                .clicked()
            {
                remove = Some(i);
            }
        });
        ui.add_space(2.0);
    }
    if let Some(i) = remove {
        st.remove(i);
        changed = true;
    }
    if changed {
        app.run(Command::SetObjectStyles { styles: st });
    } else if let Some(i) = apply {
        app.apply_style(i);
    }
    let linked = !app.styled_selection().is_empty();
    if ui
        .add_enabled(linked, egui::Button::new(tr("docker.break_style_link")))
        .rich_tip(crate::ui::tips::keyed("docker.break_style_link"))
        .clicked()
    {
        app.break_style_links();
    }
}

/// The special characters a Find or Replace field's right-click menu
/// puts in (#3679), with their codes; `^w` only in Find.
pub(crate) const FIND_CODES: [(&str, &str); 5] = [
    ("find.code_paragraph", "^p"),
    ("menu.text.code_line_break", "^l"),
    ("find.code_tab", "^t"),
    ("menu.text.code_nonbreaking_space", "^s"),
    ("find.code_spaces", "^w"),
];

/// `text` with the letters `from..to` (counted in characters) replaced
/// by `put`, and where the cursor goes after it.
pub(crate) fn put_in_field(text: &mut String, from: usize, to: usize, put: &str) -> usize {
    let byte = |n: usize| text.char_indices().nth(n).map_or(text.len(), |(b, _)| b);
    let (a, b) = (byte(from.min(to)), byte(from.max(to)));
    text.replace_range(a..b, put);
    from.min(to) + put.chars().count()
}

/// A Find or Replace field with a right-click menu: Cut, Copy, Paste,
/// Select All, and the special characters as their codes (#3678, #3679).
/// `spaces`: the menu offers `^w` (two or more spaces), a Find code only.
fn find_field(ui: &mut Ui, text: &mut String, name: &str, spaces: bool) -> egui::Response {
    let id = ui.make_persistent_id(name);
    let out = egui::TextEdit::singleline(text).id(id).show(ui);
    let all = text.chars().count();
    let (from, to) = egui::TextEdit::load_state(ui.ctx(), id)
        .and_then(|s| s.cursor.char_range())
        .map(|r| {
            let (a, b) = (r.primary.index.0, r.secondary.index.0);
            (a.min(b), a.max(b))
        })
        .unwrap_or((all, all));
    let mut put: Option<(usize, usize, String)> = None;
    let mut select_all = false;
    let response = out.response.response.clone();
    response.context_menu(|ui| {
        let picked: String = text.chars().skip(from).take(to - from).collect();
        if ui
            .add_enabled(from < to, egui::Button::new(tr("menu.edit.cut")))
            .clicked()
        {
            ui.ctx().copy_text(picked.clone());
            put = Some((from, to, String::new()));
            ui.close();
        }
        if ui
            .add_enabled(from < to, egui::Button::new(tr("menu.edit.copy")))
            .clicked()
        {
            ui.ctx().copy_text(picked);
            ui.close();
        }
        if ui.button(tr("menu.edit.paste")).clicked() {
            if let Some(t) = crate::clipboard::system_text() {
                // One line: the field has no room for line ends.
                put = Some((from, to, t.replace(['\r', '\n'], " ")));
            }
            ui.close();
        }
        if ui.button(tr("menu.edit.select_all")).clicked() {
            select_all = true;
            ui.close();
        }
        ui.separator();
        ui.menu_button(tr("find.special_character"), |ui| {
            for (key, code) in FIND_CODES {
                if code == "^w" && !spaces {
                    continue;
                }
                if ui.button(format!("{}  {code}", tr(key))).clicked() {
                    put = Some((from, to, code.to_string()));
                    ui.close();
                }
            }
        })
        .response
        .rich_tip(crate::ui::tips::pair(
            "find.special_character",
            "find.special_character_desc",
        ));
    });
    let cursor = match put {
        Some((a, b, s)) => Some(put_in_field(text, a, b, &s)),
        None => None,
    };
    if cursor.is_some() || select_all {
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
            let range = match cursor {
                Some(c) => egui::text::CCursorRange::one(egui::text::CCursor::new(c)),
                None => egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(text.chars().count()),
                ),
            };
            state.cursor.set_char_range(Some(range));
            state.store(ui.ctx(), id);
        }
        ui.memory_mut(|m| m.request_focus(id));
    }
    response
}

fn find_replace(app: &mut App, ui: &mut Ui) {
    let mut st = std::mem::take(&mut app.find_state);
    ui.horizontal(|ui| {
        ui.radio_value(&mut st.kind, 0, tr("dialog.find_text"))
            .rich_tip(crate::ui::tips::pair(
                "dialog.find_text",
                "docker.find_text_desc",
            ));
        ui.radio_value(&mut st.kind, 1, tr("dialog.find_objects"))
            .rich_tip(crate::ui::tips::pair(
                "dialog.find_objects",
                "docker.find_objects_desc",
            ));
        ui.radio_value(&mut st.kind, 2, tr("find.properties"))
            .rich_tip(crate::ui::tips::pair(
                "find.properties",
                "find.properties_desc",
            ));
    });
    if st.kind == 2 {
        find_properties(app, ui, &mut st);
        app.find_state = st;
        return;
    }
    ui.horizontal(|ui| {
        ui.label(tr("dialog.find"));
        find_field(ui, &mut st.find, "find_replace_find", true).rich_tip(crate::ui::tips::pair(
            "docker.find_field",
            "docker.find_field_desc",
        ));
    });
    if st.kind == 0 {
        ui.horizontal(|ui| {
            ui.label(tr("dialog.replace_with"));
            find_field(ui, &mut st.replace, "find_replace_replace", false).rich_tip(
                crate::ui::tips::pair("docker.replace_field", "docker.replace_field_desc"),
            );
        });
        ui.checkbox(&mut st.match_case, tr("dialog.match_case"))
            .rich_tip(crate::ui::tips::pair(
                "dialog.match_case",
                "docker.match_case_desc",
            ));
        ui.checkbox(&mut st.whole_word, tr("dialog.whole_word"))
            .rich_tip(crate::ui::tips::pair(
                "dialog.whole_word",
                "docker.whole_word_desc",
            ));
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(tr("dialog.find_next"))
            .rich_tip(crate::ui::tips::pair(
                "dialog.find_next",
                "docker.find_next_desc",
            ))
            .clicked()
        {
            st.results =
                crate::export::find_shapes(app, &st.find, st.kind, st.match_case, st.whole_word);
            if !st.results.is_empty() {
                st.cursor = (st.cursor + 1) % st.results.len();
                let id = st.results[st.cursor];
                app.select(vec![id]);
            }
        }
        if ui
            .button(tr("dialog.find_all"))
            .rich_tip(crate::ui::tips::pair(
                "dialog.find_all",
                "docker.find_all_desc",
            ))
            .clicked()
        {
            st.results =
                crate::export::find_shapes(app, &st.find, st.kind, st.match_case, st.whole_word);
            app.select(st.results.clone());
        }
        if st.kind == 0 {
            if ui
                .button(tr("dialog.replace"))
                .rich_tip(crate::ui::tips::pair(
                    "dialog.replace",
                    "docker.replace_desc",
                ))
                .clicked()
            {
                crate::export::replace_text(
                    app,
                    &st.find,
                    &st.replace,
                    st.match_case,
                    st.whole_word,
                    false,
                );
            }
            if ui
                .button(tr("dialog.replace_all"))
                .rich_tip(crate::ui::tips::pair(
                    "dialog.replace_all",
                    "docker.replace_all_desc",
                ))
                .clicked()
            {
                crate::export::replace_text(
                    app,
                    &st.find,
                    &st.replace,
                    st.match_case,
                    st.whole_word,
                    true,
                );
            }
        }
    });
    ui.label(trf(
        "dialog.found_n",
        &[("n", &st.results.len().to_string())],
    ));
    app.find_state = st;
}

/// Find and Replace by properties: fill, outline, outline width, font and
/// object type, on the page or in the whole drawing (#1071).
fn find_properties(app: &mut App, ui: &mut Ui, st: &mut crate::ui::dialogs::FindReplaceState) {
    use crate::ui::color_picker::color_button;
    use tracedraw_core::find::ObjectType;
    let units = app.outline_units();
    let width_field = |ui: &mut Ui, mm: &mut f64| {
        let mut v = units.from_mm(*mm);
        if ui
            .add(
                crate::ui::field::NumField::new(&mut v)
                    .speed(0.05)
                    .suffix(format!(" {}", units.short())),
            )
            .changed()
        {
            *mm = units.to_mm(v).max(0.0);
        }
    };
    let pf = &mut st.props;
    ui.horizontal(|ui| {
        ui.label(tr("find.look_in"));
        ui.radio_value(&mut pf.whole_drawing, false, tr("find.this_page"))
            .rich_tip(crate::ui::tips::pair(
                "find.this_page",
                "find.this_page_desc",
            ));
        ui.radio_value(&mut pf.whole_drawing, true, tr("find.whole_drawing"))
            .rich_tip(crate::ui::tips::pair(
                "find.whole_drawing",
                "find.whole_drawing_desc",
            ));
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tr("find.find_objects_with")).strong());
    egui::Grid::new("find_props_grid")
        .num_columns(2)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            ui.checkbox(&mut pf.use_type, tr("find.object_type"))
                .rich_tip(crate::ui::tips::keyed("find.object_type"));
            ui.add_enabled_ui(pf.use_type, |ui| {
                let cur = ObjectType::ALL
                    .get(pf.object_type)
                    .map(|t| tr(&format!("find.type.{}", t.key())))
                    .unwrap_or_default();
                crate::ui::keynav::ComboBox::from_id_salt("find_props_type")
                    .selected_text(cur)
                    .show_ui(ui, |ui| {
                        for (i, t) in ObjectType::ALL.iter().enumerate() {
                            ui.selectable_value(
                                &mut pf.object_type,
                                i,
                                tr(&format!("find.type.{}", t.key())),
                            );
                        }
                    });
            });
            ui.end_row();

            ui.checkbox(&mut pf.use_fill, tr("find.fill"))
                .rich_tip(crate::ui::tips::keyed("find.fill"));
            ui.add_enabled_ui(pf.use_fill, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut pf.fill_none, tr("find.none"))
                        .rich_tip(crate::ui::tips::pair("find.none", "find.no_fill_desc"));
                    ui.add_enabled_ui(!pf.fill_none, |ui| color_button(ui, &mut pf.fill));
                });
            });
            ui.end_row();

            ui.checkbox(&mut pf.use_outline, tr("find.outline"))
                .rich_tip(crate::ui::tips::keyed("find.outline"));
            ui.add_enabled_ui(pf.use_outline, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut pf.outline_none, tr("find.none"))
                        .rich_tip(crate::ui::tips::pair("find.none", "find.no_outline_desc"));
                    ui.add_enabled_ui(!pf.outline_none, |ui| color_button(ui, &mut pf.outline));
                });
            });
            ui.end_row();

            ui.checkbox(&mut pf.use_width, tr("find.outline_width"))
                .rich_tip(crate::ui::tips::keyed("find.outline_width"));
            ui.add_enabled_ui(pf.use_width, |ui| width_field(ui, &mut pf.width));
            ui.end_row();

            ui.checkbox(&mut pf.use_font, tr("find.font"))
                .rich_tip(crate::ui::tips::keyed("find.font"));
            // Every font the drawing's text uses, not only the clicked
            // word's (#2485).
            ui.add_enabled_ui(pf.use_font, |ui| {
                let used = app.doc().font_families_used();
                let shown = if pf.font.trim().is_empty() {
                    tr("find.pick_font")
                } else {
                    pf.font.clone()
                };
                crate::ui::keynav::ComboBox::from_id_salt("find_props_font")
                    .selected_text(shown)
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        if used.is_empty() {
                            ui.weak(tr("find.no_fonts_used"));
                        }
                        for f in &used {
                            if ui.selectable_label(pf.font == *f, f).clicked() {
                                pf.font = f.clone();
                            }
                        }
                    })
                    .response
                    .rich_tip(crate::ui::tips::pair("find.font", "find.font_used_desc"));
            });
            ui.end_row();
        });
    ui.horizontal(|ui| {
        let first = app
            .selection
            .first()
            .and_then(|id| app.doc().find_shape(*id))
            .cloned();
        if ui
            .add_enabled(
                first.is_some(),
                egui::Button::new(tr("find.from_selection")),
            )
            .rich_tip(crate::ui::tips::pair(
                "find.from_selection",
                "find.from_selection_desc",
            ))
            .clicked()
        {
            if let Some(s) = &first {
                pf.take_from(s);
            }
        }
        if ui
            .button(tr("find.reset"))
            .rich_tip(crate::ui::tips::pair("find.reset", "find.reset_desc"))
            .clicked()
        {
            let whole = pf.whole_drawing;
            *pf = Default::default();
            pf.whole_drawing = whole;
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tr("find.replace_with")).strong());
    egui::Grid::new("find_props_replace")
        .num_columns(2)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            ui.checkbox(&mut pf.set_fill, tr("find.fill"))
                .rich_tip(crate::ui::tips::pair("find.fill", "find.set_fill_desc"));
            ui.add_enabled_ui(pf.set_fill, |ui| color_button(ui, &mut pf.new_fill));
            ui.end_row();
            ui.checkbox(&mut pf.set_outline, tr("find.outline"))
                .rich_tip(crate::ui::tips::pair(
                    "find.outline",
                    "find.set_outline_desc",
                ));
            ui.add_enabled_ui(pf.set_outline, |ui| color_button(ui, &mut pf.new_outline));
            ui.end_row();
            ui.checkbox(&mut pf.set_width, tr("find.outline_width"))
                .rich_tip(crate::ui::tips::pair(
                    "find.outline_width",
                    "find.set_width_desc",
                ));
            ui.add_enabled_ui(pf.set_width, |ui| width_field(ui, &mut pf.new_width));
            ui.end_row();
            ui.checkbox(&mut pf.set_font, tr("find.font"))
                .rich_tip(crate::ui::tips::pair("find.font", "find.set_font_desc"));
            // The font list, to browse for the new font (#2485).
            ui.add_enabled_ui(pf.set_font, |ui| {
                let (pick, _) = crate::ui::font_list::font_list_live(
                    app,
                    ui,
                    "find_replace_font",
                    &pf.new_font,
                    160.0,
                );
                if let Some(p) = pick {
                    pf.new_font = p.family;
                }
            });
            ui.end_row();
            // Which characters take the new font (#2585).
            ui.label("");
            ui.add_enabled_ui(pf.set_font, |ui| {
                ui.horizontal_wrapped(|ui| {
                    use tracedraw_core::find::CharClass;
                    for c in CharClass::ALL {
                        let key = format!("find.chars_{}", c.key());
                        ui.radio_value(&mut pf.font_chars, c, tr(&key))
                            .rich_tip(crate::ui::tips::pair(&key, "find.chars_desc"));
                    }
                });
            });
            ui.end_row();
            ui.checkbox(&mut pf.western_digits, tr("find.western_digits"))
                .rich_tip(crate::ui::tips::pair(
                    "find.western_digits",
                    "find.western_digits_desc",
                ));
            ui.end_row();
        });
    let can_find = !pf.query().is_empty();
    let can_replace = can_find && !pf.replacement().is_empty();
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(can_find, egui::Button::new(tr("find.previous")))
            .rich_tip(crate::ui::tips::pair("find.previous", "find.previous_desc"))
            .clicked()
        {
            crate::export::step_found(app, st, false);
            st.results = st.props.hits.iter().map(|h| h.id).collect();
        }
        if ui
            .add_enabled(can_find, egui::Button::new(tr("dialog.find_next")))
            .rich_tip(crate::ui::tips::pair(
                "dialog.find_next",
                "docker.find_next_desc",
            ))
            .clicked()
        {
            crate::export::step_found(app, st, true);
            st.results = st.props.hits.iter().map(|h| h.id).collect();
        }
        if ui
            .add_enabled(can_find, egui::Button::new(tr("dialog.find_all")))
            .rich_tip(crate::ui::tips::pair(
                "dialog.find_all",
                "docker.find_all_desc",
            ))
            .clicked()
        {
            let here = crate::export::select_found(app, st);
            st.results = st.props.hits.iter().map(|h| h.id).collect();
            app.status = trf(
                "find.status_found",
                &[
                    ("n", &st.props.hits.len().to_string()),
                    ("here", &here.to_string()),
                ],
            );
        }
        if ui
            .add_enabled(can_replace, egui::Button::new(tr("dialog.replace_all")))
            .rich_tip(crate::ui::tips::pair(
                "dialog.replace_all",
                "find.replace_all_desc",
            ))
            .clicked()
        {
            let n = crate::export::replace_properties(app, st);
            st.results = st.props.hits.iter().map(|h| h.id).collect();
            app.status = trf("find.status_replaced", &[("n", &n.to_string())]);
        }
    });
    ui.label(trf(
        "dialog.found_n",
        &[("n", &st.props.hits.len().to_string())],
    ));
}

fn scripts(app: &mut App, ui: &mut Ui) {
    let debugging = app.script_debug.is_some() || app.macros.run.is_some();
    // The language of the snippet: JavaScript, or VBA (run as a macro).
    ui.horizontal(|ui| {
        let vba = app.macros.scripts_vba;
        let js_r = ui
            .selectable_label(!vba, "JavaScript")
            .rich_tip_with(|| format!("JavaScript\n{}", tr("docker.js_desc")));
        let vba_r = ui
            .add_enabled(!crate::files::WEB, egui::Button::selectable(vba, "VBA"))
            .rich_tip(tr("docker.vba_hint"))
            // Greyed in the browser with the reason (#1416).
            .rich_disabled_tip(tr("macros.desktop_only"));
        if (js_r.clicked() && vba) || (vba_r.clicked() && !vba) {
            std::mem::swap(&mut app.script_source, &mut app.macros.other_source);
            app.macros.scripts_vba = !vba;
            if app.macros.scripts_vba && app.script_source.is_empty() {
                app.script_source = "Sub Main()\n    Dim s As Shape\n    Set s = ActiveLayer.CreateRectangle2(1, 1, 2, 1)\n    s.Fill.UniformColor.CMYKAssign 0, 100, 100, 0\n    Debug.Print \"Objects: \" & ActivePage.Shapes.Count\nEnd Sub\n".into();
            }
        }
    });
    let vba = app.macros.scripts_vba;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(!debugging, egui::Button::new(tr("docker.run")))
            .rich_tip(crate::ui::tips::pair(
                "docker.run",
                "docker.run_script_desc",
            ))
            .clicked()
        {
            let src = app.script_source.clone();
            if vba {
                app.run_vba_snippet(&src);
            } else {
                app.run_script(&src);
            }
        }
        if app.macros.run.is_some()
            && ui
                .button(tr("docker.stop"))
                .rich_tip(crate::ui::tips::pair(
                    "docker.stop",
                    "docker.stop_script_desc",
                ))
                .clicked()
        {
            app.stop_macro();
        }
        // In the browser the button stays, greyed with the reason (#1416).
        if !vba
            && ui
                .add_enabled(
                    !debugging && !crate::files::WEB,
                    egui::Button::new(tr("docker.debug")),
                )
                .rich_tip(tr("docker.debug_hint"))
                .rich_disabled_tip(if crate::files::WEB {
                    tr("docker.debug_web")
                } else {
                    tr("docker.debug_hint")
                })
                .clicked()
        {
            app.start_script_debug();
        }
        if ui
            .selectable_label(app.recording.is_some(), tr("docker.record"))
            .rich_tip(crate::ui::tips::pair("docker.record", "docker.record_desc"))
            .clicked()
        {
            app.toggle_recording();
        }
        let (lang, ext) = if vba {
            ("VBA", "bas")
        } else {
            ("JavaScript", "js")
        };
        if ui
            .button(tr("docker.load"))
            .rich_tip(crate::ui::tips::pair(
                "docker.load",
                "docker.load_script_desc",
            ))
            .clicked()
        {
            crate::files::Dialog::new()
                .add_filter(lang, &[ext])
                .pick_file(app, |app, p| {
                    if let Ok(s) = crate::files::read_to_string(p) {
                        app.script_source = s;
                    }
                });
        }
        if ui
            .button(tr("docker.save"))
            .rich_tip(crate::ui::tips::pair(
                "docker.save",
                "docker.save_script_desc",
            ))
            .clicked()
        {
            // The save dialog, a download, or the system's save picker on
            // Android (#1402).
            let source = app.script_source.clone();
            crate::files::Dialog::new()
                .add_filter(lang, &[ext])
                .set_file_name(format!("macro.{ext}"))
                .save_bytes(app, source);
        }
        if ui
            .button(tr("docker.clear"))
            .rich_tip(crate::ui::tips::pair(
                "docker.clear",
                "docker.clear_output_desc",
            ))
            .clicked()
        {
            app.scripts_output.clear();
        }
        if !vba
            && ui
                .button(tr("docker.install_filter"))
                .rich_tip(tr("docker.install_filter_hint"))
                .clicked()
        {
            let src = app.script_source.clone();
            app.install_script_filter(&src);
        }
        // A script that defines TraceDrawPlugin: its commands go to
        // Tools > Plugins (#1802).
        if !vba
            && ui
                .button(tr("docker.install_plugin"))
                .rich_tip(crate::ui::tips::keyed("docker.install_plugin"))
                .clicked()
        {
            let src = app.script_source.clone();
            app.install_plugin(&src);
        }
    });
    if !app.settings.script_filters.is_empty() {
        ui.strong(tr("docker.script_filters"));
        let mut remove = None;
        let mut edit = None;
        for f in &app.settings.script_filters {
            ui.horizontal(|ui| {
                ui.label(f.label());
                if ui
                    .small_button(tr("docker.edit"))
                    .rich_tip(crate::ui::tips::pair(
                        "docker.edit",
                        "docker.edit_filter_desc",
                    ))
                    .clicked()
                {
                    edit = Some(f.source.clone());
                }
                if ui
                    .small_button(tr("docker.remove"))
                    .rich_tip(crate::ui::tips::pair(
                        "docker.remove",
                        "docker.remove_filter_desc",
                    ))
                    .clicked()
                {
                    remove = Some(f.name.clone());
                }
            });
        }
        if let Some(src) = edit {
            app.script_source = src;
        }
        if let Some(name) = remove {
            app.remove_script_filter(&name);
        }
    }
    if app.script_source.is_empty() && !vba {
        app.script_source = "// Example\nconst r = ActiveLayer.CreateRectangle(20, 20, 60, 40);\nr.Fill.ApplyUniformFill(Color.CMYK(0, 100, 100, 0));\nprint('Objects: ' + ActivePage.Shapes.Count);\n".into();
    }
    if debugging {
        script_debug_bar(app, ui);
    }
    egui::ScrollArea::vertical()
        .id_salt("script_src")
        .max_height(220.0)
        .show(ui, |ui| script_editor(app, ui));
    ui.separator();
    ui.strong(tr("docker.output"));
    egui::ScrollArea::vertical()
        .id_salt("script_out")
        .max_height(140.0)
        .show(ui, |ui| {
            for line in &app.scripts_output {
                ui.label(egui::RichText::new(line).monospace().size(11.0));
            }
        });
    dim(
        ui,
        tr(if vba {
            "docker.scripts_hint_vba"
        } else {
            "docker.scripts_hint"
        }),
    );
}

/// Continue, Step and Stop for a script in the debugger, where it is,
/// and an expression to evaluate where it paused.
fn script_debug_bar(app: &mut App, ui: &mut Ui) {
    use crate::scripting::DebugCtl;
    let paused = app.debug_paused;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(paused.is_some(), egui::Button::new(tr("docker.continue")))
            .clicked()
        {
            app.debug_control(DebugCtl::Continue);
        }
        if ui
            .add_enabled(paused.is_some(), egui::Button::new(tr("docker.step")))
            .clicked()
        {
            app.debug_control(DebugCtl::Step);
        }
        if ui.button(tr("docker.stop")).clicked() {
            app.debug_control(DebugCtl::Stop);
        }
        match paused {
            Some(line) => ui.label(trf("docker.paused_at", &[("line", &line.to_string())])),
            None => ui.label(tr("docker.running")),
        };
    });
    if paused.is_some() {
        ui.horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut app.debug_expr)
                    .hint_text(tr("docker.expression"))
                    .desired_width(ui.available_width() - 80.0),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button(tr("docker.evaluate")).clicked() || enter)
                && !app.debug_expr.trim().is_empty()
            {
                let e = app.debug_expr.trim().to_string();
                app.debug_control(DebugCtl::Eval(e));
            }
        });
    }
}

/// Width of the editor's margin with the line numbers and breakpoints.
const GUTTER: f32 = 34.0;

/// The script editor: line numbers and breakpoints (red dots, set and
/// cleared by a click in the margin) to its left, the paused line marked.
fn script_editor(app: &mut App, ui: &mut Ui) {
    let out = egui::TextEdit::multiline(&mut app.script_source)
        .code_editor()
        .desired_width(f32::INFINITY)
        .desired_rows(10)
        .margin(egui::Margin {
            left: GUTTER as i8,
            right: 4,
            top: 2,
            bottom: 2,
        })
        .show(ui);
    let rect = out.response.response.rect;
    let painter = ui.painter_at(rect);
    let font = egui::FontId::monospace(10.0);
    // The span of each line of the script (wrapped rows belong to the
    // line they continue) and the middle of its first row.
    let mut tops: Vec<(f32, f32)> = Vec::new();
    let mut mids: Vec<f32> = Vec::new();
    let mut new_line = true;
    for row in &out.galley.rows {
        let r = row.rect().translate(out.galley_pos.to_vec2());
        if new_line {
            tops.push((r.top(), r.bottom()));
            mids.push(r.center().y);
        } else if let Some(last) = tops.last_mut() {
            last.1 = r.bottom();
        }
        new_line = row.ends_with_newline;
    }
    let gutter = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.left() + GUTTER - 4.0, rect.bottom()),
    );
    painter.rect_filled(gutter.shrink(1.0), 0.0, Tokens::panel());
    for (i, (top, bottom)) in tops.iter().enumerate() {
        let line = i as u32 + 1;
        let y = mids.get(i).copied().unwrap_or((top + bottom) / 2.0);
        if app.debug_paused == Some(line) {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.left(), *top),
                    egui::pos2(rect.right(), *bottom),
                ),
                0.0,
                egui::Color32::from_rgba_unmultiplied(255, 220, 0, 60),
            );
        }
        if app.breakpoints.contains(&line) {
            painter.circle_filled(
                egui::pos2(rect.left() + 7.0, y),
                4.0,
                egui::Color32::from_rgb(0xD0, 0x30, 0x30),
            );
        }
        painter.text(
            egui::pos2(rect.left() + GUTTER - 7.0, y),
            egui::Align2::RIGHT_CENTER,
            line.to_string(),
            font.clone(),
            Tokens::text_dim(),
        );
    }
    // The margin takes its own clicks (laid over the editor's).
    let margin = ui.interact(gutter, ui.id().with("script_gutter"), egui::Sense::click());
    if margin.clicked() {
        if let Some(p) = margin.interact_pointer_pos().or(margin.hover_pos()) {
            if let Some(i) = tops.iter().position(|(t, b)| p.y >= *t && p.y < *b) {
                app.toggle_breakpoint(i as u32 + 1);
            }
        }
    }
}

fn palettes(app: &mut App, ui: &mut Ui) {
    crate::ui::palette_manager::show(app, ui);
}

fn current_effect<F: Fn(&Effect) -> bool>(app: &App, f: F) -> Option<Effect> {
    app.selected_shapes()
        .first()
        .and_then(|s| s.effects.iter().find(|e| f(e)).cloned())
}

fn lens(app: &mut App, ui: &mut Ui) {
    if let Some(Effect::Lens(l)) = current_effect(app, |e| matches!(e, Effect::Lens(_))) {
        if app.lens_synced_to != app.selection.first().copied() {
            app.lens = crate::lens::LensSettings::from_lens(&l);
            app.lens_synced_to = app.selection.first().copied();
        }
    }
    let keys = crate::lens::LENS_KEYS;
    crate::ui::keynav::ComboBox::from_id_salt("lens_kind")
        .selected_text(tr(keys[app.lens.kind]))
        .show_ui(ui, |ui| {
            for (i, k) in keys.iter().enumerate() {
                if ui.selectable_label(app.lens.kind == i, tr(k)).clicked() {
                    app.lens.kind = i;
                    // Tinted grayscale starts black: plain grays (#2800).
                    if i == 9 && app.lens.color == tracedraw_core::Color::WHITE {
                        app.lens.color = tracedraw_core::Color::BLACK;
                    }
                    // Cut starts with a black shadow at 60 %.
                    if i == 13 && app.lens.color == tracedraw_core::Color::WHITE {
                        app.lens.color = tracedraw_core::Color::BLACK;
                        app.lens.rate = 60.0;
                    }
                }
            }
        })
        .response
        .rich_tip(crate::ui::tips::keyed("docker.lens_kind"));
    let l = &mut app.lens;
    match l.kind {
        1 => {
            ui.add(crate::ui::Slider::new(&mut l.rate, -100.0..=100.0).text(tr("docker.rate")))
                .rich_tip(crate::ui::tips::pair(
                    "docker.rate",
                    "docker.lens_rate_desc",
                ));
        }
        2 | 3 | 10 => {
            ui.add(crate::ui::Slider::new(&mut l.rate, 0.0..=100.0).text(tr("docker.rate")))
                .rich_tip(crate::ui::tips::pair(
                    "docker.rate",
                    "docker.lens_rate_desc",
                ));
            ui.horizontal(|ui| {
                ui.label(tr("docker.color"));
                color_button(ui, &mut l.color);
            });
        }
        4 => {
            ui.horizontal(|ui| {
                ui.label(tr("docker.from"));
                color_button(ui, &mut l.color_to);
                ui.label(tr("docker.to"));
                color_button(ui, &mut l.color);
            });
        }
        5 => {
            ui.add(crate::ui::Slider::new(&mut l.rate, -100.0..=100.0).text(tr("docker.rate")))
                .rich_tip(crate::ui::tips::pair(
                    "docker.rate",
                    "docker.lens_rate_desc",
                ));
        }
        6 => {
            ui.add(
                crate::ui::Slider::new(&mut l.rate, 0.0..=100.0)
                    .text(tr("docker.palette_rotation")),
            )
            .rich_tip(crate::ui::tips::keyed("docker.palette_rotation"));
        }
        8 => {
            ui.add(crate::ui::Slider::new(&mut l.amount, 1.0..=10.0).text(tr("docker.amount")))
                .rich_tip(crate::ui::tips::pair(
                    "docker.amount",
                    "docker.lens_amount_desc",
                ));
        }
        9 => {
            ui.horizontal(|ui| {
                ui.label(tr("docker.color"));
                color_button(ui, &mut l.color);
            });
        }
        11 => {
            // The objects beneath: their outlines and their insides (#2800).
            ui.horizontal(|ui| {
                ui.label(tr("docker.outline"));
                color_button(ui, &mut l.color_to);
                ui.label(tr("docker.fill"));
                color_button(ui, &mut l.color);
            });
        }
        13 => {
            // Cut: the spurs where the lens's middle line crosses the
            // objects beneath, the cut itself and the shadow under it.
            ui.add(
                crate::ui::Slider::new(&mut l.spur_length, 0.0..=50.0)
                    .text(tr("lens.cut_spur_length"))
                    .suffix(" mm"),
            )
            .rich_tip(crate::ui::tips::pair(
                "lens.cut_spur_length",
                "lens.cut_spur_length_desc",
            ));
            ui.add(
                crate::ui::Slider::new(&mut l.spur_width, 0.0..=50.0)
                    .text(tr("lens.cut_spur_width"))
                    .suffix(" mm"),
            )
            .rich_tip(crate::ui::tips::pair(
                "lens.cut_spur_width",
                "lens.cut_spur_width_desc",
            ));
            ui.add(
                crate::ui::Slider::new(&mut l.curve, 0.5..=6.0).text(tr("lens.cut_curve")),
            )
            .rich_tip(crate::ui::tips::pair(
                "lens.cut_curve",
                "lens.cut_curve_desc",
            ));
            ui.add(crate::ui::Slider::new(&mut l.gap, 0.0..=10.0).text(tr("lens.cut_gap")).suffix(" mm"))
                .rich_tip(crate::ui::tips::pair("lens.cut_gap", "lens.cut_gap_desc"));
            ui.add(
                crate::ui::Slider::new(&mut l.rate, 0.0..=100.0).text(tr("lens.cut_shadow"))
                    .suffix(" %"),
            )
            .rich_tip(crate::ui::tips::pair(
                "lens.cut_shadow",
                "lens.cut_shadow_desc",
            ));
            ui.horizontal(|ui| {
                ui.label(tr("docker.color"));
                color_button(ui, &mut l.color);
                ui.checkbox(&mut l.shadow_up, tr("lens.cut_shadow_up"))
                    .rich_tip(crate::ui::tips::pair(
                        "lens.cut_shadow_up",
                        "lens.cut_shadow_up_desc",
                    ));
            });
        }
        12 => {
            // The effects, in the order they run, each with its default
            // settings; Effects > Effect Picker adds them too.
            let mut remove = None;
            for (i, e) in l.effects.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}. {}", i + 1, tr(&format!("fx.{}", e.id))));
                    if ui
                        .small_button("\u{00D7}")
                        .rich_tip(crate::ui::tips::keyed("docker.lens_remove_effect"))
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                l.effects.remove(i);
            }
            crate::ui::keynav::ComboBox::from_id_salt("lens_fx_add")
                .selected_text(tr("lens.add_effect"))
                .height(360.0)
                .show_ui(ui, |ui| {
                    for (group, list) in crate::fx::groups() {
                        ui.label(
                            egui::RichText::new(tr(group))
                                .color(Tokens::text_dim())
                                .size(11.0),
                        );
                        for fx in list.iter() {
                            if ui
                                .selectable_label(false, tr(&format!("fx.{}", fx.id)))
                                .clicked()
                            {
                                l.effects.push(crate::fx::new_effect(fx.id));
                            }
                        }
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("lens.add_effect"));
        }
        _ => {}
    }
    ui.checkbox(&mut l.frozen, tr("docker.frozen"))
        .rich_tip(crate::ui::tips::keyed("docker.frozen"));
    let has = !app.selection.is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(has, egui::Button::new(tr("docker.apply")))
            .rich_tip(crate::ui::tips::pair(
                "docker.apply",
                "docker.lens_apply_desc",
            ))
            .clicked()
        {
            match app.lens.to_lens() {
                Some(lens) => {
                    if app.lens.frozen {
                        app.push_effect(Effect::Lens(lens), true);
                        app.freeze_lens();
                    } else {
                        app.push_effect(Effect::Lens(lens), true);
                    }
                }
                None => {
                    app.remove_effects_of_kind(&Effect::Lens(tracedraw_core::live::Lens::Invert))
                }
            }
        }
        if ui
            .add_enabled(has, egui::Button::new(tr("docker.remove")))
            .rich_tip(crate::ui::tips::pair(
                "docker.remove",
                "docker.lens_remove_desc",
            ))
            .clicked()
        {
            app.remove_effects_of_kind(&Effect::Lens(tracedraw_core::live::Lens::Invert));
        }
    });
    dim(ui, tr("docker.lens_hint"));
}

/// A small picture button of the Blend docker; true when clicked.
fn blend_pic(
    ui: &mut Ui,
    tip: &str,
    on: bool,
    enabled: bool,
    draw: impl Fn(&egui::Painter, egui::Rect, Color32),
) -> bool {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
    let resp = resp.rich_tip(tip);
    let p = ui.painter();
    if on {
        p.rect_filled(r, 2.0, crate::theme::Tokens::tool_hover());
        p.rect_stroke(
            r,
            2.0,
            egui::Stroke::new(1.0, Tokens::ACCENT),
            egui::StrokeKind::Inside,
        );
    } else if resp.hovered() && enabled {
        p.rect_stroke(
            r,
            2.0,
            egui::Stroke::new(1.0, Tokens::border()),
            egui::StrokeKind::Inside,
        );
    }
    let ink = if enabled {
        Tokens::text()
    } else {
        Tokens::text_dim()
    };
    draw(p, r.shrink(5.0), ink);
    enabled && resp.clicked()
}

use egui::Color32;

/// Where the blend's start and end objects sit along its path, in % of
/// its length: changing either moves the object there (#2946).
pub(crate) fn blend_path_ends(app: &mut App, ui: &mut Ui, opt: &tracedraw_core::live::BlendOptions) {
    let (a, b) = opt.path_ends();
    let (mut from, mut to) = (a * 100.0, b * 100.0);
    let mut moved = false;
    for (key, v) in [
        ("docker.blend_path_start", &mut from),
        ("docker.blend_path_end", &mut to),
    ] {
        ui.horizontal(|ui| {
            ui.label(tr(key));
            moved |= ui
                .add(
                    crate::ui::field::NumField::new(v)
                        .range(0.0..=100.0)
                        .speed(0.5)
                        .max_decimals(1)
                        .suffix(" %"),
                )
                .rich_tip(crate::ui::tips::keyed(key))
                .changed();
        });
    }
    if moved {
        app.set_blend_path_ends(from / 100.0, to / 100.0);
    }
}

/// The Blend docker: steps or spacing, rotation with Loop, the path
/// options, the acceleration of objects and colours (with Apply to
/// sizing, Link acceleration and Reset), the way the colours turn, and
/// Apply for two selected objects.
fn blend(app: &mut App, ui: &mut Ui) {
    use crate::ui::docker_form::section;
    use tracedraw_core::live::{BlendOptions, ColorBlend};
    let cur = app.selected_blends().into_iter().next().map(|(_, e)| e);
    let (mut steps, mut accel_o, mut accel_c, mut rot, mut rotate_on_path, has_path, mut opt) =
        match &cur {
            Some(Effect::Blend {
                steps,
                accel_objects,
                accel_colors,
                rotation,
                rotate_on_path,
                path,
                options,
                ..
            }) => (
                *steps,
                *accel_objects,
                *accel_colors,
                *rotation,
                *rotate_on_path,
                path.is_some(),
                *options,
            ),
            _ => (
                app.blend_steps,
                0.0,
                0.0,
                0.0,
                false,
                false,
                app.blend_options,
            ),
        };
    let mut changed = false;
    // A bent line is followed as a path is: spacing and turning with it
    // apply.
    let follows = has_path || opt.bend.is_some();
    let mut spacing_mm = opt.spacing.unwrap_or(app.blend_spacing_mm);
    // Steps or spacing: spacing needs a path.
    ui.horizontal(|ui| {
        let by_spacing = opt.spacing.is_some();
        if blend_pic(
            ui,
            &tr("docker.blend_by_steps"),
            !by_spacing,
            true,
            |p, r, c| {
                let s = egui::Stroke::new(1.3, c);
                let pts = [
                    r.left_bottom(),
                    egui::pos2(r.left() + r.width() / 3.0, r.bottom()),
                    egui::pos2(r.left() + r.width() / 3.0, r.center().y),
                    egui::pos2(r.left() + 2.0 * r.width() / 3.0, r.center().y),
                    egui::pos2(r.left() + 2.0 * r.width() / 3.0, r.top()),
                    r.right_top(),
                ];
                p.add(egui::Shape::line(pts.to_vec(), s));
            },
        ) && by_spacing
        {
            opt.spacing = None;
            changed = true;
        }
        if blend_pic(
            ui,
            &tr("docker.blend_by_spacing"),
            by_spacing,
            follows,
            |p, r, c| {
                let s = egui::Stroke::new(1.3, c);
                let y = r.center().y;
                p.line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], s);
                for x in [r.left(), r.right()] {
                    p.line_segment([egui::pos2(x, y - 4.0), egui::pos2(x, y + 4.0)], s);
                }
            },
        ) && !by_spacing
        {
            opt.spacing = Some(spacing_mm);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("docker.steps"));
        changed |= ui
            .add_enabled(
                opt.spacing.is_none(),
                crate::ui::field::NumField::new(&mut steps).range(1..=999),
            )
            .rich_tip(crate::ui::tips::pair(
                "docker.steps",
                "toolbar.blend_steps_desc",
            ))
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label(tr("docker.blend_spacing"));
        let units = app.units;
        let mut v = units.from_mm(spacing_mm);
        if ui
            .add_enabled(
                opt.spacing.is_some(),
                crate::ui::field::NumField::new(&mut v)
                    .speed(0.1)
                    .max_decimals(3)
                    .suffix(format!(" {}", units.short())),
            )
            .rich_tip(crate::ui::tips::keyed("docker.blend_spacing"))
            .rich_disabled_tip(crate::ui::tips::keyed("docker.blend_spacing"))
            .changed()
        {
            spacing_mm = units.to_mm(v).max(0.01);
            opt.spacing = Some(spacing_mm);
            changed = true;
        }
    });
    app.blend_spacing_mm = spacing_mm;
    ui.horizontal(|ui| {
        ui.label(tr("docker.rotation"));
        changed |= ui
            .add(
                crate::ui::field::NumField::new(&mut rot)
                    .range(-3600.0..=3600.0)
                    .speed(1.0)
                    .max_decimals(1)
                    .suffix("\u{b0}"),
            )
            .rich_tip(crate::ui::tips::pair(
                "docker.rotation",
                "toolbar.blend_rotation_desc",
            ))
            .changed();
    });
    changed |= ui
        .add_enabled(
            rot != 0.0,
            egui::Checkbox::new(&mut opt.loop_rotation, tr("docker.blend_loop")),
        )
        .rich_tip(crate::ui::tips::keyed("docker.blend_loop"))
        .rich_disabled_tip(crate::ui::tips::keyed("docker.blend_loop"))
        .changed();
    changed |= ui
        .add_enabled(
            has_path,
            egui::Checkbox::new(&mut opt.full_path, tr("docker.blend_full_path")),
        )
        .rich_tip(crate::ui::tips::keyed("docker.blend_full_path"))
        .rich_disabled_tip(crate::ui::tips::keyed("docker.blend_full_path"))
        .changed();
    changed |= ui
        .add_enabled(
            follows,
            egui::Checkbox::new(&mut rotate_on_path, tr("docker.rotate_all_objects")),
        )
        .rich_tip(crate::ui::tips::keyed("docker.rotate_all_objects"))
        .rich_disabled_tip(crate::ui::tips::keyed("docker.rotate_all_objects"))
        .changed();
    // Two groups: object by object, each in its own colours.
    changed |= ui
        .checkbox(&mut opt.by_parts, tr("docker.blend_by_parts"))
        .rich_tip(crate::ui::tips::keyed("docker.blend_by_parts"))
        .changed();
    // A bent line (the round handle in its middle, Blend tool) made
    // straight again.
    if opt.bend.is_some()
        && ui
            .button(tr("docker.blend_straighten"))
            .rich_tip(crate::ui::tips::keyed("docker.blend_straighten"))
            .clicked()
    {
        opt.bend = None;
        changed = true;
    }
    if has_path {
        blend_path_ends(app, ui, &opt);
    }
    section(ui, Some(&tr("docker.blend_acceleration")));
    ui.spacing_mut().slider_width = (ui.available_width() - 80.0).clamp(80.0, 200.0);
    ui.horizontal(|ui| {
        ui.label(tr("docker.blend_object"));
        if ui
            .add(crate::ui::Slider::new(&mut accel_o, -1.0..=1.0).show_value(false))
            .rich_tip(crate::ui::tips::pair(
                "docker.blend_object",
                "docker.object_acceleration_desc",
            ))
            .changed()
        {
            changed = true;
            if app.blend_link_accel {
                accel_c = accel_o;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("docker.blend_color"));
        if ui
            .add_enabled(
                !app.blend_link_accel,
                crate::ui::Slider::new(&mut accel_c, -1.0..=1.0).show_value(false),
            )
            .rich_tip(crate::ui::tips::pair(
                "docker.blend_color",
                "docker.color_acceleration_desc",
            ))
            .changed()
        {
            changed = true;
        }
    });
    changed |= ui
        .checkbox(&mut opt.accel_sizing, tr("docker.blend_apply_sizing"))
        .rich_tip(crate::ui::tips::keyed("docker.blend_apply_sizing"))
        .changed();
    let mut link = app.blend_link_accel;
    if ui
        .checkbox(&mut link, tr("docker.blend_link_accel"))
        .rich_tip(crate::ui::tips::keyed("docker.blend_link_accel"))
        .changed()
    {
        app.blend_link_accel = link;
        if link && accel_c != accel_o {
            accel_c = accel_o;
            changed = true;
        }
    }
    // One row high: laid out right to left over the whole rest of the
    // panel, it pushed Color Blend out of sight (#815).
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 24.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if ui
                .add_enabled(
                    accel_o != 0.0 || accel_c != 0.0,
                    egui::Button::new(tr("docker.reset")).min_size(egui::vec2(90.0, 22.0)),
                )
                .rich_tip(crate::ui::tips::pair(
                    "docker.reset",
                    "docker.blend_reset_desc",
                ))
                .clicked()
            {
                accel_o = 0.0;
                accel_c = 0.0;
                changed = true;
            }
        },
    );
    section(ui, Some(&tr("docker.blend_color_blend")));
    ui.horizontal(|ui| {
        for (how, key) in [
            (ColorBlend::Direct, "docker.blend_direct"),
            (ColorBlend::Clockwise, "docker.blend_clockwise"),
            (
                ColorBlend::CounterClockwise,
                "docker.blend_counterclockwise",
            ),
        ] {
            let on = opt.color_blend == how;
            if blend_pic(ui, &tr(key), on, true, |p, r, c| {
                let s = egui::Stroke::new(1.3, c);
                let ctr = r.center();
                let rad = r.width().min(r.height()) / 2.0;
                match how {
                    ColorBlend::Direct => {
                        p.line_segment([r.left_bottom(), r.right_top()], s);
                    }
                    _ => {
                        let pts: Vec<egui::Pos2> = (0..=12)
                            .map(|k| {
                                let a = std::f32::consts::PI * (0.15 + 1.4 * k as f32 / 12.0);
                                let a = if how == ColorBlend::Clockwise { -a } else { a };
                                ctr + egui::vec2(a.cos(), a.sin()) * rad
                            })
                            .collect();
                        let end = *pts.last().unwrap_or(&ctr);
                        p.add(egui::Shape::line(pts, s));
                        p.circle_filled(end, 2.2, c);
                    }
                }
            }) && !on
            {
                opt.color_blend = how;
                changed = true;
            }
        }
    });
    // A blend between two texts: their characters linked, so the word
    // changes once in both ends (#1568).
    if let Some((_, linked)) = app.blend_text_ends() {
        let mut on = linked;
        if ui
            .checkbox(&mut on, tr("docker.blend_link_text"))
            .rich_tip(crate::ui::tips::keyed("docker.blend_link_text"))
            .changed()
        {
            app.set_blend_text_link(on);
        }
    }
    // Apply: two objects selected and no blend yet, a blend between them
    // with these settings (#815).
    let two = cur.is_none() && app.selection.len() == 2;
    let apply = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 28.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.add_enabled(
                    two,
                    egui::Button::new(tr("docker.apply")).min_size(egui::vec2(90.0, 22.0)),
                )
                .rich_tip(crate::ui::tips::pair(
                    "docker.apply",
                    "docker.blend_apply_desc",
                ))
                .rich_disabled_tip(crate::ui::tips::pair(
                    "docker.apply",
                    "docker.blend_apply_desc",
                ))
                .clicked()
            },
        )
        .inner;
    app.blend_steps = steps;
    app.blend_options = BlendOptions { ..opt };
    if apply {
        app.blend_selection(steps, accel_o, accel_c, rot);
    }
    if let Some(Effect::Blend { .. }) = cur {
        if changed {
            app.change_blends("Blend", |e| {
                let mut e = e.clone();
                if let Effect::Blend {
                    steps: s,
                    accel_objects,
                    accel_colors,
                    rotation,
                    rotate_on_path: r,
                    options,
                    ..
                } = &mut e
                {
                    *s = steps;
                    *accel_objects = accel_o;
                    *accel_colors = accel_c;
                    *rotation = rot;
                    *r = rotate_on_path;
                    *options = opt;
                }
                Some(e)
            });
        }
        section(ui, None);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(tr("docker.new_path"))
                .rich_tip(crate::ui::tips::keyed("docker.new_path"))
                .clicked()
            {
                app.pending_blend_path = true;
                app.status = tr("status.click_blend_path");
            }
            if has_path
                && ui
                    .button(tr("docker.detach_path"))
                    .rich_tip(crate::ui::tips::keyed("docker.detach_path"))
                    .clicked()
            {
                app.detach_blend_path();
            }
            if ui
                .button(tr("docker.clear_blend"))
                .rich_tip(crate::ui::tips::keyed("docker.clear_blend"))
                .clicked()
            {
                app.clear_blend();
            }
            if ui
                .button(tr("docker.break_apart"))
                .rich_tip(crate::ui::tips::pair(
                    "docker.break_apart",
                    "toolbar.effect_break_apart_desc",
                ))
                .clicked()
            {
                app.break_effect_groups();
            }
        });
    } else {
        let two = app.selection.len() == 2;
        let tip = crate::ui::tips::pair("docker.apply", "docker.blend_apply_desc");
        if crate::ui::docker_form::wide_button(ui, &tr("docker.apply"), two, &tip) {
            app.blend_selection(steps, accel_o, accel_c, rot);
        }
        if !two {
            dim(ui, tr("docker.blend_hint"));
        }
    }
}

fn contour(app: &mut App, ui: &mut Ui) {
    crate::ui::contour_bar::contour_controls(app, ui);
    crate::ui::contour_bar::contour_actions(app, ui);
}

fn extrude(app: &mut App, ui: &mut Ui) {
    crate::ui::extrude_bar::extrude_docker(app, ui);
}

fn bevel(app: &mut App, ui: &mut Ui) {
    let cur = current_effect(app, |e| matches!(e, Effect::Bevel { .. }));
    // The panel shows the selected object's own bevel every frame, so its
    // sliders edit that bevel (after Undo, or another object picked and
    // picked back too) instead of the last settings used.
    if let Some(Effect::Bevel {
        distance,
        light_angle,
        intensity,
        style,
        shadow_color,
        light_color,
        level,
        emboss_color,
        directional,
    }) = &cur
    {
        {
            app.bevel = crate::effects_ui::BevelSettings {
                distance: *distance,
                light_angle: *light_angle,
                intensity: *intensity,
                style: *style,
                shadow_color: *shadow_color,
                light_color: *light_color,
                level: *level,
                emboss_color: *emboss_color,
                directional: *directional,
            };
            app.bevel_synced_to = app.selection.first().copied();
        }
    }
    // The colour an own emboss colour starts from: the object's solid fill.
    let fill_color = app
        .selected_shapes()
        .first()
        .and_then(|s| match &s.fill {
            tracedraw_core::Fill::Solid(c) => Some(*c),
            _ => None,
        })
        .unwrap_or(Color::rgb8(128, 128, 128));
    use crate::ui::docker_form::{row, section};
    use crate::ui::tips::{keyed, pair};
    let b = &mut app.bevel;
    let mut changed = false;
    // The style: three equal buttons across the panel.
    ui.horizontal(|ui| {
        let w = (ui.available_width() - ui.spacing().item_spacing.x * 2.0) / 3.0;
        for (style, key) in [
            (0, "docker.soft_edge"),
            (1, "docker.emboss"),
            (2, "docker.bevel_center"),
        ] {
            let pick = ui
                .add_sized(
                    [w, 26.0],
                    egui::Button::selectable(b.style == style, tr(key)),
                )
                .rich_tip(keyed(key))
                .clicked();
            if pick && b.style != style {
                b.style = style;
                changed = true;
            }
        }
    });
    changed |= ui
        .add(
            crate::ui::Slider::new(&mut b.distance, 0.1..=50.0)
                .text(tr("docker.depth"))
                .logarithmic(true),
        )
        .rich_tip(pair("docker.depth", "docker.bevel_distance_desc"))
        .changed();
    let mut level = i32::from(b.level);
    let top = i32::from(tracedraw_core::live::MAX_BEVEL_LEVEL);
    if ui
        .add(
            crate::ui::Slider::new(&mut level, 1..=top)
                .text(tr("docker.level"))
                .integer(),
        )
        .rich_tip(keyed("docker.level"))
        .changed()
    {
        b.level = level.clamp(1, top) as u8;
        changed = true;
    }
    section(ui, None);
    changed |= ui
        .add(
            crate::ui::Slider::new(&mut b.light_angle, 0.0..=360.0)
                .text(tr("docker.light_angle"))
                .suffix("°"),
        )
        .rich_tip(pair("docker.light_angle", "extrude.light_angle_desc"))
        .changed();
    changed |= ui
        .add(crate::ui::Slider::new(&mut b.intensity, 0.0..=100.0).text(tr("docker.intensity")))
        .rich_tip(pair("docker.intensity", "extrude.light_intensity_desc"))
        .changed();
    changed |= ui
        .checkbox(&mut b.directional, tr("docker.bevel_smooth_light"))
        .rich_tip(keyed("docker.bevel_smooth_light"))
        .changed();
    section(ui, None);
    row(ui, &tr("docker.light_color"), |ui| {
        changed |= color_button(ui, &mut b.light_color);
    });
    row(ui, &tr("docker.shadow_color"), |ui| {
        changed |= color_button(ui, &mut b.shadow_color);
    });
    row(ui, &tr("docker.emboss_color"), |ui| {
        let mut own = b.emboss_color.is_some();
        if ui
            .checkbox(&mut own, "")
            .rich_tip(keyed("docker.emboss_color"))
            .changed()
        {
            // Switched on, it starts from the object's fill, the colour the
            // band was toned from.
            b.emboss_color = own.then_some(b.emboss_color.unwrap_or(fill_color));
            changed = true;
        }
        if let Some(c) = &mut b.emboss_color {
            changed |= color_button(ui, c);
        } else {
            dim(ui, tr("docker.emboss_color_off"));
        }
    });
    let bv = app.bevel.clone();
    let effect = Effect::Bevel {
        distance: bv.distance,
        light_angle: bv.light_angle,
        intensity: bv.intensity,
        style: bv.style,
        shadow_color: bv.shadow_color,
        light_color: bv.light_color,
        level: bv.level,
        emboss_color: bv.emboss_color,
        directional: bv.directional,
    };
    let has = !app.selection.is_empty();
    // Live: a change shows on the selection at once, also before Apply
    // (#1034); Remove takes it away.
    if has && changed {
        app.push_effect(effect.clone(), true);
    }
    ui.horizontal(|ui| {
        if cur.is_none()
            && ui
                .add_enabled(has, egui::Button::new(tr("docker.apply")))
                .rich_tip(crate::ui::tips::pair(
                    "docker.apply",
                    "docker.bevel_apply_desc",
                ))
                .clicked()
        {
            app.push_effect(effect.clone(), true);
        }
        if cur.is_some()
            && ui
                .button(tr("docker.remove"))
                .rich_tip(crate::ui::tips::pair(
                    "docker.remove",
                    "docker.bevel_remove_desc",
                ))
                .clicked()
        {
            app.remove_effects_of_kind(&effect);
        }
    });
}

fn brush_strokes(app: &mut App, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        for m in crate::media::MediaMode::ALL {
            if ui
                .selectable_label(app.media_mode == m, tr(m.key()))
                .rich_tip_with(|| format!("{}\n{}", tr(m.key()), tr(&format!("{}_desc", m.key()))))
                .clicked()
            {
                app.media_mode = m;
            }
        }
    });
    let u = app.units;
    let mut w = u.from_mm(app.media_width);
    if ui
        .add(
            crate::ui::Slider::new(&mut w, 0.1..=u.from_mm(50.0))
                .text(tr("docker.stroke_width"))
                .logarithmic(true),
        )
        .rich_tip(crate::ui::tips::pair(
            "docker.stroke_width",
            "docker.media_width_desc",
        ))
        .changed()
    {
        app.media_width = u.to_mm(w);
    }
    match app.media_mode {
        crate::media::MediaMode::Calligraphic => {
            ui.add(
                crate::ui::Slider::new(&mut app.media_angle, 0.0..=180.0)
                    .text(tr("docker.nib_angle"))
                    .suffix("°"),
            )
            .rich_tip(crate::ui::tips::keyed("docker.nib_angle"));
        }
        crate::media::MediaMode::Preset | crate::media::MediaMode::Brush => {
            ui.horizontal_wrapped(|ui| {
                for (i, k) in crate::media::BRUSH_KEYS.iter().enumerate() {
                    if ui
                        .selectable_label(app.media_preset == i, tr(k))
                        .rich_tip_with(|| format!("{}\n{}", tr(k), tr("docker.media_preset_desc")))
                        .clicked()
                    {
                        app.media_preset = i;
                    }
                }
            });
        }
        crate::media::MediaMode::Sprayer => {
            ui.horizontal_wrapped(|ui| {
                for (i, k) in crate::media::SPRAY_KEYS.iter().enumerate() {
                    if ui
                        .selectable_label(app.media_preset == i, tr(k))
                        .rich_tip_with(|| format!("{}\n{}", tr(k), tr("docker.media_preset_desc")))
                        .clicked()
                    {
                        app.media_preset = i;
                    }
                }
            });
            ui.add(
                crate::ui::Slider::new(&mut app.media_spacing, 1.0..=50.0)
                    .text(tr("docker.spacing")),
            )
            .rich_tip(crate::ui::tips::pair(
                "docker.spacing",
                "docker.spray_spacing_desc",
            ));
        }
        crate::media::MediaMode::Tiles => {
            ui.horizontal_wrapped(|ui| {
                for (i, k) in crate::media::TILE_KEYS.iter().enumerate() {
                    if ui
                        .selectable_label(app.media_preset == i, tr(k))
                        .rich_tip_with(|| format!("{}\n{}", tr(k), tr("docker.media_preset_desc")))
                        .clicked()
                    {
                        app.media_preset = i;
                    }
                }
            });
        }
        crate::media::MediaMode::Merge => {}
        crate::media::MediaMode::Expression => {
            ui.add(
                crate::ui::Slider::new(&mut app.media_pressure, 0.0..=1.0)
                    .text(tr("docker.pressure")),
            )
            .rich_tip(crate::ui::tips::pair(
                "docker.pressure",
                "docker.pressure_desc",
            ));
        }
    }
    ui.horizontal(|ui| {
        ui.label(tr("docker.smoothing"));
        ui.add(crate::ui::Slider::new(
            &mut app.media_smoothing,
            0.0..=100.0,
        ))
        .rich_tip(crate::ui::tips::pair(
            "docker.smoothing",
            "docker.smoothing_desc",
        ));
    });
    let has_curve = app
        .selected_shapes()
        .iter()
        .any(|s| matches!(s.kind, ShapeKind::Path { .. }));
    if ui
        .add_enabled(has_curve, egui::Button::new(tr("docker.apply_to_curve")))
        .rich_tip(crate::ui::tips::keyed("docker.apply_to_curve"))
        .rich_disabled_tip(crate::ui::tips::keyed("docker.apply_to_curve"))
        .clicked()
    {
        app.apply_media_to_selection();
    }
    dim(ui, tr("docker.brush_strokes_hint"));
}

/// A square icon button drawn by `paint`, with its tip.
fn icon_btn(
    ui: &mut Ui,
    enabled: bool,
    tip: &str,
    paint: impl FnOnce(&egui::Painter, egui::Rect, egui::Color32),
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (r, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), sense);
    if enabled && resp.hovered() {
        ui.painter()
            .rect_filled(r, 2.0, crate::theme::Tokens::tool_hover());
    }
    let color = if enabled {
        crate::theme::Tokens::icon()
    } else {
        crate::theme::Tokens::icon_off()
    };
    paint(ui.painter(), r.shrink(5.0), color);
    resp.rich_tip(tip)
}

/// Image > Color Mask: ten colour slots (in use, colour, tolerance),
/// hide or show the colours in use, Apply on the selected bitmaps.
fn bitmap_mask(app: &mut App, ui: &mut Ui) {
    use crate::bitmap_fx::MASK_SLOTS;
    let has_bitmap = app
        .selected_shapes()
        .iter()
        .any(|s| matches!(s.kind, ShapeKind::Bitmap { .. }));
    ui.horizontal(|ui| {
        if icon_btn(ui, true, &tr("docker.mask_eyedropper"), |p, r, c| {
            crate::ui::icons::draw(p, r, crate::tools::Tool::ColorEyedropper, c)
        })
        .clicked()
        {
            // The next click on the drawing picks the slot's colour.
            app.start_mask_color_pick();
        }
        if icon_btn(ui, has_bitmap, &tr("docker.mask_from_bitmap"), |p, r, c| {
            // A bitmap with a grid of samples.
            p.rect_stroke(r, 0.0, egui::Stroke::new(1.2, c), egui::StrokeKind::Inside);
            for i in 1..3 {
                let t = i as f32 / 3.0;
                p.line_segment(
                    [
                        r.lerp_inside(egui::vec2(t, 0.0)),
                        r.lerp_inside(egui::vec2(t, 1.0)),
                    ],
                    egui::Stroke::new(0.8, c),
                );
                p.line_segment(
                    [
                        r.lerp_inside(egui::vec2(0.0, t)),
                        r.lerp_inside(egui::vec2(1.0, t)),
                    ],
                    egui::Stroke::new(0.8, c),
                );
            }
        })
        .clicked()
        {
            app.mask_colors_from_bitmap();
        }
    });
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(0xC8)))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("mask_slots")
                .max_height(200.0)
                .show(ui, |ui| {
                    for i in 0..MASK_SLOTS {
                        let picked = app.mask.picked == i;
                        let row = ui.horizontal(|ui| {
                            let slot = &mut app.mask.slots[i];
                            ui.checkbox(&mut slot.on, "")
                                .rich_tip(crate::ui::tips::keyed("docker.mask_slot_on"));
                            let (r, resp) = ui.allocate_exact_size(
                                egui::vec2((ui.available_width() - 40.0).max(60.0), 16.0),
                                egui::Sense::click(),
                            );
                            let resp = resp.rich_tip(crate::ui::tips::keyed("docker.mask_slot"));
                            let [cr, cg, cb] = slot.color.to_rgb8();
                            ui.painter().rect(
                                r,
                                0.0,
                                egui::Color32::from_rgb(cr, cg, cb),
                                egui::Stroke::new(
                                    if picked { 2.0 } else { 1.0 },
                                    if picked {
                                        crate::theme::Tokens::ACCENT
                                    } else {
                                        egui::Color32::from_gray(0xA0)
                                    },
                                ),
                                egui::StrokeKind::Inside,
                            );
                            ui.label(slot.tolerance.to_string());
                            resp.clicked()
                        });
                        if row.inner {
                            app.mask.picked = i;
                        }
                    }
                });
        });
    ui.horizontal(|ui| {
        if icon_btn(ui, true, &tr("docker.mask_save"), |p, r, c| {
            crate::ui::icons::draw_action(p, r, crate::ui::icons::Action::Save, c)
        })
        .clicked()
        {
            // The save dialog, a download, or the system's save picker on
            // Android (#1402).
            let text = app.mask.to_text();
            crate::files::Dialog::new()
                .add_filter(tr("docker.mask_file"), &["msk"])
                .set_file_name("mask.msk".to_string())
                .save_bytes(app, text);
        }
        if icon_btn(ui, true, &tr("docker.mask_open"), |p, r, c| {
            crate::ui::icons::draw_action(p, r, crate::ui::icons::Action::Open, c)
        })
        .clicked()
        {
            crate::files::Dialog::new()
                .add_filter(tr("docker.mask_file"), &["msk"])
                .pick_file(app, |app, path| match crate::files::read(&path) {
                    Ok(bytes) => {
                        if !app.mask.read_text(&String::from_utf8_lossy(&bytes)) {
                            app.status = tr("docker.mask_bad_file");
                        }
                    }
                    Err(e) => app.status = e.to_string(),
                });
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let i = app.mask.picked.min(MASK_SLOTS - 1);
            let used = app.mask.slots[i] != crate::bitmap_fx::MaskSlot::default();
            if icon_btn(ui, used, &tr("docker.mask_delete"), |p, r, c| {
                // A bin.
                let s = egui::Stroke::new(1.2, c);
                p.line_segment(
                    [
                        r.left_top() + egui::vec2(0.0, 3.0),
                        r.right_top() + egui::vec2(0.0, 3.0),
                    ],
                    s,
                );
                p.rect_stroke(
                    egui::Rect::from_min_max(
                        r.left_top() + egui::vec2(2.5, 4.0),
                        r.right_bottom() - egui::vec2(2.5, 0.0),
                    ),
                    1.0,
                    s,
                    egui::StrokeKind::Inside,
                );
            })
            .clicked()
            {
                app.mask.slots[i] = Default::default();
            }
        });
    });
    ui.add_space(4.0);
    ui.label(tr("docker.tolerance"));
    ui.horizontal(|ui| {
        let i = app.mask.picked.min(MASK_SLOTS - 1);
        let tol = &mut app.mask.slots[i].tolerance;
        let tip = crate::ui::tips::pair("docker.tolerance", "docker.mask_tolerance_desc");
        ui.add(
            crate::ui::Slider::new(tol, 0..=100)
                .show_value(false)
                .reserve(80.0),
        )
        .rich_tip(&tip);
        ui.add(
            crate::ui::field::NumField::new(tol)
                .range(0..=100)
                .suffix(" %"),
        )
        .rich_tip(&tip);
    });
    ui.horizontal(|ui| {
        ui.label(tr("docker.mask_colors"));
        ui.vertical(|ui| {
            ui.radio_value(&mut app.mask.hide, true, tr("docker.mask_hide"))
                .rich_tip(crate::ui::tips::keyed("docker.mask_hide"));
            ui.radio_value(&mut app.mask.hide, false, tr("docker.mask_show"))
                .rich_tip(crate::ui::tips::keyed("docker.mask_show"));
        });
    });
    ui.add_space(4.0);
    let can = has_bitmap && !app.mask.active().is_empty();
    if ui
        .add_enabled(
            can,
            egui::Button::new(tr("docker.apply")).min_size(egui::vec2(ui.available_width(), 26.0)),
        )
        .rich_tip(crate::ui::tips::pair(
            "docker.apply",
            "docker.mask_apply_desc",
        ))
        .rich_disabled_tip(crate::ui::tips::pair(
            "docker.apply",
            "docker.mask_apply_desc",
        ))
        .clicked()
    {
        app.apply_bitmap_mask();
    }
    if !has_bitmap {
        dim(ui, tr("docker.bitmap_mask_hint"));
    }
}

fn object_data(app: &mut App, ui: &mut Ui) {
    // The fields of many objects at once, as a table (#3084).
    let sheet_open = egui::Id::new("object_data_sheet_open");
    let mut open = ui.data(|d| d.get_temp::<bool>(sheet_open)).unwrap_or(false);
    if ui
        .button(tr("docker.data_sheet"))
        .rich_tip(crate::ui::tips::pair(
            "docker.data_sheet",
            "docker.data_sheet_tip",
        ))
        .clicked()
    {
        open = !open;
    }
    if open {
        data_sheet_window(app, ui.ctx(), &mut open);
    }
    ui.data_mut(|d| d.insert_temp(sheet_open, open));
    let Some(s) = app.selected_shapes().into_iter().next() else {
        dim(ui, tr("docker.select_one_object"));
        return;
    };
    let mut data = s.data.clone();
    let mut changed = false;
    let mut remove = None;
    egui::Grid::new("objdata").num_columns(3).show(ui, |ui| {
        ui.strong(tr("docker.field"));
        ui.strong(tr("docker.value"));
        ui.label("");
        ui.end_row();
        for (i, (k, v)) in data.iter_mut().enumerate() {
            let field = egui::Id::new(("object_data_field", i));
            let (_, typed) = kept_text(ui, field, s.id, &k.clone(), |ui, text| {
                ui.add(egui::TextEdit::singleline(text).desired_width(90.0))
                    .rich_tip(crate::ui::tips::pair(
                        "docker.field",
                        "docker.data_field_desc",
                    ))
            });
            if let Some(name) = typed {
                *k = name;
                changed = true;
            }
            let field = egui::Id::new(("object_data_value", i));
            let (_, typed) = kept_text(ui, field, s.id, &v.clone(), |ui, text| {
                ui.add(egui::TextEdit::singleline(text).desired_width(120.0))
                    .rich_tip(crate::ui::tips::pair(
                        "docker.value",
                        "docker.data_value_desc",
                    ))
            });
            if let Some(value) = typed {
                *v = value;
                changed = true;
            }
            if ui
                .small_button("x")
                .rich_tip(crate::ui::tips::keyed("docker.data_remove"))
                .clicked()
            {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        data.remove(i);
        changed = true;
    }
    ui.horizontal(|ui| {
        if ui
            .button(tr("docker.add_field"))
            .rich_tip(crate::ui::tips::keyed("docker.add_field"))
            .clicked()
        {
            data.push((format!("Field {}", data.len() + 1), String::new()));
            changed = true;
        }
        for k in ["Name", "Cost", "Comments", "CDRStaticID"] {
            if !data.iter().any(|(f, _)| f == k)
                && ui
                    .small_button(k)
                    .rich_tip_with(|| format!("{k}\n{}", tr("docker.data_add_named_desc")))
                    .clicked()
            {
                data.push((k.into(), String::new()));
                changed = true;
            }
        }
    });
    if changed {
        app.run(Command::SetObjectData { shape: s.id, data });
    }
    // Copy the fields to the other selected objects (#3262).
    if app.selected_shapes().len() > 1
        && ui
            .button(tr("docker.data_copy_to_selection"))
            .rich_tip(crate::ui::tips::pair(
                "docker.data_copy_to_selection",
                "docker.data_copy_to_selection_tip",
            ))
            .clicked()
    {
        app.copy_object_data_to_selection();
    }
    // Summary of a cost field across the selection.
    let total: f64 = app
        .selected_shapes()
        .iter()
        .filter_map(|s| {
            s.data
                .iter()
                .find(|(k, _)| k == "Cost")
                .and_then(|(_, v)| v.parse::<f64>().ok())
        })
        .sum();
    if total > 0.0 {
        ui.label(trf("docker.total_cost", &[("v", &format!("{total:.2}"))]));
    }
}

/// One row of the object data sheet: an object, how deep in groups it
/// lies under the objects listed, and its fields.
#[derive(Debug, Clone, PartialEq)]
struct SheetRow {
    id: ShapeId,
    depth: usize,
    data: Vec<(String, String)>,
}

/// The rows of the data sheet for `roots` (the objects selected, else
/// those of the page): each object, then the objects of its groups down
/// to `levels` levels deeper.
fn sheet_rows(roots: &[tracedraw_core::Shape], levels: usize) -> Vec<SheetRow> {
    fn walk(s: &tracedraw_core::Shape, depth: usize, levels: usize, out: &mut Vec<SheetRow>) {
        out.push(SheetRow {
            id: s.id,
            depth,
            data: s.data.clone(),
        });
        if depth < levels {
            if let ShapeKind::Group { children } = &s.kind {
                for c in children {
                    walk(c, depth + 1, levels, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    for s in roots {
        walk(s, 0, levels, &mut out);
    }
    out
}

/// A field a person keeps on an object (not one TraceDraw keeps for
/// itself, named with a dot, such as `fill.rule`).
fn user_field(name: &str) -> bool {
    !name.contains('.')
}

/// The columns of the sheet: every field the rows hold, the usual ones
/// first, then the others in the order they first come.
fn sheet_columns(rows: &[SheetRow]) -> Vec<String> {
    let mut cols: Vec<String> = Vec::new();
    for k in ["Name", "Cost", "Comments"] {
        if rows.iter().any(|r| r.data.iter().any(|(f, _)| f == k)) {
            cols.push(k.to_string());
        }
    }
    for r in rows {
        for (k, _) in &r.data {
            if user_field(k) && !cols.contains(k) {
                cols.push(k.clone());
            }
        }
    }
    cols
}

/// A value as a number, with a currency sign, spaces or thousands
/// commas around it allowed ("$1,250.50").
fn sheet_number(v: &str) -> Option<f64> {
    let t: String = v
        .trim()
        .chars()
        .filter(|c| !matches!(c, '$' | '€' | '£' | '¥' | ',' | ' '))
        .collect();
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// The total of column `col` over the top rows (depth 0, so a group and
/// its objects are not counted twice): `None` unless some value is a
/// number and every filled one is.
fn sheet_total(rows: &[SheetRow], col: &str) -> Option<f64> {
    let mut sum = 0.0;
    let mut any = false;
    for r in rows.iter().filter(|r| r.depth == 0) {
        let Some((_, v)) = r.data.iter().find(|(k, _)| k == col) else {
            continue;
        };
        if v.trim().is_empty() {
            continue;
        }
        sum += sheet_number(v)?;
        any = true;
    }
    any.then_some(sum)
}

/// The object data sheet: the fields of the selected objects (or of the
/// page's objects) as a table to read and edit, with the objects of
/// their groups to the levels asked and a total under the number columns.
fn data_sheet_window(app: &mut App, ctx: &egui::Context, open: &mut bool) {
    let levels_id = egui::Id::new("object_data_sheet_levels");
    let mut levels = ctx.data(|d| d.get_temp::<usize>(levels_id)).unwrap_or(0);
    let roots: Vec<tracedraw_core::Shape> = {
        let sel = app.selected_shapes();
        if sel.is_empty() {
            app.page_layers()
                .iter()
                .flat_map(|l| l.shapes.iter().cloned())
                .collect()
        } else {
            sel
        }
    };
    let rows = sheet_rows(&roots, levels);
    let cols = sheet_columns(&rows);
    let mut edit: Option<(ShapeId, String, String)> = None;
    egui::Window::new(tr("docker.data_sheet"))
        .id(egui::Id::new("object_data_sheet"))
        .open(open)
        .default_size([640.0, 280.0])
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("docker.data_sheet_levels"));
                ui.add(egui::DragValue::new(&mut levels).range(0..=9));
            });
            ui.separator();
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("object_data_sheet_grid")
                    .striped(true)
                    .num_columns(cols.len() + 1)
                    .show(ui, |ui| {
                        ui.label("");
                        for c in &cols {
                            ui.strong(c);
                        }
                        ui.end_row();
                        let mut n = 0;
                        for r in &rows {
                            if r.depth == 0 {
                                n += 1;
                            }
                            let num = if r.depth == 0 {
                                n.to_string()
                            } else {
                                format!("{}-", "  ".repeat(r.depth))
                            };
                            ui.label(num);
                            for (ci, c) in cols.iter().enumerate() {
                                let stored = r
                                    .data
                                    .iter()
                                    .find(|(k, _)| k == c)
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or_default();
                                let field = egui::Id::new(("object_data_sheet_cell", ci));
                                let (_, typed) =
                                    kept_text(ui, field, (r.id, ci), &stored, |ui, text| {
                                        ui.add(egui::TextEdit::singleline(text).desired_width(80.0))
                                    });
                                if let Some(v) = typed {
                                    edit = Some((r.id, c.clone(), v));
                                }
                            }
                            ui.end_row();
                        }
                        ui.strong(tr("docker.data_sheet_total"));
                        for c in &cols {
                            match sheet_total(&rows, c) {
                                Some(t) => ui.strong(format!("{t:.2}")),
                                None => ui.label(""),
                            };
                        }
                        ui.end_row();
                    });
            });
        });
    ctx.data_mut(|d| d.insert_temp(levels_id, levels));
    if let Some((id, field, value)) = edit {
        if let Some(s) = app.doc().find_shape(id) {
            let mut data = s.data.clone();
            match data.iter_mut().find(|(k, _)| *k == field) {
                Some((_, v)) => *v = value,
                None => data.push((field, value)),
            }
            app.run(Command::SetObjectData { shape: id, data });
        }
    }
}

/// The Hyperlink field of the Links and Bookmarks docker.
pub fn hyperlink_field_id() -> egui::Id {
    egui::Id::new("links.hyperlink")
}

/// A text field showing `stored` that keeps what is typed into it while
/// it has the focus: `add` draws it on the text to show. Gives the text
/// shown and, on the frame the field is left (Enter, Tab, a click
/// elsewhere), the text typed when it differs from `stored`. `of` names
/// what the field shows the text of (the object selected), so text typed
/// for one object is never shown or stored for another.
pub(crate) fn kept_text(
    ui: &mut Ui,
    field: egui::Id,
    of: impl std::hash::Hash + std::fmt::Debug,
    stored: &str,
    add: impl FnOnce(&mut Ui, &mut String) -> egui::Response,
) -> (String, Option<String>) {
    // One text kept a field, with what it was typed for: text kept for
    // another object is dropped, never shown or stored later.
    let key = field.with("typed");
    let of = egui::Id::new(of);
    let kept = ui
        .data(|d| d.get_temp::<(egui::Id, String)>(key))
        .filter(|(typed_for, _)| *typed_for == of)
        .map(|(_, text)| text);
    let was_typed = kept.is_some();
    let mut text = kept.unwrap_or_else(|| stored.to_string());
    let resp = add(ui, &mut text);
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(key, (of, text.clone())));
        return (text, None);
    }
    ui.data_mut(|d| d.remove::<(egui::Id, String)>(key));
    let typed = ((was_typed || resp.lost_focus() || resp.changed()) && text != stored)
        .then(|| text.clone());
    (text, typed)
}

fn links(app: &mut App, ui: &mut Ui) {
    let shapes = app.selected_shapes();
    if shapes.is_empty() {
        dim(ui, tr("docker.select_one_object"));
        return;
    }
    let stored = shapes[0].link.clone().unwrap_or_default();
    ui.label(tr("docker.hyperlink"));
    // What is typed stays in the field until it is left (#2191): the
    // field showed the stored link again on every frame, so no key
    // typed into it ever stayed.
    let (link, typed) = kept_text(
        ui,
        hyperlink_field_id(),
        shapes[0].id,
        &stored,
        |ui, text| {
            ui.add(egui::TextEdit::singleline(text).id(hyperlink_field_id()))
                .rich_tip(crate::ui::tips::keyed("docker.hyperlink_field"))
        },
    );
    if let Some(link) = typed {
        let ids = app.selection.clone();
        app.run(Command::SetLink {
            shapes: ids,
            link: (!link.is_empty()).then_some(link),
        });
    }
    ui.horizontal(|ui| {
        for (k, prefix) in [
            ("docker.link_web", "https://"),
            ("docker.link_mail", "mailto:"),
            ("docker.link_page", "#page="),
        ] {
            if ui
                .small_button(tr(k))
                .rich_tip_with(|| format!("{}\n{}", tr(k), tr(&format!("{k}_desc"))))
                .clicked()
                && !link.starts_with(prefix)
            {
                let ids = app.selection.clone();
                app.run(Command::SetLink {
                    shapes: ids,
                    link: Some(format!("{prefix}{link}")),
                });
            }
        }
        if ui
            .small_button(tr("docker.remove"))
            .rich_tip(crate::ui::tips::pair(
                "docker.remove",
                "docker.link_remove_desc",
            ))
            .clicked()
        {
            let ids = app.selection.clone();
            app.run(Command::SetLink {
                shapes: ids,
                link: None,
            });
        }
    });
    ui.separator();
    ui.strong(tr("docker.bookmarks"));
    let named: Vec<(ShapeId, String)> = app
        .doc()
        .all_layers()
        .flat_map(|l| &l.shapes)
        .filter_map(|s| s.name.clone().map(|n| (s.id, n)))
        .collect();
    for (id, n) in named {
        if ui
            .small_button(&n)
            .rich_tip_with(|| format!("{n}\n{}", tr("docker.bookmark_desc")))
            .clicked()
        {
            app.select(vec![id]);
            app.zoom_to_selection();
        }
    }
}

/// How many instances of symbol `index` the drawing holds, on every
/// page and master layer, inside groups and clip frames too (#3248): an
/// instance in a group was not counted, so a symbol whose copies were all
/// grouped said 0.
pub(crate) fn symbol_uses(doc: &tracedraw_core::Document, index: usize) -> usize {
    fn count(s: &tracedraw_core::document::Shape, index: usize) -> usize {
        match &s.kind {
            ShapeKind::SymbolInstance { index: i } => usize::from(*i == index),
            ShapeKind::Group { children } => children.iter().map(|c| count(c, index)).sum(),
            ShapeKind::ClipFrame { frame, contents } => {
                count(frame, index) + contents.iter().map(|c| count(c, index)).sum::<usize>()
            }
            _ => 0,
        }
    }
    doc.all_layers()
        .flat_map(|l| &l.shapes)
        .map(|s| count(s, index))
        .sum()
}

fn symbols(app: &mut App, ui: &mut Ui) {
    let has = !app.selection.is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(has, egui::Button::new(tr("docker.new_symbol")))
            .rich_tip(crate::ui::tips::keyed("docker.new_symbol"))
            .rich_disabled_tip(crate::ui::tips::keyed("docker.new_symbol"))
            .clicked()
        {
            app.create_symbol_from_selection();
        }
        // Every group copying the selected one becomes an instance where
        // it stood, labels staying text (#2343).
        if ui
            .add_enabled(has, egui::Button::new(tr("docker.symbolize_similar")))
            .rich_tip(tr("docker.symbolize_similar_tip"))
            .rich_disabled_tip(tr("docker.symbolize_similar_tip"))
            .clicked()
        {
            app.symbolize_similar();
        }
        if ui
            .button(tr("docker.revert_to_objects"))
            .rich_tip(crate::ui::tips::keyed("docker.revert_to_objects"))
            .clicked()
        {
            app.revert_symbol_instances();
        }
    });
    let syms: Vec<(usize, String)> = app
        .doc()
        .symbols
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.name.clone()))
        .collect();
    let editing = app.symbol_edit_index();
    let count = syms.len();
    let mut thumbs = std::mem::take(&mut app.symbol_thumbs);
    thumbs.begin_frame();
    let drawing = (app.engine.instance(), app.engine.revision());
    for (i, name) in syms {
        ui.horizontal(|ui| {
            // What the symbol draws, beside its name (#3249).
            symbol_picture(app, ui, &mut thumbs, drawing, i);
            // A double click on the name renames the symbol in place.
            let renaming = app
                .symbol_rename
                .as_ref()
                .filter(|(idx, _)| *idx == i)
                .map(|(_, s)| s.clone());
            if let Some(mut text) = renaming {
                let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0));
                if !resp.has_focus() && !resp.lost_focus() {
                    resp.request_focus();
                }
                if ui.input(|inp| inp.key_pressed(egui::Key::Escape)) {
                    app.symbol_rename = None;
                } else if resp.lost_focus() || ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
                    app.rename_symbol(i, text.clone());
                    app.symbol_rename = None;
                } else {
                    app.symbol_rename = Some((i, text));
                }
            } else {
                let resp = ui.label(&name).rich_tip(tr("docker.rename_symbol_tip"));
                if resp.double_clicked() {
                    app.symbol_rename = Some((i, name.clone()));
                }
            }
            if ui
                .small_button(tr("docker.insert"))
                .rich_tip(crate::ui::tips::pair(
                    "docker.insert",
                    "docker.insert_symbol_desc",
                ))
                .clicked()
            {
                app.insert_symbol_instance(i);
            }
            // Edit: the symbol's objects edited on the page; Finish while
            // they are (#1980).
            if editing == Some(i) {
                if ui
                    .small_button(tr("clip.finish"))
                    .rich_tip(tr("menu.object.symbol_finish"))
                    .clicked()
                {
                    app.finish_clip_frame_edit();
                }
                // Cancel: the edit ends and the symbol stays as it was
                // (#3437).
                if ui
                    .small_button(tr("docker.cancel_symbol_edit"))
                    .rich_tip(tr("docker.cancel_symbol_edit_tip"))
                    .clicked()
                {
                    app.cancel_symbol_edit();
                }
            } else if ui
                .add_enabled(
                    app.clip_frame_edit.is_none(),
                    egui::Button::new(tr("docker.edit_symbol")).small(),
                )
                .rich_tip(crate::ui::tips::pair(
                    "docker.edit_symbol",
                    "docker.edit_symbol_desc",
                ))
                .clicked()
            {
                app.edit_symbol(i);
            }
            // A copy of the symbol, placed nowhere (#3437).
            if ui
                .add_enabled(
                    app.clip_frame_edit.is_none(),
                    egui::Button::new(tr("docker.duplicate_symbol")).small(),
                )
                .rich_tip(tr("docker.duplicate_symbol_tip"))
                .clicked()
            {
                app.duplicate_symbol(i);
            }
            // Replace: merge this symbol into another. The first press
            // picks the source, a press on another symbol's button merges
            // the source into it.
            let picking = app.symbol_replace_source;
            let label = if picking == Some(i) {
                tr("dialog.cancel")
            } else if picking.is_some() {
                tr("docker.replace_target")
            } else {
                tr("docker.replace_symbol")
            };
            if ui
                .add_enabled(
                    count > 1 && app.clip_frame_edit.is_none(),
                    egui::Button::new(label).small(),
                )
                .rich_tip(tr("docker.replace_symbol_tip"))
                .clicked()
            {
                match picking {
                    Some(src) if src == i => app.symbol_replace_source = None,
                    Some(src) => {
                        app.merge_symbols(src, i);
                        app.symbol_replace_source = None;
                    }
                    None => app.symbol_replace_source = Some(i),
                }
            }
            let uses = symbol_uses(app.doc(), i);
            dim(ui, trf("docker.instances_n", &[("n", &uses.to_string())]));
        });
    }
    thumbs.trim(crate::object_thumbs::MAX_BYTES / 4);
    if thumbs.waiting {
        ui.ctx().request_repaint();
    }
    app.symbol_thumbs = thumbs;
    if app.doc().symbols.is_empty() {
        dim(ui, tr("docker.symbols_hint"));
    }
    if app.symbol_replace_source.is_some() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.symbol_replace_source = None;
        }
        dim(ui, tr("docker.replace_pick"));
    }
    if let Some(r) = app.symbolize_report.clone() {
        dim(ui, r);
    }
}

/// The side of a symbol's picture in the Symbols docker, points.
const SYMBOL_PICTURE: f32 = 28.0;

/// Symbol `index`'s definition drawn in a white tile (#3249): drawn once
/// per change of the symbol, whatever its instances.
fn symbol_picture(
    app: &App,
    ui: &mut Ui,
    thumbs: &mut crate::object_thumbs::ObjectThumbs,
    drawing: (u64, u64),
    index: usize,
) {
    let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(SYMBOL_PICTURE), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 2.0, egui::Color32::WHITE);
    p.rect_stroke(
        r,
        2.0,
        egui::Stroke::new(1.0, Tokens::border()),
        egui::StrokeKind::Inside,
    );
    // An instance of it at the origin stands for the symbol; its own key,
    // apart from any object's.
    let shape = tracedraw_core::Shape::new(
        ShapeId(u64::MAX - index as u64),
        ShapeKind::SymbolInstance { index },
    );
    // Centre the definition in the tile: definitions saved by older files
    // may sit away from the origin, which left their preview blank.
    let mut shape = shape;
    if let Some(sym) = app.doc().symbols.get(index) {
        let c = sym.shape.bounds().center();
        shape.transform = Affine::translate((-c.x, -c.y));
    }
    let ppp = ui.ctx().pixels_per_point().max(0.5);
    let inner = r.shrink(2.0);
    let px = (inner.width() * ppp).round().max(4.0) as u32;
    if let crate::object_thumbs::Thumb::Picture(id, [w, h]) =
        thumbs.get(ui.ctx(), app.doc(), drawing, &shape, px)
    {
        let size = egui::Vec2::new(w as f32, h as f32) / ppp;
        let fit = (inner.width() / size.x.max(size.y).max(1.0)).min(1.0);
        let at = egui::Rect::from_center_size(inner.center(), size * fit);
        ui.painter().image(
            id,
            at,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

/// The side of a page's miniature in the Pages docker, points (#2932).
const PAGE_THUMB: f32 = 44.0;

fn pages(app: &mut App, ui: &mut Ui) {
    let pages: Vec<(tracedraw_core::PageId, String, tracedraw_core::Size)> = app
        .doc()
        .pages
        .iter()
        .map(|p| (p.id, p.name.clone(), p.size))
        .collect();
    let n = pages.len();
    ui.horizontal(|ui| {
        if ui
            .button(tr("menu.layout.insert_page"))
            .rich_tip(crate::ui::tips::keyed("menu.layout.insert_page"))
            .clicked()
        {
            app.add_page();
        }
        if ui
            .add_enabled(n > 1, egui::Button::new(tr("menu.layout.delete_page")))
            .rich_tip(crate::ui::tips::keyed("menu.layout.delete_page"))
            .rich_disabled_tip(crate::ui::tips::keyed("menu.layout.delete_page"))
            .clicked()
        {
            app.ask_delete_page();
        }
        if ui
            .button(tr("menu.layout.duplicate_page"))
            .rich_tip(crate::ui::tips::keyed("menu.layout.duplicate_page"))
            .clicked()
        {
            app.dialog = crate::ui::dialogs::Dialog::duplicate_page();
        }
        if ui
            .selectable_label(app.page_sorter, tr("menu.view.page_sorter"))
            .rich_tip(crate::ui::tips::keyed("menu.view.page_sorter"))
            .clicked()
        {
            app.page_sorter = !app.page_sorter;
        }
    });
    let revision = app.engine.revision();
    for (i, (id, name, size)) in pages.iter().enumerate() {
        ui.horizontal(|ui| {
            let active = *id == app.page;
            // The page's miniature, so a page is found by what it shows,
            // not by its number alone (#2932).
            let (r, resp) =
                ui.allocate_exact_size(egui::vec2(PAGE_THUMB, PAGE_THUMB), egui::Sense::click());
            if active {
                ui.painter()
                    .rect_filled(r.expand(2.0), 2.0, crate::theme::Tokens::tool_active());
            }
            let thumb = crate::ui::preview::page_thumb(ui, app.doc(), revision, *id, PAGE_THUMB);
            crate::ui::preview::paint_page_thumb(ui, r.shrink(2.0), thumb, *size);
            if resp
                .rich_tip_with(|| format!("{name}\n{}", tr("docker.page_list_desc")))
                .clicked()
            {
                app.goto_page(i);
            }
            if ui
                .selectable_label(active, format!("{}. {name}", i + 1))
                .rich_tip_with(|| format!("{name}\n{}", tr("docker.page_list_desc")))
                .clicked()
            {
                app.goto_page(i);
            }
            dim(
                ui,
                format!(
                    "{} x {}",
                    app.units.from_mm(size.width).round(),
                    app.units.from_mm(size.height).round()
                ),
            );
            if i > 0
                && ui
                    .small_button("\u{23F6}")
                    .rich_tip(crate::ui::tips::keyed("docker.page_move_up"))
                    .clicked()
            {
                app.run(Command::MovePage {
                    page: *id,
                    to: i - 1,
                });
            }
            if i + 1 < n
                && ui
                    .small_button("\u{23F7}")
                    .rich_tip(crate::ui::tips::keyed("docker.page_move_down"))
                    .clicked()
            {
                app.run(Command::MovePage {
                    page: *id,
                    to: i + 1,
                });
            }
        });
    }
}

fn guidelines(app: &mut App, ui: &mut Ui) {
    crate::ui::layout_options::guidelines_docker(app, ui);
}

fn fonts(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(tr("dialog.filter"));
        ui.text_edit_singleline(&mut app.font_filter)
            .rich_tip(crate::ui::tips::keyed("docker.font_filter"));
    });
    let filter = app.font_filter.to_lowercase();
    let used = app.doc().font_families_used();
    if !used.is_empty() {
        ui.strong(tr("docker.fonts_in_document"));
        for f in &used {
            let missing = !app.font_installed(f);
            let carried = !missing && tracedraw_text::fonts().is_document_family(f);
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(app.text_font == *f, f)
                    .rich_tip(tr("docker.font_apply_hint"))
                    .clicked()
                {
                    app.apply_text_font(f);
                }
                if missing {
                    let label = ui
                        .colored_label(egui::Color32::from_rgb(200, 100, 0), tr("docker.missing"));
                    if let Some(s) = tracedraw_text::fonts().substitute(f) {
                        label.rich_tip(trf("docker.missing_hint", &[("f", &s)]));
                    }
                } else if carried {
                    ui.weak(tr("docker.embedded"))
                        .rich_tip(tr("docker.embedded_hint"));
                }
            });
        }
        ui.separator();
    }
    let families: Vec<String> = app
        .font_families
        .iter()
        .filter(|f| filter.is_empty() || f.to_lowercase().contains(&filter))
        .cloned()
        .collect();
    ui.label(trf("dialog.fonts_n", &[("n", &families.len().to_string())]));
    egui::ScrollArea::vertical().show(ui, |ui| {
        for f in families {
            if ui
                .selectable_label(app.text_font == f, &f)
                .rich_tip(tr("docker.font_apply_hint"))
                .clicked()
            {
                app.apply_text_font(&f);
            }
        }
    });
}

/// Corners: fillet, scallop or chamfer the corners of the selected
/// objects (or of the nodes chosen with the Shape tool), with a preview of
/// the result on the drawing.
fn corners(app: &mut App, ui: &mut Ui) {
    use crate::corners::CornerOp;
    let u = app.units;
    let field = |ui: &mut Ui, label: &str, tip: String, mm: &mut f64| {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut v = u.world_from_mm(*mm);
            if ui
                .add_sized(
                    [90.0, 22.0],
                    crate::ui::field::NumField::new(&mut v)
                        .range(0.0..=100_000.0)
                        .max_decimals(3)
                        .suffix(format!(" {}", u.short()))
                        .unit_mm(u.mm()),
                )
                .rich_tip(tip)
                .changed()
            {
                *mm = u.world_to_mm(v);
            }
        });
    };
    ui.strong(tr("docker.corners_operation"));
    for (op, key) in [
        (CornerOp::Fillet, "docker.corners_fillet"),
        (CornerOp::Scallop, "docker.corners_scallop"),
        (CornerOp::Chamfer, "docker.corners_chamfer"),
    ] {
        ui.radio_value(&mut app.corners.op, op, tr(key))
            .rich_tip(crate::ui::tips::keyed(key));
    }
    ui.add_space(6.0);
    let mut c = app.corners;
    if c.op == CornerOp::Chamfer {
        ui.strong(tr("docker.corners_chamfer_distance"));
        field(
            ui,
            "A:",
            crate::ui::tips::keyed("docker.corners_a"),
            &mut c.a,
        );
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!c.lock, |ui| {
                ui.label("B:");
                let mut v = u.world_from_mm(if c.lock { c.a } else { c.b });
                if ui
                    .add_sized(
                        [90.0, 22.0],
                        crate::ui::field::NumField::new(&mut v)
                            .range(0.0..=100_000.0)
                            .max_decimals(3)
                            .suffix(format!(" {}", u.short()))
                            .unit_mm(u.mm()),
                    )
                    .rich_tip(crate::ui::tips::keyed("docker.corners_b"))
                    .rich_disabled_tip(crate::ui::tips::keyed("docker.corners_b"))
                    .changed()
                {
                    c.b = u.world_to_mm(v);
                }
            });
            if crate::ui::propbar::pic_button(
                ui,
                crate::ui::propbar::Pic::Lock(c.lock),
                &tr("docker.corners_lock"),
                true,
                c.lock,
            )
            .clicked()
            {
                c.lock = !c.lock;
            }
        });
    } else {
        field(
            ui,
            &tr("docker.corners_radius"),
            crate::ui::tips::pair("docker.corners_radius", "docker.corners_radius_desc"),
            &mut c.radius,
        );
    }
    app.corners = c;
    ui.add_space(6.0);
    ui.checkbox(&mut app.settings.corners_preview, tr("dialog.preview"))
        .rich_tip(crate::ui::tips::pair(
            "dialog.preview",
            "docker.corners_preview_desc",
        ));
    // Preview what Apply would do, unless Preview is off (#1759).
    let preview = app.corner_preview();
    let valid = !preview.is_empty();
    app.docker_preview = if app.settings.corners_preview {
        preview
    } else {
        Vec::new()
    };
    ui.add_space(8.0);
    if ui
        .add_enabled(valid, egui::Button::new(tr("docker.apply")))
        .rich_tip(crate::ui::tips::pair(
            "docker.apply",
            "docker.corners_apply_desc",
        ))
        .rich_disabled_tip(crate::ui::tips::pair(
            "docker.apply",
            "docker.corners_apply_desc",
        ))
        .clicked()
    {
        app.apply_corners();
        app.docker_preview.clear();
    }
}

/// Join Curves: the joint (Extend, Chamfer, Fillet, Bezier Curve), the
/// gap tolerance and the fillet radius.
fn join_curves(app: &mut App, ui: &mut Ui) {
    use crate::corners::JoinKind;
    let u = app.units;
    let label = |k: JoinKind| {
        tr(match k {
            JoinKind::Extend => "docker.join_extend",
            JoinKind::Chamfer => "docker.join_chamfer",
            JoinKind::Fillet => "docker.join_fillet",
            JoinKind::Bezier => "docker.join_bezier",
        })
    };
    let mut j = app.join_settings;
    ui.horizontal(|ui| {
        ui.label(tr("docker.join_mode"));
        crate::ui::keynav::ComboBox::from_id_salt("join_mode")
            .selected_text(label(j.mode))
            .width(140.0)
            .show_ui(ui, |ui| {
                for k in [
                    JoinKind::Extend,
                    JoinKind::Chamfer,
                    JoinKind::Fillet,
                    JoinKind::Bezier,
                ] {
                    ui.selectable_value(&mut j.mode, k, label(k));
                }
            })
            .response
            .rich_tip(crate::ui::tips::pair(
                "docker.join_mode_tip",
                "docker.join_mode_desc",
            ));
    });
    let field = |ui: &mut Ui, key: &str, mm: &mut f64| {
        ui.horizontal(|ui| {
            ui.label(tr(key));
            let mut v = u.world_from_mm(*mm);
            if ui
                .add_sized(
                    [90.0, 22.0],
                    crate::ui::field::NumField::new(&mut v)
                        .range(0.0..=100_000.0)
                        .max_decimals(3)
                        .suffix(format!(" {}", u.short()))
                        .unit_mm(u.mm()),
                )
                .rich_tip(crate::ui::tips::pair(key, &format!("{key}_desc")))
                .changed()
            {
                *mm = u.world_to_mm(v);
            }
        });
    };
    field(ui, "docker.join_gap", &mut j.gap);
    if j.mode == JoinKind::Fillet {
        field(ui, "docker.corners_radius", &mut j.radius);
    }
    app.join_settings = j;
    let curves = app
        .selected_shapes()
        .iter()
        .any(|s| matches!(s.kind, ShapeKind::Path { .. }));
    ui.add_space(8.0);
    if ui
        .add_enabled(curves, egui::Button::new(tr("docker.apply")))
        .rich_tip(crate::ui::tips::pair(
            "docker.apply",
            "docker.join_apply_desc",
        ))
        .rich_disabled_tip(crate::ui::tips::pair(
            "docker.apply",
            "docker.join_apply_desc",
        ))
        .clicked()
    {
        app.join_curves();
    }
}

/// Fit Objects to Path: spacing (even or a fixed distance, between
/// centres or edges), the start and end offsets, copies (a number, or as
/// many as fit), the placement on, above or below the path, the rotation
/// (with the path or fixed), the order and the direction, with a preview
/// of where the objects go; the last selected object is the path (#1027,
/// #1091, #1196).
fn fit_to_path(app: &mut App, ui: &mut Ui) {
    use crate::ops2::{
        FitAlign, FitCopies, FitMeasure, FitOrder, FitPlace, FitReference, FitRotation, FitSpacing,
        FitSpin,
    };
    let u = app.units;
    app.fit_path_follow_selection();
    // A live fit selected (its path, an object or a copy): the panel
    // shows its settings and a change refits at once.
    let live = app.live_fit_for_selection();
    let mut f = live.as_ref().map_or(app.fit_path, |(_, l)| l.settings);
    let field = |ui: &mut Ui, key: &str, mm: &mut f64, min: f64| {
        ui.horizontal(|ui| {
            ui.label(tr(key));
            let mut v = u.world_from_mm(*mm);
            if ui
                .add_sized(
                    [90.0, 22.0],
                    crate::ui::field::NumField::new(&mut v)
                        .range(min..=100_000.0)
                        .max_decimals(3)
                        .suffix(format!(" {}", u.short()))
                        .unit_mm(u.mm()),
                )
                .rich_tip(crate::ui::tips::pair(key, &format!("{key}_desc")))
                .changed()
            {
                *mm = u.world_to_mm(v);
            }
        });
    };
    let combo = |ui: &mut Ui, key: &str, id: &str, current: &str, add: &mut dyn FnMut(&mut Ui)| {
        ui.horizontal(|ui| {
            ui.label(tr(key));
            let r = crate::ui::keynav::ComboBox::from_id_salt(id)
                .selected_text(current)
                .show_ui(ui, |ui| add(ui))
                .response;
            let rect = r.rect;
            r.rich_tip(crate::ui::tips::pair(key, &format!("{key}_desc")));
            ui.ctx().data_mut(|d| d.insert_temp(fit_combo_id(id), rect));
        });
    };
    let radio = |ui: &mut Ui, on: bool, key: &str| -> bool {
        ui.radio(on, tr(key))
            .rich_tip(crate::ui::tips::keyed(key))
            .clicked()
    };
    dim(ui, tr("docker.fit_to_path_hint"));
    let check = |ui: &mut Ui, on: &mut bool, key: &str| {
        ui.checkbox(on, tr(key))
            .rich_tip(crate::ui::tips::keyed(key));
    };
    let deg =
        |ui: &mut Ui, key: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, on: bool| {
            ui.horizontal(|ui| {
                ui.add_enabled(on, egui::Label::new(tr(key)));
                ui.add_enabled(
                    on,
                    crate::ui::field::NumField::new(v)
                        .range(range)
                        .max_decimals(2)
                        .suffix("°"),
                )
                .rich_tip(crate::ui::tips::keyed(key))
                .rich_disabled_tip(crate::ui::tips::keyed(key));
            });
        };

    // What is fitted: the objects (all but the last selected) and the path.
    ui.strong(tr("docker.fit_to_path_objects"));
    let objects = live
        .as_ref()
        .map_or(app.selection.len().saturating_sub(1), |(_, l)| {
            l.members.len()
        });
    dim(
        ui,
        trf(
            "docker.fit_to_path_selected",
            &[("n", &objects.to_string())],
        ),
    );
    check(
        ui,
        &mut f.keep_originals,
        "docker.fit_to_path_keep_originals",
    );
    // Live sits beside Group all objects, so the panel keeps its height.
    ui.horizontal(|ui| {
        if live.is_none() {
            check(ui, &mut f.group, "docker.fit_to_path_group");
        }
        check(ui, &mut f.live, "docker.fit_to_path_live");
    });
    ui.add_space(4.0);
    ui.strong(tr("docker.fit_to_path_path"));
    if app.selection.len() >= 2 {
        let used = app
            .selection
            .last()
            .and_then(|id| app.doc().find_shape(*id))
            .map(|s| crate::ui::dockers::kind_name(&s.kind))
            .unwrap_or_default();
        dim(
            ui,
            trf("docker.fit_to_path_object_used", &[("kind", &used)]),
        );
    }
    check(ui, &mut f.contiguous, "docker.fit_to_path_contiguous");
    ui.add_space(4.0);

    ui.strong(tr("docker.fit_to_path_spacing"));
    if radio(ui, f.spacing == FitSpacing::Even, "docker.fit_to_path_even") {
        f.spacing = FitSpacing::Even;
        if f.copies == FitCopies::Fill {
            f.copies = FitCopies::None;
        }
    }
    if radio(
        ui,
        f.spacing == FitSpacing::Distance,
        "docker.fit_to_path_distance",
    ) {
        f.spacing = FitSpacing::Distance;
    }
    if f.spacing == FitSpacing::Distance {
        field(
            ui,
            "docker.fit_to_path_distance_value",
            &mut f.distance,
            0.0,
        );
    }
    let measures = [
        (FitMeasure::Centres, "docker.fit_to_path_centres"),
        (FitMeasure::Edges, "docker.fit_to_path_edges"),
    ];
    let cur = measures
        .iter()
        .find(|(m, _)| *m == f.measure)
        .map_or(String::new(), |(_, k)| tr(k));
    combo(
        ui,
        "docker.fit_to_path_measure",
        "fit_path_measure",
        &cur,
        &mut |ui| {
            for (m, k) in measures {
                ui.selectable_value(&mut f.measure, m, tr(k));
            }
        },
    );
    field(ui, "docker.fit_to_path_offset", &mut f.offset, -100_000.0);
    field(
        ui,
        "docker.fit_to_path_end_offset",
        &mut f.end_offset,
        -100_000.0,
    );
    if f.spacing == FitSpacing::Distance {
        let aligns = [
            (FitAlign::Start, "docker.fit_to_path_align_start"),
            (FitAlign::Middle, "docker.fit_to_path_align_middle"),
            (FitAlign::End, "docker.fit_to_path_align_end"),
        ];
        let cur = aligns
            .iter()
            .find(|(a, _)| *a == f.align)
            .map_or(String::new(), |(_, k)| tr(k));
        combo(
            ui,
            "docker.fit_to_path_align",
            "fit_path_align",
            &cur,
            &mut |ui| {
                for (a, k) in aligns {
                    ui.selectable_value(&mut f.align, a, tr(k));
                }
            },
        );
    }

    ui.add_space(4.0);
    ui.strong(tr("docker.fit_to_path_copies"));
    if radio(
        ui,
        f.copies == FitCopies::None,
        "docker.fit_to_path_copies_none",
    ) {
        f.copies = FitCopies::None;
    }
    ui.horizontal(|ui| {
        if radio(
            ui,
            f.copies == FitCopies::Count,
            "docker.fit_to_path_copies_count",
        ) {
            f.copies = FitCopies::Count;
        }
        let mut c = f.count as f64;
        if ui
            .add_enabled(
                f.copies == FitCopies::Count,
                crate::ui::field::NumField::new(&mut c)
                    .range(1.0..=crate::fit_path::MAX_PLACED as f64)
                    .max_decimals(0),
            )
            .rich_tip(crate::ui::tips::keyed("docker.fit_to_path_copies_count"))
            .changed()
        {
            f.count = c.round().clamp(1.0, crate::fit_path::MAX_PLACED as f64) as u32;
        }
    });
    let fill_ok = f.spacing == FitSpacing::Distance;
    if ui
        .add_enabled(
            fill_ok,
            egui::RadioButton::new(
                f.copies == FitCopies::Fill,
                tr("docker.fit_to_path_copies_fill"),
            ),
        )
        .rich_tip(crate::ui::tips::keyed("docker.fit_to_path_copies_fill"))
        .rich_disabled_tip(crate::ui::tips::keyed("docker.fit_to_path_copies_fill"))
        .clicked()
    {
        f.copies = FitCopies::Fill;
    }

    ui.add_space(4.0);
    ui.strong(tr("docker.fit_to_path_place"));
    let places = [
        (FitPlace::On, "docker.fit_to_path_on"),
        (FitPlace::Above, "docker.fit_to_path_above"),
        (FitPlace::Below, "docker.fit_to_path_below"),
    ];
    for (p, k) in places {
        if radio(ui, f.place == p, k) {
            f.place = p;
        }
    }
    if f.place != FitPlace::On {
        field(ui, "docker.fit_to_path_side", &mut f.side, -100_000.0);
    }

    ui.add_space(4.0);
    let refs = [
        (
            FitReference::CenterOfRotation,
            "docker.fit_to_path_ref_pivot",
        ),
        (FitReference::BoundingBox, "docker.fit_to_path_ref_box"),
        (FitReference::OutlineBox, "docker.fit_to_path_ref_outline"),
    ];
    let cur = refs
        .iter()
        .find(|(r, _)| *r == f.reference)
        .map_or(String::new(), |(_, k)| tr(k));
    combo(
        ui,
        "docker.fit_to_path_reference",
        "fit_path_reference",
        &cur,
        &mut |ui| {
            for (r, k) in refs {
                ui.selectable_value(&mut f.reference, r, tr(k));
            }
        },
    );
    ui.horizontal(|ui| {
        let boxed = f.reference != FitReference::CenterOfRotation;
        ui.add_enabled(boxed, egui::Label::new(tr("docker.fit_to_path_origin")));
        origin_grid(ui, &mut f.origin, boxed);
    });

    ui.add_space(4.0);
    ui.strong(tr("docker.fit_to_path_rotation"));
    if radio(
        ui,
        f.rotation == FitRotation::Follow,
        "docker.fit_to_path_rotate",
    ) {
        f.rotation = FitRotation::Follow;
    }
    if radio(
        ui,
        f.rotation == FitRotation::Fixed,
        "docker.fit_to_path_fixed",
    ) {
        f.rotation = FitRotation::Fixed;
    }
    ui.horizontal(|ui| {
        ui.label(tr("docker.fit_to_path_angle"));
        ui.add_sized(
            [90.0, 22.0],
            crate::ui::field::NumField::new(&mut f.angle)
                .range(-360.0..=360.0)
                .max_decimals(2)
                .suffix("°"),
        )
        .rich_tip(crate::ui::tips::keyed("docker.fit_to_path_angle"));
    });

    check(
        ui,
        &mut f.ignore_rotation,
        "docker.fit_to_path_ignore_rotation",
    );

    ui.add_space(4.0);
    ui.strong(tr("docker.fit_to_path_modify_rotation"));
    let styles = [
        (FitSpin::None, "docker.fit_to_path_spin_none"),
        (FitSpin::Uniform, "docker.fit_to_path_spin_uniform"),
        (FitSpin::Progressive, "docker.fit_to_path_spin_progressive"),
        (FitSpin::Jitter, "docker.fit_to_path_spin_jitter"),
        (
            FitSpin::ProgressiveJitter,
            "docker.fit_to_path_spin_progressive_jitter",
        ),
    ];
    let cur = styles
        .iter()
        .find(|(st, _)| *st == f.spin)
        .map_or(String::new(), |(_, k)| tr(k));
    combo(
        ui,
        "docker.fit_to_path_spin",
        "fit_path_spin",
        &cur,
        &mut |ui| {
            for (st, k) in styles {
                ui.selectable_value(&mut f.spin, st, tr(k));
            }
        },
    );
    let [start_on, spin_on, revs_on, range_on, dir_on] = f.spin_fields();
    deg(
        ui,
        "docker.fit_to_path_start_angle",
        &mut f.start_angle,
        -360.0..=360.0,
        start_on,
    );
    deg(
        ui,
        "docker.fit_to_path_spin_angle",
        &mut f.spin_angle,
        -3600.0..=3600.0,
        spin_on,
    );
    ui.horizontal(|ui| {
        ui.add_enabled(
            revs_on,
            egui::Label::new(tr("docker.fit_to_path_revolutions")),
        );
        ui.add_enabled(
            revs_on,
            crate::ui::field::NumField::new(&mut f.revolutions)
                .range(-100.0..=100.0)
                .max_decimals(2),
        )
        .rich_tip(crate::ui::tips::keyed("docker.fit_to_path_revolutions"))
        .rich_disabled_tip(crate::ui::tips::keyed("docker.fit_to_path_revolutions"));
    });
    deg(
        ui,
        "docker.fit_to_path_range",
        &mut f.range,
        0.0..=180.0,
        range_on,
    );
    ui.add_enabled(
        dir_on,
        egui::Checkbox::new(&mut f.clockwise, tr("docker.fit_to_path_clockwise")),
    )
    .rich_tip(crate::ui::tips::keyed("docker.fit_to_path_clockwise"))
    .rich_disabled_tip(crate::ui::tips::keyed("docker.fit_to_path_clockwise"));

    ui.add_space(4.0);
    let orders = [
        (FitOrder::AsSelected, "docker.fit_to_path_order_selected"),
        (FitOrder::Reversed, "docker.fit_to_path_order_reversed"),
        (FitOrder::LeftToRight, "docker.fit_to_path_order_ltr"),
        (FitOrder::RightToLeft, "docker.fit_to_path_order_rtl"),
        (FitOrder::LargestFirst, "docker.fit_to_path_order_largest"),
        (FitOrder::SmallestFirst, "docker.fit_to_path_order_smallest"),
        (FitOrder::Alternate, "docker.fit_to_path_order_alternate"),
        (FitOrder::TopToBottom, "docker.fit_to_path_order_ttb"),
        (FitOrder::BottomToTop, "docker.fit_to_path_order_btt"),
        (FitOrder::TallToShort, "docker.fit_to_path_order_tall"),
        (FitOrder::ShortToTall, "docker.fit_to_path_order_short"),
        (FitOrder::WideToNarrow, "docker.fit_to_path_order_wide"),
        (FitOrder::NarrowToWide, "docker.fit_to_path_order_narrow"),
        (FitOrder::Random, "docker.fit_to_path_order_random"),
    ];
    let cur = orders
        .iter()
        .find(|(o, _)| *o == f.order)
        .map_or(String::new(), |(_, k)| tr(k));
    // The order sorts the objects fitted: one object (or one group) has
    // none to change, so the list is greyed and says why (#2740).
    let sortable = objects >= 2;
    ui.add_enabled_ui(sortable, |ui| {
        combo(
            ui,
            "docker.fit_to_path_order",
            "fit_path_order",
            &cur,
            &mut |ui| {
                for (o, k) in orders {
                    ui.selectable_value(&mut f.order, o, tr(k));
                }
            },
        );
    });
    if !sortable {
        dim(ui, tr("docker.fit_to_path_order_needs_two"));
    }
    ui.checkbox(&mut f.reverse, tr("docker.fit_to_path_reverse"))
        .rich_tip(crate::ui::tips::keyed("docker.fit_to_path_reverse"));
    if let Some((path, l)) = live {
        // Live off lets the objects go where they are.
        if f != l.settings {
            if f.live {
                app.set_live_fit_settings(path, f);
            } else {
                app.release_live_fit(path);
            }
        }
        return;
    }
    if f != app.fit_path {
        app.fit_path_edited = true;
    }
    app.fit_path = f;
    let valid = app.selection.len() >= 2;
    let preview = app.fit_objects_to_path_preview().to_vec();
    app.docker_preview.extend(preview);
    ui.add_space(8.0);
    if ui
        .add_enabled(valid, egui::Button::new(tr("docker.apply")))
        .rich_tip(crate::ui::tips::pair(
            "docker.apply",
            "docker.fit_to_path_apply_desc",
        ))
        .rich_disabled_tip(crate::ui::tips::pair(
            "docker.apply",
            "docker.fit_to_path_apply_desc",
        ))
        .clicked()
    {
        app.fit_objects_to_path();
        app.docker_preview.clear();
    }
}

/// Where the Fit Objects to Path panel drew its list `id` (such as
/// "fit_path_order") last frame, kept in the context's data.
pub(crate) fn fit_combo_id(id: &str) -> egui::Id {
    egui::Id::new(("fit_path_combo", id))
}

/// The id of square `i` (0 to 8, left to right from the top row) of the
/// Fit Objects to Path panel's Origin grid.
pub(crate) fn fit_origin_id(i: u8) -> egui::Id {
    egui::Id::new(("fit_path_origin", i))
}

/// Nine small squares, three by three, joined by lines: the point of each
/// object's box that goes on the path. The chosen one is filled.
fn origin_grid(ui: &mut Ui, origin: &mut u8, enabled: bool) {
    const CELL: f32 = 12.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(CELL * 3.0, CELL * 3.0), egui::Sense::hover());
    let color = if enabled {
        crate::theme::Tokens::text()
    } else {
        crate::theme::Tokens::text_dim()
    };
    let stroke = egui::Stroke::new(1.0, color);
    let centre = |i: u8| {
        rect.min
            + egui::vec2(
                (i % 3) as f32 * CELL + CELL / 2.0,
                (i / 3) as f32 * CELL + CELL / 2.0,
            )
    };
    let p = ui.painter().clone();
    for row in 0..3u8 {
        p.line_segment([centre(row * 3), centre(row * 3 + 2)], stroke);
        p.line_segment([centre(row), centre(row + 6)], stroke);
    }
    let tip = crate::ui::tips::keyed("docker.fit_to_path_origin");
    for i in 0..9u8 {
        let r = egui::Rect::from_center_size(centre(i), egui::vec2(7.0, 7.0));
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let resp = ui.interact(r.expand(2.0), fit_origin_id(i), sense);
        let fill = if *origin == i {
            color
        } else if resp.hovered() && enabled {
            crate::theme::Tokens::tool_hover()
        } else {
            crate::theme::Tokens::panel()
        };
        p.rect_filled(r, 0.0, fill);
        p.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
        if resp.rich_tip(tip.clone()).clicked() && enabled {
            *origin = i;
        }
    }
}

#[cfg(test)]
mod data_sheet_tests {
    use super::*;

    fn obj(
        id: u64,
        data: &[(&str, &str)],
        children: Vec<tracedraw_core::Shape>,
    ) -> tracedraw_core::Shape {
        let kind = if children.is_empty() {
            ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 1.0, 1.0),
                radius: 0.0,
                corners: None,
            }
        } else {
            ShapeKind::Group { children }
        };
        let mut s = tracedraw_core::Shape::new(ShapeId(id), kind);
        s.data = data
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        s
    }

    /// The sheet lists the objects with every field as a column, the
    /// objects of a group only to the levels asked, and totals the cost
    /// of the listed objects once (#3084).
    #[test]
    fn the_data_sheet_lists_fields_and_totals_costs() {
        let roots = vec![
            obj(
                1,
                &[
                    ("Name", "ART"),
                    ("Cost", "$15.00"),
                    ("Comments", "2020-PRES"),
                ],
                vec![],
            ),
            obj(
                2,
                &[("Name", "SIGN"), ("Cost", "68"), ("Size", ".75\"x7.25\"")],
                vec![obj(
                    3,
                    &[
                        ("Name", "TEXT"),
                        ("Cost", "1,000"),
                        ("fill.rule", "evenodd"),
                    ],
                    vec![],
                )],
            ),
        ];
        let rows = sheet_rows(&roots, 0);
        assert_eq!(rows.iter().map(|r| r.id.0).collect::<Vec<_>>(), [1, 2]);
        let rows = sheet_rows(&roots, 1);
        assert_eq!(
            rows.iter().map(|r| (r.id.0, r.depth)).collect::<Vec<_>>(),
            [(1, 0), (2, 0), (3, 1)]
        );
        assert_eq!(sheet_columns(&rows), ["Name", "Cost", "Comments", "Size"]);
        assert_eq!(sheet_total(&rows, "Cost"), Some(83.0));
        assert_eq!(sheet_total(&rows, "Name"), None);
        assert_eq!(sheet_total(&rows, "Size"), None);
        assert_eq!(sheet_number("$1,250.50"), Some(1250.5));
    }
}

#[cfg(test)]
mod blend_docker_tests {
    use super::*;

    /// The Blend panel shows its Color Blend choices and an Apply button
    /// in a panel of ordinary height, and Apply blends the two selected
    /// objects (#815).
    #[test]
    fn the_blend_panel_shows_color_blend_and_applies() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let r = |app: &mut App, x: f64| {
            app.new_shape(tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(x, 10.0, x + 20.0, 30.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect")
        };
        let a = r(&mut app, 10.0);
        let b = r(&mut app, 100.0);
        app.select(vec![a, b]);
        let run = |app: &mut App, events: Vec<egui::Event>| -> Vec<(String, egui::Rect)> {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| blend(app, ui));
            out.textures_delta.clear();
            let mut texts = Vec::new();
            for s in &out.shapes {
                if let egui::Shape::Text(t) = &s.shape {
                    texts.push((t.galley.text().to_string(), t.visual_bounding_rect()));
                }
            }
            texts
        };
        run(&mut app, Vec::new());
        let texts = run(&mut app, Vec::new());
        let find = |texts: &[(String, egui::Rect)], key: &str| {
            texts.iter().find(|(t, _)| *t == tr(key)).map(|(_, r)| *r)
        };
        let color = find(&texts, "docker.blend_color_blend").expect("Color Blend shown");
        let apply = find(&texts, "docker.apply").expect("Apply shown");
        assert!(
            color.bottom() < 600.0 && apply.bottom() < 600.0,
            "{color:?} {apply:?}"
        );
        let at = apply.center();
        run(&mut app, vec![egui::Event::PointerMoved(at)]);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut app, vec![button(true)]);
        run(&mut app, vec![button(false)]);
        let s = app.doc().find_shape(a).expect("start");
        assert!(
            s.effects.iter().any(|e| matches!(e, Effect::Blend { .. })),
            "{:?}",
            s.effects
        );
    }
}

#[cfg(test)]
mod find_properties_tests {
    use super::*;
    use tracedraw_core::{Color, Fill};

    /// The Find and Replace panel, Properties: Find All selects the red
    /// objects of the page, Next goes to the red one on page 2, and
    /// Replace All turns every red fill green in one undo step (#1071).
    #[test]
    fn find_and_replace_by_fill_color_across_pages() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        let red = Color::cmyk_pct(0.0, 100.0, 100.0, 0.0);
        let green = Color::rgb8(0, 200, 0);
        let rect = |app: &mut App, x: f64, c: Color| {
            let id = app
                .new_shape(tracedraw_core::ShapeKind::Rect {
                    rect: tracedraw_core::geometry::Rect::new(x, 10.0, x + 20.0, 30.0),
                    radius: 0.0,
                    corners: None,
                })
                .expect("rect");
            app.select(vec![id]);
            app.apply_fill(Fill::Solid(c));
            id
        };
        let a = rect(&mut app, 10.0, red);
        let b = rect(&mut app, 50.0, Color::rgb8(0, 0, 255));
        app.add_page();
        let c = rect(&mut app, 10.0, red);
        app.goto_page(0);
        app.select(vec![a]);
        let run = |app: &mut App, events: Vec<egui::Event>| -> Vec<(String, egui::Rect)> {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| find_replace(app, ui));
            out.textures_delta.clear();
            out.shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => {
                        Some((t.galley.text().to_string(), t.visual_bounding_rect()))
                    }
                    _ => None,
                })
                .collect()
        };
        let click = |app: &mut App, key: &str| {
            let texts = run(app, Vec::new());
            let at = texts
                .iter()
                .find(|(t, _)| *t == tr(key))
                .map(|(_, r)| r.center())
                .unwrap_or_else(|| panic!("{key} shown"));
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            run(app, vec![egui::Event::PointerMoved(at)]);
            run(app, vec![button(true)]);
            run(app, vec![button(false)]);
        };
        app.find_state.kind = 2;
        run(&mut app, Vec::new());
        // From Selection takes the red fill of `a`.
        click(&mut app, "find.from_selection");
        assert!(app.find_state.props.use_fill);
        assert_eq!(app.find_state.props.fill, red);
        // Look for the fill only.
        app.find_state.props.use_outline = false;
        app.find_state.props.use_font = false;
        click(&mut app, "dialog.find_all");
        assert_eq!(app.selection, vec![a], "the red object of this page");
        app.find_state.props.whole_drawing = true;
        click(&mut app, "dialog.find_all");
        assert_eq!(app.find_state.props.hits.len(), 2);
        click(&mut app, "dialog.find_next");
        click(&mut app, "dialog.find_next");
        assert_eq!(app.page_index(), 1, "Next goes to the second page");
        assert_eq!(app.selection, vec![c]);
        click(&mut app, "find.previous");
        assert_eq!(app.page_index(), 0);
        assert_eq!(app.selection, vec![a]);
        // Replace red with green everywhere.
        app.find_state.props.set_fill = true;
        app.find_state.props.new_fill = green;
        click(&mut app, "dialog.replace_all");
        let fill = |app: &App, id| app.doc().find_shape(id).map(|s| s.fill.clone());
        assert_eq!(fill(&app, a), Some(Fill::Solid(green)));
        assert_eq!(fill(&app, c), Some(Fill::Solid(green)));
        assert_eq!(fill(&app, b), Some(Fill::Solid(Color::rgb8(0, 0, 255))));
        app.undo();
        assert_eq!(fill(&app, a), Some(Fill::Solid(red)), "one undo step");
        assert_eq!(fill(&app, c), Some(Fill::Solid(red)));
    }
}

#[cfg(test)]
mod corners_docker_tests {
    use super::*;

    /// The Corners panel's Preview box hides the dashed result on the
    /// drawing and brings it back; it is on at first, kept in the
    /// settings, and Apply still works with it off (#1759).
    #[test]
    fn the_corners_preview_can_be_turned_off() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let id = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 60.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        let run = |app: &mut App, events: Vec<egui::Event>| -> Vec<(String, egui::Rect)> {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| corners(app, ui));
            out.textures_delta.clear();
            out.shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => {
                        Some((t.galley.text().to_string(), t.visual_bounding_rect()))
                    }
                    _ => None,
                })
                .collect()
        };
        let click = |app: &mut App, key: &str| {
            let texts = run(app, Vec::new());
            let at = texts
                .iter()
                .find(|(t, _)| *t == tr(key))
                .map(|(_, r)| r.center())
                .unwrap_or_else(|| panic!("{key} shown"));
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            run(app, vec![egui::Event::PointerMoved(at)]);
            run(app, vec![button(true)]);
            run(app, vec![button(false)]);
        };
        assert!(app.settings.corners_preview, "on at first");
        assert!(crate::settings::Settings::default().corners_preview);
        run(&mut app, Vec::new());
        assert_eq!(app.docker_preview.len(), 1, "the preview shows");
        click(&mut app, "dialog.preview");
        assert!(!app.settings.corners_preview, "the box turned it off");
        run(&mut app, Vec::new());
        assert!(app.docker_preview.is_empty(), "no preview on the drawing");
        // Kept in the settings file.
        let json = serde_json::to_string(&app.settings).expect("settings");
        let back: crate::settings::Settings = serde_json::from_str(&json).expect("read back");
        assert!(!back.corners_preview);
        // Apply is still offered and works without the preview.
        click(&mut app, "docker.apply");
        assert!(matches!(
            app.doc().find_shape(id).map(|s| &s.kind),
            Some(tracedraw_core::ShapeKind::Path { .. })
        ));
        click(&mut app, "dialog.preview");
        assert!(app.settings.corners_preview, "and back on");
    }
}

#[cfg(test)]
mod symbol_docker_tests {
    use super::*;

    /// The Symbols panel counts every instance of a symbol, those inside
    /// a group or a clip frame too (#3248): grouped instances were not
    /// counted, so a symbol whose copies were all grouped said 0.
    #[test]
    fn grouped_symbol_instances_are_counted() {
        let mut app = App::headless();
        let square = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("square");
        app.select(vec![square]);
        app.create_symbol_from_selection();
        assert_eq!(symbol_uses(app.doc(), 0), 1);
        let mut copies = Vec::new();
        for _ in 0..4 {
            app.insert_symbol_instance(0);
            copies.push(app.selection[0]);
        }
        assert_eq!(symbol_uses(app.doc(), 0), 5);
        app.select(copies[..3].to_vec());
        app.group_selection();
        assert_eq!(symbol_uses(app.doc(), 0), 5, "three of them in a group");
        // One inside a clip frame.
        let frame = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 300.0, 300.0),
                radius: 0.0,
                corners: None,
            })
            .expect("frame");
        app.place_inside(vec![copies[3]], frame);
        assert_eq!(symbol_uses(app.doc(), 0), 5, "one in a clip frame");
        let all: Vec<ShapeId> = app.doc().pages[0].layers[0]
            .shapes
            .iter()
            .map(|s| s.id)
            .collect();
        app.select(all);
        app.group_selection();
        assert_eq!(symbol_uses(app.doc(), 0), 5, "groups in a group");
        assert_eq!(symbol_uses(app.doc(), 1), 0);
    }
}

/// Whether the object data field `key` is one TraceDraw keeps for itself
/// (a name with a dot, as `bitmap.link` or `anchor.1`, and the
/// connector's and hinting's): Copy to Selection leaves those alone.
pub(crate) fn data_field_is_internal(key: &str) -> bool {
    key.contains('.') || matches!(key, "connector" | "hinting")
}

impl App {
    /// Object Data's Copy to Selection (#3262): the fields of the first
    /// selected object go to the other selected objects, as one undo
    /// step; a field they have of the same name takes the value, their
    /// other fields stay, and fields TraceDraw keeps for itself are not
    /// copied.
    pub fn copy_object_data_to_selection(&mut self) {
        let shapes = self.selected_shapes();
        let Some((first, rest)) = shapes.split_first() else {
            return;
        };
        let fields: Vec<(String, String)> = first
            .data
            .iter()
            .filter(|(k, _)| !data_field_is_internal(k))
            .cloned()
            .collect();
        let cmds: Vec<Command> = rest
            .iter()
            .filter_map(|s| {
                let mut data = s.data.clone();
                for (k, v) in &fields {
                    match data.iter_mut().find(|(f, _)| f == k) {
                        Some(entry) => entry.1 = v.clone(),
                        None => data.push((k.clone(), v.clone())),
                    }
                }
                (data != s.data).then_some(Command::SetObjectData { shape: s.id, data })
            })
            .collect();
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Copy Object Data", &cmds) {
            self.status = e.to_string();
        }
    }
}

#[cfg(test)]
mod object_data_copy_tests {
    use super::*;
    use tracedraw_core::document::ShapeKind;
    use tracedraw_core::geometry::Rect;

    /// Copy to Selection gives the first object's own fields to the
    /// others in one undo step, keeping their other fields and leaving
    /// TraceDraw's own fields alone (#3262).
    #[test]
    fn copy_to_selection_gives_the_fields_in_one_step() {
        let mut app = App::headless();
        let rect = |app: &mut App, x: f64| {
            app.new_shape(ShapeKind::Rect {
                rect: Rect::new(x, 0.0, x + 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect")
        };
        let a = rect(&mut app, 0.0);
        let b = rect(&mut app, 20.0);
        app.run(Command::SetObjectData {
            shape: a,
            data: vec![
                ("Name".into(), "Bolt".into()),
                ("Cost".into(), "3".into()),
                ("bitmap.link".into(), "/a.png".into()),
            ],
        });
        app.run(Command::SetObjectData {
            shape: b,
            data: vec![("Cost".into(), "9".into()), ("Size".into(), "M4".into())],
        });
        app.select(vec![a, b]);
        let depth = app.engine.history_labels().0.len();
        app.copy_object_data_to_selection();
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let data = app.doc().find_shape(b).expect("b").data.clone();
        assert_eq!(
            data,
            vec![
                ("Cost".to_string(), "3".to_string()),
                ("Size".to_string(), "M4".to_string()),
                ("Name".to_string(), "Bolt".to_string()),
            ]
        );
        app.undo();
        let data = app.doc().find_shape(b).expect("b").data.clone();
        assert_eq!(data.len(), 2);
    }
}

#[cfg(test)]
mod find_field_tests {
    use super::*;

    /// The Find and Replace fields' right-click menu puts its text where
    /// the cursor is, or over the letters picked, counted in characters
    /// (#3678); the special characters go in as their codes (#3679).
    #[test]
    fn the_find_fields_menu_puts_text_at_the_cursor() {
        let mut t = "größe text".to_string();
        assert_eq!(put_in_field(&mut t, 5, 5, "^p"), 7);
        assert_eq!(t, "größe^p text");
        // Over a selection made backwards: the selection goes.
        assert_eq!(put_in_field(&mut t, 12, 8, "^t"), 10);
        assert_eq!(t, "größe^p ^t");
        // Cut: nothing put over the picked letters.
        assert_eq!(put_in_field(&mut t, 0, 5, ""), 0);
        assert_eq!(t, "^p ^t");
        // At the end.
        assert_eq!(put_in_field(&mut t, 5, 5, "^w"), 7);
        assert_eq!(t, "^p ^t^w");
        let codes: Vec<&str> = FIND_CODES.iter().map(|(_, c)| *c).collect();
        assert_eq!(codes, ["^p", "^l", "^t", "^s", "^w"]);
        for (key, _) in FIND_CODES {
            assert_ne!(tr(key), key, "{key} is in the language files");
        }
    }
}

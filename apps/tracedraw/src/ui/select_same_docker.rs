//! Select Same combines properties, then selects matches in one command.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::field::NumField;
use crate::ui::tips::RichTip;
use egui::{Response, RichText, Stroke, Ui};
use tracedraw_core::select_same::SizeComparison;

fn heading(ui: &mut Ui, key: &str) {
    ui.add_space(12.0);
    ui.label(
        RichText::new(tr(key))
            .font(crate::theme::bold(12.0))
            .color(Tokens::text_dim()),
    );
    ui.add_space(4.0);
}

fn check(ui: &mut Ui, checked: &mut bool, key: &'static str, width: f32) {
    let response = ui
        .allocate_ui_with_layout(
            egui::vec2(width, 32.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_max_width(width);
                ui.spacing_mut().interact_size.y = 32.0;
                ui.checkbox(checked, tr(key)).rich_tip(tr(key))
            },
        )
        .inner;
    record(&response, key);
}

fn widest_label(ui: &Ui, keys: &[&str]) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    keys.iter()
        .map(|key| {
            ui.painter()
                .layout_no_wrap(tr(key), font.clone(), Tokens::text())
                .size()
                .x
        })
        .fold(0.0_f32, f32::max)
}

fn size_comparison(ui: &mut Ui, value: &mut SizeComparison, width: f32) {
    let selected = match value {
        SizeComparison::Equal => "same.equal",
        SizeComparison::Larger => "same.larger",
        SizeComparison::Smaller => "same.smaller",
    };
    let response = egui::ComboBox::from_id_salt("same_size_comparison")
        .selected_text(tr(selected))
        .width(width)
        .show_ui(ui, |ui| {
            for (mode, key) in [
                (SizeComparison::Equal, "same.equal"),
                (SizeComparison::Larger, "same.larger"),
                (SizeComparison::Smaller, "same.smaller"),
            ] {
                ui.selectable_value(value, mode, tr(key))
                    .rich_tip(crate::ui::tips::keyed(key));
            }
        })
        .response
        .rich_tip(crate::ui::tips::keyed(selected));
    record(&response, "same.size_comparison");
}

fn tolerance(ui: &mut Ui, value: &mut f64, width: f32) {
    let response = ui
        .add_sized(
            [width, 28.0],
            NumField::new(value)
                .range(0.0..=100_000.0)
                .step(0.1)
                .max_decimals(3)
                .suffix(" mm")
                .unit_mm(1.0)
                .wheel_on_hover(false),
        )
        .rich_tip(tr("same.tolerance"));
    record(&response, "same.tolerance");
}

fn record(response: &Response, key: &'static str) {
    #[cfg(test)]
    response.ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new(key), (response.rect, response.enabled()));
    });
    #[cfg(not(test))]
    let _ = (response, key);
}

/// Draw in the docker's scroll area. Matching only runs on an explicit click.
pub fn select_same(app: &mut App, ui: &mut Ui) {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    let references = app.selection.len();
    egui::Frame::new()
        .fill(Tokens::field())
        .corner_radius(4.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (icon, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                crate::ui::menu_glyphs::draw(
                    ui.painter(),
                    icon,
                    crate::ui::menu_glyphs::Glyph::SelectAll,
                    Tokens::icon(),
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(tr("same.reference")).font(crate::theme::bold(13.0)));
                    let summary = if references == 0 {
                        tr("same.empty")
                    } else {
                        trf("same.reference_count", &[("n", &references.to_string())])
                    };
                    ui.label(RichText::new(summary).size(12.0).color(Tokens::text_dim()));
                });
            });
        })
        .response
        .rich_tip(tr("same.match_hint"));

    let options = &mut app.select_same.options;
    heading(ui, "same.attributes");
    let width = ui.available_width();
    let property_width = widest_label(
        ui,
        &[
            "same.fill",
            "same.kind",
            "same.stroke_color",
            "same.stroke_width",
            "same.font",
            "same.font_size",
        ],
    ) + ui.spacing().icon_width
        + ui.spacing().icon_spacing;
    if width >= 200.0 && (width - 8.0) / 2.0 >= property_width {
        let column_width = (width - 8.0) / 2.0;
        egui::Grid::new("same_attributes")
            .num_columns(2)
            .min_col_width(column_width)
            .max_col_width(column_width)
            .spacing(egui::vec2(8.0, 4.0))
            .show(ui, |ui| {
                check(ui, &mut options.fill, "same.fill", column_width);
                check(ui, &mut options.kind, "same.kind", column_width);
                ui.end_row();
                check(
                    ui,
                    &mut options.stroke_color,
                    "same.stroke_color",
                    column_width,
                );
                check(
                    ui,
                    &mut options.stroke_width,
                    "same.stroke_width",
                    column_width,
                );
                ui.end_row();
                check(ui, &mut options.font, "same.font", column_width);
                check(ui, &mut options.font_size, "same.font_size", column_width);
                ui.end_row();
            });
    } else {
        check(ui, &mut options.fill, "same.fill", width);
        check(ui, &mut options.kind, "same.kind", width);
        check(ui, &mut options.stroke_color, "same.stroke_color", width);
        check(ui, &mut options.stroke_width, "same.stroke_width", width);
        check(ui, &mut options.font, "same.font", width);
        check(ui, &mut options.font_size, "same.font_size", width);
    }
    check(ui, &mut options.size, "same.size", width);
    if options.size {
        egui::Frame::new()
            .fill(Tokens::field())
            .corner_radius(4.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let width = ui.available_width();
                let comparison_width =
                    widest_label(ui, &["same.equal", "same.larger", "same.smaller"])
                        + ui.spacing().icon_width
                        + ui.spacing().icon_spacing
                        + 2.0 * ui.spacing().button_padding.x;
                let label_width = widest_label(ui, &["same.tolerance"]);
                if width >= comparison_width + label_width + 88.0 + 16.0 {
                    ui.horizontal(|ui| {
                        let field_width = width - comparison_width - label_width - 16.0;
                        size_comparison(ui, &mut options.size_comparison, comparison_width);
                        ui.label(RichText::new(tr("same.tolerance")).color(Tokens::text_dim()));
                        tolerance(ui, &mut options.tolerance_mm, field_width);
                    });
                } else {
                    size_comparison(ui, &mut options.size_comparison, width);
                    ui.label(
                        RichText::new(tr("same.tolerance"))
                            .small()
                            .color(Tokens::text_dim()),
                    );
                    tolerance(ui, &mut options.tolerance_mm, width);
                }
            });
    }
    heading(ui, "same.scope");
    let scope_width =
        widest_label(ui, &["same.page", "same.layer"]) + 2.0 * ui.spacing().button_padding.x;
    if width >= 200.0 && (width - 8.0) / 2.0 >= scope_width {
        ui.horizontal(|ui| {
            let button_width = (width - 8.0) / 2.0;
            for (value, key) in [(false, "same.page"), (true, "same.layer")] {
                let response = ui
                    .add_sized(
                        [button_width, 32.0],
                        egui::Button::new(tr(key)).selected(options.current_layer_only == value),
                    )
                    .rich_tip(tr(key));
                if response.clicked() {
                    options.current_layer_only = value;
                }
            }
        });
    } else {
        let selected = if options.current_layer_only {
            "same.layer"
        } else {
            "same.page"
        };
        egui::ComboBox::from_id_salt("same_scope")
            .selected_text(tr(selected))
            .width(width)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut options.current_layer_only, false, tr("same.page"));
                ui.selectable_value(&mut options.current_layer_only, true, tr("same.layer"));
            })
            .response
            .rich_tip(tr(selected));
    }
    check(
        ui,
        &mut options.exclude_containers,
        "same.exclude_containers",
        width,
    );

    let has_criteria = options.has_criteria();
    let ready = app.has_document() && references > 0 && has_criteria;
    ui.add_space(12.0);
    let response = ui
        .add_enabled_ui(ready, |ui| {
            ui.add_sized(
                [width, 36.0],
                egui::Button::new(RichText::new(tr("same.apply")).strong())
                    .fill(Tokens::tool_active())
                    .stroke(Stroke::new(1.0, Tokens::selected_row_line())),
            )
        })
        .inner
        .rich_tip(tr("same.apply_desc"))
        .rich_disabled_tip(tr(if references == 0 {
            "same.empty"
        } else if !has_criteria {
            "same.no_criteria"
        } else {
            "same.apply_desc"
        }));
    record(&response, "same.apply");
    if response.clicked() {
        let count = app.select_same_matches();
        app.status = trf("same.matches", &[("n", &count.to_string())]);
    }
    if !has_criteria {
        ui.label(tr("same.no_criteria"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect};
    use tracedraw_core::{Color, Fill, ShapeId, ShapeKind};

    fn rectangle(app: &mut App, x: f64, color: Color) -> ShapeId {
        app.new_shape_styled(
            ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(x, 0.0, x + 10.0, 10.0),
                radius: 0.0,
                corners: None,
            },
            |shape| shape.fill = Fill::Solid(color),
        )
        .expect("rectangle")
    }

    fn draw(ctx: &egui::Context, app: &mut App, width: f32, events: Vec<Event>) -> Rect {
        crate::ui::apply_theme(ctx, crate::theme::theme().id());
        let mut bounds = Rect::NOTHING;
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 1200.0))),
                events,
                ..Default::default()
            },
            |ui| {
                select_same(app, ui);
                bounds = ui.min_rect();
            },
        );
        output.textures_delta.clear();
        bounds
    }

    fn control(ctx: &egui::Context, key: &'static str) -> (Rect, bool) {
        ctx.data(|data| data.get_temp(egui::Id::new(key)))
            .expect("drawn control")
    }

    fn click(ctx: &egui::Context, app: &mut App, key: &'static str) {
        let at = control(ctx, key).0.center();
        draw(ctx, app, 300.0, vec![Event::PointerMoved(at)]);
        for pressed in [true, false] {
            draw(
                ctx,
                app,
                300.0,
                vec![Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
    }

    #[test]
    fn action_needs_a_reference_and_a_matching_property() {
        crate::i18n::with_thread_language("en", || {
            let ctx = crate::theme::ui_context();
            let mut app = App::headless();
            draw(&ctx, &mut app, 300.0, Vec::new());
            assert!(!control(&ctx, "same.apply").1);
            let reference = rectangle(&mut app, 0.0, Color::rgb8(200, 40, 60));
            app.select(vec![reference]);
            draw(&ctx, &mut app, 300.0, Vec::new());
            assert!(control(&ctx, "same.apply").1);
            click(&ctx, &mut app, "same.fill");
            click(&ctx, &mut app, "same.kind");
            assert!(!control(&ctx, "same.apply").1);
            click(&ctx, &mut app, "same.apply");
            assert_eq!(app.selection, vec![reference]);
        });
    }

    #[test]
    fn clicking_selects_matches_and_undo_restores_only_the_selection() {
        crate::i18n::with_thread_language("en", || {
            let ctx = crate::theme::ui_context();
            let mut app = App::headless();
            let red = Color::rgb8(200, 40, 60);
            let reference = rectangle(&mut app, 0.0, red);
            let matching = rectangle(&mut app, 20.0, red);
            let other = rectangle(&mut app, 40.0, Color::rgb8(30, 100, 200));
            app.select(vec![reference]);
            app.engine.mark_saved();
            let revision = app.engine.revision();
            draw(&ctx, &mut app, 300.0, Vec::new());
            click(&ctx, &mut app, "same.apply");
            assert!(app.selection.contains(&reference));
            assert!(app.selection.contains(&matching));
            assert!(!app.selection.contains(&other));
            assert_eq!(app.engine.revision(), revision);
            assert!(!app.engine.is_dirty());
            app.undo();
            assert_eq!(app.selection, vec![reference]);
            assert!(app.doc().find_shape(other).is_some());
            app.redo();
            assert_eq!(app.selection.len(), 2);
        });
    }

    #[test]
    fn excluding_containers_from_the_panel_skips_their_contents_and_can_be_undone() {
        crate::i18n::with_thread_language("en", || {
            let ctx = crate::theme::ui_context();
            let mut app = App::headless();
            let red = Color::rgb8(200, 40, 60);
            let reference = rectangle(&mut app, 0.0, red);
            let group_a = rectangle(&mut app, 20.0, red);
            let group_b = rectangle(&mut app, 40.0, red);
            app.select(vec![group_a, group_b]);
            app.group_selection();
            let group = *app.selection.first().expect("group");
            let matching = rectangle(&mut app, 60.0, red);
            let mut content = app.doc().find_shape(reference).expect("reference").clone();
            let mut frame = content.clone();
            frame.id = app.engine.new_shape_id();
            content.id = app.engine.new_shape_id();
            let clip_child = content.id;
            let container = app
                .new_shape(ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents: vec![content],
                })
                .expect("clip frame");
            app.engine.mark_saved();
            let before = app.doc().clone();
            let revision = app.engine.revision();
            for kind in [true, false] {
                app.select_same.options.kind = kind;
                app.select_same.options.exclude_containers = false;
                app.select(vec![reference]);
                draw(&ctx, &mut app, 300.0, Vec::new());
                click(&ctx, &mut app, "same.apply");
                assert_eq!(app.selection.len(), 5);
                for id in [reference, matching, group_a, group_b, clip_child] {
                    assert!(app.selection.contains(&id));
                }
                app.select(vec![reference]);
                draw(&ctx, &mut app, 300.0, Vec::new());
                click(&ctx, &mut app, "same.exclude_containers");
                assert!(app.select_same.options.exclude_containers);
                click(&ctx, &mut app, "same.apply");
                assert_eq!(app.selection, vec![reference, matching]);
                app.undo();
                assert_eq!(app.selection, vec![reference]);
                app.redo();
                assert_eq!(app.selection, vec![reference, matching]);
                click(&ctx, &mut app, "same.exclude_containers");
                assert!(!app.select_same.options.exclude_containers);
                app.select(vec![reference]);
                draw(&ctx, &mut app, 300.0, Vec::new());
                click(&ctx, &mut app, "same.apply");
                assert_eq!(app.selection.len(), 5);
                assert!(app.selection.contains(&group_a));
                assert!(app.selection.contains(&group_b));
                assert!(app.selection.contains(&clip_child));
                assert!(app.doc().find_shape(group).is_some());
                assert!(app.doc().find_shape(container).is_some());
                assert_eq!(app.doc(), &before);
                assert_eq!(app.engine.revision(), revision);
                assert!(!app.engine.is_dirty());
            }
        });
    }

    #[test]
    fn attributes_share_aligned_columns_and_default_panel_stays_compact() {
        for language in ["en", "zh-CN"] {
            crate::i18n::with_thread_language(language, || {
                let ctx = crate::theme::ui_context();
                let mut app = App::headless();
                let reference = rectangle(&mut app, 0.0, Color::rgb8(200, 40, 60));
                app.select(vec![reference]);
                let bounds = draw(&ctx, &mut app, 300.0, Vec::new());
                for (left, right) in [
                    ("same.fill", "same.kind"),
                    ("same.stroke_color", "same.stroke_width"),
                    ("same.font", "same.font_size"),
                ] {
                    let left = control(&ctx, left).0;
                    let right = control(&ctx, right).0;
                    assert!((left.center().y - right.center().y).abs() <= 1.0);
                    assert!(left.right() <= right.left());
                    assert!(left.height() >= 30.0 && right.height() >= 30.0);
                }
                assert!(bounds.height() <= 430.0, "{language}: {bounds:?}");
                let apply = control(&ctx, "same.apply").0;
                assert!(apply.height() >= 36.0);
                assert!((apply.width() - bounds.width()).abs() <= 1.0);
            });
        }
    }

    #[test]
    fn narrow_panels_fit_long_labels_and_the_dimension_controls_in_both_themes() {
        for language in ["en", "zh-CN", "de", "fr", "ru"] {
            crate::i18n::with_thread_language(language, || {
                for theme in [crate::theme::Theme::Light, crate::theme::Theme::Dark] {
                    crate::theme::set_theme(theme);
                    let ctx = crate::theme::ui_context();
                    let mut app = App::headless();
                    app.select_same.options.size = true;
                    for width in [200.0, 240.0, 260.0, 300.0, 320.0] {
                        let bounds = draw(&ctx, &mut app, width, Vec::new());
                        assert!(bounds.right() <= width + 1.0, "{language}: {bounds:?}");
                        for key in [
                            "same.fill",
                            "same.kind",
                            "same.stroke_color",
                            "same.stroke_width",
                            "same.font",
                            "same.font_size",
                            "same.size",
                            "same.exclude_containers",
                            "same.size_comparison",
                            "same.tolerance",
                            "same.apply",
                        ] {
                            let control = control(&ctx, key).0;
                            assert!(
                                control.right() <= width + 1.0,
                                "{language}/{key}: {control:?}"
                            );
                        }
                    }
                }
            });
        }
        crate::theme::set_theme(crate::theme::Theme::Light);
    }

    /// Isolate the panel from canvas rendering and compare one reference in
    /// a small drawing with one in a 10,000-object drawing. Adding unrelated
    /// objects must not turn an idle panel frame into another matching query.
    /// Run with `cargo test --release -p tracedraw --lib -- --ignored
    /// frame_times_of_select_same_panel --nocapture`.
    #[test]
    #[ignore]
    fn frame_times_of_select_same_panel() {
        const FRAMES: u32 = 300;
        crate::i18n::with_thread_language("en", || {
            let fixture = |count: usize| {
                let mut document = tracedraw_core::Document::default();
                let mut shapes = Vec::with_capacity(count);
                for index in 0..count {
                    let id = document.ids_mut().shape();
                    let x = (index % 100) as f64 * 12.0;
                    let y = (index / 100) as f64 * 12.0;
                    shapes.push(tracedraw_core::Shape::new(
                        id,
                        ShapeKind::Rect {
                            rect: tracedraw_core::Rect::new(x, y, x + 10.0, y + 10.0),
                            radius: 0.0,
                            corners: None,
                        },
                    ));
                }
                let reference = shapes[0].id;
                document.pages[0].layers[0].shapes = shapes;
                let mut app = App::headless();
                app.open_document(document, None);
                app.select(vec![reference]);
                app.select_same.options.size = true;
                let ctx = crate::theme::ui_context();
                for _ in 0..24 {
                    draw(&ctx, &mut app, 300.0, Vec::new());
                }
                (ctx, app)
            };
            let (small_ctx, mut small) = fixture(1);
            let (large_ctx, mut large) = fixture(10_000);
            let small_revision = small.engine.revision();
            let large_revision = large.engine.revision();
            let measure = |ctx: &egui::Context, app: &mut App| {
                let start = std::time::Instant::now();
                for index in 0..FRAMES {
                    std::hint::black_box(draw(
                        ctx,
                        app,
                        300.0,
                        vec![Event::PointerMoved(Pos2::new(
                            260.0 + (index % 20) as f32,
                            1100.0,
                        ))],
                    ));
                }
                start.elapsed() / FRAMES
            };
            let mut small_times = Vec::new();
            let mut large_times = Vec::new();
            for _ in 0..5 {
                small_times.push(measure(&small_ctx, &mut small));
                large_times.push(measure(&large_ctx, &mut large));
            }
            small_times.sort();
            large_times.sort();
            eprintln!(
                "Select Same panel, one selected reference, median of five {FRAMES}-frame batches: \
                 1 object {:?}/frame, 10000 objects {:?}/frame, ratio {:.3}",
                small_times[2],
                large_times[2],
                large_times[2].as_secs_f64() / small_times[2].as_secs_f64(),
            );
            assert_eq!(small.selection.len(), 1);
            assert_eq!(large.selection.len(), 1);
            assert_eq!(small.engine.revision(), small_revision);
            assert_eq!(large.engine.revision(), large_revision);
            assert!(!small.engine.is_dirty() && !large.engine.is_dirty());
        });
    }

    /// Render real widgets without opening a window for visual review.
    #[test]
    #[ignore]
    fn select_same_visual_check() {
        use crate::soft_canvas::{Canvas, Textures};
        let dir =
            std::path::PathBuf::from(std::env::var("SELECT_SAME_SHOTS").expect("output folder"));
        std::fs::create_dir_all(&dir).expect("screenshot folder");
        for theme in [crate::theme::Theme::Light, crate::theme::Theme::Dark] {
            crate::theme::set_theme(theme);
            for language in ["en", "zh-CN"] {
                crate::i18n::with_thread_language(language, || {
                    let mut app = App::headless();
                    let reference = rectangle(&mut app, 0.0, Color::rgb8(200, 40, 60));
                    app.select(vec![reference]);
                    for (tag, width, dimensions) in
                        [("default", 300, false), ("dimensions", 240, true)]
                    {
                        // Each screenshot starts with a complete font atlas.
                        // A fresh texture store cannot use another context's
                        // incremental atlas updates.
                        let ctx = crate::theme::ui_context();
                        crate::ui::apply_theme(&ctx, theme.id());
                        app.select_same.options.size = dimensions;
                        let height = 760;
                        let mut textures = Textures::default();
                        let mut canvas = Canvas::new(width, height);
                        for _ in 0..4 {
                            let mut output = ctx.run_ui(
                                RawInput {
                                    screen_rect: Some(Rect::from_min_size(
                                        Pos2::ZERO,
                                        egui::vec2(width as f32, height as f32),
                                    )),
                                    ..Default::default()
                                },
                                |ui| {
                                    ui.painter()
                                        .rect_filled(ui.max_rect(), 0.0, Tokens::panel());
                                    ui.label(
                                        RichText::new(tr("docker.select_same")).strong().size(16.0),
                                    );
                                    ui.add_space(8.0);
                                    select_same(&mut app, ui);
                                },
                            );
                            textures.apply(&output.textures_delta);
                            let primitives = ctx.tessellate(
                                std::mem::take(&mut output.shapes),
                                output.pixels_per_point,
                            );
                            canvas = Canvas::new(width, height);
                            canvas.paint(&textures, &primitives);
                            for id in &output.textures_delta.free {
                                textures.0.remove(id);
                            }
                            output.textures_delta.clear();
                        }
                        let pixels: Vec<u8> = canvas
                            .px
                            .iter()
                            .flat_map(|pixel| {
                                let alpha = pixel[3];
                                let mut rgba = [0; 4];
                                for (i, value) in pixel.iter().take(3).enumerate() {
                                    rgba[i] = if alpha > 0.0 {
                                        (value / alpha * 255.0).round().clamp(0.0, 255.0) as u8
                                    } else {
                                        0
                                    };
                                }
                                rgba[3] = (alpha * 255.0).round() as u8;
                                rgba
                            })
                            .collect();
                        image::save_buffer(
                            dir.join(format!("select-same-{}-{language}-{tag}.png", theme.id())),
                            &pixels,
                            width as u32,
                            height as u32,
                            image::ColorType::Rgba8,
                        )
                        .expect("saved screenshot");
                    }
                });
            }
        }
        crate::theme::set_theme(crate::theme::Theme::Light);
    }
}

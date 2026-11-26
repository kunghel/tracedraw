//! The property bar of a text fitted to a path (Pick tool): orientation,
//! vertical placement, placement along the path, distance from it, offset
//! along it, the two mirrors and Straighten Text.

use crate::ui::btn_icons::IconUi;
use crate::app::App;
use crate::i18n::tr;
use crate::ui::field::NumField;
use crate::ui::propbar::{pic_button, sep, Pic};
use crate::ui::tips::RichTip;
use egui::{vec2, Button, Color32, Painter, Rect, Shape, Stroke, Ui};
use tracedraw_core::document::{PathTextOrientation, PathTextVertical, TextAlign};
use tracedraw_core::geometry::{Point, Vec2};

const ORIENTATIONS: [(PathTextOrientation, &str); 5] = [
    (PathTextOrientation::Rotate, "toolbar.path_rotate"),
    (
        PathTextOrientation::VerticalSkew,
        "toolbar.path_vertical_skew",
    ),
    (
        PathTextOrientation::HorizontalSkew,
        "toolbar.path_horizontal_skew",
    ),
    (PathTextOrientation::Upright, "toolbar.path_upright"),
    (PathTextOrientation::Radial, "toolbar.path_radial"),
];

const VERTICALS: [(PathTextVertical, &str); 4] = [
    (PathTextVertical::Baseline, "toolbar.path_on_baseline"),
    (PathTextVertical::Ascender, "toolbar.path_on_ascender"),
    (PathTextVertical::Descender, "toolbar.path_on_descender"),
    (PathTextVertical::Center, "toolbar.path_on_center"),
];

/// Where along the path the text sits: an alignment, or on a closed
/// path centred on its top or bottom.
#[derive(Clone, Copy, PartialEq)]
enum Placement {
    Along(TextAlign),
    Top,
    Bottom,
}

const PLACEMENTS: [(Placement, &str); 5] = [
    (Placement::Along(TextAlign::Left), "toolbar.path_place_left"),
    (
        Placement::Along(TextAlign::Center),
        "toolbar.path_place_center",
    ),
    (
        Placement::Along(TextAlign::Right),
        "toolbar.path_place_right",
    ),
    (Placement::Top, "toolbar.path_place_top"),
    (Placement::Bottom, "toolbar.path_place_bottom"),
];

/// Sample letters and a guide. The same drawing fits in the button
/// and in the larger row.
#[derive(Clone, Copy)]
enum Preview {
    Orientation(PathTextOrientation),
    Vertical(PathTextVertical),
    Placement(Placement),
}

impl Preview {
    fn paint(self, painter: &Painter, rect: Rect, ink: Color32) {
        // Small stroke letters keep the examples independent of UI fonts.
        const A: &[&[(f64, f64)]] = &[&[(-3., 0.), (0., -9.), (3., 0.)], &[(-2., -3.), (2., -3.)]];
        const B: &[&[(f64, f64)]] = &[&[
            (-2.8, 0.),
            (-2.8, -9.),
            (0.6, -9.),
            (2.5, -7.8),
            (2.5, -6.),
            (0.8, -4.7),
            (-2.8, -4.7),
            (0.8, -4.7),
            (2.9, -3.4),
            (2.9, -1.4),
            (1., 0.),
            (-2.8, 0.),
        ]];
        const C: &[&[(f64, f64)]] = &[&[
            (2.7, -8.),
            (0.8, -9.),
            (-1.1, -8.7),
            (-2.8, -7.),
            (-2.8, -2.),
            (-1.1, -0.3),
            (0.8, 0.),
            (2.7, -1.),
        ]];
        const G: &[&[(f64, f64)]] = &[
            &[
                (2.6, -1.6),
                (0.9, -0.3),
                (-1.3, -0.4),
                (-2.7, -2.),
                (-2.7, -4.3),
                (-1.3, -5.9),
                (0.9, -6.),
                (2.6, -4.6),
            ],
            &[(2.6, -6.), (2.6, 1.), (1.4, 3.), (-1.8, 3.)],
        ];
        let scale = (rect.width() / 56.0).min(rect.height() / 26.0);
        if scale <= 0.0 {
            return;
        }
        let at = |x: f64, y: f64| rect.center() + vec2(x as f32, y as f32 - 15.0) * scale;
        let guide_stroke = Stroke::new((1.1 * scale).max(0.85), ink.gamma_multiply(0.75));
        let ink_stroke = Stroke::new((2.2 * scale).max(1.0), ink);
        let letter = |glyph: &[&[(f64, f64)]], x, y: f64, size, orientation, slope: f64| {
            // Text uses upward Y. The preview guide uses screen coordinates.
            let frame = tracedraw_text::path_letter_frame(
                orientation,
                Vec2::new(1., -slope).normalize(),
                Point::new(x, -y),
                Point::new(0., -39.), // The sample arc's centre.
            );
            for line in glyph {
                let points = line.iter().map(|&(gx, gy)| {
                    let p = frame * Point::new(gx * size, -gy * size);
                    at(x + p.x, y - p.y)
                });
                painter.add(Shape::line(points.collect(), ink_stroke));
            }
        };
        let upright = PathTextOrientation::Upright;
        match self {
            Self::Orientation(orientation) => {
                let arc = |x: f64| 16.0 + 0.022 * x * x;
                painter.add(Shape::line(
                    (-6..=6)
                        .map(|i| {
                            let x = f64::from(i) * 3.8;
                            at(x, arc(x))
                        })
                        .collect(),
                    guide_stroke,
                ));
                for (glyph, x) in [(A, -11.), (B, 0.), (C, 11.)] {
                    letter(glyph, x, arc(x), 1.15, orientation, 0.044 * x);
                }
            }
            Self::Vertical(vertical) => {
                painter.line_segment([at(-21., 15.), at(21., 15.)], guide_stroke);
                let y = 15.
                    + match vertical {
                        PathTextVertical::Baseline => 0.,
                        PathTextVertical::Ascender => 9.,
                        PathTextVertical::Descender => -3.,
                        PathTextVertical::Center => 3.,
                    };
                letter(A, -5., y, 1., upright, 0.);
                letter(G, 5., y, 1., upright, 0.);
            }
            Self::Placement(Placement::Along(align)) => {
                let arc = |x: f64| 16. + 0.014 * x * x;
                painter.add(Shape::line(
                    (-6..=6)
                        .map(|i| {
                            let x = f64::from(i) * 4.;
                            at(x, arc(x))
                        })
                        .collect(),
                    guide_stroke,
                ));
                let center = match align {
                    TextAlign::Center => 0.,
                    TextAlign::Right => 14.,
                    _ => -14.,
                };
                for (glyph, x) in [(A, center - 6.5), (B, center), (C, center + 6.5)] {
                    letter(glyph, x, arc(x), 0.78, upright, 0.);
                }
            }
            Self::Placement(placement) => {
                let bottom = placement == Placement::Bottom;
                painter.add(Shape::closed_line(
                    (0..24)
                        .map(|i| {
                            let t = f64::from(i) * std::f64::consts::TAU / 24.;
                            at(23. * t.cos(), 18. + 8. * t.sin())
                        })
                        .collect(),
                    guide_stroke,
                ));
                for (glyph, x) in [(A, -7.), (B, 0.), (C, 7.)] {
                    let dy = 8. * (1.0_f64 - x * x / (23. * 23.)).sqrt();
                    let y = if bottom { 18. + dy } else { 18. - dy };
                    letter(glyph, x, y, 0.85, upright, 0.);
                }
            }
        }
    }
}

/// An illustrated drop-down list, returning the item when it changes.
fn choice<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    tip: &str,
    current: T,
    items: &[(T, &str)],
    preview: impl Fn(T) -> Preview,
) -> Option<T> {
    let mut picked = None;
    let label = items
        .iter()
        .find(|(v, _)| *v == current)
        .map(|(_, k)| tr(k))
        .unwrap_or_default();
    let current_preview = preview(current);
    crate::ui::keynav::ComboBox::from_id_salt(id)
        .width(98.0)
        .selected_text(label.as_str())
        .show_preview(
            ui,
            tip,
            |p, rect, ink| current_preview.paint(p, rect, ink),
            |ui| {
                for &(value, key) in items {
                    let selected = value == current;
                    let row = ui.add_sized(
                        vec2(ui.available_width(), 34.0),
                        Button::selectable(selected, ""),
                    );
                    let label = tr(key);
                    crate::ui::keynav::name_illustrated_row(&row, label.clone(), selected);
                    let ink = if selected {
                        ui.visuals().selection.stroke.color
                    } else {
                        ui.style().interact(&row).text_color()
                    };
                    if ui.is_rect_visible(row.rect) {
                        preview(value).paint(ui.painter(), row.rect.shrink(2.0), ink);
                    }
                    if row.rich_tip(label).clicked() && !selected {
                        picked = Some(value);
                    }
                }
            },
        )
        .response
        .rich_tip(format!("{tip}\n{label}"));
    picked
}

pub fn text_path_bar(app: &mut App, ui: &mut Ui) {
    let Some(ft) = app.fitted_text() else {
        return;
    };
    let tp = ft.on_path.clone();
    if let Some(o) = choice(
        ui,
        "path_orientation",
        &tr("toolbar.path_orientation"),
        tp.orientation,
        &ORIENTATIONS,
        Preview::Orientation,
    ) {
        app.edit_fitted_text(|t, _| t.orientation = o);
    }
    if let Some(v) = choice(
        ui,
        "path_vertical",
        &tr("toolbar.path_vertical"),
        tp.vertical,
        &VERTICALS,
        Preview::Vertical,
    ) {
        app.edit_fitted_text(|t, _| t.vertical = v);
    }
    let along = match ft.align {
        TextAlign::Center | TextAlign::Right => ft.align,
        _ => TextAlign::Left,
    };
    let current = match app.fitted_text_round_placement() {
        Some(false) => Placement::Top,
        Some(true) => Placement::Bottom,
        None => Placement::Along(along),
    };
    // Top and bottom only on a closed path.
    let shown = if app.fitted_text_closed() {
        &PLACEMENTS[..]
    } else {
        &PLACEMENTS[..3]
    };
    match choice(
        ui,
        "path_placement",
        &tr("toolbar.path_placement"),
        current,
        shown,
        Preview::Placement,
    ) {
        Some(Placement::Along(a)) => app.edit_fitted_text(|_, align| *align = a),
        Some(Placement::Top) => app.place_fitted_text_round(false),
        Some(Placement::Bottom) => app.place_fitted_text_round(true),
        None => {}
    }
    sep(ui);
    let u = app.units;
    for (key, value, is_distance) in [
        ("toolbar.path_distance", tp.distance, true),
        ("toolbar.path_offset", tp.offset, false),
    ] {
        let mut v = u.from_mm(value);
        let r = ui
            .add(
                NumField::new(&mut v)
                    .speed(0.1)
                    .suffix(format!(" {}", u.short())),
            )
            .rich_tip(tr(key));
        if r.changed() {
            let mm = u.to_mm(v);
            app.edit_fitted_text(|t, _| {
                if is_distance {
                    t.set_distance(mm);
                } else {
                    t.offset = mm;
                }
            });
        }
    }
    // Tick snapping: a drag off the path moves the text in steps.
    let snap = app.settings.path_tick_snap;
    if ui
        .iselectable(snap, "toolbar.path_tick_snap")
        .rich_tip(tr("toolbar.path_tick_snap_tip"))
        .clicked()
    {
        app.settings.path_tick_snap = !snap;
    }
    let mut step = u.from_mm(app.settings.path_tick_mm);
    let r = ui
        .add_enabled(
            snap,
            NumField::new(&mut step)
                .speed(0.1)
                .suffix(format!(" {}", u.short())),
        )
        .rich_tip(tr("toolbar.path_tick_spacing"));
    if r.changed() {
        let mm = u.to_mm(step);
        if mm.is_finite() && mm > 0.0 {
            app.settings.path_tick_mm = mm.min(1000.0);
        }
    }
    sep(ui);
    let (h, v) = tp.mirrors();
    if pic_button(ui, Pic::MirrorH, &tr("toolbar.mirror_h"), true, h).clicked() {
        app.mirror_fitted_text(!h, v);
    }
    if pic_button(ui, Pic::MirrorV, &tr("toolbar.mirror_v"), true, v).clicked() {
        app.mirror_fitted_text(h, !v);
    }
    sep(ui);
    // Follow Path (#3571): the text fitted again whenever its path
    // changes. Shown while the path it was fitted to is in the drawing.
    if let Some(link) = app.fitted_text_link() {
        if ui
            .iselectable(link.follow, "toolbar.path_follow")
            .rich_tip(tr("toolbar.path_follow_tip"))
            .clicked()
        {
            app.set_text_follows_path(!link.follow);
        }
    }
    if ui
        .ibutton("menu.text.straighten_text")
        .rich_tip(tr("toolbar.path_straighten_tip"))
        .clicked()
    {
        app.straighten_text();
    }
}

#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::i18n::tr;
    use tracedraw_core::document::ShapeKind;
    use tracedraw_core::geometry::{Point, Rect};

    /// The texts drawn on the bar and the values of its illustrated lists.
    fn bar_texts(app: &mut App) -> Vec<String> {
        let ctx = crate::theme::ui_context();
        ctx.enable_accesskit();
        let mut texts = Vec::new();
        let mut list_values = Vec::new();
        // Twice: the first frame only lays out the bar.
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(6000.0, 60.0),
                    )),
                    ..Default::default()
                },
                |ui| crate::ui::toolbar::property_bar(app, ui),
            );
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::epaint::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect();
            if let Some(update) = out.platform_output.accesskit_update {
                for (_, node) in update.nodes {
                    if node.role() == egui::accesskit::Role::ComboBox {
                        list_values.extend(node.value().map(str::to_owned));
                    }
                }
            }
        }
        texts.extend(list_values);
        texts
    }

    #[test]
    fn an_illustrated_list_shows_named_previews_and_picks_with_keys() {
        use super::{choice, Preview, ORIENTATIONS};
        use crate::ui::keynav;
        use egui::{Event, Key, Modifiers};
        use tracedraw_core::document::PathTextOrientation;

        let ctx = crate::theme::ui_context();
        let mut orientation = PathTextOrientation::Upright;
        let mut combo_id = egui::Id::NULL;
        let mut time = 0.0;
        let mut run = |event, orientation: &mut PathTextOrientation, focus| {
            if focus {
                ctx.memory_mut(|m| m.request_focus(combo_id));
            }
            let mut events = event;
            let mut line_count = 0;
            let mut fills = Vec::new();
            for _ in 0..5 {
                time += 0.1;
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events: std::mem::take(&mut events),
                        time: Some(time),
                        ..Default::default()
                    },
                    |ui| {
                        keynav::frame(ui.ctx(), false);
                        egui::Area::new(egui::Id::new("path_preview_test"))
                            .fixed_pos(egui::pos2(50.0, 50.0))
                            .show(ui.ctx(), |ui| {
                                combo_id = ui
                                    .make_persistent_id(egui::IdSalt::new("path_orientation_test"));
                                if let Some(next) = choice(
                                    ui,
                                    "path_orientation_test",
                                    &tr("toolbar.path_orientation"),
                                    *orientation,
                                    &ORIENTATIONS,
                                    Preview::Orientation,
                                ) {
                                    *orientation = next;
                                }
                            });
                        keynav::end_frame(ui.ctx());
                    },
                );
                out.textures_delta.clear();
                line_count = out
                    .shapes
                    .iter()
                    .filter(|shape| matches!(&shape.shape, egui::Shape::Path(_)))
                    .count();
                fills = out
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) => Some(rect.fill),
                        _ => None,
                    })
                    .collect();
            }
            (line_count, fills)
        };
        let press = |key| {
            vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]
        };
        let (closed, fills) = run(Vec::new(), &mut orientation, false);
        assert!(closed >= 4, "the current option is drawn in the button");
        assert!(fills.contains(&crate::theme::Tokens::control_fill()));
        assert!(fills.contains(&crate::theme::Tokens::button()));
        let (open, _) = run(press(Key::ArrowDown), &mut orientation, true);
        assert!(
            open >= closed + 4 * ORIENTATIONS.len(),
            "every row has a diagram, with {open} paths open and {closed} closed"
        );
        assert_eq!(keynav::highlighted(&ctx), Some(tr("toolbar.path_upright")));
        run(press(Key::Home), &mut orientation, false);
        assert_eq!(keynav::highlighted(&ctx), Some(tr("toolbar.path_rotate")));
        assert_eq!(orientation, PathTextOrientation::Upright);
        run(press(Key::Enter), &mut orientation, false);
        assert_eq!(orientation, PathTextOrientation::Rotate);
    }

    /// With the Shape tool on a text, the bar has the text fields (bold,
    /// italic and underline among them) before the character offsets;
    /// it had the offsets alone, so the font of a text or of its chosen
    /// characters could not be set there (#2365). A rectangle shows none.
    #[test]
    fn the_shape_tool_shows_the_text_fields_on_a_text() {
        tracedraw_text::install();
        let mut app = App::headless();
        app.start_text(Point::new(10.0, 20.0), None);
        app.text_insert("Words");
        app.finish_text();
        let t = app.selection[0];
        app.tool = crate::tools::Tool::Shape;
        app.selection = vec![t];
        let texts = bar_texts(&mut app);
        for b in ["B", "I", "U"] {
            assert!(texts.iter().any(|s| s == b), "{b}: {texts:?}");
        }
        let r = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.tool = crate::tools::Tool::Shape;
        app.selection = vec![r];
        let texts = bar_texts(&mut app);
        assert!(!texts.iter().any(|s| s == "B"), "{texts:?}");
    }

    /// With the Shape tool, the bar reads the format of the characters
    /// chosen: Bold shows on for a bold one and off for another, and a
    /// second click on Bold takes the bold off again. The bar kept what
    /// it had read before, so neither showed and the second click did
    /// nothing (#2365).
    #[test]
    fn the_shape_tool_bar_reads_and_undoes_the_chosen_characters_bold() {
        tracedraw_text::install();
        let mut app = App::headless();
        app.start_text(Point::new(10.0, 20.0), None);
        app.text_insert("abc");
        app.finish_text();
        let t = app.selection[0];
        app.tool = crate::tools::Tool::Shape;
        app.selection = vec![t];
        let bold_of = |app: &App| match &app.doc().find_shape(t).expect("text").kind {
            ShapeKind::Text { spans, .. } => spans
                .iter()
                .flat_map(|sp| sp.text.chars().map(move |_| sp.bold))
                .collect::<Vec<_>>(),
            _ => panic!("text"),
        };
        // Bold on the middle character, from the bar.
        app.node_selection = vec![(t, 1)];
        bar_texts(&mut app);
        assert!(!app.text_bold);
        app.text_bold = true;
        app.apply_text_style();
        assert_eq!(bold_of(&app), vec![false, true, false]);
        // Another character chosen: the bar shows it is not bold; the
        // bold one again: it is.
        app.node_selection = vec![(t, 0)];
        bar_texts(&mut app);
        assert!(!app.text_bold);
        app.node_selection = vec![(t, 1)];
        bar_texts(&mut app);
        assert!(app.text_bold);
        // A second click on Bold takes it off.
        app.text_bold = false;
        app.apply_text_style();
        assert_eq!(bold_of(&app), vec![false, false, false]);
        bar_texts(&mut app);
        assert!(!app.text_bold);
    }

    /// With the Pick tool, a selected text fitted to a path shows how it
    /// follows the path (the placement list with Top and Bottom, Straighten
    /// Text) as well as the text fields; a plain text shows no path fields.
    /// The text arm came first and hid them, so text could not be put at
    /// the bottom of a circle reading left to right (#193).
    #[test]
    fn a_fitted_text_shows_the_path_fields_in_the_property_bar() {
        tracedraw_text::install();
        let mut app = App::headless();
        app.tool = crate::tools::Tool::Pick;
        let e = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(40.0, 100.0, 140.0, 160.0),
                arc: None,
            })
            .expect("ellipse");
        app.start_text(Point::new(10.0, 20.0), None);
        app.text_insert("COCHIN");
        app.finish_text();
        let t = app.selection[0];
        app.selection = vec![t];
        let straighten = tr("menu.text.straighten_text");
        let plain = bar_texts(&mut app);
        assert!(!plain.contains(&straighten), "{plain:?}");
        app.selection = vec![t, e];
        app.fit_text_to_path();
        app.selection = vec![t];
        assert!(app.fitted_text().is_some());
        let fitted = bar_texts(&mut app);
        assert!(fitted.contains(&straighten), "{fitted:?}");
        // Follow Path is there, off, for a text fitted to a path that is
        // in the drawing (#3571).
        assert!(fitted.contains(&tr("toolbar.path_follow")), "{fitted:?}");
        assert!(!app.fitted_text_link().expect("link").follow);
        // The placement list shows where the text is: Top after fitting.
        assert!(fitted.contains(&tr("toolbar.path_place_top")), "{fitted:?}");
        // Bottom from the list: centred there, both mirrors, and the bar
        // says so.
        app.place_fitted_text_round(true);
        assert_eq!(app.fitted_text_round_placement(), Some(true));
        let bottom = bar_texts(&mut app);
        assert!(
            bottom.contains(&tr("toolbar.path_place_bottom")),
            "{bottom:?}"
        );
    }
}

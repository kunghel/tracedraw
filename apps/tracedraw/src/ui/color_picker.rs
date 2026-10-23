//! The colour picker of the Properties docker's colour rows: a swatch
//! that opens a popup beside it with the colour viewer (saturation and
//! brightness square, hue strip) and the sliders of the colour's own
//! model. The popup opens next to the swatch, on its right when there is
//! room and else on its left, and always stays inside the window. The
//! colour keeps its model: a CMYK colour is edited in CMYK, an RGB one in
//! RGB, and a Model list switches it on demand (the colour converts and
//! the object holds a colour of the new model). A click outside the
//! popup or Esc closes it.

use crate::i18n::tr;
use crate::theme::Tokens;
use egui::{Color32, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use tracedraw_core::Color;

/// Gap between the swatch and the popup, and the popup's margin from the
/// window edges (points).
const GAP: f32 = 4.0;
const MARGIN: f32 = 4.0;
/// The popup's size before it has been measured once.
const FIRST_SIZE: Vec2 = Vec2::new(250.0, 330.0);
/// The width the popup's contents get.
const CONTENT_WIDTH: f32 = 232.0;

/// What a picker keeps while it is open.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Open {
    /// The hue the viewer shows, kept while the colour has none (greys).
    hue: f32,
    /// The popup's size, as measured on the frame before.
    size: Vec2,
}

/// The swatch and popup of the picker shown last, for checks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown {
    pub swatch: Rect,
    pub popup: Rect,
}

fn last_id() -> Id {
    Id::new("tracedraw.color_picker.last")
}

/// The picker that was open on the last frame (its swatch and popup).
#[cfg(test)]
pub fn last_shown(ctx: &egui::Context) -> Option<Shown> {
    ctx.data(|d| d.get_temp::<Shown>(last_id()))
}

/// Where a popup of `size` opens beside the swatch `anchor` inside the
/// window `screen`: to the right of the swatch when it fits there, else
/// to its left, else as close as the window allows; its top level with
/// the swatch's, moved up or down to stay inside the window.
pub fn place(anchor: Rect, size: Vec2, screen: Rect) -> Rect {
    let lo_x = screen.left() + MARGIN;
    let hi_x = (screen.right() - MARGIN - size.x).max(lo_x);
    let right = anchor.right() + GAP;
    let left = anchor.left() - GAP - size.x;
    let x = if right <= hi_x {
        right
    } else if left >= lo_x {
        left
    } else {
        right.clamp(lo_x, hi_x)
    };
    let lo_y = screen.top() + MARGIN;
    let hi_y = (screen.bottom() - MARGIN - size.y).max(lo_y);
    let y = anchor.top().clamp(lo_y, hi_y);
    Rect::from_min_size(Pos2::new(x, y), size)
}

/// The model a colour is edited in: its own, or CMYK for a spot ink or
/// the registration colour (which have no sliders of their own).
pub fn edit_model(c: Color) -> &'static str {
    match c {
        Color::Spot { .. } | Color::Registration => "CMYK",
        other => other.model_name(),
    }
}

/// A colour chosen in the viewer (hue, saturation, brightness) as a
/// colour of `model`.
pub fn from_viewer(h: f32, s: f32, b: f32, model: &str) -> Color {
    Color::Hsb { h, s, b }.convert_to(model)
}

/// Whether the popup closes: on Esc, or on a press outside it and its
/// swatch. While its Model list is open a press is not outside: the list
/// opens below the popup and reaches past it, and a press on its lower
/// entries (Lab, Grayscale, YIQ) is a choice (#440).
fn closes(pressed_at: Option<Pos2>, esc: bool, popup: Rect, swatch: Rect, list_open: bool) -> bool {
    esc || (!list_open && pressed_at.is_some_and(|p| !popup.contains(p) && !swatch.contains(p)))
}

/// A colour swatch that opens the picker. True when the colour changed.
pub fn color_button(ui: &mut Ui, c: &mut Color) -> bool {
    let (swatch, resp) = ui.allocate_exact_size(Vec2::new(40.0, 18.0), Sense::click());
    let id = resp.id.with("color_picker");
    let [r, g, b] = c.to_rgb8();
    ui.painter()
        .rect_filled(swatch.shrink(1.0), 2.0, Color32::from_rgb(r, g, b));
    ui.painter().rect_stroke(
        swatch,
        2.0,
        Stroke::new(1.0, Tokens::control_border()),
        egui::StrokeKind::Inside,
    );
    let mut open = ui.data(|d| d.get_temp::<Open>(id));
    if resp.clicked() {
        open = match open {
            Some(_) => None,
            None => Some(Open {
                hue: c.to_hsb().0 as f32,
                size: FIRST_SIZE,
            }),
        };
    }
    let mut changed = false;
    if let Some(mut st) = open {
        let screen = ui.ctx().content_rect();
        let rect = place(swatch, st.size, screen);
        let model = edit_model(*c);
        let area = egui::Area::new(id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .constrain(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(CONTENT_WIDTH);
                    if let Some((h, s, b)) = crate::ui::color_docker::viewer(ui, st.hue, *c) {
                        st.hue = h;
                        *c = from_viewer(h, s, b, model);
                        changed = true;
                    }
                    ui.add_space(6.0);
                    // The model the colour is kept in, changed on demand:
                    // the colour converts, and from then on the object
                    // holds a colour of that model.
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr("docker.model"))
                                .color(Tokens::text_dim())
                                .size(11.0),
                        );
                        egui::ComboBox::from_id_salt(id.with("model"))
                            .selected_text(model)
                            .width(96.0)
                            .show_ui(ui, |ui| {
                                for m in crate::ui::color_docker::MODELS {
                                    if ui.selectable_label(model == m, m).clicked() && model != m {
                                        *c = c.convert_to(m);
                                        changed = true;
                                    }
                                }
                            });
                    });
                    let model = edit_model(*c);
                    let mut m = c.convert_to(model);
                    if crate::ui::color_docker::sliders(ui, &mut m) {
                        *c = m;
                        changed = true;
                    }
                    ui.label(
                        egui::RichText::new(crate::app::color_description(*c))
                            .color(Tokens::text_dim())
                            .size(11.0),
                    );
                });
            });
        let popup = area.response.rect;
        st.size = popup.size();
        // A press outside the popup and its swatch, or Esc, closes it.
        let (pressed_at, esc) = ui.input(|i| {
            (
                i.pointer
                    .any_pressed()
                    .then(|| i.pointer.interact_pos())
                    .flatten(),
                i.key_pressed(egui::Key::Escape),
            )
        });
        let list_open = egui::Popup::is_any_open(ui.ctx());
        open = (!closes(pressed_at, esc, popup, swatch, list_open)).then_some(st);
        ui.data_mut(|d| {
            d.insert_temp(last_id(), Shown { swatch, popup });
        });
    }
    ui.data_mut(|d| match open {
        Some(st) => {
            d.insert_temp(id, st);
        }
        None => d.remove::<Open>(id),
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))
    }

    #[test]
    fn the_popup_opens_beside_its_swatch_inside_the_window() {
        let size = Vec2::new(250.0, 330.0);
        // Room on the right: it opens there, level with the swatch.
        let swatch = Rect::from_min_size(Pos2::new(300.0, 200.0), Vec2::new(40.0, 18.0));
        let r = place(swatch, size, screen());
        assert_eq!(r.min, Pos2::new(344.0, 200.0));
        // A swatch near the right edge (a docker there): on its left,
        // never past the window's edge.
        let swatch = Rect::from_min_size(Pos2::new(1300.0, 200.0), Vec2::new(40.0, 18.0));
        let r = place(swatch, size, screen());
        assert_eq!(r.max.x, 1296.0);
        assert!(r.right() <= swatch.left());
        assert!(screen().contains_rect(r));
        // Near the bottom: moved up into the window, still beside it (it
        // covers nothing below the swatch's own row on its column).
        let swatch = Rect::from_min_size(Pos2::new(1300.0, 800.0), Vec2::new(40.0, 18.0));
        let r = place(swatch, size, screen());
        assert!(screen().contains_rect(r));
        assert!(r.right() <= swatch.left());
        assert_eq!(r.bottom(), 896.0);
        // A window too narrow for either side: kept inside it.
        let small = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 900.0));
        let swatch = Rect::from_min_size(Pos2::new(150.0, 100.0), Vec2::new(40.0, 18.0));
        let r = place(swatch, size, small);
        assert!(r.left() >= 4.0 && r.right() <= 296.0, "{r:?}");
    }

    /// The popup's model list offers every model the Colour docker does,
    /// and a colour switched to one of them is kept in it from then on,
    /// edited there (#286).
    #[test]
    fn the_model_can_be_changed_on_demand() {
        let red = Color::rgb8(220, 30, 30);
        for m in crate::ui::color_docker::MODELS {
            let c = red.convert_to(m);
            assert_eq!(c.model_name(), m);
            assert_eq!(edit_model(c), m, "edited in the model chosen");
            // Still the same colour to the eye (Grayscale keeps the
            // lightness only, by its nature).
            let [r, g, b] = c.to_rgb8();
            if m == "Grayscale" {
                assert!(r == g && g == b && r > 60 && r < 160, "{m}: {r} {g} {b}");
            } else {
                assert!(
                    (r as i32 - 220).abs() < 8
                        && (g as i32 - 30).abs() < 8
                        && (b as i32 - 30).abs() < 8,
                    "{m}: {r} {g} {b}"
                );
            }
        }
        assert!(crate::ui::color_docker::MODELS.contains(&"CMYK"));
        assert!(crate::ui::color_docker::MODELS.contains(&"RGB"));
    }

    #[test]
    fn a_colour_is_edited_in_its_own_model() {
        let red_cmyk = Color::cmyk_pct(0.0, 100.0, 100.0, 0.0);
        assert_eq!(edit_model(red_cmyk), "CMYK");
        let (h, s, b) = red_cmyk.to_hsb();
        let c = from_viewer(h as f32, s as f32 * 0.5, b as f32, edit_model(red_cmyk));
        assert!(matches!(c, Color::Cmyk { .. }), "{c:?}");
        let rgb = Color::rgb8(10, 20, 30);
        assert!(matches!(
            from_viewer(10.0, 0.5, 0.5, edit_model(rgb)),
            Color::Rgb { .. }
        ));
        assert_eq!(edit_model(Color::Registration), "CMYK");
    }

    /// A press below the popup closes it, unless the Model list is open
    /// there: its lower entries reach past the popup (#440).
    #[test]
    fn a_choice_in_the_model_list_keeps_the_popup_open() {
        let popup = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(250.0, 330.0));
        let swatch = Rect::from_min_size(Pos2::new(50.0, 100.0), Vec2::new(40.0, 18.0));
        let below = Some(Pos2::new(150.0, 450.0));
        assert!(closes(below, false, popup, swatch, false));
        assert!(!closes(below, false, popup, swatch, true));
        assert!(!closes(
            Some(Pos2::new(150.0, 200.0)),
            false,
            popup,
            swatch,
            false
        ));
        assert!(closes(None, true, popup, swatch, true), "Esc always closes");
    }

    /// A click on a swatch near the window's right edge opens the popup
    /// on its left, inside the window, and the swatch's colour stays CMYK.
    #[test]
    fn a_swatch_click_opens_the_popup_inside_the_window() {
        let ctx = crate::theme::ui_context();
        let mut c = Color::cmyk_pct(0.0, 100.0, 100.0, 0.0);
        let at = Pos2::new(1340.0, 209.0);
        let run = |ctx: &egui::Context, c: &mut Color, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen()),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Area::new(Id::new("swatch_area"))
                    .fixed_pos(Pos2::new(1320.0, 200.0))
                    .show(ui.ctx(), |ui| color_button(ui, c));
            });
            out.textures_delta.clear();
        };
        run(&ctx, &mut c, Vec::new());
        run(&ctx, &mut c, vec![egui::Event::PointerMoved(at)]);
        run(&ctx, &mut c, vec![egui::Event::PointerMoved(at)]);
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(&ctx, &mut c, vec![press(true)]);
        run(&ctx, &mut c, vec![press(false)]);
        run(&ctx, &mut c, Vec::new());
        let shown = last_shown(&ctx).expect("the popup is open");
        assert!(screen().contains_rect(shown.popup), "{shown:?}");
        assert!(shown.popup.right() <= shown.swatch.left(), "{shown:?}");
        assert!(matches!(c, Color::Cmyk { .. }));
    }
}

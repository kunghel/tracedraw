//! Tools > Replace Color (#2858): one colour of the drawing changed for
//! another wherever it is used (fills, gradient stops, outlines, text,
//! effects, symbols), in one undo step (`Command::ReplaceColors`). The
//! colour to find is picked from the colours the drawing uses, so an RGB
//! colour can be swapped for its CMYK version (or any other) in the whole
//! drawing at once.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::ui::dialogs::window;
use egui::{Color32, Context, RichText, Sense, Stroke, Vec2};
use tracedraw_core::{Color, Command};

/// The dialog's values.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceColorState {
    /// The colour looked for (one the drawing uses).
    pub find: Option<Color>,
    /// The colour put in its place.
    pub with: Color,
}

impl ReplaceColorState {
    /// The colour of the first selected object (its fill, or else its
    /// outline) to find, when it has one; else nothing picked yet.
    pub fn new(app: &App) -> Self {
        let find = app.selected_shapes().iter().find_map(|s| {
            match &s.fill {
                tracedraw_core::Fill::Solid(c) => Some(*c),
                _ => None,
            }
            .or_else(|| s.stroke.as_ref().map(|st| st.color))
        });
        ReplaceColorState {
            find,
            with: find.unwrap_or(Color::Cmyk {
                c: 0.0,
                m: 0.0,
                y: 0.0,
                k: 1.0,
            }),
        }
    }
}

impl App {
    /// Tools > Replace Color: its dialog.
    pub fn replace_color_dialog(&mut self) {
        if !self.has_document() {
            return;
        }
        self.dialog = crate::ui::dialogs::Dialog::ReplaceColor(ReplaceColorState::new(self));
    }

    /// Every use of `find` in the drawing changed to `with`, one undo
    /// step; the status line says how many changed. False when none did.
    pub fn replace_color(&mut self, find: Color, with: Color) -> bool {
        if find == with || !self.has_document() {
            return false;
        }
        let pairs = vec![(find, with)];
        // How many uses change, on a copy (the drawing's data is shared).
        let mut copy = self.doc().clone();
        let n = tracedraw_core::color_convert::replace_colors(&mut copy, &pairs);
        if n == 0 {
            self.status = tr("replace_color.none");
            return false;
        }
        if let Err(e) = self
            .engine
            .run_batch("Replace Color", &[Command::ReplaceColors { pairs }])
        {
            self.status = e.to_string();
            return false;
        }
        self.status = trf("replace_color.done", &[("n", &n.to_string())]);
        true
    }
}

/// A swatch of `c` that can be clicked; framed when `picked`.
fn swatch(ui: &mut egui::Ui, c: Color, picked: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::click());
    let [r, g, b] = c.to_rgb8();
    ui.painter()
        .rect_filled(rect.shrink(2.0), 2.0, Color32::from_rgb(r, g, b));
    let stroke = if picked {
        Stroke::new(2.0, ui.visuals().selection.stroke.color)
    } else {
        Stroke::new(1.0, crate::theme::Tokens::control_border())
    };
    ui.painter()
        .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    resp.on_hover_text(crate::ui::palette::swatch_tip("", c))
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut ReplaceColorState, close: &mut bool) {
    let mut help = false;
    let mut ok = false;
    let used = crate::color_styles::document_colors(app.doc());
    if st.find.is_some_and(|c| !used.contains(&c)) {
        st.find = None;
    }
    window(ctx, tr("replace_color.title")).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.strong(tr("replace_color.find"));
        if used.is_empty() {
            ui.label(RichText::new(tr("replace_color.no_colors")).weak());
        }
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(2.0);
                    for c in &used {
                        if swatch(ui, *c, st.find == Some(*c)).clicked() {
                            st.find = Some(*c);
                        }
                    }
                });
            });
        if let Some(c) = st.find {
            ui.label(RichText::new(crate::ui::palette::swatch_tip("", c)).weak());
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.strong(tr("replace_color.with"));
            crate::ui::color_picker::color_button(ui, &mut st.with);
        });
        ui.label(RichText::new(crate::ui::palette::swatch_tip("", st.with)).weak());
        ui.add_space(4.0);
        ui.label(RichText::new(tr("replace_color.note")).weak());
        let (o, h) = crate::ui::dialogs::help_ok_cancel(ui, close);
        ok = o;
        help = h;
    });
    if ok {
        if let Some(find) = st.find {
            app.replace_color(find, st.with);
        }
    }
    if help {
        app.open_dialog_help();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::Dialog;
    use tracedraw_core::{Fill, ShapeKind};

    fn rect(app: &mut App, c: Color) -> tracedraw_core::ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(c),
        });
        app.run(Command::SetStroke {
            shapes: vec![id],
            stroke: None,
        });
        id
    }

    fn fill_of(app: &App, id: tracedraw_core::ShapeId) -> Color {
        match app.doc().find_shape(id).map(|s| s.fill.clone()) {
            Some(Fill::Solid(c)) => c,
            _ => panic!("solid"),
        }
    }

    /// An RGB colour used by several objects becomes a CMYK colour in all
    /// of them at once, selected or not; other colours stay; one undo
    /// step takes it back (#2858).
    #[test]
    fn a_colour_is_replaced_in_the_whole_drawing_in_one_step() {
        let mut app = App::headless();
        app.show_welcome = false;
        let red = Color::rgb8(220, 20, 20);
        let blue = Color::rgb8(20, 20, 220);
        let cmyk = Color::Cmyk {
            c: 0.0,
            m: 1.0,
            y: 1.0,
            k: 0.0,
        };
        let a = rect(&mut app, red);
        let b = rect(&mut app, red);
        let c = rect(&mut app, blue);
        app.select(vec![a]);
        app.replace_color_dialog();
        let Dialog::ReplaceColor(st) = &app.dialog else {
            panic!("open");
        };
        assert_eq!(st.find, Some(red), "the selected object's colour is found");
        assert!(app.replace_color(red, cmyk));
        assert_eq!(fill_of(&app, a), cmyk);
        assert_eq!(fill_of(&app, b), cmyk);
        assert_eq!(fill_of(&app, c), blue);
        app.undo();
        assert_eq!(fill_of(&app, a), red);
        assert_eq!(fill_of(&app, b), red);
        // A colour nobody uses changes nothing and says so.
        assert!(!app.replace_color(Color::rgb8(1, 2, 3), cmyk));
        assert_eq!(app.status, tr("replace_color.none"));
    }
}

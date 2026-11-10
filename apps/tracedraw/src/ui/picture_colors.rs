//! A picture dragged from the drawing and let go on a palette bar (#2137):
//! a small window asks how many colours to take from it (a slider with
//! its number box), shows them, and OK adds them to the palette it was
//! dropped on, the colours it already has skipped. Cancel adds nothing.
//!
//! The colours are those of Add Colors from Bitmap
//! ([`crate::ops2::picture_colors_from_sample`]): the picture is read
//! once, when it is dropped, and the colours are worked out again only
//! when the number changes, never every frame. A built-in or locked
//! palette takes no colours: OK then makes a new palette of the user's
//! with them. See docs/behavior/color-palettes.md.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::palette::Row;
use crate::ui::tips::RichTip;
use egui::{Context, Pos2};
use std::sync::Arc;
use tracedraw_core::Color;

/// How many colours the window offers at first, and its range.
pub const DEFAULT_COUNT: u32 = 8;
pub const MIN_COUNT: u32 = 2;
pub const MAX_COUNT: u32 = 64;

/// The palette a picture was dropped on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteTarget {
    /// A palette of [`App::palettes`] (the default palette's bar too).
    Palette(usize),
    /// The document palette (one undo step).
    Document,
}

/// The window's state: where the colours go, how many, and the colours
/// for that number, kept until it changes.
#[derive(Debug, Clone, PartialEq)]
pub struct PictureColorsState {
    pub target: PaletteTarget,
    pub count: u32,
    /// The picture's pixels the colours are chosen from, read once.
    sample: Arc<Vec<[u8; 3]>>,
    /// The colours for `shown_for` colours.
    colors: Vec<Color>,
    shown_for: u32,
    /// How many times the colours were worked out (a check that they are
    /// not every frame).
    pub computed: u32,
}

impl PictureColorsState {
    pub fn new(target: PaletteTarget, sample: Vec<[u8; 3]>) -> Self {
        let mut st = PictureColorsState {
            target,
            count: DEFAULT_COUNT,
            sample: Arc::new(sample),
            colors: Vec::new(),
            shown_for: 0,
            computed: 0,
        };
        st.colors();
        st
    }

    /// The colours for [`Self::count`], worked out again only when the
    /// count changed since the last time.
    pub fn colors(&mut self) -> &[Color] {
        self.count = self.count.clamp(MIN_COUNT, MAX_COUNT);
        if self.shown_for != self.count {
            self.colors = crate::ops2::picture_colors_from_sample(&self.sample, self.count);
            self.shown_for = self.count;
            self.computed += 1;
        }
        &self.colors
    }
}

impl App {
    /// The palette whose strip lies at `at` (screen), the strip drawn
    /// last (over the others) first.
    pub fn palette_target_at(&self, at: Pos2) -> Option<PaletteTarget> {
        let (which, _) = self
            .palette_strip_rects
            .iter()
            .rev()
            .find(|(_, r)| r.contains(at))?;
        self.palette_target_of(*which)
    }

    /// The palette a strip shows.
    pub fn palette_target_of(&self, which: Row) -> Option<PaletteTarget> {
        match which {
            Row::Main => self
                .visible_palettes
                .first()
                .map(|i| PaletteTarget::Palette(*i)),
            Row::Document => self.has_document().then_some(PaletteTarget::Document),
            Row::Extra(k) => {
                let r = self.extra_palettes.get(k)?;
                if r.document {
                    self.has_document().then_some(PaletteTarget::Document)
                } else {
                    (r.index < self.palettes.len()).then_some(PaletteTarget::Palette(r.index))
                }
            }
        }
    }

    /// The selection is one picture and the pointer is over a palette
    /// strip: a move let go now gives that palette the picture's colours
    /// instead of moving it.
    pub fn picture_drop_target(&self) -> Option<PaletteTarget> {
        if self.selection.len() != 1 || self.selected_bitmap().is_none() {
            return None;
        }
        self.pointer_screen.and_then(|p| self.palette_target_at(p))
    }

    /// Open the window that takes the selected picture's colours to
    /// `target`. False when no picture is selected or it cannot be read.
    pub fn open_picture_colors(&mut self, target: PaletteTarget) -> bool {
        let Some(sample) = self.selected_bitmap_sample() else {
            return false;
        };
        self.dialog = crate::ui::dialogs::Dialog::PictureColors(Box::new(PictureColorsState::new(
            target, sample,
        )));
        true
    }

    /// `target` takes colours itself (else OK makes a new palette).
    pub fn picture_target_takes_colors(&self, target: PaletteTarget) -> bool {
        match target {
            PaletteTarget::Document => self.has_document(),
            PaletteTarget::Palette(i) => self.palette_takes_colors(i),
        }
    }

    /// The name `target` is shown under.
    pub fn picture_target_name(&self, target: PaletteTarget) -> String {
        match target {
            PaletteTarget::Document => self.document_palette_name(),
            PaletteTarget::Palette(i) => self.palette_name(i),
        }
    }

    /// OK in the window: `colors` join `target` (those it has not got
    /// yet), or a new palette of the user's when `target` cannot be
    /// changed. Says in the status line what happened; returns how many
    /// colours were added.
    pub fn add_picture_colors(&mut self, target: PaletteTarget, colors: &[Color]) -> usize {
        let count_in = |app: &App, t: PaletteTarget| match t {
            PaletteTarget::Document if app.has_document() => app.doc().palette.len(),
            PaletteTarget::Document => 0,
            PaletteTarget::Palette(i) => app.palettes.get(i).map_or(0, |p| p.colors.len()),
        };
        let target = if self.picture_target_takes_colors(target) {
            target
        } else {
            let name = self.next_palette_name();
            match self.new_palette(&name) {
                Some(i) => {
                    self.settings.save();
                    PaletteTarget::Palette(i)
                }
                None => return 0,
            }
        };
        let before = count_in(self, target);
        match target {
            PaletteTarget::Document => {
                self.add_colors_to_document_palette(colors);
            }
            PaletteTarget::Palette(i) => {
                self.add_colors_to_palette(i, colors);
            }
        }
        let added = count_in(self, target).saturating_sub(before);
        let name = self.picture_target_name(target);
        self.status = if added == 0 {
            trf("palette.extract_none", &[("name", &name)])
        } else {
            trf(
                "palette.extract_added",
                &[("n", &added.to_string()), ("name", &name)],
            )
        };
        added
    }
}

/// The window: the number of colours, the colours, OK and Cancel.
pub fn dialog(app: &mut App, ctx: &Context, st: &mut PictureColorsState, close: &mut bool) {
    let takes = app.picture_target_takes_colors(st.target);
    let name = app.picture_target_name(st.target);
    crate::ui::dialogs::window(ctx, tr("palette.extract_title")).show(ctx, |ui| {
        ui.set_max_width(360.0);
        let note = if takes {
            trf("palette.extract_into", &[("name", &name)])
        } else {
            trf("palette.extract_new_palette", &[("name", &name)])
        };
        ui.label(note);
        ui.add_space(6.0);
        ui.add(
            crate::ui::slider::Slider::new(&mut st.count, MIN_COUNT..=MAX_COUNT)
                .label(tr("palette.extract_count"))
                .integer()
                .width(300.0),
        )
        .rich_tip(crate::ui::tips::pair(
            "palette.extract_count",
            "palette.extract_count_tip",
        ));
        ui.add_space(6.0);
        // The colours, 16 to a row; worked out only when the number
        // changed.
        let colors = st.colors().to_vec();
        let side = 18.0;
        let gap = 2.0;
        let per_row = 16usize;
        let rows = colors.len().div_ceil(per_row).max(1);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(per_row as f32 * (side + gap), rows as f32 * (side + gap)),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        for (k, c) in colors.iter().enumerate() {
            let at = rect.min
                + egui::vec2(
                    (k % per_row) as f32 * (side + gap),
                    (k / per_row) as f32 * (side + gap),
                );
            let r = egui::Rect::from_min_size(at, egui::vec2(side, side));
            painter.rect_filled(r, 0.0, crate::canvas::to_color32(*c));
            painter.rect_stroke(
                r,
                0.0,
                egui::Stroke::new(1.0, Tokens::border()),
                egui::StrokeKind::Inside,
            );
        }
        ui.label(
            egui::RichText::new(trf(
                "palette.extract_found",
                &[("n", &colors.len().to_string())],
            ))
            .color(Tokens::text_dim())
            .size(11.0),
        );
        ui.add_space(6.0);
        if crate::ui::dialogs::ok_cancel(ui, close) {
            app.add_picture_colors(st.target, &colors);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Drag;
    use tracedraw_core::geometry::Point;

    /// A drawing with a picture of four colours, selected, a palette of
    /// the user's in a bar of its own, and where its strip and the
    /// default palette's lie.
    fn app_with_picture() -> (App, usize) {
        let mut app = App::headless();
        app.show_welcome = false;
        let img = image::RgbaImage::from_fn(40, 40, |x, y| match (x < 20, y < 20) {
            (true, true) => image::Rgba([220, 20, 30, 255]),
            (false, true) => image::Rgba([20, 160, 40, 255]),
            (true, false) => image::Rgba([30, 40, 200, 255]),
            (false, false) => image::Rgba([255, 255, 255, 255]),
        });
        let id = app.place_image(img, None).expect("picture");
        app.select(vec![id]);
        let mine = app.new_palette("Mine").expect("palette");
        let k = app
            .extra_palettes
            .iter()
            .position(|r| !r.document && r.index == mine)
            .expect("its bar");
        app.palette_strip_rects = vec![
            (
                Row::Main,
                egui::Rect::from_min_size(egui::pos2(0.0, 900.0), egui::vec2(800.0, 25.0)),
            ),
            (
                Row::Extra(k),
                egui::Rect::from_min_size(egui::pos2(0.0, 870.0), egui::vec2(800.0, 25.0)),
            ),
            (
                Row::Document,
                egui::Rect::from_min_size(egui::pos2(0.0, 840.0), egui::vec2(800.0, 25.0)),
            ),
        ];
        (app, mine)
    }

    fn moving(app: &mut App, at: Pos2) {
        app.drag = Drag::Move {
            start: Point::new(20.0, 20.0),
            last: Point::new(40.0, 60.0),
            total: tracedraw_core::geometry::Vec2::new(20.0, 40.0),
            start_bounds: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
            source: None,
            start_geometry: None,
        };
        app.pointer_screen = Some(at);
    }

    /// The menu path and the drop give the same colours (one function).
    #[test]
    fn the_window_shows_the_colours_add_colors_from_bitmap_takes() {
        let (app, _) = app_with_picture();
        let sample = app.selected_bitmap_sample().expect("sample");
        let mut st = PictureColorsState::new(PaletteTarget::Document, sample);
        for n in [2, 3, 8, 16, 64] {
            st.count = n;
            let menu = app.selected_bitmap_colors(n).expect("colours");
            assert_eq!(st.colors(), menu.as_slice(), "{n}");
        }
        // Four colours in the picture: no more than four, however many
        // are asked for.
        st.count = 64;
        assert_eq!(st.colors().len(), 4);
        // Out of range: kept in it.
        st.count = 500;
        st.colors();
        assert_eq!(st.count, MAX_COUNT);
        st.count = 0;
        st.colors();
        assert_eq!(st.count, MIN_COUNT);
    }

    /// The colours are worked out when the number changes, not on every
    /// frame the window is drawn.
    #[test]
    fn the_preview_is_worked_out_only_when_the_number_changes() {
        let (mut app, mine) = app_with_picture();
        assert!(app.open_picture_colors(PaletteTarget::Palette(mine)));
        let ctx = crate::theme::ui_context();
        let frame = |app: &mut App| {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                crate::ui::dialogs::show(app, &ui.ctx().clone())
            });
            out.textures_delta.clear();
        };
        let computed = |app: &App| match &app.dialog {
            crate::ui::dialogs::Dialog::PictureColors(st) => st.computed,
            _ => panic!("the window closed"),
        };
        for _ in 0..5 {
            frame(&mut app);
        }
        assert_eq!(computed(&app), 1, "once, when it opened");
        if let crate::ui::dialogs::Dialog::PictureColors(st) = &mut app.dialog {
            st.count = 3;
        }
        for _ in 0..5 {
            frame(&mut app);
        }
        assert_eq!(computed(&app), 2, "once more for the new number");
    }

    /// A picture let go on a palette bar opens the window for that
    /// palette and stays where it was; OK adds its colours (skipping
    /// those the palette has), Cancel adds nothing.
    #[test]
    fn a_picture_dropped_on_a_palette_bar_offers_its_colours() {
        let (mut app, mine) = app_with_picture();
        let id = app.selection[0];
        let before = app.doc().find_shape(id).expect("picture").clone();
        app.palettes[mine].colors = vec![("Red".into(), Color::rgb8(220, 20, 30))];
        moving(&mut app, egui::pos2(100.0, 880.0));
        assert_eq!(
            app.picture_drop_target(),
            Some(PaletteTarget::Palette(mine))
        );
        app.end_drag();
        let crate::ui::dialogs::Dialog::PictureColors(st) = &mut app.dialog else {
            panic!("no window: {:?}", app.dialog);
        };
        assert_eq!(st.target, PaletteTarget::Palette(mine));
        assert_eq!(st.count, DEFAULT_COUNT);
        let colors = st.colors().to_vec();
        assert_eq!(colors.len(), 4);
        assert_eq!(
            app.doc().find_shape(id).expect("picture").transform,
            before.transform,
            "the picture did not move"
        );
        // Cancel: the palette as it was.
        app.dialog = crate::ui::dialogs::Dialog::None;
        assert_eq!(app.palettes[mine].colors.len(), 1);
        // OK: the three it had not got.
        assert_eq!(
            app.add_picture_colors(PaletteTarget::Palette(mine), &colors),
            3
        );
        let got: Vec<Color> = app.palettes[mine].colors.iter().map(|(_, c)| *c).collect();
        assert_eq!(got.len(), 4);
        for c in &colors {
            assert!(got.contains(c), "{c:?}");
        }
        // Again: nothing new.
        assert_eq!(
            app.add_picture_colors(PaletteTarget::Palette(mine), &colors),
            0
        );
    }

    /// The document palette takes them as one undo step.
    #[test]
    fn a_picture_dropped_on_the_document_palette_adds_one_undo_step() {
        let (mut app, _) = app_with_picture();
        moving(&mut app, egui::pos2(100.0, 850.0));
        app.end_drag();
        let crate::ui::dialogs::Dialog::PictureColors(st) = &mut app.dialog else {
            panic!("no window");
        };
        assert_eq!(st.target, PaletteTarget::Document);
        let colors = st.colors().to_vec();
        let before = app.doc().palette.clone();
        app.add_picture_colors(PaletteTarget::Document, &colors);
        for c in &colors {
            assert!(app.doc().palette.contains(c));
        }
        app.undo();
        assert_eq!(app.doc().palette, before);
    }

    /// A built-in palette is not changed: OK makes a new palette of the
    /// user's with the colours.
    #[test]
    fn a_builtin_palette_gets_a_new_palette_instead() {
        let (mut app, _) = app_with_picture();
        moving(&mut app, egui::pos2(100.0, 910.0));
        let main = app.visible_palettes[0];
        assert!(app.palettes[main].builtin);
        assert_eq!(
            app.picture_drop_target(),
            Some(PaletteTarget::Palette(main))
        );
        app.end_drag();
        let crate::ui::dialogs::Dialog::PictureColors(st) = &mut app.dialog else {
            panic!("no window");
        };
        let colors = st.colors().to_vec();
        let target = st.target;
        assert!(!app.picture_target_takes_colors(target));
        let builtin = app.palettes[main].colors.clone();
        let n = app.palettes.len();
        assert_eq!(app.add_picture_colors(target, &colors), 4);
        assert_eq!(app.palettes[main].colors, builtin, "unchanged");
        assert_eq!(app.palettes.len(), n + 1);
        let new = &app.palettes[n];
        assert!(!new.builtin);
        assert_eq!(new.colors.len(), 4);
    }

    /// Anything but one picture moved onto a palette does what it did:
    /// the objects move (or join the document palette there).
    #[test]
    fn other_objects_dropped_on_a_palette_bar_are_not_taken_as_pictures() {
        let (mut app, _) = app_with_picture();
        let rect = app
            .new_shape(tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![rect]);
        moving(&mut app, egui::pos2(100.0, 880.0));
        assert_eq!(app.picture_drop_target(), None);
        app.end_drag();
        assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::None));
        // Two objects, the picture among them: not a picture either.
        let pic = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.iter())
            .find(|s| matches!(s.kind, tracedraw_core::ShapeKind::Bitmap { .. }))
            .map(|s| s.id)
            .expect("picture");
        app.select(vec![pic, rect]);
        moving(&mut app, egui::pos2(100.0, 880.0));
        assert_eq!(app.picture_drop_target(), None);
        // Off every strip: nothing.
        app.select(vec![pic]);
        moving(&mut app, egui::pos2(100.0, 100.0));
        assert_eq!(app.picture_drop_target(), None);
    }
}

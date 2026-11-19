//! File > Import with the placement cursor (#1936): after the file is
//! chosen nothing comes in yet; the pointer becomes a corner bracket and
//! a click puts the top left corner of what the file holds there at its
//! own size, a drag gives the box it is fitted in (its proportions kept),
//! Space or Enter centres it on the page, and Esc or a right click calls
//! the import off.
//!
//! Several files picked at once (discussion #2695) come in one after
//! another, each placed in turn, and end up selected together; Esc calls
//! off the one waiting and those after it.

use crate::app::App;
use crate::i18n::tr;
use egui::{PointerButton, Response};
use tracedraw_core::geometry::{Affine, Point, Rect};
use tracedraw_core::{Command, ShapeId};

/// A chosen file waiting for its place.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPlacing {
    pub path: std::path::PathBuf,
    /// Where a drag that sizes the import started (page space).
    pub drag_from: Option<Point>,
}

/// Where an import goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Its top left corner here, at its own size.
    Corner(Point),
    /// Fitted in this box, its proportions kept, from the box's top left.
    Fit(Rect),
    /// In the middle of the page, at its own size.
    PageCenter,
}

/// A placement waiting for an import that asked a question first (how
/// to read a PDF's text, a password): it applies once the import came.
#[derive(Debug, Clone, PartialEq)]
pub struct WaitingPlacement {
    pub placement: Placement,
    pub before: Vec<ShapeId>,
}

/// Several files coming in from one File > Import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportBatch {
    /// The objects the files before the latest one brought.
    pub came: Vec<ShapeId>,
    /// The selection before the latest file started coming in.
    pub before: Vec<ShapeId>,
}

/// Smallest box side a drag sizes an import to, page millimetres: a
/// shorter drag is a click.
const MIN_FIT: f64 = 0.5;

/// The change that puts objects with bounds `b` where `placement` says,
/// on a page of bounds `page`. Page space has y up: the top left corner
/// of `b` is `(x0, y1)`.
pub fn placement_transform(b: Rect, placement: Placement, page: Rect) -> Option<Affine> {
    let top_left = Point::new(b.x0, b.y1);
    let t = match placement {
        Placement::Corner(p) => Affine::translate(p - top_left),
        Placement::PageCenter => Affine::translate(page.center() - b.center()),
        Placement::Fit(r) => {
            let (w, h) = (b.width(), b.height());
            let s = match (w > 1e-9, h > 1e-9) {
                (true, true) => (r.width() / w).min(r.height() / h),
                (true, false) => r.width() / w,
                (false, true) => r.height() / h,
                (false, false) => 1.0,
            };
            if !(s.is_finite() && s > 0.0) {
                return None;
            }
            let corner = Point::new(r.x0.min(r.x1), r.y0.max(r.y1));
            Affine::translate(corner.to_vec2())
                * Affine::scale(s)
                * Affine::translate(-top_left.to_vec2())
        }
    };
    let c = t.as_coeffs();
    c.iter().all(|v| v.is_finite()).then_some(t)
}

impl App {
    /// File > Import with a file chosen: wait for its place when Options
    /// > Global > File Formats says so, else bring it in at once.
    pub fn import_picked_many(&mut self, paths: Vec<std::path::PathBuf>) {
        let mut paths = paths.into_iter();
        let Some(first) = paths.next() else {
            return;
        };
        self.import_queue = paths.collect();
        self.import_batch = (!self.import_queue.is_empty()).then(|| ImportBatch {
            came: Vec::new(),
            before: self.selection.clone(),
        });
        self.import_picked(&first);
    }

    /// Each frame: once the file before has come in (placed, its
    /// questions answered), the next file picked with it starts; after
    /// the last, everything the files brought is selected.
    pub fn advance_import_queue(&mut self) {
        if self.import_batch.is_none()
            || self.import_placing.is_some()
            || self.import_place_waiting.is_some()
            || !matches!(self.dialog, crate::ui::dialogs::Dialog::None)
        {
            return;
        }
        let selection = self.selection.clone();
        let Some(batch) = self.import_batch.as_mut() else {
            return;
        };
        if selection != batch.before {
            for id in selection {
                if !batch.came.contains(&id) {
                    batch.came.push(id);
                }
            }
        }
        batch.before = self.selection.clone();
        match self.import_queue.pop_front() {
            Some(next) => self.import_picked(&next),
            None => {
                let Some(batch) = self.import_batch.take() else {
                    return;
                };
                let doc = self.doc();
                let came: Vec<ShapeId> = batch
                    .came
                    .into_iter()
                    .filter(|id| doc.shape_on_page(*id).is_some())
                    .collect();
                if !came.is_empty() {
                    self.select(came);
                }
            }
        }
    }

    pub fn import_picked(&mut self, path: &std::path::Path) {
        if self.settings.formats.import_with_cursor {
            self.begin_import_placement(path);
        } else {
            self.import_path(path);
        }
    }

    /// The pointer becomes the placement cursor for `path`.
    pub fn begin_import_placement(&mut self, path: &std::path::Path) {
        self.import_placing = Some(ImportPlacing {
            path: path.to_path_buf(),
            drag_from: None,
        });
        if let Some(tag) = self.import_placement_tag() {
            self.status = tag;
        }
    }

    /// What the pointer's tag and the status line say while an import
    /// waits for its place: the file's name and how to place it.
    pub fn import_placement_tag(&self) -> Option<String> {
        let placing = self.import_placing.as_ref()?;
        let name = placing
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Some(crate::i18n::trf("status.import_place", &[("name", &name)]))
    }

    /// Esc or a right click: nothing comes in.
    pub fn cancel_import_placement(&mut self) {
        if self.import_placing.take().is_some() {
            // The files picked with it are called off too; what came
            // before stays, selected together.
            self.import_queue.clear();
            self.status = tr("status.cancelled");
        }
    }

    /// The import waiting for its place comes in where `placement` says,
    /// as one undo step.
    pub fn place_import(&mut self, placement: Placement) {
        let Some(placing) = self.import_placing.take() else {
            return;
        };
        self.status.clear();
        let before = self.selection.clone();
        self.import_path_as_in_file(&placing.path);
        if self.selection != before && !self.selection.is_empty() {
            self.apply_import_placement(placement);
        } else if self.import_question_open() {
            // The import comes once the question is answered.
            self.import_place_waiting = Some(WaitingPlacement { placement, before });
        }
    }

    /// Whether `path` is read again on a later frame: the file waiting
    /// for its place, those picked with it, or a PDF whose question
    /// (how to read its text, its password) is open. A browser keeps an
    /// upload while it is (#3231).
    pub fn reads_later(&self, path: &std::path::Path) -> bool {
        use crate::ui::dialogs::Dialog;
        self.import_placing.as_ref().is_some_and(|p| p.path == path)
            || self.import_queue.iter().any(|p| p == path)
            || match &self.dialog {
                Dialog::PdfText(st) => st.path == path,
                Dialog::PdfPassword(st) => st.path == path,
                _ => false,
            }
    }

    /// Whether an import is asking a question before it reads the file.
    fn import_question_open(&self) -> bool {
        use crate::ui::dialogs::Dialog;
        match &self.dialog {
            Dialog::PdfText(st) => st.import,
            Dialog::PdfPassword(st) => st.import,
            Dialog::PdfPages(_) => true,
            _ => false,
        }
    }

    /// Each frame: an import that asked a question first goes where it
    /// was placed once it came; called off with the question, nothing
    /// moves.
    pub fn finish_waiting_import_placement(&mut self) {
        if self.import_place_waiting.is_none() || self.import_question_open() {
            return;
        }
        let Some(w) = self.import_place_waiting.take() else {
            return;
        };
        if w.before != self.selection && !self.selection.is_empty() {
            self.apply_import_placement(w.placement);
        }
    }

    /// Move (and for a fitted box, scale) the selection, just imported,
    /// in the import's own undo step.
    fn apply_import_placement(&mut self, placement: Placement) {
        let Some(b) = self.selection_bounds() else {
            return;
        };
        let Some(transform) = placement_transform(b, placement, self.page_rect()) else {
            return;
        };
        if transform == Affine::IDENTITY {
            return;
        }
        let cmd = Command::TransformShapes {
            shapes: self.selection.clone(),
            transform,
        };
        if let Err(e) = self.engine.amend(&cmd) {
            self.status = e.to_string();
        }
    }

    /// The drawing's input while an import waits for its place; true
    /// while it does (no tool sees the pointer). `p` is the pointer on
    /// the page.
    pub fn import_placement_input(&mut self, response: &Response, p: Point) -> bool {
        let Some(placing) = &mut self.import_placing else {
            return false;
        };
        if response.secondary_clicked() {
            self.cancel_import_placement();
            return true;
        }
        if response.drag_started_by(PointerButton::Primary) {
            let from = response
                .ctx
                .input(|i| i.pointer.press_origin())
                .map(|s| self.view.to_page(s))
                .unwrap_or(p);
            placing.drag_from = Some(from);
            return true;
        }
        if response.drag_stopped_by(PointerButton::Primary) {
            let Some(from) = placing.drag_from.take() else {
                return true;
            };
            let r = Rect::from_points(from, p);
            let placement = if r.width() < MIN_FIT || r.height() < MIN_FIT {
                Placement::Corner(from)
            } else {
                Placement::Fit(r)
            };
            self.place_import(placement);
            return true;
        }
        if response.clicked_by(PointerButton::Primary) {
            self.place_import(Placement::Corner(p));
        }
        true
    }

    /// The keys while an import waits for its place: Space or Enter
    /// centres it on the page, Esc calls it off. True when one was used.
    pub fn import_placement_keys(&mut self, ctx: &egui::Context) -> bool {
        use egui::{Key, Modifiers};
        if self.import_placing.is_none() {
            return false;
        }
        let key = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if key(Key::Escape) {
            self.cancel_import_placement();
            return true;
        }
        if key(Key::Space) || key(Key::Enter) {
            self.place_import(Placement::PageCenter);
            return true;
        }
        false
    }

    /// The box a drag is sizing the import to, page space.
    pub fn import_placement_box(&self) -> Option<Rect> {
        let placing = self.import_placing.as_ref()?;
        let from = placing.drag_from?;
        Some(Rect::from_points(from, self.pointer_page?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-9
    }

    #[test]
    fn a_click_puts_the_top_left_corner_there() {
        let b = Rect::new(10.0, 20.0, 30.0, 60.0);
        let page = Rect::new(0.0, 0.0, 210.0, 297.0);
        let t = placement_transform(b, Placement::Corner(Point::new(100.0, 200.0)), page)
            .expect("move");
        assert!(close(t * Point::new(10.0, 60.0), Point::new(100.0, 200.0)));
        assert!(close(t * Point::new(30.0, 20.0), Point::new(120.0, 160.0)));
    }

    #[test]
    fn a_drag_fits_the_import_in_its_box_keeping_its_proportions() {
        // 20 x 40 into a 100 x 100 box dragged from the lower right up
        // to the left: 50 x 100 from the box's top left corner.
        let b = Rect::new(10.0, 20.0, 30.0, 60.0);
        let page = Rect::new(0.0, 0.0, 210.0, 297.0);
        let r = Rect::from_points(Point::new(150.0, 100.0), Point::new(50.0, 200.0));
        let t = placement_transform(b, Placement::Fit(r), page).expect("fit");
        assert!(close(t * Point::new(10.0, 60.0), Point::new(50.0, 200.0)));
        assert!(close(t * Point::new(30.0, 20.0), Point::new(100.0, 100.0)));
    }

    #[test]
    fn space_centres_it_on_the_page() {
        let b = Rect::new(10.0, 20.0, 30.0, 60.0);
        let page = Rect::new(0.0, 0.0, 210.0, 297.0);
        let t = placement_transform(b, Placement::PageCenter, page).expect("centre");
        assert!(close(t * b.center(), page.center()));
    }
}

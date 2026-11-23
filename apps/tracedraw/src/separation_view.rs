//! View > Separations: the drawing window shows the current page as one
//! ink prints it, in that ink's colour (cyan ink in cyan), while you keep
//! drawing; Composite goes back to every ink. The drawing itself is not
//! changed: the canvas draws a copy of the page made for the ink.

use crate::app::App;
use std::rc::Rc;
use tracedraw_core::Document;

impl App {
    /// The inks the drawing prints with, as View > Separations lists
    /// them: the four process inks, then each spot ink, by name.
    pub fn separation_names(&self) -> Vec<String> {
        crate::export::plates(self.doc())
            .into_iter()
            .enumerate()
            .map(|(i, (_, name))| match i {
                0 => crate::i18n::tr("fxp.cyan"),
                1 => crate::i18n::tr("fxp.magenta"),
                2 => crate::i18n::tr("fxp.yellow"),
                3 => crate::i18n::tr("fxp.black"),
                _ => name,
            })
            .collect()
    }

    /// Show the ink at `plate` (an index of [`Self::separation_names`]) in
    /// the drawing window, or every ink with `None`.
    pub fn set_view_plate(&mut self, plate: Option<usize>) {
        self.view_plate = plate;
        self.view_plate_doc.replace(None);
    }

    /// The drawing with the current page as the ink of View > Separations
    /// prints it, for the canvas; kept until the drawing, the page or the
    /// ink changes. `None` while every ink shows.
    pub fn separation_view_doc(&self) -> Option<Rc<Document>> {
        let plate = self.view_plate?;
        let rev = self.engine.revision();
        if let Some((r, p, page, doc)) = self.view_plate_doc.borrow().as_ref() {
            if *r == rev && *p == plate && *page == self.page {
                return Some(doc.clone());
            }
        }
        let doc = self.doc();
        let count = crate::export::plates(doc).len();
        if plate >= count {
            return None;
        }
        let sheet = doc.pages.iter().position(|p| p.id == self.page)?;
        let only: Vec<bool> = (0..count).map(|i| i == plate).collect();
        let shown = crate::ui::print_preview::shown_page_inks(doc, sheet, &only, true, true)?;
        let mut copy = doc.clone();
        copy.pages[sheet] = shown;
        let copy = Rc::new(copy);
        self.view_plate_doc
            .replace(Some((rev, plate, self.page, copy.clone())));
        Some(copy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{Color, Command, Fill, ShapeKind};

    fn fill_of(doc: &Document, id: tracedraw_core::ShapeId) -> Color {
        match &doc.find_shape(id).expect("shape").fill {
            Fill::Solid(c) => *c,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_window_shows_one_ink_in_its_colour_and_the_drawing_stays() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        let red = Color::Cmyk {
            c: 0.0,
            m: 1.0,
            y: 1.0,
            k: 0.0,
        };
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(red),
        });
        assert!(app.separation_view_doc().is_none());
        assert_eq!(app.separation_names().len(), 4);
        // Magenta: the red shows its magenta.
        app.set_view_plate(Some(1));
        let shown = app.separation_view_doc().expect("magenta");
        assert_eq!(
            fill_of(&shown, id),
            Color::Cmyk {
                c: 0.0,
                m: 1.0,
                y: 0.0,
                k: 0.0
            }
        );
        // Cyan: nothing of it prints.
        app.set_view_plate(Some(0));
        let shown = app.separation_view_doc().expect("cyan");
        assert_eq!(
            fill_of(&shown, id),
            Color::Cmyk {
                c: 0.0,
                m: 0.0,
                y: 0.0,
                k: 0.0
            }
        );
        // The drawing keeps its red, and an edit shows at once.
        assert_eq!(fill_of(app.doc(), id), red);
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(Color::Cmyk {
                c: 0.5,
                m: 0.0,
                y: 0.0,
                k: 0.0,
            }),
        });
        let shown = app.separation_view_doc().expect("cyan again");
        assert!(matches!(fill_of(&shown, id), Color::Cmyk { c, .. } if (c - 0.5).abs() < 1e-6));
        app.set_view_plate(None);
        assert!(app.separation_view_doc().is_none());
    }
}

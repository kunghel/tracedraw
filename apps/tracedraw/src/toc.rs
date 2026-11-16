//! Text > Table of Contents (#1660): texts marked as headings, and a
//! table of contents listing them with their page numbers, which the
//! drawing works out anew whenever it is drawn, printed or exported (see
//! `tracedraw_core::toc`).

use crate::app::App;
use crate::i18n::tr;
use tracedraw_core::document::{ParagraphStyle, TabKind, TabStop};
use tracedraw_core::geometry::{Point, Rect, Size};
use tracedraw_core::toc::TocField;
use tracedraw_core::{Color, Command, Fill, Shape, ShapeId, ShapeKind, TextSpan};

/// The space a table of contents keeps from the page's edges when the
/// drawing has no page margins, mm.
const TOC_INSET: f64 = 20.0;

/// The size of a new table of contents' text, points.
const TOC_SIZE_PT: f64 = 12.0;

impl App {
    /// The selected texts that are not tables of contents.
    fn heading_candidates(&self) -> Vec<ShapeId> {
        self.text_shapes()
            .into_iter()
            .filter(|s| s.toc.is_none())
            .map(|s| s.id)
            .collect()
    }

    /// The heading level of the selected texts when they all share one
    /// (`Some(None)`: none of them is a heading), `None` with no text
    /// selected or levels mixed.
    pub fn selected_toc_level(&self) -> Option<Option<u8>> {
        let levels: Vec<Option<u8>> = self
            .text_shapes()
            .into_iter()
            .filter(|s| s.toc.is_none())
            .map(|s| s.toc_level)
            .collect();
        let first = *levels.first()?;
        levels.iter().all(|l| *l == first).then_some(first)
    }

    /// Text > Table of Contents > Heading 1, 2, 3 and Not a Heading: the
    /// selected texts become headings at `level` (or none), listed in the
    /// drawing's tables of contents, as one undo step. False when no text
    /// is selected or nothing changed.
    pub fn set_toc_level(&mut self, level: Option<u8>) -> bool {
        let shapes = self.heading_candidates();
        if shapes.is_empty() || self.selected_toc_level() == Some(level) {
            return false;
        }
        self.run(Command::SetTocLevel { shapes, level });
        true
    }

    /// Text > Table of Contents > Insert Table of Contents: a paragraph
    /// text on the current page, as wide and tall as the page inside its
    /// margins (or 20 mm in from the edges), listing the headings with
    /// their page numbers after a dot leader, under the title Contents.
    /// It is selected. False without a layer to put it on.
    pub fn insert_toc(&mut self) -> bool {
        if !self.has_document() {
            return false;
        }
        let Some(layer) = self.active_layer() else {
            return false;
        };
        let index = self.page_index();
        let page = self.page_rect();
        let inset = Rect::new(
            page.x0 + TOC_INSET,
            page.y0 + TOC_INSET,
            (page.x1 - TOC_INSET).max(page.x0 + TOC_INSET),
            (page.y1 - TOC_INSET).max(page.y0 + TOC_INSET),
        );
        let area = self.doc().margin_box(index, page).unwrap_or(inset);
        let (w, h) = (area.width().max(10.0), area.height().max(10.0));
        let title = tr("toc.title");
        let id = self.engine.new_shape_id();
        let mut shape = Shape::new(
            id,
            ShapeKind::Text {
                spans: vec![TextSpan::new(
                    title.clone(),
                    self.text_font.clone(),
                    TOC_SIZE_PT,
                )],
                origin: Point::new(area.x0, area.y1 - h),
                frame: Some(Size::new(w, h)),
                align: tracedraw_core::TextAlign::Left,
                para: ParagraphStyle::default(),
                on_path: None,
            },
        );
        self.doc().defaults.apply_to(&mut shape);
        // The page numbers at the right edge after a dot leader, set after
        // the drawing's text defaults.
        if let ShapeKind::Text { para, .. } = &mut shape.kind {
            para.tabs = vec![TabStop {
                pos: w,
                kind: TabKind::Right,
                leader: Some('.'),
            }];
        }
        shape.fill = Fill::Solid(Color::BLACK);
        shape.stroke = None;
        shape.name = Some(title.clone());
        shape.toc = Some(TocField { title, levels: 3 });
        if let Err(e) = self.engine.run_batch(
            "Insert Table of Contents",
            &[Command::AddShape { layer, shape }],
        ) {
            self.status = e.to_string();
            return false;
        }
        self.select(vec![id]);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::document::spans_text;

    /// #1660: headings marked from the Text menu are listed in a table of
    /// contents inserted on the first page, and the list follows them.
    #[test]
    fn the_table_of_contents_lists_the_headings_marked() {
        let mut app = App::headless();
        app.show_welcome = false;
        app.add_page();
        app.add_page();
        let pages: Vec<_> = app.doc().pages.iter().map(|p| p.id).collect();
        assert_eq!(pages.len(), 3);
        // A chapter title on page 2 and a section on page 3.
        let heading = |app: &mut App, page, text: &str, level| {
            app.goto_page(page);
            let id = app
                .new_shape(ShapeKind::Text {
                    spans: vec![TextSpan::new(text, "Sans", 18.0)],
                    origin: Point::new(20.0, 200.0),
                    frame: None,
                    align: tracedraw_core::TextAlign::Left,
                    para: ParagraphStyle::default(),
                    on_path: None,
                })
                .expect("a layer");
            app.select(vec![id]);
            assert!(app.set_toc_level(Some(level)));
            assert!(!app.set_toc_level(Some(level)), "no change");
            assert_eq!(app.selected_toc_level(), Some(Some(level)));
            id
        };
        let chapter = heading(&mut app, 1, "Chapter one", 1);
        heading(&mut app, 2, "A section", 2);
        assert_eq!(app.engine.undo_label(), Some("Table of Contents Heading"));
        app.goto_page(0);
        assert!(app.insert_toc());
        let toc = app.selection[0];
        let shown = |app: &App| {
            let layers = app.doc().layers_drawn_for_page(pages[0]).expect("page");
            layers
                .iter()
                .flat_map(|l| l.shapes.iter())
                .find(|s| s.id == toc)
                .and_then(|s| match &s.kind {
                    ShapeKind::Text { spans, .. } => Some(spans_text(spans)),
                    _ => None,
                })
                .unwrap_or_default()
        };
        assert_eq!(
            shown(&app),
            format!("{}\nChapter one\t2\n    A section\t3", tr("toc.title"))
        );
        // The page's text area, a right tab with a dot leader at its end.
        let Some(Shape {
            kind:
                ShapeKind::Text {
                    frame: Some(f),
                    para,
                    ..
                },
            ..
        }) = app.doc().find_shape(toc)
        else {
            unreachable!("a paragraph text");
        };
        assert!((f.width - (app.page_size().width - 2.0 * TOC_INSET)).abs() < 1e-9);
        assert_eq!(para.tabs[0].leader, Some('.'));
        assert_eq!(para.tabs[0].kind, TabKind::Right);
        // A table of contents is no heading itself.
        assert!(!app.set_toc_level(Some(1)));
        // The chapter no longer a heading: the list follows.
        app.goto_page(1);
        app.select(vec![chapter]);
        assert!(app.set_toc_level(None));
        assert_eq!(app.selected_toc_level(), Some(None));
        assert_eq!(
            shown(&app),
            format!("{}\n    A section\t3", tr("toc.title"))
        );
        app.undo();
        assert_eq!(
            shown(&app),
            format!("{}\nChapter one\t2\n    A section\t3", tr("toc.title"))
        );
    }
}

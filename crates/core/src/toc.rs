//! Tables of contents (#1660). A text marked as a heading
//! ([`Shape::toc_level`], levels 1 to 3) is listed, with the number of
//! the page it is on, in every table of contents of the drawing: a
//! paragraph text carrying a [`TocField`]. The list is worked out anew
//! each time the page is drawn, printed or exported, so it follows the
//! headings as they are edited, moved to other pages or deleted, and the
//! pages as they are added, removed or reordered.

use crate::document::{spans_text, Document, Shape, ShapeKind};
use serde::{Deserialize, Serialize};

/// The deepest heading level.
pub const MAX_LEVEL: u8 = 3;

/// How a table of contents shows the headings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TocField {
    /// The first line, such as "Contents"; empty for none.
    pub title: String,
    /// The levels listed: 1 lists the level 1 headings alone, 3 all.
    pub levels: u8,
}

impl Default for TocField {
    fn default() -> Self {
        TocField {
            title: String::new(),
            levels: MAX_LEVEL,
        }
    }
}

/// One heading of the drawing, as a table of contents lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct TocEntry {
    pub level: u8,
    /// The heading's first line of text.
    pub text: String,
    /// The page it is on, 0-based.
    pub page: usize,
    /// The page's number as the drawing's page number fields show it.
    pub label: String,
}

/// Every heading of the drawing in reading order: page by page, and on
/// a page from the top down, then left to right. A heading inside a
/// group counts; hidden ones and master layers do not. The page numbers
/// follow the first page number field on the master layers (its first
/// number, style and text around it), else count from 1.
pub fn entries(doc: &Document) -> Vec<TocEntry> {
    let field = doc
        .master
        .iter()
        .flat_map(|l| l.shapes.iter())
        .find_map(page_number_field);
    let mut out = Vec::new();
    for (index, page) in doc.pages.iter().enumerate() {
        let mut found: Vec<(f64, f64, u8, String)> = Vec::new();
        for layer in page.layers.iter().filter(|l| l.visible) {
            for s in &layer.shapes {
                collect(s, crate::geometry::Affine::IDENTITY, &mut found);
            }
        }
        found.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.total_cmp(&b.0)));
        let label = field
            .map(|f| f.label_in(index, doc.pages.len()))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| (index + 1).to_string());
        out.extend(found.into_iter().map(|(_, _, level, text)| TocEntry {
            level,
            text,
            page: index,
            label: label.clone(),
        }));
    }
    out
}

fn page_number_field(s: &Shape) -> Option<&crate::document::PageNumberField> {
    s.page_number.as_ref().or(match &s.kind {
        ShapeKind::Group { children } => children.iter().find_map(page_number_field),
        _ => None,
    })
}

/// The headings in `s` (itself or inside its groups), with where each
/// starts on the page.
fn collect(s: &Shape, outer: crate::geometry::Affine, out: &mut Vec<(f64, f64, u8, String)>) {
    if !s.visible {
        return;
    }
    let t = outer * s.transform;
    match &s.kind {
        ShapeKind::Text {
            spans,
            origin,
            frame,
            ..
        } => {
            let Some(level) = s.toc_level.filter(|l| (1..=MAX_LEVEL).contains(l)) else {
                return;
            };
            if s.toc.is_some() {
                return;
            }
            let text = heading_text(&spans_text(spans));
            if text.is_empty() {
                return;
            }
            // Paragraph text by the top of its frame (its origin is the
            // frame's bottom left), artistic text by its first baseline.
            let top = frame.map_or(0.0, |f| f.height);
            let at = t * crate::geometry::Point::new(origin.x, origin.y + top);
            out.push((at.x, at.y, level, text));
        }
        ShapeKind::Group { children } => {
            for c in children {
                collect(c, t, out);
            }
        }
        _ => {}
    }
}

/// A heading's text as listed: its first line that holds anything,
/// trimmed, its runs of spaces made one.
pub fn heading_text(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|l| !l.is_empty())
        .unwrap_or_default()
}

impl TocField {
    /// The text of the table of contents for `entries`: the title, then a
    /// line per heading of the levels listed, indented four spaces a level
    /// below the first, a tab, and its page number. With no heading to
    /// list, the title alone.
    pub fn text(&self, entries: &[TocEntry]) -> String {
        let levels = self.levels.clamp(1, MAX_LEVEL);
        let mut lines: Vec<String> = Vec::new();
        if !self.title.trim().is_empty() {
            lines.push(self.title.trim().to_string());
        }
        for e in entries.iter().filter(|e| e.level <= levels) {
            let indent = "    ".repeat(usize::from(e.level.saturating_sub(1)));
            lines.push(format!("{indent}{}\t{}", e.text, e.label));
        }
        lines.join("\n")
    }
}

impl Shape {
    /// This object, or one inside it, is a table of contents.
    pub fn holds_toc(&self) -> bool {
        self.toc.is_some()
            || match &self.kind {
                ShapeKind::Group { children } => children.iter().any(Shape::holds_toc),
                _ => false,
            }
    }

    /// Tables of contents in this object list `entries`, in the style of
    /// their first run of text. One with nothing to list keeps its text.
    pub fn show_toc(&mut self, entries: &[TocEntry]) {
        if let (Some(f), ShapeKind::Text { spans, .. }) = (&self.toc, &mut self.kind) {
            let text = f.text(entries);
            if !text.is_empty() {
                spans.truncate(1);
                if let Some(first) = spans.first_mut() {
                    first.text = text;
                }
            }
        }
        if let ShapeKind::Group { children } = &mut self.kind {
            for c in children {
                c.show_toc(entries);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Layer, Page, PageNumberField, TextSpan};
    use crate::geometry::{Affine, Point, Size};

    fn next_id() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1_000_000);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    fn text(s: &str, x: f64, y: f64, level: Option<u8>) -> Shape {
        let mut sh = Shape::new(
            crate::id::ShapeId(next_id()),
            ShapeKind::Text {
                spans: vec![TextSpan::new(s, "Sans", 12.0)],
                origin: Point::new(0.0, 0.0),
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            },
        );
        sh.transform = Affine::translate((x, y));
        sh.toc_level = level;
        sh
    }

    fn book(pages: usize) -> Document {
        let mut doc = Document::new("book", Size::new(100.0, 200.0));
        while doc.pages.len() < pages {
            let id = doc.ids_mut().page();
            let layer = doc.ids_mut().layer();
            doc.pages.push(Page {
                id,
                name: String::new(),
                size: Size::new(100.0, 200.0),
                layers: vec![Layer::new(layer, "L")],
                guides: Vec::new(),
                background: None,
            });
        }
        doc
    }

    /// Headings are listed page by page, top down on a page, with their
    /// page numbers; the list follows the drawing as it changes.
    #[test]
    fn the_table_of_contents_follows_the_headings_and_pages() {
        let mut doc = book(3);
        let toc_page = doc.pages[0].id;
        let mut toc = text("(contents)", 10.0, 190.0, None);
        toc.toc = Some(TocField {
            title: "Contents".into(),
            levels: 2,
        });
        doc.pages[0].layers[0].shapes.push(toc);
        // Page 2: a chapter low on the page and one high up; page 3: a
        // section in a group, and a level 3 heading not listed.
        doc.pages[1].layers[0]
            .shapes
            .push(text("Second  chapter\nmore", 10.0, 50.0, Some(1)));
        doc.pages[1].layers[0]
            .shapes
            .push(text("First chapter", 10.0, 180.0, Some(1)));
        doc.pages[1].layers[0]
            .shapes
            .push(text("Body text", 10.0, 120.0, None));
        let mut group = Shape::new(
            crate::id::ShapeId(next_id()),
            ShapeKind::Group {
                children: vec![text("A section", 0.0, 0.0, Some(2))],
            },
        );
        group.transform = Affine::translate((10.0, 150.0));
        doc.pages[2].layers[0].shapes.push(group);
        doc.pages[2].layers[0]
            .shapes
            .push(text("Detail", 10.0, 100.0, Some(3)));
        let shown = |doc: &Document| {
            let layers = doc.layers_drawn_for_page(toc_page).expect("page");
            layers
                .iter()
                .flat_map(|l| l.shapes.iter())
                .find_map(|s| match &s.kind {
                    ShapeKind::Text { spans, .. } if s.toc.is_some() => Some(spans_text(spans)),
                    _ => None,
                })
                .unwrap_or_default()
        };
        assert_eq!(
            shown(&doc),
            "Contents\nFirst chapter\t2\nSecond chapter\t2\n    A section\t3"
        );
        // The stored text stays as it was: the list is made when drawn.
        assert!(matches!(
            &doc.pages[0].layers[0].shapes[0].kind,
            ShapeKind::Text { spans, .. } if spans_text(spans) == "(contents)"
        ));
        // A page inserted before the chapters moves their numbers on.
        let mut moved = book(4);
        moved.pages[0].layers = doc.pages[0].layers.clone();
        moved.pages[2].layers = doc.pages[1].layers.clone();
        moved.pages[3].layers = doc.pages[2].layers.clone();
        let first = moved.pages[0].id;
        let layers = moved.layers_drawn_for_page(first).expect("page");
        assert!(matches!(
            &layers[0].shapes[0].kind,
            ShapeKind::Text { spans, .. }
                if spans_text(spans) == "Contents\nFirst chapter\t3\nSecond chapter\t3\n    A section\t4"
        ));
        // Page numbers as the master layer's page number field shows them.
        let master = doc.ids_mut().layer();
        let mut number = text("1", 50.0, 5.0, None);
        number.page_number = Some(PageNumberField {
            start_at: 1,
            first_page: 2,
            style: 2,
            prefix: String::new(),
            suffix: String::new(),
            ..Default::default()
        });
        let mut layer = Layer::new(master, "Master");
        layer.shapes.push(number);
        doc.master.push(layer);
        assert_eq!(
            shown(&doc),
            "Contents\nFirst chapter\ti\nSecond chapter\ti\n    A section\tii"
        );
        // Deleting the headings leaves the title alone.
        for p in &mut doc.pages[1..] {
            p.layers[0].shapes.clear();
        }
        assert_eq!(shown(&doc), "Contents");
    }

    #[test]
    fn heading_text_is_the_first_line_with_anything() {
        assert_eq!(heading_text("\n  One   two \nthree"), "One two");
        assert_eq!(heading_text("   "), "");
        let f = TocField {
            title: String::new(),
            levels: 9,
        };
        let e = |level| TocEntry {
            level,
            text: "H".into(),
            page: 0,
            label: "1".into(),
        };
        assert_eq!(f.text(&[e(1), e(3)]), "H\t1\n        H\t1");
        assert_eq!(f.text(&[]), "");
    }
}

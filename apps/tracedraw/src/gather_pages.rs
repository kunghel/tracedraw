//! Layout > Gather Pages on Page 1 (#2098): the objects of every page
//! side by side on the first page, left to right in page order, a gap
//! between them, to print or cut a run of pages on one sheet; the first
//! page grows to hold them and the emptied pages go, when asked. One
//! undo step. See docs/behavior/pages.md.

use crate::app::App;
use crate::i18n::{tr, trf};
use tracedraw_core::geometry::Affine;
use tracedraw_core::{Command, LayerId, ShapeId, Size};

/// The widest gap between the pages' objects, mm.
pub const GAP_MAX: f64 = 1000.0;

impl App {
    /// Gather the objects of every page on the first page, each page's
    /// own placed to the right of the one before with `gap` mm between,
    /// their tops in line; with `resize` the first page takes the size
    /// of all of them, with `delete` the other pages go. Returns whether
    /// anything changed.
    pub fn gather_pages(&mut self, gap: f64, resize: bool, delete: bool) -> bool {
        let gap = if gap.is_finite() {
            gap.clamp(0.0, GAP_MAX)
        } else {
            0.0
        };
        let doc = self.doc();
        if doc.pages.len() < 2 {
            self.status = tr("gather_pages.one_page");
            return false;
        }
        let first = &doc.pages[0];
        let first_id = first.id;
        let total_w: f64 = doc.pages.iter().map(|p| p.size.width).sum::<f64>()
            + gap * (doc.pages.len() - 1) as f64;
        let max_h = doc.pages.iter().map(|p| p.size.height).fold(0.0, f64::max);
        let top = if resize { max_h } else { first.size.height };
        // A page's layer goes into the first page's layer at the same
        // place in the list, else its first.
        let targets: Vec<LayerId> = first.layers.iter().map(|l| l.id).collect();
        let Some(&fallback) = targets.first() else {
            return false;
        };
        let mut count: std::collections::HashMap<LayerId, usize> = first
            .layers
            .iter()
            .map(|l| (l.id, l.shapes.len()))
            .collect();
        let mut cmds = Vec::new();
        let mut moved_any = false;
        let mut x = first.size.width + gap;
        // The first page's own objects keep their place across; their
        // tops come in line with the others when the page grows.
        if resize && (top - first.size.height).abs() > 1e-9 {
            let own: Vec<ShapeId> = first
                .layers
                .iter()
                .flat_map(|l| l.shapes.iter().map(|s| s.id))
                .collect();
            if !own.is_empty() {
                cmds.push(Command::TransformShapes {
                    shapes: own,
                    transform: Affine::translate((0.0, top - first.size.height)),
                });
            }
        }
        for page in doc.pages.iter().skip(1) {
            let mut ids = Vec::new();
            for (li, layer) in page.layers.iter().enumerate() {
                let to = targets.get(li).copied().unwrap_or(fallback);
                for s in &layer.shapes {
                    let n = count.entry(to).or_default();
                    cmds.push(Command::Reorder {
                        shape: s.id,
                        layer: to,
                        index: *n,
                    });
                    *n += 1;
                    ids.push(s.id);
                }
            }
            if !ids.is_empty() {
                moved_any = true;
                cmds.push(Command::TransformShapes {
                    shapes: ids,
                    transform: Affine::translate((x, top - page.size.height)),
                });
            }
            x += page.size.width + gap;
        }
        if resize {
            cmds.push(Command::ResizePage {
                page: first_id,
                size: Size::new(total_w, max_h),
            });
        }
        let others: Vec<_> = doc.pages.iter().skip(1).map(|p| p.id).collect();
        let pages = doc.pages.len();
        if delete {
            cmds.extend(others.into_iter().map(|page| Command::DeletePage { page }));
        }
        if !moved_any && !delete && !resize {
            self.status = tr("gather_pages.nothing");
            return false;
        }
        if let Err(e) = self.engine.run_batch("Gather Pages", &cmds) {
            self.status = e.to_string();
            return false;
        }
        self.goto_page(0);
        self.status = trf("gather_pages.done", &[("n", &pages.to_string())]);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;
    use tracedraw_core::ShapeKind;

    /// Three pages' objects come side by side on the first, which grows
    /// to hold them; the other pages go; one undo step brings it all
    /// back.
    #[test]
    fn every_pages_objects_come_side_by_side_on_the_first() {
        let mut app = App::headless();
        let size = app.doc().pages[0].size;
        let rect = |app: &mut App| {
            app.new_shape(ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 30.0, 20.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect")
        };
        let a = rect(&mut app);
        let mut ids = vec![a];
        for _ in 0..2 {
            app.add_page();
            let last = app.doc().pages.len() - 1;
            app.goto_page(last);
            ids.push(rect(&mut app));
        }
        assert_eq!(app.doc().pages.len(), 3);
        let before = app.engine.history_labels().0.len();
        assert!(app.gather_pages(10.0, true, true));
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        let doc = app.doc();
        assert_eq!(doc.pages.len(), 1);
        let p = &doc.pages[0];
        assert!((p.size.width - (3.0 * size.width + 20.0)).abs() < 1e-9);
        let xs: Vec<f64> = ids
            .iter()
            .map(|id| doc.find_shape(*id).expect("kept").bounds().x0)
            .collect();
        assert!((xs[0] - 10.0).abs() < 1e-9);
        assert!((xs[1] - (10.0 + size.width + 10.0)).abs() < 1e-9);
        assert!((xs[2] - (10.0 + 2.0 * (size.width + 10.0))).abs() < 1e-9);
        assert!(p.layers[0].shapes.len() == 3);
        app.undo();
        assert_eq!(app.doc().pages.len(), 3);
        // Kept pages and size: the objects only move.
        assert!(app.gather_pages(0.0, false, false));
        assert_eq!(app.doc().pages.len(), 3);
        assert_eq!(app.doc().pages[0].size, size);
        assert_eq!(app.doc().pages[0].layers[0].shapes.len(), 3);
        // One page: nothing to gather.
        let mut one = App::headless();
        assert!(!one.gather_pages(5.0, true, true));
    }
}

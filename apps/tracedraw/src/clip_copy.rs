//! Object > Clip Frame > Copy Contents From (D#4221): the contents of a
//! clip frame clicked next are copied into each selected object, placed
//! in it as they sit in their own frame (the same offset from the
//! frame's middle); an object that is no clip frame yet becomes one.

use crate::app::App;
use crate::i18n::tr;
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::Affine,
    Command, ShapeId,
};

impl App {
    /// Wait for a click on the clip frame whose contents to copy into the
    /// selected objects.
    pub fn start_copy_clip_contents(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.pending_copy_clip = true;
        self.status = tr("status.click_clip_source");
    }

    /// Copy the contents of the clip frame `source` into every selected
    /// object (but `source`): each gets copies, with ids of their own,
    /// moved by how far its middle lies from the source frame's; one undo
    /// step. False when `source` is no clip frame or holds nothing.
    pub fn copy_clip_contents_from(&mut self, source: ShapeId) -> bool {
        let Some(src) = self.doc().shape_on_page(source) else {
            return false;
        };
        let ShapeKind::ClipFrame { contents, .. } = &src.kind else {
            self.status = tr("status.not_clip_source");
            return false;
        };
        if contents.is_empty() {
            self.status = tr("status.not_clip_source");
            return false;
        }
        // The contents on the page: the frame's transform taken in.
        let on_page: Vec<Shape> = contents
            .iter()
            .map(|c| {
                let mut c = c.clone();
                c.transform_by(src.transform);
                c
            })
            .collect();
        let from = src.bounds().center();
        let targets: Vec<ShapeId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| *id != source)
            .filter(|id| {
                self.doc().shape(*id).is_ok_and(|(_, s)| {
                    !s.locked && crate::draw_inside::is_closed_shape(s)
                })
            })
            .collect();
        if targets.is_empty() {
            self.status = tr("status.not_clip_target");
            return false;
        }
        let mut cmds = Vec::new();
        for t in &targets {
            let Ok((layer, target)) = self.doc().shape(*t).map(|(l, s)| (l.id, s.clone())) else {
                continue;
            };
            let d = target.bounds().center() - from;
            let mut ids = Vec::new();
            for c in &on_page {
                let mut copy = {
                    let engine = &mut self.engine;
                    c.with_fresh_ids(&mut || engine.new_shape_id())
                };
                copy.transform_by(Affine::translate(d));
                ids.push(copy.id);
                cmds.push(Command::AddShape {
                    layer,
                    shape: copy,
                });
            }
            cmds.push(Command::PlaceInside {
                contents: ids,
                frame: *t,
            });
        }
        if let Err(e) = self.engine.run_batch("Copy Contents From", &cmds) {
            self.status = e.to_string();
            return false;
        }
        self.select(targets);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    fn rect(app: &mut App, r: Rect) -> ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: r,
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    }

    /// A clip frame's picture copied into two other boxes: each becomes
    /// a clip frame with its own copy, as far off its middle as in the
    /// source; the source keeps its contents. One undo step.
    #[test]
    fn clip_contents_are_copied_into_other_objects() {
        let mut app = App::headless();
        let frame = rect(&mut app, Rect::new(0.0, 0.0, 40.0, 40.0));
        let inside = rect(&mut app, Rect::new(10.0, 10.0, 30.0, 50.0));
        app.place_inside(vec![inside], frame);
        let a = rect(&mut app, Rect::new(100.0, 0.0, 140.0, 40.0));
        let b = rect(&mut app, Rect::new(200.0, 100.0, 240.0, 140.0));
        app.select(vec![a, b]);
        app.start_copy_clip_contents();
        assert!(app.pending_copy_clip);
        app.pending_copy_clip = false;
        assert!(app.copy_clip_contents_from(frame));
        for (t, dx, dy) in [(a, 100.0, 0.0), (b, 200.0, 100.0)] {
            let s = app.doc().find_shape(t).expect("target").clone();
            let ShapeKind::ClipFrame { contents, .. } = &s.kind else {
                panic!("a clip frame");
            };
            assert_eq!(contents.len(), 1);
            assert_ne!(contents[0].id, inside);
            let c = app.doc().shape_on_page(contents[0].id).expect("copy").bounds();
            assert!((c.x0 - (10.0 + dx)).abs() < 1e-6 && (c.y0 - (10.0 + dy)).abs() < 1e-6, "{c:?}");
        }
        assert!(app.doc().find_shape(inside).is_some(), "the source keeps it");
        app.undo();
        assert!(matches!(
            app.doc().find_shape(a).map(|s| &s.kind),
            Some(ShapeKind::Rect { .. })
        ));
        // A plain object is no source.
        app.select(vec![a]);
        assert!(!app.copy_clip_contents_from(b));
    }

    /// Objects dropped on a clip frame inside another one join its
    /// contents where it shows them (#4227).
    #[test]
    fn objects_go_into_a_clip_frame_nested_in_another() {
        let mut app = App::headless();
        let outer = rect(&mut app, Rect::new(0.0, 0.0, 100.0, 100.0));
        let inner = rect(&mut app, Rect::new(20.0, 20.0, 60.0, 60.0));
        let pic = rect(&mut app, Rect::new(30.0, 30.0, 50.0, 50.0));
        app.place_inside(vec![pic], inner);
        app.place_inside(vec![inner], outer);
        // The outer frame moved: the inner one's space is no longer the
        // page's.
        app.run(Command::TransformShapes {
            shapes: vec![outer],
            transform: Affine::translate((5.0, 7.0)),
        });
        let extra = rect(&mut app, Rect::new(40.0, 40.0, 45.0, 45.0));
        app.place_inside(vec![extra], inner);
        let ShapeKind::ClipFrame { contents, .. } =
            &app.doc().find_shape(inner).expect("inner").kind
        else {
            panic!("a clip frame");
        };
        assert_eq!(contents.len(), 2);
        assert!(app.doc().shape(extra).is_err(), "off the layer");
        let b = app.doc().shape_on_page(extra).expect("inside").bounds();
        assert!((b.x0 - 40.0).abs() < 1e-6 && (b.y0 - 40.0).abs() < 1e-6, "{b:?}");
    }
}

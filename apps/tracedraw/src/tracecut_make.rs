//! TraceCut's Make tools: eyelet marks along an edge, the panels of a
//! box with finger joints and the flat pattern of a folded tray. The
//! geometry is `tracedraw_tracecut::eyelets`, `boxes` and `tray`; this
//! module works the results out for the preview and turns them into a
//! new group, one undo step for OK.
//! See docs/behavior/tracecut.md.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::tracecut_cut::{fmt_len, AddStyle, Change, Changes};
use tracedraw_core::geometry::{Point, Rect, Shape as _, Vec2};
use tracedraw_core::Color;
use tracedraw_tracecut::boxes::{self, BoxSpec};
use tracedraw_tracecut::eyelets::{self, EyeletSpec};
use tracedraw_tracecut::tray::{self, Place, TrayError, TraySpec};

/// The colour of a folded tray's grooves (the first pass) and of its cut
/// lines (the second).
pub const GROOVE_COLOR: (u8, u8, u8) = (0, 0x60, 0xFF);
pub const CUT_COLOR: (u8, u8, u8) = (0xE0, 0, 0);

impl App {
    /// Eyelet marks along the selection's box or its closed curves, as
    /// a group of hairline circles.
    pub fn tracecut_eyelets(&self, spec: &EyeletSpec) -> Changes {
        let targets = self.tracecut_targets();
        let mut out = Changes {
            label: "Eyelets",
            ..Default::default()
        };
        if targets.is_empty() {
            out.summary = tr("tracecut.select_first");
            return out;
        }
        let outlines = self.tracecut_outlines(&targets);
        let centres = eyelets::eyelets(&outlines, spec);
        if centres.is_empty() {
            out.summary = tr("tracecut.no_eyelets");
            return out;
        }
        let paths = eyelets::marks(&centres, spec.diameter);
        out.summary = trf(
            "tracecut.eyelets_found",
            &[("n", &centres.len().to_string())],
        );
        out.preview = paths.clone();
        out.changes = vec![Change::AddGroup {
            name: "tracecut.eyelets_group",
            paths,
            style: AddStyle::Hairline,
        }];
        out
    }

    /// Box Maker: the panels of the box laid out side by side from the
    /// page's bottom-left corner, the gap in from it, as a group of
    /// hairline outlines. Needs no selection.
    pub fn tracecut_box(&self, spec: &BoxSpec) -> Changes {
        let mut out = Changes {
            label: "Box Maker",
            hide_objects: true,
            ..Default::default()
        };
        let panels = match boxes::panels(spec) {
            Ok(p) => p,
            Err(boxes::BoxError::TooSmall) => {
                out.summary = tr("tracecut.box_too_small");
                return out;
            }
        };
        let gap = spec.clamped().gap;
        let page = self.page_rect();
        let paths = boxes::layout(&panels, Point::new(page.x0 + gap, page.y0 + gap), gap);
        let all = paths
            .iter()
            .map(|p| p.bounding_box())
            .reduce(|a, b| a.union(b));
        let (w, h) = all.map_or((0.0, 0.0), |b| (b.width(), b.height()));
        out.summary = trf(
            "tracecut.box_found",
            &[
                ("n", &paths.len().to_string()),
                ("w", &fmt_len(self.units, w)),
                ("h", &fmt_len(self.units, h)),
            ],
        );
        out.preview = paths.clone();
        out.changes = vec![Change::AddGroup {
            name: "tracecut.box_group",
            paths,
            style: AddStyle::Hairline,
        }];
        out
    }
}

impl App {
    /// The box of the selection's outlines (strokes left out), for
    /// Folded Tray's size and place.
    pub fn tracecut_selection_box(&self) -> Option<Rect> {
        self.tracecut_outlines(&self.tracecut_targets())
            .iter()
            .flatten()
            .map(|p| p.bounding_box())
            .filter(|b| {
                b.x0.is_finite() && b.y0.is_finite() && b.x1.is_finite() && b.y1.is_finite()
            })
            .reduce(|a, b| a.union(b))
    }

    /// Folded Tray: the flat pattern as a group holding the grooves
    /// (blue, open lines, first) and the cut lines (red, closed), placed
    /// the gap in from the page's bottom-left corner or the gap to the
    /// right of the selection. Needs no selection.
    pub fn tracecut_tray(&self, spec: &TraySpec) -> Changes {
        let mut out = Changes {
            label: "Folded Tray",
            hide_objects: true,
            ..Default::default()
        };
        let p = match tray::pattern(spec) {
            Ok(p) => p,
            Err(e) => {
                out.summary = tr(match e {
                    TrayError::TooShallow => "tracecut.tray_too_shallow",
                    TrayError::TooSmall => "tracecut.tray_too_small",
                    TrayError::NoRoomForWindow => "tracecut.tray_no_window",
                });
                return out;
            }
        };
        let s = spec.clamped();
        let page = self.page_rect();
        let at = match (s.place, self.tracecut_selection_box()) {
            (Place::BesideSelection, Some(b)) => Point::new(b.x1 + s.gap, b.y0),
            _ => Point::new(page.x0 + s.gap, page.y0 + s.gap),
        };
        let p = p.translated(Vec2::new(at.x, at.y));
        let cuts = p.cut_paths();
        let grooves = p.groove_paths();
        out.summary = trf(
            "tracecut.tray_found",
            &[
                ("w", &fmt_len(self.units, p.bounds.width())),
                ("h", &fmt_len(self.units, p.bounds.height())),
                ("grooves", &grooves.len().to_string()),
                ("cuts", &cuts.len().to_string()),
            ],
        );
        out.preview = cuts.clone();
        out.scored = grooves.clone();
        let rgb = |c: (u8, u8, u8)| Color::rgb8(c.0, c.1, c.2);
        out.changes = vec![Change::AddNested {
            name: "tracecut.tray_group",
            parts: vec![
                (
                    "tracecut.tray_grooves",
                    grooves,
                    AddStyle::Line(rgb(GROOVE_COLOR)),
                ),
                ("tracecut.tray_cut", cuts, AddStyle::Line(rgb(CUT_COLOR))),
            ],
        }];
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{Fill, ShapeKind};
    use tracedraw_tracecut::eyelets::Along;

    #[test]
    fn eyelets_go_round_the_selection_as_one_group_in_one_step() {
        let mut app = App::headless();
        let banner = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 400.0, 200.0),
                radius: 0.0,
                corners: None,
            })
            .expect("banner");
        app.select(vec![banner]);
        let spec = EyeletSpec {
            spacing: 100.0,
            diameter: 8.0,
            ..Default::default()
        };
        let c = app.tracecut_eyelets(&spec);
        assert_eq!(c.preview.len(), 12);
        let before = app.engine.history_labels().0.len();
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        assert_eq!(
            app.engine.history_labels().0.last().copied(),
            Some("Eyelets")
        );
        let g = app
            .doc()
            .find_shape(app.selection[0])
            .expect("group")
            .clone();
        assert_eq!(
            g.name.as_deref(),
            Some(tr("tracecut.eyelets_group").as_str())
        );
        let ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        assert_eq!(children.len(), 12);
        assert!(children
            .iter()
            .all(|s| s.fill == Fill::None && s.stroke.is_some()));
        let b = children[0].page_path().bounding_box();
        assert!((b.width() - 8.0).abs() < 1e-6);
        assert!((b.center().x - 15.0).abs() < 1e-6 && (b.center().y - 15.0).abs() < 1e-6);
        // Along the curve: the same places for a rectangle.
        app.select(vec![banner]);
        let along = app.tracecut_eyelets(&EyeletSpec {
            along: Along::Curve,
            ..spec
        });
        assert_eq!(along.preview.len(), 12);
        app.undo();
        assert!(app.doc().find_shape(g.id).is_none());
    }

    #[test]
    fn the_box_maker_adds_its_panels_without_a_selection() {
        let mut app = App::headless();
        let spec = BoxSpec::default();
        let c = app.tracecut_box(&spec);
        assert_eq!(c.preview.len(), 6);
        assert!(c.hide_objects);
        let before = app.engine.history_labels().0.len();
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        assert_eq!(
            app.engine.history_labels().0.last().copied(),
            Some("Box Maker")
        );
        let g = app
            .doc()
            .find_shape(app.selection[0])
            .expect("group")
            .clone();
        let ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        assert_eq!(children.len(), 6);
        // The front first, the gap in from the page's corner, at its
        // outside size.
        let b = children[0].page_path().bounding_box();
        assert!(
            (b.x0 - 5.0).abs() < 1e-6 && (b.y0 - 5.0).abs() < 1e-6,
            "{b:?}"
        );
        assert!((b.width() - 200.0).abs() < 1e-6 && (b.height() - 100.0).abs() < 1e-6);
        assert!(children
            .iter()
            .all(|s| s.fill == Fill::None && s.stroke.is_some()));
        // Open: five panels.
        let open = app.tracecut_box(&BoxSpec {
            open_top: true,
            ..spec
        });
        assert_eq!(open.preview.len(), 5);
        // Too small for the material: nothing to add.
        let small = app.tracecut_box(&BoxSpec {
            height: 10.0,
            ..spec
        });
        assert!(small.changes.is_empty());
        assert_eq!(small.summary, tr("tracecut.box_too_small"));
        // The window works it out with nothing selected.
        app.select(Vec::new());
        let w = app.tracecut_work_out(
            crate::tracecut::ToolId::BoxMaker,
            &crate::tracecut::ToolSettings::default(),
        );
        assert!(matches!(&w.outcome, crate::tracecut::Outcome::Changes(c) if c.preview.len() == 6));
    }

    #[test]
    fn the_folded_tray_adds_grooves_then_cuts_in_one_step() {
        let mut app = App::headless();
        app.select(Vec::new());
        let spec = TraySpec {
            window: true,
            ..Default::default()
        };
        let c = app.tracecut_tray(&spec);
        assert!(c.hide_objects);
        assert_eq!(c.scored.len(), 4);
        assert_eq!(c.preview.len(), 2, "the window and the outline");
        let before = app.engine.history_labels().0.len();
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        assert_eq!(
            app.engine.history_labels().0.last().copied(),
            Some("Folded Tray")
        );
        let g = app
            .doc()
            .find_shape(app.selection[0])
            .expect("group")
            .clone();
        assert_eq!(g.name.as_deref(), Some(tr("tracecut.tray_group").as_str()));
        let ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        assert_eq!(children.len(), 2);
        let (grooves, cut) = (&children[0], &children[1]);
        assert_eq!(
            grooves.name.as_deref(),
            Some(tr("tracecut.tray_grooves").as_str())
        );
        assert_eq!(cut.name.as_deref(), Some(tr("tracecut.tray_cut").as_str()));
        let kids = |s: &tracedraw_core::Shape| match &s.kind {
            ShapeKind::Group { children } => children.clone(),
            _ => Vec::new(),
        };
        let blue = Color::rgb8(GROOVE_COLOR.0, GROOVE_COLOR.1, GROOVE_COLOR.2);
        let red = Color::rgb8(CUT_COLOR.0, CUT_COLOR.1, CUT_COLOR.2);
        let gs = kids(grooves);
        assert_eq!(gs.len(), 4);
        assert!(gs
            .iter()
            .all(|s| matches!(s.kind, ShapeKind::Path { closed: false, .. })
                && s.fill == Fill::None
                && s.stroke.as_ref().is_some_and(|k| k.color == blue)));
        let cs = kids(cut);
        assert_eq!(cs.len(), 2);
        assert!(cs
            .iter()
            .all(|s| matches!(s.kind, ShapeKind::Path { closed: true, .. })
                && s.stroke.as_ref().is_some_and(|k| k.color == red)));
        // The gap in from the page's corner, at the flat size.
        let b = cs[1].page_path().bounding_box();
        let page = app.page_rect();
        assert!((b.x0 - page.x0 - 10.0).abs() < 1e-6 && (b.y0 - page.y0 - 10.0).abs() < 1e-6);
        assert!((b.width() - 698.0).abs() < 1e-6 && (b.height() - 498.0).abs() < 1e-6);
        // CNC Toolpath takes the cut lines as closed outlines and the
        // grooves as open lines cut on the line.
        app.select(vec![g.id]);
        let prog = app.tracecut_cnc(&tracedraw_tracecut::cnc::CncSpec::default());
        assert_eq!(prog.toolpaths.iter().filter(|t| !t.closed).count(), 4);
        assert_eq!(prog.toolpaths.iter().filter(|t| t.closed).count(), 2);
    }

    #[test]
    fn the_folded_tray_goes_beside_the_selection_or_says_why_not() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(20.0, 30.0, 120.0, 80.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![id]);
        let b = app.tracecut_selection_box().expect("box");
        assert!((b.width() - 100.0).abs() < 1e-6 && (b.height() - 50.0).abs() < 1e-6);
        let spec = TraySpec {
            place: Place::BesideSelection,
            gap: 15.0,
            ..Default::default()
        };
        let c = app.tracecut_tray(&spec);
        let all = c
            .preview
            .iter()
            .map(|p| p.bounding_box())
            .reduce(|a, b| a.union(b))
            .expect("paths");
        assert!((all.x0 - (b.x1 + 15.0)).abs() < 1e-6 && (all.y0 - b.y0).abs() < 1e-6);
        // Too shallow for the allowance: nothing to add, and why.
        let shallow = app.tracecut_tray(&TraySpec {
            depth: 1.0,
            thickness: 8.0,
            ..Default::default()
        });
        assert!(shallow.changes.is_empty());
        assert_eq!(shallow.summary, tr("tracecut.tray_too_shallow"));
        let shut = app.tracecut_tray(&TraySpec {
            window: true,
            window_inset: 1000.0,
            ..Default::default()
        });
        assert_eq!(shut.summary, tr("tracecut.tray_no_window"));
    }

    #[test]
    fn without_room_or_selection_nothing_is_added() {
        let mut app = App::headless();
        assert!(app
            .tracecut_eyelets(&EyeletSpec::default())
            .changes
            .is_empty());
        let small = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("small");
        app.select(vec![small]);
        let c = app.tracecut_eyelets(&EyeletSpec {
            along: Along::Curve,
            ..Default::default()
        });
        assert!(c.changes.is_empty());
        assert_eq!(c.summary, tr("tracecut.no_eyelets"));
        assert!(!app.tracecut_apply_changes(&c));
    }
}

//! The Shape tool's corner widgets (#635): a small circle inside each
//! corner of the curves being edited; dragging one into the corner rounds
//! it, live, with the arc's middle under the pointer. With nodes chosen
//! only their corners show a widget and round; with none, every corner
//! rounds together. Open curves round their inner corners too. A corner
//! already rounded (an arc tangent to its two sides, straight or curved,
//! #2223) keeps a widget on its arc, so its radius can change again
//! (#872). Corners round on
//! the page, so on a stretched curve a new rounding is round; a corner
//! rounded before the stretch keeps its stretched arc and its widget,
//! and rounds in the curve's own space until it is sharp again (#1000).
//! They show only
//! with Options > Nodes and Handles > corner widgets on (#884). See
//! docs/behavior/shape-tool.md, Corner widgets.

use crate::app::{App, Drag};
use crate::tools::Tool;
use tracedraw_core::corner_cut::{
    corner_reach, corners, corners_and_fillets, cut_corners, fillets, max_fillet, unround, Corner,
    CornerCut, Fillet,
};
use tracedraw_core::geometry::{Affine, BezPath, Point};
use tracedraw_core::{Command, ShapeId, ShapeKind};

/// How far a widget sits from its node, along the corner's middle,
/// screen pixels.
pub const WIDGET_OFFSET_PX: f32 = 18.0;
/// The widget's radius and the reach of a press on it, screen pixels.
pub const WIDGET_RADIUS_PX: f32 = 4.0;
const WIDGET_PICK_PX: f32 = 7.0;
/// A curve with more corners than this shows widgets only on its chosen
/// nodes (a traced outline would be covered in them).
const MAX_WIDGETS: usize = 32;
/// A sharp corner shows a widget only when rounding it can move its
/// middle at least this far on the screen, pixels: a node where the
/// curve barely turns, or between very short sides, has no widget, as
/// dragging it would change nothing that shows.
const MIN_ROUNDING_PX: f64 = 2.0;

/// The corners of the curves the Shape tool edits, worked out once per
/// change of the drawing or of the selection.
#[derive(Default)]
pub struct CornerCache {
    key: Option<(u64, Vec<ShapeId>)>,
    curves: Vec<(ShapeId, Affine, Vec<Spot>)>,
}

/// A corner that takes a widget: a sharp one, or one already rounded
/// (#872), whose `corner` is the sharp corner its sides meet at.
struct Spot {
    /// From the curve's own space to the space the corner was found in:
    /// the page, or the curve's own space for a rounding made before the
    /// curve was stretched (#1000).
    space: Affine,
    /// A rounding made before the curve was stretched, found in its own
    /// space.
    stretched: bool,
    corner: Corner,
    /// How far the rounded corner's arc lies into the corner (0 when
    /// sharp), as its radius puts it between straight sides: what a drag
    /// starts from.
    depth: f64,
    /// Where the arc's middle is, along the corner's middle: where the
    /// widget shows. Curved sides put it nearer or further (#2223).
    shown: f64,
    /// The nodes that choose it: the corner's, or every node of the arc.
    nodes: Vec<usize>,
    /// How far into the corner (mm on the page) the most rounding it
    /// takes on its own reaches; infinite for a corner rounded already
    /// and on a curve with too many corners to measure each.
    reach: f64,
}

/// One widget on the screen: the curve, its corner, how far its arc lies
/// into the corner already, and where it is.
#[derive(Debug, Clone, Copy)]
pub struct Widget {
    pub shape: ShapeId,
    /// See `Spot::space`; `corner` is in that space.
    pub space: Affine,
    pub stretched: bool,
    pub corner: Corner,
    pub depth: f64,
    pub at: egui::Pos2,
    /// Its corner is chosen (one of its nodes is among the chosen nodes).
    pub chosen: bool,
}

/// The drag of a corner widget: the curve as it was, with the rounded
/// corners the drag changes made sharp again, the corners it rounds
/// (`None`: every corner), the grabbed corner, how far its arc lay into
/// it when the drag began, the largest radius they all take, and where
/// the drag began. The path, corner and points are in `space` (see
/// `Spot::space`).
#[derive(Debug, Clone, PartialEq)]
pub struct CornerDrag {
    pub shape: ShapeId,
    pub space: Affine,
    pub path: BezPath,
    pub closed: bool,
    pub only: Option<Vec<usize>>,
    pub corner: Corner,
    pub depth: f64,
    pub max: f64,
    pub from: Point,
    pub begun: bool,
}

/// A selected curve's corners on the page, for the property bar's corner
/// radius (#2057): its sharp corners and its rounded ones.
struct PageCorners {
    shape: ShapeId,
    /// From the curve's own space to the page.
    to_page: Affine,
    page: BezPath,
    closed: bool,
    sharp: Vec<Corner>,
    rounded: Vec<Fillet>,
}

/// How close two corner points on the page are taken to be one, mm.
const SAME_CORNER_MM: f64 = 1e-5;

/// The fillet radius whose arc's middle is `d` further into a corner of
/// angle `theta` than the corner was: the arc's middle lies
/// `r (1 / sin(theta / 2) - 1)` from the corner.
pub fn radius_for(d: f64, theta: f64) -> f64 {
    let k = 1.0 / (theta / 2.0).sin() - 1.0;
    if d > 0.0 && k > 1e-9 && k.is_finite() {
        d / k
    } else {
        0.0
    }
}

impl App {
    /// The corner widgets to show now, in screen space; none while one is
    /// dragged, and none at all unless Options > Nodes and Handles turns
    /// them on (#884).
    pub fn corner_widgets(&self) -> Vec<Widget> {
        if !self.settings.nodes.corner_widgets
            || self.tool != Tool::Shape
            || matches!(self.drag, Drag::CornerWidget(_))
        {
            return Vec::new();
        }
        self.refresh_corner_cache();
        let cache = self.corner_cache.borrow();
        let mut out = Vec::new();
        for (id, t, list) in &cache.curves {
            let chosen: Vec<usize> = self
                .node_selection
                .iter()
                .filter(|(s, _)| s == id)
                .map(|(_, i)| *i)
                .collect();
            // A curve with many corners shows only the chosen ones'
            // widgets; otherwise every corner has one.
            // With one corner at a time off, chosen nodes show only their
            // corners' widgets, as before.
            let many = list.len() > MAX_WIDGETS;
            if chosen.is_empty() && many {
                continue;
            }
            let only_chosen =
                many || (!chosen.is_empty() && !self.settings.nodes.corner_one_at_a_time);
            let zoom = f64::from(self.view.zoom);
            for spot in list {
                let is_chosen = spot.nodes.iter().any(|n| chosen.contains(n));
                if only_chosen && !is_chosen {
                    continue;
                }
                // No widget where rounding would change nothing that shows.
                if spot.reach * zoom < MIN_ROUNDING_PX {
                    continue;
                }
                let c = &spot.corner;
                let to_page = *t * spot.space.inverse();
                // A rounded corner's widget sits off the middle of its arc.
                let mid = c.at + c.inside * spot.shown;
                let node = self.view.to_screen(to_page * mid);
                let toward = self.view.to_screen(to_page * (mid + c.inside));
                let d = toward - node;
                if d.length() < 1e-6 || !d.x.is_finite() || !d.y.is_finite() {
                    continue;
                }
                out.push(Widget {
                    shape: *id,
                    space: spot.space,
                    stretched: spot.stretched,
                    corner: *c,
                    depth: spot.depth,
                    at: node + d.normalized() * WIDGET_OFFSET_PX,
                    chosen: is_chosen,
                });
            }
        }
        out
    }

    /// Work the corners of the selected curves out again when the drawing
    /// or the selection changed since the last time.
    fn refresh_corner_cache(&self) {
        // Not the objects that show their frame only (#2213).
        let targets: Vec<ShapeId> = self.node_targets();
        let key = (self.engine.revision(), targets);
        let mut cache = self.corner_cache.borrow_mut();
        if cache.key.as_ref() != Some(&key) {
            cache.curves = key
                .1
                .iter()
                .filter_map(|id| {
                    let s = self.doc().find_shape(*id)?;
                    let ShapeKind::Path { path, .. } = &s.kind else {
                        return None;
                    };
                    if s.spline.is_some() {
                        return None;
                    }
                    let t = self.doc().parent_transform(*id)? * s.transform;
                    if t.determinant().abs() < 1e-12 {
                        return None;
                    }
                    // Corners and roundings on the page, then the roundings
                    // made before a stretch, round in the curve's own space.
                    let page = t * path.clone();
                    let (sharp, rounded) = corners_and_fillets(&page);
                    // How far each sharp corner rounds, measured where
                    // its widgets can all show.
                    let reach = if sharp.len() <= MAX_WIDGETS {
                        corner_reach(&page)
                    } else {
                        Vec::new()
                    };
                    let mut spots: Vec<Spot> = sharp
                        .into_iter()
                        .enumerate()
                        .map(|(i, c)| Spot {
                            space: t,
                            stretched: false,
                            corner: c,
                            depth: 0.0,
                            shown: 0.0,
                            nodes: vec![c.node],
                            reach: reach.get(i).copied().unwrap_or(f64::INFINITY),
                        })
                        .collect();
                    let taken: Vec<usize> = rounded.iter().flat_map(|f| f.nodes.clone()).collect();
                    let stretched = fillets(path)
                        .into_iter()
                        .filter(|f| !f.nodes.iter().any(|n| taken.contains(n)))
                        .map(|f| (Affine::IDENTITY, true, f));
                    let rounded = rounded.into_iter().map(|f| (t, false, f)).chain(stretched);
                    spots.extend(rounded.filter_map(|(space, stretched, f)| {
                        let depth = f.depth();
                        Some(Spot {
                            space,
                            stretched,
                            corner: Corner {
                                node: *f.nodes.first()?,
                                at: f.corner,
                                inside: f.inside,
                                theta: f.theta,
                            },
                            depth,
                            shown: f.middle,
                            nodes: f.nodes,
                            reach: f64::INFINITY,
                        })
                    }));
                    Some((*id, t, spots))
                })
                .collect();
            cache.key = Some(key);
        }
    }

    /// The corner widget under the page point `p`.
    pub fn corner_widget_at(&self, p: Point) -> Option<Widget> {
        let s = self.view.to_screen(p);
        self.corner_widgets()
            .into_iter()
            .map(|w| (w, (w.at - s).length()))
            .filter(|(_, d)| *d <= WIDGET_PICK_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(w, _)| w)
    }

    /// The nodes that choose the widget's corner: its node, or every node
    /// of its rounded corner's arc.
    fn widget_nodes(&self, w: &Widget) -> Vec<usize> {
        let cache = self.corner_cache.borrow();
        cache
            .curves
            .iter()
            .filter(|(id, _, _)| *id == w.shape)
            .flat_map(|(_, _, list)| list.iter())
            .find(|s| s.corner.node == w.corner.node && s.stretched == w.stretched)
            .map(|s| s.nodes.clone())
            .unwrap_or_else(|| vec![w.corner.node])
    }

    /// Ctrl+click on a widget: its corner is added to the chosen corners,
    /// or taken out when it is chosen already, so one corner or several
    /// can be chosen and then rounded together by dragging one of them.
    pub fn toggle_corner_widget(&mut self, w: Widget) {
        let nodes = self.widget_nodes(&w);
        if w.chosen {
            self.node_selection
                .retain(|(id, i)| !(*id == w.shape && nodes.contains(i)));
        } else {
            if let Some(n) = nodes.first() {
                self.node_selection.push((w.shape, *n));
            }
        }
        self.segment_spot = None;
    }

    /// Begin dragging the widget `w` from the page point `p`. The drag
    /// rounds the widget's own corner; when that corner's node is among
    /// the chosen nodes, every chosen node's corner; with `all` (Shift),
    /// every corner of the curve.
    pub fn start_corner_drag(&mut self, w: Widget, p: Point, all: bool) -> bool {
        let Some(s) = self.doc().find_shape(w.shape) else {
            return false;
        };
        let ShapeKind::Path { path, closed } = &s.kind else {
            return false;
        };
        let Some(t) = self
            .doc()
            .parent_transform(w.shape)
            .map(|pt| pt * s.transform)
        else {
            return false;
        };
        if t.determinant().abs() < 1e-12 || w.space.determinant().abs() < 1e-12 {
            return false;
        }
        // On the page, or in the curve's own space for a corner rounded
        // before the curve was stretched (#1000).
        let path = &(w.space * path.clone());
        let rounded = fillets(path);
        let chosen: Vec<usize> = self
            .node_selection
            .iter()
            .filter(|(id, _)| *id == w.shape)
            .map(|(_, i)| *i)
            .collect();
        // The widget's own nodes: its corner's node, or every node of its
        // rounded corner.
        let own = self.widget_nodes(&w);
        let one_at_a_time = self.settings.nodes.corner_one_at_a_time;
        let chosen = if all {
            Vec::new()
        } else if !one_at_a_time || w.chosen || own.iter().any(|n| chosen.contains(n)) {
            chosen
        } else {
            own
        };
        // The rounded corners the drag changes become sharp again first
        // (#872): every one with Shift, else those of the nodes taken;
        // the nodes taken are renumbered to match.
        let which: Vec<usize> = rounded
            .iter()
            .enumerate()
            .filter(|(_, f)| chosen.is_empty() || f.nodes.iter().any(|n| chosen.contains(n)))
            .map(|(i, _)| i)
            .collect();
        let (base, map) = if which.is_empty() {
            (path.clone(), Vec::new())
        } else {
            unround(path, Some(&which))
        };
        let renumber = |nodes: &mut dyn Iterator<Item = usize>| -> Vec<usize> {
            let mut out: Vec<usize> = if which.is_empty() {
                nodes.collect()
            } else {
                nodes
                    .filter_map(|i| map.get(i).copied().flatten())
                    .collect()
            };
            out.sort_unstable();
            out.dedup();
            out
        };
        let chosen = renumber(&mut chosen.into_iter());
        // A stretched rounding changes on its own, never with the sharp
        // corners, which round on the page.
        let only = if !chosen.is_empty() {
            Some(chosen)
        } else if !w.stretched {
            None
        } else {
            let mut own = which
                .iter()
                .filter_map(|i| rounded.get(*i))
                .flat_map(|f| f.nodes.iter().copied());
            Some(renumber(&mut own))
        };
        let Some(max) = max_fillet(&base, only.as_deref()) else {
            return false;
        };
        self.drag = Drag::CornerWidget(Box::new(CornerDrag {
            shape: w.shape,
            space: w.space,
            path: base,
            closed: *closed,
            only,
            corner: w.corner,
            depth: w.depth,
            max,
            from: w.space * t.inverse() * p,
            begun: false,
        }));
        // The cut renumbers the nodes.
        self.node_selection.clear();
        self.segment_spot = None;
        true
    }

    /// The widget dragged to the page point `p`: the corners round so the
    /// grabbed one's arc passes under the pointer, at most as far as
    /// every one of them still fits; one undo step for the drag.
    pub fn corner_drag_to(&mut self, mut d: CornerDrag, p: Point) {
        let Some(t) = self
            .doc()
            .find_shape(d.shape)
            .and_then(|s| Some(self.doc().parent_transform(d.shape)? * s.transform))
        else {
            return;
        };
        if t.determinant().abs() < 1e-12 || d.space.determinant().abs() < 1e-12 {
            return;
        }
        let lp = d.space * t.inverse() * p;
        // A rounded corner's arc starts where it was (#872).
        let along: f64 = d.depth + (lp - d.from).dot(d.corner.inside);
        let r = radius_for(along, d.corner.theta).min(d.max * 0.999);
        let path = if r > 1e-6 {
            cut_corners(&d.path, CornerCut::Fillet(r), d.only.as_deref()).0
        } else {
            d.path.clone()
        };
        let path = d.space.inverse() * path;
        let cmd = Command::SetShapeKind {
            shape: d.shape,
            kind: ShapeKind::Path {
                path,
                closed: d.closed,
            },
        };
        let _ = if d.begun {
            self.engine.amend(&cmd)
        } else {
            self.engine.run_with_label(&cmd, "Fillet")
        };
        let u = self.units;
        let digits = self.settings.precision.min(10) as usize;
        self.status = crate::i18n::trf(
            "status.corner_radius",
            &[(
                "r",
                &format!("{:.*} {}", digits, u.world_from_mm(r.max(0.0)), u.label()),
            )],
        );
        d.begun = true;
        self.drag = Drag::CornerWidget(Box::new(d));
    }

    /// The selected curves whose corners the property bar's corner radius
    /// sets (#2057): curves (not B-splines) with a sharp or a rounded
    /// corner, on the page. Only with the corner widgets on, as the
    /// widgets are the rounding the field stands for.
    fn page_corners(&self) -> Vec<PageCorners> {
        if !self.settings.nodes.corner_widgets || self.tool != Tool::Shape {
            return Vec::new();
        }
        let mut out = Vec::new();
        // Not the objects that show their frame only (#2213).
        for id in self.node_targets() {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            let ShapeKind::Path { path, closed } = &s.kind else {
                continue;
            };
            if s.spline.is_some() {
                continue;
            }
            let Some(t) = self.doc().parent_transform(id).map(|pt| pt * s.transform) else {
                continue;
            };
            if t.determinant().abs() < 1e-12 {
                continue;
            }
            let page = t * path.clone();
            let (sharp, rounded) = corners_and_fillets(&page);
            if sharp.is_empty() && rounded.is_empty() {
                continue;
            }
            out.push(PageCorners {
                shape: id,
                to_page: t,
                page,
                closed: *closed,
                sharp,
                rounded,
            });
        }
        out
    }

    /// The chosen nodes of curve `id`.
    fn chosen_of(&self, id: ShapeId) -> Vec<usize> {
        self.node_selection
            .iter()
            .filter(|(s, _)| *s == id)
            .map(|(_, i)| *i)
            .collect()
    }

    /// The corners of `c` the field acts on, as (sharp corner on the
    /// page, radius: 0 when sharp), in path order: the chosen ones (a
    /// chosen ring, or a node of its arc chosen) when `chosen_only`,
    /// else every one.
    fn field_corners(&self, c: &PageCorners, chosen_only: bool) -> Vec<(usize, Point, f64)> {
        let chosen = self.chosen_of(c.shape);
        let mut out: Vec<(usize, Point, f64)> = c
            .sharp
            .iter()
            .filter(|k| !chosen_only || chosen.contains(&k.node))
            .map(|k| (k.node, k.at, 0.0))
            .collect();
        out.extend(
            c.rounded
                .iter()
                .filter(|f| !chosen_only || f.nodes.iter().any(|n| chosen.contains(n)))
                .filter_map(|f| Some((*f.nodes.first()?, f.corner, f.radius))),
        );
        out.sort_by_key(|k| k.0);
        out
    }

    /// Whether a corner of the selected curves is chosen: then the field
    /// shows and sets the chosen corners only.
    fn corner_chosen(&self, curves: &[PageCorners]) -> bool {
        curves
            .iter()
            .any(|c| !self.field_corners(c, true).is_empty())
    }

    /// What the Shape tool's corner radius field shows (#2057): the
    /// radius of the chosen corners, or of every corner of the selected
    /// curves with none chosen (0 for a sharp one), on the page in mm,
    /// and whether they differ (the field is then empty). `None` when no
    /// selected curve has a corner to round.
    /// Read from the corners worked out for the widgets, once per change
    /// of the drawing or the selection, as the bar asks every frame.
    pub fn corner_rounding(&self) -> Option<(f64, bool)> {
        if !self.settings.nodes.corner_widgets || self.tool != Tool::Shape {
            return None;
        }
        self.refresh_corner_cache();
        let cache = self.corner_cache.borrow();
        // The corners rounded on the page (a rounding made before a
        // stretch is not one the field sets), in path order per curve.
        let spots = |chosen_only: bool| {
            let mut out: Vec<f64> = Vec::new();
            for (id, _, list) in &cache.curves {
                let chosen = self.chosen_of(*id);
                let mut here: Vec<(usize, f64)> = list
                    .iter()
                    .filter(|s| !s.stretched)
                    .filter(|s| !chosen_only || s.nodes.iter().any(|n| chosen.contains(n)))
                    .map(|s| (s.corner.node, radius_for(s.depth, s.corner.theta)))
                    .collect();
                here.sort_by_key(|k| k.0);
                out.extend(here.into_iter().map(|k| k.1));
            }
            out
        };
        let chosen = spots(true);
        let radii = if chosen.is_empty() {
            spots(false)
        } else {
            chosen
        };
        let first = *radii.first()?;
        let mixed = radii.iter().any(|r| (r - first).abs() > 1e-6);
        Some((first, mixed))
    }

    /// Round the corners the corner radius field stands for to `r` mm on
    /// the page (#2057): the chosen corners, or every corner of the
    /// selected curves with none chosen; 0 takes the rounding off. With
    /// the bar's corner kind set to Scallop or Chamfer (#1147) the
    /// corners are cut that way instead, `r` along each side. A
    /// radius larger than a corner takes is cut to the largest every one
    /// of them fits, as a drag of a widget is. One undo step; the chosen
    /// corners stay chosen. False when nothing changed.
    pub fn set_corner_rounding(&mut self, r: f64) -> bool {
        use crate::corners::CornerOp;
        if !r.is_finite() {
            return false;
        }
        let r = r.max(0.0);
        let op = self.corner_bar_op;
        let curves = self.page_corners();
        let chosen_only = self.corner_chosen(&curves);
        let mut cmds = Vec::new();
        let mut kept: Vec<(ShapeId, Vec<Point>)> = Vec::new();
        let mut used: Option<f64> = None;
        for c in &curves {
            let targets = self.field_corners(c, chosen_only);
            if targets.is_empty() {
                continue;
            }
            let points: Vec<Point> = targets.iter().map(|k| k.1).collect();
            let near = |q: Point| points.iter().any(|p| (q - *p).hypot() < SAME_CORNER_MM);
            // The rounded corners taken first go back to their sharp
            // corners, then the corners round again at the new radius.
            let which: Vec<usize> = c
                .rounded
                .iter()
                .enumerate()
                .filter(|(_, f)| near(f.corner))
                .map(|(i, _)| i)
                .collect();
            let base = if which.is_empty() {
                c.page.clone()
            } else {
                unround(&c.page, Some(&which)).0
            };
            let only: Vec<usize> = corners(&base)
                .into_iter()
                .filter(|k| near(k.at))
                .map(|k| k.node)
                .collect();
            let rounded = match (op, max_fillet(&base, Some(&only))) {
                (CornerOp::Fillet, Some(max)) if r > 1e-9 && !only.is_empty() => {
                    let rr = r.min(max * 0.999);
                    used = Some(used.map_or(rr, |u: f64| u.min(rr)));
                    if rr > 1e-9 {
                        cut_corners(&base, CornerCut::Fillet(rr), Some(&only)).0
                    } else {
                        base
                    }
                }
                // Scalloped or chamfered (#1147), as the Corners docker
                // cuts them: the distance along the sides, cut down until
                // every chosen corner takes it.
                (CornerOp::Scallop | CornerOp::Chamfer, _) if r > 1e-9 && !only.is_empty() => {
                    let cut = |d: f64| match op {
                        CornerOp::Chamfer => CornerCut::Chamfer(d, d),
                        _ => CornerCut::Scallop(d),
                    };
                    let mut d = r;
                    let mut made = cut_corners(&base, cut(d), Some(&only));
                    for _ in 0..60 {
                        if made.1 >= only.len() || d < 1e-6 {
                            break;
                        }
                        d *= 0.9;
                        made = cut_corners(&base, cut(d), Some(&only));
                    }
                    if made.1 > 0 {
                        used = Some(used.map_or(d, |u: f64| u.min(d)));
                        made.0
                    } else {
                        base
                    }
                }
                _ => base,
            };
            if rounded == c.page {
                continue;
            }
            let path = c.to_page.inverse() * rounded;
            cmds.push(Command::SetShapeKind {
                shape: c.shape,
                kind: ShapeKind::Path {
                    path,
                    closed: c.closed,
                },
            });
            if chosen_only {
                kept.push((c.shape, points));
            }
        }
        if cmds.is_empty() {
            return false;
        }
        let label = match op {
            _ if r <= 1e-9 => "Clear Rounding",
            CornerOp::Fillet => "Fillet",
            CornerOp::Scallop => "Scallop",
            CornerOp::Chamfer => "Chamfer",
        };
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
            return false;
        }
        // The rounding renumbers the nodes: the corners chosen before are
        // found again by their sharp corners, which stay where they were.
        self.segment_spot = None;
        if chosen_only {
            self.node_selection.clear();
            for c in self.page_corners() {
                let Some((_, points)) = kept.iter().find(|(id, _)| *id == c.shape) else {
                    continue;
                };
                for (node, at, _) in self.field_corners(&c, false) {
                    if points
                        .iter()
                        .any(|p| (at - *p).hypot() < SAME_CORNER_MM * 10.0)
                    {
                        self.node_selection.push((c.shape, node));
                    }
                }
            }
        } else {
            self.node_selection.clear();
        }
        if let Some(u) = used {
            let units = self.units;
            let digits = self.settings.precision.min(10) as usize;
            self.status = crate::i18n::trf(
                "status.corner_radius",
                &[(
                    "r",
                    &format!(
                        "{:.*} {}",
                        digits,
                        units.world_from_mm(u.max(0.0)),
                        units.label()
                    ),
                )],
            );
        }
        true
    }

    /// A plain click on a corner widget chooses its corner alone, so its
    /// ring is drawn chosen and the property bar's corner radius sets it
    /// (#2057); Ctrl+click adds it or takes it out
    /// ([`toggle_corner_widget`](Self::toggle_corner_widget)).
    pub fn choose_corner_widget(&mut self, w: Widget) {
        let nodes = self.widget_nodes(&w);
        self.node_selection = nodes
            .first()
            .map(|n| vec![(w.shape, *n)])
            .unwrap_or_default();
        self.node_run = None;
        self.segment_spot = None;
    }
}

/// Draw the corner widgets: white circles with a ring in the node
/// colour, over the nodes; a chosen corner's is filled with the colour.
pub fn draw(app: &App, painter: &egui::Painter, color: egui::Color32) {
    for w in app.corner_widgets() {
        let fill = if w.chosen {
            color
        } else {
            egui::Color32::WHITE
        };
        painter.circle(w.at, WIDGET_RADIUS_PX, fill, egui::Stroke::new(1.5, color));
        let dot = if w.chosen {
            egui::Color32::WHITE
        } else {
            color
        };
        painter.circle_filled(w.at, 1.5, dot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Shape as _;

    fn shapes() -> Vec<(&'static str, BezPath)> {
        let mut out = Vec::new();
        let mut lens = BezPath::new();
        lens.move_to((20.0, 40.0));
        lens.curve_to((30.0, 52.0), (50.0, 52.0), (60.0, 40.0));
        lens.curve_to((50.0, 28.0), (30.0, 28.0), (20.0, 40.0));
        lens.close_path();
        out.push(("lens", lens));
        let mut heart = BezPath::new();
        heart.move_to((40.0, 20.0));
        heart.curve_to((10.0, 40.0), (25.0, 65.0), (40.0, 50.0));
        heart.curve_to((55.0, 65.0), (70.0, 40.0), (40.0, 20.0));
        heart.close_path();
        out.push(("heart", heart));
        let mut half = BezPath::new();
        half.move_to((20.0, 20.0));
        half.line_to((60.0, 20.0));
        half.curve_to((60.0, 46.0), (20.0, 46.0), (20.0, 20.0));
        half.close_path();
        out.push(("half disc", half));
        let mut drop = BezPath::new();
        drop.move_to((20.0, 20.0));
        drop.curve_to((28.0, 30.0), (32.0, 40.0), (35.0, 60.0));
        drop.curve_to((38.0, 40.0), (42.0, 30.0), (50.0, 20.0));
        drop.line_to((20.0, 20.0));
        drop.close_path();
        out.push(("drop", drop));
        out
    }

    /// A corner between curved sides rounds, keeps its widget once it is
    /// rounded, rounds further from the radius it has and goes back to
    /// sharp, as a corner between straight sides does (#2223). Before,
    /// its widget was gone after the first drag, however short, and the
    /// corner could not be rounded any more.
    #[test]
    fn corners_between_curves_keep_their_widgets_and_round_again() {
        for (name, path) in shapes() {
            let sharp = corners(&path);
            for i in 0..sharp.len() {
                let mut app = App::headless();
                let id = app
                    .new_shape(ShapeKind::Path {
                        path: path.clone(),
                        closed: true,
                    })
                    .expect("a curve");
                app.select(vec![id]);
                app.tool = Tool::Shape;
                app.settings.nodes.corner_widgets = true;
                let path_of = |app: &App| match &app.doc().find_shape(id).expect("curve").kind {
                    ShapeKind::Path { path, .. } => path.clone(),
                    _ => panic!("a curve"),
                };
                let widgets = app.corner_widgets();
                assert_eq!(widgets.len(), sharp.len(), "{name}");
                let w = widgets[i];
                let at = w.corner.at;
                // The widget of the corner at `at`, dragged `by` mm into
                // it; the radius the drag gave.
                let pull = |app: &mut App, by: f64| -> f64 {
                    let w = app
                        .corner_widgets()
                        .into_iter()
                        .find(|w| (w.corner.at - at).hypot() < 1e-6)
                        .unwrap_or_else(|| panic!("{name} corner {i}: no widget"));
                    let p = app.view.to_page(w.at);
                    assert!(app.start_corner_drag(w, p, false), "{name} corner {i}");
                    let Drag::CornerWidget(d) = app.drag.clone() else {
                        panic!("{name} corner {i}: no drag");
                    };
                    app.corner_drag_to(*d, p + w.corner.inside * by);
                    app.drag = Drag::None;
                    fillets(&path_of(app))
                        .iter()
                        .find(|f| (f.corner - at).hypot() < 1e-6)
                        .map_or(0.0, |f| f.radius)
                };
                // A short drag: a small rounding, and the widget stays.
                let first = pull(&mut app, 0.2);
                assert!(first > 0.01, "{name} corner {i}: {first}");
                assert_eq!(app.corner_widgets().len(), sharp.len(), "{name} corner {i}");
                assert_eq!(corners(&path_of(&app)).len(), sharp.len() - 1, "{name}");
                // Dragged again, it rounds further from there.
                let second = pull(&mut app, 0.2);
                assert!(second > first * 1.5, "{name} corner {i}: {first} {second}");
                assert_eq!(app.corner_widgets().len(), sharp.len(), "{name} corner {i}");
                // The property bar's field reads it and sets it.
                app.node_selection.clear();
                assert!(app.set_corner_rounding(first), "{name} corner {i}");
                let radii: Vec<f64> = fillets(&path_of(&app)).iter().map(|f| f.radius).collect();
                assert_eq!(radii.len(), sharp.len(), "{name} corner {i}: {radii:?}");
                assert!(
                    radii.iter().all(|r| (r - first).abs() < 1e-6),
                    "{name} corner {i}: {first} {radii:?}"
                );
                // Clear Rounding gives the sharp curve back.
                assert!(app.set_corner_rounding(0.0), "{name} corner {i}");
                let back = path_of(&app);
                assert_eq!(corners(&back).len(), sharp.len(), "{name} corner {i}");
                assert!(fillets(&back).is_empty(), "{name} corner {i}");
                assert!(
                    (back.area() - path.area()).abs() < 1e-6,
                    "{name} corner {i}"
                );
            }
        }
    }

    fn disc(x: f64, y: f64, r: f64) -> BezPath {
        tracedraw_core::geometry::ellipse_path(tracedraw_core::geometry::Rect::new(
            x - r,
            y - r,
            x + r,
            y + r,
        ))
    }

    /// The Shape tool on a shape cut out of three discs whose corners the
    /// Corners panel rounded (#2222): every rounded corner has its ring,
    /// and only they do. The shaping commands leave a disc its own
    /// nodes, and a piece of a disc between two of them read as a
    /// rounded corner, so the shape showed rings along its round sides,
    /// with a rounding radius of the disc's. Each ring rounds its corner
    /// further,
    /// and Clear Rounding gives the sharp shape back.
    #[test]
    fn a_shape_cut_out_of_discs_keeps_one_ring_per_rounded_corner() {
        use tracedraw_core::shaping::{overlay, Op};
        let a = disc(60.0, 60.0, 30.0);
        let b = disc(95.0, 65.0, 30.0);
        let c = disc(72.0, 27.0, 30.0);
        let cut = overlay(&overlay(&a, &b, Op::Trim), &c, Op::Trim);
        let sharp = corners(&cut);
        assert_eq!(sharp.len(), 3, "{sharp:?}");
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Path {
                path: cut.clone(),
                closed: true,
            })
            .expect("a curve");
        app.select(vec![id]);
        app.settings.nodes.corner_widgets = true;
        let path_of = |app: &App| match &app.doc().find_shape(id).expect("curve").kind {
            ShapeKind::Path { path, .. } => path.clone(),
            _ => panic!("a curve"),
        };
        // Sharp: a ring in each corner, none along the round sides.
        app.tool = Tool::Shape;
        assert!(fillets(&cut).is_empty(), "{:?}", fillets(&cut));
        assert_eq!(app.corner_widgets().len(), 3);
        assert_eq!(app.corner_rounding(), Some((0.0, false)));
        // The Corners panel rounds them.
        app.tool = Tool::Pick;
        app.corners.radius = 3.0;
        assert_eq!(app.apply_corners(), 3);
        // Back in the Shape tool: the three rounded corners, each with
        // its ring, read at the radius they were given.
        app.tool = Tool::Shape;
        let rounded = fillets(&path_of(&app));
        assert_eq!(rounded.len(), 3, "{rounded:?}");
        for f in &rounded {
            assert!((f.radius - 3.0).abs() < 1e-6, "{rounded:?}");
            assert!(
                sharp.iter().any(|c| (c.at - f.corner).hypot() < 1e-6),
                "{rounded:?}"
            );
        }
        let widgets = app.corner_widgets();
        assert_eq!(widgets.len(), 3);
        let radius = app.corner_rounding().expect("corners");
        assert!((radius.0 - 3.0).abs() < 1e-6 && !radius.1, "{radius:?}");
        // Each ring rounds its corner further.
        for w in widgets {
            let at = w.corner.at;
            let w = app
                .corner_widgets()
                .into_iter()
                .find(|w| (w.corner.at - at).hypot() < 1e-6)
                .expect("the corner's ring");
            let p = app.view.to_page(w.at);
            assert!(app.start_corner_drag(w, p, false));
            let Drag::CornerWidget(d) = app.drag.clone() else {
                panic!("no drag");
            };
            app.corner_drag_to(*d, p + w.corner.inside * 0.5);
            app.drag = Drag::None;
            let now = fillets(&path_of(&app));
            assert_eq!(now.len(), 3, "{now:?}");
            let r = now
                .iter()
                .find(|f| (f.corner - at).hypot() < 1e-6)
                .map_or(0.0, |f| f.radius);
            assert!(r > 3.2, "{r}");
            assert_eq!(app.corner_widgets().len(), 3);
        }
        // Clear Rounding: the sharp shape again.
        app.node_selection.clear();
        assert!(app.set_corner_rounding(0.0));
        let back = path_of(&app);
        assert!(fillets(&back).is_empty());
        assert_eq!(corners(&back).len(), 3);
        assert!((back.area() - cut.area()).abs() < 1e-6);
    }

    /// A corner with a smooth node close beside it shows its ring and
    /// rounds as far as its sides go (#2224, #2225, #2226): a disc with
    /// a bite taken out one degree past its top node has half a
    /// millimetre of curve between the corner and the node. The corner
    /// counted as too short to round and had no ring to grab; its
    /// rounding stopped at the node, half a millimetre, and left a
    /// sliver of a segment before it.
    #[test]
    fn a_corner_beside_a_smooth_node_shows_its_ring_and_rounds() {
        use tracedraw_core::shaping::{overlay, Op};
        let turn = 89f64.to_radians();
        let at = Point::new(60.0 + 30.0 * turn.cos(), 60.0 + 30.0 * turn.sin());
        let bitten = overlay(
            &disc(60.0, 60.0, 30.0),
            &disc(at.x + 30.0, at.y, 30.0),
            Op::Trim,
        );
        let sharp = corners(&bitten);
        assert_eq!(sharp.len(), 2, "{sharp:?}");
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Path {
                path: bitten.clone(),
                closed: true,
            })
            .expect("a curve");
        app.select(vec![id]);
        app.tool = Tool::Shape;
        app.settings.nodes.corner_widgets = true;
        // Seen at 4 px to the millimetre, about the whole page.
        app.view.zoom = 4.0;
        let path_of = |app: &App| match &app.doc().find_shape(id).expect("curve").kind {
            ShapeKind::Path { path, .. } => path.clone(),
            _ => panic!("a curve"),
        };
        let ring = |app: &App| {
            app.corner_widgets()
                .into_iter()
                .find(|w| (w.corner.at - at).hypot() < 1e-3)
        };
        assert_eq!(app.corner_widgets().len(), 2);
        let w = ring(&app).expect("the ring of the corner beside the node");
        // Dragged 2 mm into the corner: a rounding of millimetres, not
        // of the half millimetre before the node.
        let p = app.view.to_page(w.at);
        assert!(app.start_corner_drag(w, p, false));
        let Drag::CornerWidget(d) = app.drag.clone() else {
            panic!("no drag");
        };
        assert!(d.max > 5.0, "{}", d.max);
        app.corner_drag_to(*d, p + w.corner.inside * 2.0);
        app.drag = Drag::None;
        let now = path_of(&app);
        let rounded = fillets(&now);
        assert_eq!(rounded.len(), 1, "{rounded:?}");
        assert!(rounded[0].radius > 2.0, "{rounded:?}");
        assert!((rounded[0].corner - at).hypot() < 0.05, "{rounded:?}");
        // No sliver of a segment is left beside the arc.
        let shortest = now
            .segments()
            .map(|s| tracedraw_core::geometry::ParamCurveArclen::arclen(&s, 1e-6))
            .fold(f64::INFINITY, f64::min);
        assert!(shortest > 0.05, "{shortest}");
        // The ring stays, and dragged back out the corner is sharp
        // again, where it was.
        assert_eq!(app.corner_widgets().len(), 2);
        let w = ring(&app).unwrap_or_else(|| {
            app.corner_widgets()
                .into_iter()
                .find(|w| (w.corner.at - at).hypot() < 0.05)
                .expect("the rounded corner's ring")
        });
        let p = app.view.to_page(w.at);
        assert!(app.start_corner_drag(w, p, false));
        let Drag::CornerWidget(d) = app.drag.clone() else {
            panic!("no drag");
        };
        app.corner_drag_to(*d, p - w.corner.inside * 20.0);
        app.drag = Drag::None;
        let back = path_of(&app);
        assert!(fillets(&back).is_empty());
        assert_eq!(corners(&back).len(), 2);
        assert!(
            (back.area() - bitten.area()).abs() < 0.05,
            "{} {}",
            back.area(),
            bitten.area()
        );
    }

    /// A ring dragged as far as its corner rounds stays (#2211): the
    /// corner keeps its ring at the largest rounding it takes, and the
    /// ring dragged back out makes it sharp again.
    #[test]
    fn a_corner_rounded_as_far_as_it_goes_keeps_its_ring() {
        use tracedraw_core::shaping::{overlay, Op};
        let mut all = shapes();
        let a = disc(60.0, 60.0, 30.0);
        let b = disc(95.0, 65.0, 30.0);
        let c = disc(72.0, 27.0, 30.0);
        all.push(("lens of two discs", overlay(&a, &b, Op::Intersect)));
        all.push(("disc with a bite", overlay(&a, &b, Op::Trim)));
        all.push((
            "disc with two bites",
            overlay(&overlay(&a, &b, Op::Trim), &c, Op::Trim),
        ));
        all.push(("two discs welded", overlay(&a, &b, Op::Weld)));
        let mut failures = Vec::new();
        for (name, path) in all {
            let sharp = corners(&path);
            for i in 0..sharp.len() {
                let mut app = App::headless();
                let id = app
                    .new_shape(ShapeKind::Path {
                        path: path.clone(),
                        closed: true,
                    })
                    .expect("a curve");
                app.select(vec![id]);
                app.tool = Tool::Shape;
                app.settings.nodes.corner_widgets = true;
                let path_of = |app: &App| match &app.doc().find_shape(id).expect("curve").kind {
                    ShapeKind::Path { path, .. } => path.clone(),
                    _ => panic!("a curve"),
                };
                let at = sharp[i].at;
                let pull = |app: &mut App, by: f64| -> bool {
                    let Some(w) = app
                        .corner_widgets()
                        .into_iter()
                        // A side cut down to next to nothing reads its
                        // corner back less exactly.
                        .filter(|w| (w.corner.at - at).hypot() < 5.0)
                        .min_by(|a, b| {
                            (a.corner.at - at)
                                .hypot()
                                .total_cmp(&(b.corner.at - at).hypot())
                        })
                    else {
                        return false;
                    };
                    let p = app.view.to_page(w.at);
                    if !app.start_corner_drag(w, p, false) {
                        return false;
                    }
                    let Drag::CornerWidget(d) = app.drag.clone() else {
                        return false;
                    };
                    app.corner_drag_to(*d, p + w.corner.inside * by);
                    app.drag = Drag::None;
                    true
                };
                if app.corner_widgets().len() != sharp.len() {
                    failures.push(format!("{name} corner {i}: rings before"));
                    continue;
                }
                // Far past what it takes: the largest rounding.
                if !pull(&mut app, 1000.0) {
                    failures.push(format!("{name} corner {i}: no ring to drag"));
                    continue;
                }
                let rounded = fillets(&path_of(&app));
                if rounded.len() != 1 {
                    failures.push(format!(
                        "{name} corner {i}: {} rounded corners at the largest rounding",
                        rounded.len()
                    ));
                    continue;
                }
                if app.corner_widgets().len() != sharp.len() {
                    failures.push(format!(
                        "{name} corner {i}: {} rings of {} at the largest rounding",
                        app.corner_widgets().len(),
                        sharp.len()
                    ));
                    continue;
                }
                // And back out: sharp again.
                if !pull(&mut app, -2000.0) {
                    failures.push(format!("{name} corner {i}: no ring to drag back"));
                    continue;
                }
                let back = path_of(&app);
                if !fillets(&back).is_empty() || corners(&back).len() != sharp.len() {
                    failures.push(format!("{name} corner {i}: not sharp again"));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

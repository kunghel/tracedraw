//! The Shape tool's corner widgets (#635): a small circle inside each
//! corner of the curves being edited; dragging one into the corner rounds
//! it, live, with the arc's middle under the pointer. With nodes chosen
//! only their corners show a widget and round; with none, every corner
//! rounds together. Open curves round their inner corners too. A corner
//! already rounded (an arc tangent to two straight sides) keeps a widget
//! on its arc, so its radius can change again (#872). Corners round on
//! the page, so on a stretched curve a new rounding is round; a corner
//! rounded before the stretch keeps its stretched arc and its widget,
//! and rounds in the curve's own space until it is sharp again (#1000).
//! They show only
//! with Options > Nodes and Handles > corner widgets on (#884). See
//! docs/behavior/shape-tool.md, Corner widgets.

use crate::app::{App, Drag};
use crate::tools::Tool;
use tracedraw_core::corner_cut::{
    corners, corners_and_fillets, cut_corners, fillets, max_fillet, unround, Corner, CornerCut,
    Fillet,
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
    /// sharp).
    depth: f64,
    /// The nodes that choose it: the corner's, or every node of the arc.
    nodes: Vec<usize>,
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
            for spot in list {
                let is_chosen = spot.nodes.iter().any(|n| chosen.contains(n));
                if only_chosen && !is_chosen {
                    continue;
                }
                let c = &spot.corner;
                let to_page = *t * spot.space.inverse();
                // A rounded corner's widget sits off the middle of its arc.
                let mid = c.at + c.inside * spot.depth;
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
        let targets: Vec<ShapeId> = self
            .selection
            .iter()
            .map(|id| self.node_target(*id))
            .collect();
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
                    let (sharp, rounded) = corners_and_fillets(&(t * path.clone()));
                    let mut spots: Vec<Spot> = sharp
                        .into_iter()
                        .map(|c| Spot {
                            space: t,
                            stretched: false,
                            corner: c,
                            depth: 0.0,
                            nodes: vec![c.node],
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
                            nodes: f.nodes,
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
        for id in self.selection.iter().map(|id| self.node_target(*id)) {
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
    /// selected curves with none chosen; 0 takes the rounding off. A
    /// radius larger than a corner takes is cut to the largest every one
    /// of them fits, as a drag of a widget is. One undo step; the chosen
    /// corners stay chosen. False when nothing changed.
    pub fn set_corner_rounding(&mut self, r: f64) -> bool {
        if !r.is_finite() {
            return false;
        }
        let r = r.max(0.0);
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
            let rounded = match max_fillet(&base, Some(&only)) {
                Some(max) if r > 1e-9 && !only.is_empty() => {
                    let rr = r.min(max * 0.999);
                    used = Some(used.map_or(rr, |u: f64| u.min(rr)));
                    if rr > 1e-9 {
                        cut_corners(&base, CornerCut::Fillet(rr), Some(&only)).0
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
        let label = if r > 1e-9 { "Fillet" } else { "Clear Rounding" };
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

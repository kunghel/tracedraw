//! The Mesh Fill tool. A click on an object gives it a mesh fill of the
//! property bar's rows and columns (or selects an object that has one);
//! its nodes, those on the mesh's edge too, are clicked, dragged (their
//! colour goes with them, and they snap) and coloured from the colour
//! palette; the selected node shows a handle on each grid line leaving
//! it, dragged to bend the line; a double-click adds grid lines through
//! the point, and a double-click on a node or Delete removes the lines
//! through it (the selected node for Delete). The
//! property bar sets the grid, Smooth colour, the selected node's
//! transparency and Clear Mesh.

use crate::app::App;
use crate::i18n::tr;
use egui::{PointerButton, Response};
use tracedraw_core::{
    geometry::{Point, Shape as _},
    Color, Command, Fill, Mesh, MeshHandle, ShapeId,
};

/// Mesh nodes count from 200 in the effect node numbers (see
/// `interaction2::effect_nodes`).
pub const MESH_NODE_BASE: usize = 200;

/// The handles of mesh nodes count from here in the effect node numbers,
/// four a node (see [`mesh_handle_index`]), past any node's number.
pub const MESH_HANDLE_BASE: usize = 1_000_000;

/// The effect node number of handle `h` of mesh node `i`.
pub fn mesh_handle_index(i: usize, h: MeshHandle) -> usize {
    MESH_HANDLE_BASE + i * 4 + h as usize
}

/// The mesh node and handle an effect node number stands for, if it is
/// a handle's.
pub fn mesh_handle_of(index: usize) -> Option<(usize, MeshHandle)> {
    let k = index.checked_sub(MESH_HANDLE_BASE)?;
    let h = match k % 4 {
        0 => MeshHandle::Right,
        1 => MeshHandle::Left,
        2 => MeshHandle::Down,
        _ => MeshHandle::Up,
    };
    Some((k / 4, h))
}

/// The effect node number is a mesh handle's.
pub fn is_mesh_handle(index: usize) -> bool {
    index >= MESH_HANDLE_BASE
}

/// The mesh node an effect node number stands for, if it is a node's
/// (not a handle's or a perspective node's).
pub fn mesh_node_of(index: usize) -> Option<usize> {
    if is_mesh_handle(index) {
        return None;
    }
    index.checked_sub(MESH_NODE_BASE)
}

impl App {
    /// The selected mesh node of a selected object: (object, node).
    pub fn selected_mesh_node(&self) -> Option<(ShapeId, usize)> {
        let (id, k) = self.selected_effect_node?;
        let i = k.checked_sub(MESH_NODE_BASE)?;
        if !self.selection.contains(&id) {
            return None;
        }
        match &self.doc().find_shape(id)?.fill {
            Fill::Mesh(m) if i < m.nodes.len() => Some((id, i)),
            _ => None,
        }
    }

    /// Every chosen mesh node of the object of the chosen one: that node
    /// first, then those chosen with it by Shift+click or a marquee
    /// (discussion #2387).
    pub fn selected_mesh_nodes(&self) -> Option<(ShapeId, Vec<usize>)> {
        let (id, i) = self.selected_mesh_node()?;
        let count = match &self.doc().find_shape(id)?.fill {
            Fill::Mesh(m) => m.nodes.len(),
            _ => 0,
        };
        let mut out = vec![i];
        if let Some((key, more)) = &self.mesh_more_nodes {
            if *key == (id, MESH_NODE_BASE + i) {
                for &k in more {
                    if k < count && !out.contains(&k) {
                        out.push(k);
                    }
                }
            }
        }
        Some((id, out))
    }

    /// Choose the mesh nodes `nodes` of object `id`, the first the one
    /// the handles show on; none chooses no node.
    pub fn choose_mesh_nodes(&mut self, id: ShapeId, nodes: &[usize]) {
        let Some((&first, more)) = nodes.split_first() else {
            self.selected_effect_node = None;
            self.mesh_more_nodes = None;
            return;
        };
        let key = (id, MESH_NODE_BASE + first);
        self.selected_effect_node = Some(key);
        self.mesh_more_nodes = Some((key, more.to_vec()));
    }

    /// Shift+click on mesh node `i` of object `id`: it joins the chosen
    /// nodes, or leaves them when it was chosen. A node of another
    /// object starts a new choice.
    pub fn toggle_mesh_node(&mut self, id: ShapeId, i: usize) {
        let mut nodes = match self.selected_mesh_nodes() {
            Some((sid, nodes)) if sid == id => nodes,
            _ => Vec::new(),
        };
        match nodes.iter().position(|&k| k == i) {
            Some(at) => {
                nodes.remove(at);
            }
            None => nodes.push(i),
        }
        self.choose_mesh_nodes(id, &nodes);
    }

    /// The mesh nodes of the selected meshes inside `area` (page space):
    /// a marquee with the Mesh Fill tool chooses them, with `add` (Shift)
    /// they join those chosen. The first mesh with nodes inside wins.
    pub fn choose_mesh_nodes_in(&mut self, area: tracedraw_core::geometry::Rect, add: bool) {
        self.choose_mesh_nodes_where(|q| area.contains(q), add);
    }

    /// Choose the nodes of a selected mesh for which `inside` holds (page
    /// space): the box's, or the freehand loop's (discussion #2730);
    /// `add` (Shift) keeps those chosen already.
    pub fn choose_mesh_nodes_where(&mut self, inside: impl Fn(Point) -> bool, add: bool) {
        let found = self.selected_shapes().iter().find_map(|s| {
            let Fill::Mesh(m) = &s.fill else {
                return None;
            };
            let inside: Vec<usize> = m
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| inside(s.transform * n.pos))
                .map(|(k, _)| k)
                .collect();
            (!inside.is_empty()).then_some((s.id, inside))
        });
        let Some((id, inside)) = found else {
            if !add {
                self.choose_mesh_nodes(ShapeId(0), &[]);
            }
            return;
        };
        let mut nodes = match self.selected_mesh_nodes() {
            Some((sid, nodes)) if add && sid == id => nodes,
            _ => Vec::new(),
        };
        for k in inside {
            if !nodes.contains(&k) {
                nodes.push(k);
            }
        }
        self.choose_mesh_nodes(id, &nodes);
    }

    /// The colour the Mesh Fill bar's eyedropper reads at `p`: a selected
    /// mesh's own colour there, else what the Colour Eyedropper reads
    /// (a picture's pixel, a fill, an outline).
    pub fn mesh_color_at(&self, p: Point) -> Option<Color> {
        for s in self.selected_shapes() {
            let Fill::Mesh(m) = &s.fill else {
                continue;
            };
            let lp = s.transform.inverse() * p;
            if let Some((r, c, u, v)) = m.locate(lp) {
                let gu = (c as f64 + u) / m.cols.max(1) as f64;
                let gv = (r as f64 + v) / m.rows.max(1) as f64;
                if let Some(n) = m.sample(gu, gv) {
                    return Some(n.color);
                }
            }
        }
        self.sample_color_at(p, false)
    }

    /// The first selected object with a mesh fill, and its mesh.
    pub fn selected_mesh(&self) -> Option<(ShapeId, Mesh)> {
        self.selection
            .iter()
            .find_map(|id| match &self.doc().find_shape(*id)?.fill {
                Fill::Mesh(m) => Some((*id, m.clone())),
                _ => None,
            })
    }

    /// Give object `id` a mesh fill of the bar's grid over its bounds, its
    /// fill's colour in every node; one undo step.
    pub fn apply_mesh_fill(&mut self, id: ShapeId) {
        let Some(s) = self.doc().find_shape(id).cloned() else {
            return;
        };
        let b = s.local_path().bounding_box();
        if !(b.width() > 0.0 && b.height() > 0.0) {
            return;
        }
        let base = s.fill.preview_color().unwrap_or(Color::WHITE);
        let mesh = Mesh::new(
            b,
            self.mesh_rows.clamp(1, 50),
            self.mesh_cols.clamp(1, 50),
            base,
        );
        self.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Mesh(mesh),
        });
        self.selected_effect_node = None;
    }

    /// The Mesh Fill tool's clicks and drags.
    pub fn mesh_tool_input(&mut self, response: &Response, p: Point) {
        // The bar's eyedropper: the click samples a colour for the chosen
        // nodes (discussion #2387).
        if self.mesh_eyedropper && response.clicked_by(PointerButton::Primary) {
            self.mesh_eyedropper = false;
            match self.mesh_color_at(p) {
                Some(c) if self.color_mesh_node(c) => {
                    self.status = crate::i18n::trf(
                        "status.sampled_color",
                        &[("c", &crate::app::color_description(c))],
                    );
                }
                _ => {}
            }
            return;
        }
        if self.effect_node_input(response, p) {
            return;
        }
        // A drag from outside the nodes of a selected mesh: a marquee
        // that chooses the nodes inside it, Shift adding them (the box
        // follows the pointer and ends in `finish_node_marquee`, the
        // freehand loop in `finish_node_lasso`).
        if response.drag_started_by(PointerButton::Primary) && self.selected_mesh().is_some() {
            let start = response
                .ctx
                .input(|i| i.pointer.press_origin())
                .map(|o| self.view.to_page(o))
                .unwrap_or(p);
            self.node_lasso_keeps = response.ctx.input(|i| i.modifiers.shift);
            // Freehand on the bar's Selection mode: a loop drawn around
            // the nodes, as the Shape tool has (discussion #2730).
            self.drag = if self.node_lasso {
                crate::app::Drag::NodeLasso {
                    points: vec![start, p],
                }
            } else {
                crate::app::Drag::NodeMarquee { start, current: p }
            };
            return;
        }
        if response.clicked_by(PointerButton::Primary) {
            // Inside an object counts, filled or not: the mesh fills it.
            match self.hit_test_inside(p) {
                Some(id) => {
                    self.select(vec![id]);
                    self.selected_effect_node = None;
                    let meshed = self
                        .doc()
                        .find_shape(id)
                        .is_some_and(|s| matches!(s.fill, Fill::Mesh(_)));
                    if !meshed {
                        self.apply_mesh_fill(id);
                    }
                }
                None => {
                    self.select(Vec::new());
                    self.selected_effect_node = None;
                }
            }
        }
    }

    /// Change the selected meshes with `f`, one undo step named `label`.
    fn edit_meshes(&mut self, label: &'static str, f: impl Fn(ShapeId, &mut Mesh)) -> bool {
        let mut cmds = Vec::new();
        for id in self.selection.clone() {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            let Fill::Mesh(m) = &s.fill else {
                continue;
            };
            let mut m2 = m.clone();
            f(id, &mut m2);
            if &m2 != m {
                cmds.push(Command::SetFill {
                    shapes: vec![id],
                    fill: Fill::Mesh(m2),
                });
            }
        }
        if cmds.is_empty() {
            return false;
        }
        if let Err(e) = self.engine.run_batch(label, &cmds) {
            self.status = e.to_string();
            return false;
        }
        true
    }

    /// The bar's grid: new meshes take it and the selected ones change to
    /// it, keeping their colours where the old grid had them.
    pub fn set_mesh_grid(&mut self, rows: u32, cols: u32) {
        self.mesh_rows = rows.clamp(1, 50);
        self.mesh_cols = cols.clamp(1, 50);
        let (r, c) = (self.mesh_rows, self.mesh_cols);
        if self.edit_meshes("Mesh Grid", |_, m| {
            if (m.rows, m.cols) != (r, c) {
                *m = m.regrid(r, c);
            }
        }) {
            self.selected_effect_node = None;
        }
    }

    /// Smooth colour on or off for the selected meshes.
    pub fn set_mesh_smooth(&mut self, on: bool) {
        self.edit_meshes("Smooth Mesh Colour", |_, m| m.smooth = on);
    }

    /// The chosen nodes' opacity (1 opaque).
    pub fn set_mesh_node_alpha(&mut self, alpha: f32) {
        let Some((sid, nodes)) = self.selected_mesh_nodes() else {
            return;
        };
        let alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.edit_meshes("Mesh Node Transparency", |id, m| {
            if id == sid {
                for &i in &nodes {
                    if let Some(n) = m.nodes.get_mut(i) {
                        n.alpha = alpha;
                    }
                }
            }
        });
    }

    /// A palette colour for the chosen mesh nodes, all of them in one
    /// undo step (with the Mesh Fill or Shape tool). False when no node
    /// is chosen.
    pub fn color_mesh_node(&mut self, c: Color) -> bool {
        let Some((sid, nodes)) = self.selected_mesh_nodes() else {
            return false;
        };
        self.edit_meshes("Mesh Node Colour", |id, m| {
            if id == sid {
                for &i in &nodes {
                    if let Some(n) = m.nodes.get_mut(i) {
                        n.color = c;
                    }
                }
            }
        });
        true
    }

    /// Ctrl+click on a palette colour with mesh nodes chosen: a tenth of
    /// the colour mixed into each chosen node's own colour, all of them
    /// in one undo step, so repeated clicks move them towards it (#2616).
    /// False when no node is chosen.
    pub fn mix_into_mesh_nodes(&mut self, c: Color) -> bool {
        let Some((sid, nodes)) = self.selected_mesh_nodes() else {
            return false;
        };
        self.edit_meshes("Mesh Node Colour", |id, m| {
            if id == sid {
                for &i in &nodes {
                    if let Some(n) = m.nodes.get_mut(i) {
                        n.color = crate::ui::palette::mix(n.color, c, 0.1);
                    }
                }
            }
        });
        true
    }

    /// Delete with a mesh node selected: the grid lines through it go
    /// (not those on the edge).
    pub fn delete_mesh_lines(&mut self) -> bool {
        let Some((sid, i)) = self.selected_mesh_node() else {
            return false;
        };
        let removed = self.edit_meshes("Delete Mesh Lines", |id, m| {
            if id == sid {
                m.remove_lines_through(i);
            }
        });
        if removed {
            self.selected_effect_node = None;
        } else {
            self.status = tr("status.mesh_edge_stays");
        }
        true
    }

    /// Shuffle on the Mesh Fill bar: every selected object (its own grid,
    /// else the bar's) gets a new mesh of colours drawn from the chosen
    /// colour set, its inner nodes nudged off the grid, smooth colour on;
    /// each click a new mix, all in one undo step. False when nothing
    /// selected can take a fill.
    pub fn shuffle_mesh(&mut self) -> bool {
        let colors = self.mesh_shuffle_colors();
        let mut cmds = Vec::new();
        for (k, id) in self.selection.clone().into_iter().enumerate() {
            let Some(s) = self.doc().find_shape(id) else {
                continue;
            };
            if matches!(
                s.kind,
                tracedraw_core::ShapeKind::Group { .. } | tracedraw_core::ShapeKind::Bitmap { .. }
            ) {
                continue;
            }
            let b = s.local_path().bounding_box();
            if !(b.width() > 0.0 && b.height() > 0.0) {
                continue;
            }
            let (rows, cols) = match &s.fill {
                Fill::Mesh(m) => (m.rows, m.cols),
                _ => (self.mesh_rows, self.mesh_cols),
            };
            let seed = self.mesh_seed.wrapping_add(k as u64 * 7919);
            cmds.push(Command::SetFill {
                shapes: vec![id],
                fill: Fill::Mesh(random_mesh(b, rows, cols, &colors, seed, 0.35)),
            });
        }
        if cmds.is_empty() {
            return false;
        }
        self.mesh_seed = self.mesh_seed.wrapping_add(1);
        if let Err(e) = self.engine.run_batch("Shuffle Mesh", &cmds) {
            self.status = e.to_string();
            return false;
        }
        self.selected_effect_node = None;
        true
    }

    /// The colours Shuffle draws from: the chosen set, or the document
    /// palette (past the sets' end, or when a set is missing).
    pub fn mesh_shuffle_colors(&self) -> Vec<Color> {
        match MESH_PALETTES.get(self.mesh_palette) {
            Some((_, set)) => set.iter().map(|v| hex(*v)).collect(),
            None => {
                let c: Vec<Color> = self.palette.iter().map(|(_, c)| *c).collect();
                if c.len() >= 2 {
                    c
                } else {
                    MESH_PALETTES[0].1.iter().map(|v| hex(*v)).collect()
                }
            }
        }
    }

    /// Clear Mesh: the selected objects lose their mesh fills (no fill).
    pub fn clear_mesh(&mut self) {
        let shapes: Vec<ShapeId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| {
                self.doc()
                    .find_shape(*id)
                    .is_some_and(|s| matches!(s.fill, Fill::Mesh(_)))
            })
            .collect();
        if shapes.is_empty() {
            return;
        }
        self.run(Command::SetFill {
            shapes,
            fill: Fill::None,
        });
        self.selected_effect_node = None;
    }
}

/// The colour sets of the Mesh Fill bar's Shuffle: a name key and five
/// colours (0xRRGGBB).
pub const MESH_PALETTES: [(&str, [u32; 5]); 6] = [
    (
        "toolbar.mesh_colors_aurora",
        [0x0B1D51, 0x3A86FF, 0x2EC4B6, 0x8338EC, 0xFF6EC7],
    ),
    (
        "toolbar.mesh_colors_sunset",
        [0x2D0B3A, 0xC0306A, 0xFF7B54, 0xFFB26B, 0xFFE3A3],
    ),
    (
        "toolbar.mesh_colors_ocean",
        [0x03045E, 0x0077B6, 0x00B4D8, 0x90E0EF, 0xCAF0F8],
    ),
    (
        "toolbar.mesh_colors_candy",
        [0xFFADAD, 0xFFD6A5, 0xFDFFB6, 0xCAFFBF, 0xBDB2FF],
    ),
    (
        "toolbar.mesh_colors_forest",
        [0x1B4332, 0x2D6A4F, 0x52B788, 0xB7E4C7, 0xF1C453],
    ),
    (
        "toolbar.mesh_colors_neon",
        [0x10002B, 0xFF006E, 0xFB5607, 0xFFBE0B, 0x3A86FF],
    ),
];

fn hex(v: u32) -> Color {
    Color::rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A random mesh over `bounds`: `rows` x `cols` cells, each node a colour
/// of `colors` (or a mix of two), the inner nodes moved up to `warp` of a
/// cell off the grid, smooth colour on. The same seed gives the same mesh.
pub fn random_mesh(
    bounds: tracedraw_core::geometry::Rect,
    rows: u32,
    cols: u32,
    colors: &[Color],
    seed: u64,
    warp: f64,
) -> Mesh {
    let (rows, cols) = (rows.clamp(1, 50), cols.clamp(1, 50));
    let base = colors.first().copied().unwrap_or(Color::WHITE);
    let mut mesh = Mesh::new(bounds, rows, cols, base);
    mesh.smooth = true;
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    };
    let warp = if warp.is_finite() {
        warp.clamp(0.0, 0.45)
    } else {
        0.0
    };
    let (cw, ch) = (bounds.width() / cols as f64, bounds.height() / rows as f64);
    for r in 0..=rows {
        for c in 0..=cols {
            let i = (r * (cols + 1) + c) as usize;
            let Some(n) = mesh.nodes.get_mut(i) else {
                continue;
            };
            if !colors.is_empty() {
                let a = colors[(next() * colors.len() as f64) as usize % colors.len()];
                let b = colors[(next() * colors.len() as f64) as usize % colors.len()];
                n.color = crate::ui::palette::mix(a, b, (next() * 0.5) as f32);
            }
            let inner = r > 0 && r < rows && c > 0 && c < cols;
            if inner {
                let dx = (next() * 2.0 - 1.0) * warp * cw;
                let dy = (next() * 2.0 - 1.0) * warp * ch;
                n.pos = tracedraw_core::geometry::Point::new(n.pos.x + dx, n.pos.y + dy);
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{Rect, ShapeKind};

    fn square(app: &mut App) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 40.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.select(vec![id]);
        id
    }

    fn mesh(app: &App, id: ShapeId) -> Mesh {
        match &app.doc().find_shape(id).expect("shape").fill {
            Fill::Mesh(m) => m.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn shuffle_makes_a_new_random_mesh_each_click() {
        let mut app = App::headless();
        let id = square(&mut app);
        app.mesh_rows = 3;
        app.mesh_cols = 4;
        let depth = app.engine.history_labels().0.len();
        assert!(app.shuffle_mesh());
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let a = mesh(&app, id);
        assert_eq!((a.rows, a.cols), (3, 4));
        assert!(a.smooth);
        // Every colour comes from the set (or a mix of two of its colours).
        let set = app.mesh_shuffle_colors();
        assert_eq!(set.len(), 5);
        let distinct: std::collections::HashSet<[u8; 3]> =
            a.nodes.iter().map(|n| n.color.to_rgb8()).collect();
        assert!(distinct.len() >= 3, "{distinct:?}");
        // The edge stays on the bounds, inner nodes leave the grid.
        let grid = Mesh::new(Rect::new(0.0, 0.0, 40.0, 40.0), 3, 4, Color::WHITE);
        let moved = a
            .nodes
            .iter()
            .zip(&grid.nodes)
            .filter(|(n, g)| (n.pos - g.pos).hypot() > 1e-6)
            .count();
        assert!(moved > 0 && moved <= 2 * 3, "{moved}");
        for (n, g) in a.nodes.iter().zip(&grid.nodes) {
            let on_edge = [0.0, 40.0].contains(&g.pos.x) || [0.0, 40.0].contains(&g.pos.y);
            if on_edge {
                assert!((n.pos - g.pos).hypot() < 1e-9);
            }
        }
        // The next click keeps the grid and makes another mix.
        assert!(app.shuffle_mesh());
        let b = mesh(&app, id);
        assert_eq!((b.rows, b.cols), (3, 4));
        assert_ne!(a, b);
        app.undo();
        assert_eq!(mesh(&app, id), a);
        // The same seed gives the same mesh.
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(
            random_mesh(r, 2, 2, &set, 9, 0.3),
            random_mesh(r, 2, 2, &set, 9, 0.3)
        );
        // The document palette is a choice too; nothing selected does nothing.
        app.mesh_palette = MESH_PALETTES.len();
        assert!(app.mesh_shuffle_colors().len() >= 2);
        app.select(vec![]);
        assert!(!app.shuffle_mesh());
    }

    /// The tool's grid makes the mesh; a palette colour paints the selected
    /// node (the fill stays a mesh); transparency, smooth colour, Delete
    /// and Clear Mesh act on it.
    #[test]
    fn the_mesh_tool_builds_colours_and_clears_meshes() {
        let mut app = App::headless();
        let id = square(&mut app);
        app.tool = crate::tools::Tool::MeshFill;
        app.mesh_rows = 3;
        app.mesh_cols = 2;
        app.apply_mesh_fill(id);
        let m = mesh(&app, id);
        assert_eq!((m.rows, m.cols, m.nodes.len()), (3, 2, 12));
        // No node chosen: a palette colour fills the object as usual.
        assert!(!app.color_mesh_node(Color::rgb8(255, 0, 0)));
        // Node 4 (row 1, column 1) chosen: only it turns red.
        app.selected_effect_node = Some((id, MESH_NODE_BASE + 4));
        app.apply_fill(Fill::Solid(Color::rgb8(255, 0, 0)));
        let m = mesh(&app, id);
        assert_eq!(m.nodes[4].color, Color::rgb8(255, 0, 0));
        assert_ne!(m.nodes[3].color, Color::rgb8(255, 0, 0));
        app.set_mesh_node_alpha(0.25);
        assert_eq!(mesh(&app, id).nodes[4].alpha, 0.25);
        app.set_mesh_smooth(true);
        assert!(mesh(&app, id).smooth);
        // A new grid keeps the colours where they were.
        app.set_mesh_grid(6, 4);
        let m = mesh(&app, id);
        assert_eq!((m.rows, m.cols), (6, 4));
        assert_eq!(m.node(2, 2).expect("node").color, Color::rgb8(255, 0, 0));
        assert!(m.smooth);
        // Delete: the lines through an inner node go; an edge node's stay.
        app.selected_effect_node = Some((id, MESH_NODE_BASE + 6)); // row 1, column 1
        assert!(app.delete_mesh_lines());
        let m = mesh(&app, id);
        assert_eq!((m.rows, m.cols), (5, 3));
        app.selected_effect_node = Some((id, MESH_NODE_BASE));
        assert!(app.delete_mesh_lines());
        assert_eq!((mesh(&app, id).rows, mesh(&app, id).cols), (5, 3));
        app.clear_mesh();
        assert_eq!(app.doc().find_shape(id).expect("shape").fill, Fill::None);
    }

    /// Ctrl+click on a palette colour with mesh nodes chosen mixes a
    /// tenth of it into each of them, one undo step a click; the other
    /// nodes and the fill's kind stay (#2616).
    #[test]
    fn ctrl_click_on_a_colour_mixes_it_into_the_chosen_mesh_nodes() {
        let mut app = App::headless();
        let id = square(&mut app);
        app.tool = crate::tools::Tool::MeshFill;
        app.mesh_rows = 2;
        app.mesh_cols = 2;
        app.apply_mesh_fill(id);
        let white = Color::cmyk_pct(0.0, 0.0, 0.0, 0.0);
        let cyan = Color::cmyk_pct(100.0, 0.0, 0.0, 0.0);
        // Nodes 0 and 4 white, the rest left as they were.
        app.choose_mesh_nodes(id, &[0, 4]);
        app.apply_fill(Fill::Solid(white));
        let before = mesh(&app, id);
        assert_eq!(before.nodes[0].color, white);
        app.mix_into_fill(cyan);
        let m = mesh(&app, id);
        let tenth = crate::ui::palette::mix(white, cyan, 0.1);
        assert_eq!(m.nodes[0].color, tenth);
        assert_eq!(m.nodes[4].color, tenth);
        assert_eq!(m.nodes[1].color, before.nodes[1].color);
        // Again: another tenth of the way.
        app.mix_into_fill(cyan);
        let twice = crate::ui::palette::mix(tenth, cyan, 0.1);
        assert_eq!(mesh(&app, id).nodes[0].color, twice);
        // Each click is one undo step.
        app.undo();
        assert_eq!(mesh(&app, id).nodes[0].color, tenth);
        // No node chosen: nothing to mix into, the mesh stays a mesh.
        app.choose_mesh_nodes(id, &[]);
        app.mix_into_fill(cyan);
        assert_eq!(mesh(&app, id).nodes[0].color, tenth);
    }
}

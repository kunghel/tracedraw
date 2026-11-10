//! The Common Shapes tool's own set, My shapes: shapes the user adds from
//! the selection (Add Selection as Common Shape), drawn like the
//! built-in ones by dragging a box, and removed when no longer wanted.
//! A shape is the outline of the objects selected when it was added, in
//! one curve, kept in a unit square with the proportions it had
//! (`aspect`, width over height) so Ctrl keeps them while drawing.
//!
//! The set is kept in the preferences folder as `my_shapes.json`
//! (`{"kind": "tracedraw-my-shapes", "version": 1, "shapes": [...]}`,
//! each outline an SVG path string), in local storage in a browser. A
//! missing, damaged or foreign file gives an empty set; shapes beyond
//! the limits are left out. See docs/behavior/shape-tools.md.

use crate::app::App;
use crate::i18n::{tr, trf};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracedraw_core::geometry::{Affine, BezPath, PathEl, Point, Rect, Shape as _};
use tracedraw_core::ShapeKind;

pub const KIND: &str = "tracedraw-my-shapes";
pub const VERSION: u32 = 1;
/// The most shapes the set keeps.
pub const MAX_SHAPES: usize = 200;
/// Outlines with more elements are not kept (the set's file stays small
/// and its thumbnails cheap).
pub const MAX_ELEMENTS: usize = 5_000;
/// Larger files are not read.
const MAX_FILE: u64 = 16 * 1024 * 1024;
/// Names are cut to this many characters.
const MAX_NAME: usize = 120;
/// The browser build's key in local storage.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const WEB_KEY: &str = "tracedraw-my-shapes";

/// One shape of the set.
#[derive(Debug, Clone, PartialEq)]
pub struct MyShape {
    pub name: String,
    /// The outline in the unit square (x right, y up, both 0 to 1).
    pub unit: BezPath,
    /// Width over height of the objects it was made from.
    pub aspect: f64,
    /// Some subpath is closed (the shape is filled when drawn).
    pub closed: bool,
    /// Tells shapes apart while TraceDraw runs (the picker).
    pub uid: u64,
}

impl MyShape {
    /// The outline drawn into `r`.
    pub fn path(&self, r: Rect) -> BezPath {
        let w = r.width();
        let h = r.height();
        Affine::new([w, 0.0, 0.0, h, r.x0, r.y0]) * self.unit.clone()
    }
}

/// The far corner of a box dragged from `from` to `to` changed to the
/// proportions `aspect` (width over height), `from` staying where it is
/// (Ctrl while drawing a shape of My shapes): the longer side of the drag
/// decides the size.
pub fn keep_proportions(aspect: f64, from: Point, to: Point) -> Point {
    let a = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    };
    let d = to - from;
    let w = d.x.abs().max(d.y.abs() * a);
    let h = w / a;
    let sx = if d.x < 0.0 { -1.0 } else { 1.0 };
    let sy = if d.y < 0.0 { -1.0 } else { 1.0 };
    Point::new(from.x + w * sx, from.y + h * sy)
}

/// A shape as the file keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredShape {
    #[serde(default)]
    name: String,
    /// The unit-square outline as an SVG path.
    path: String,
    #[serde(default = "one")]
    aspect: f64,
    #[serde(default = "yes")]
    closed: bool,
}

fn one() -> f64 {
    1.0
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StoredFile {
    kind: String,
    version: u32,
    shapes: Vec<StoredShape>,
}

impl Default for StoredFile {
    fn default() -> Self {
        StoredFile {
            kind: KIND.to_string(),
            version: VERSION,
            shapes: Vec::new(),
        }
    }
}

/// The name cut to `MAX_NAME` characters, on one line.
fn clean_name(name: &str) -> String {
    let one_line: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_NAME)
        .collect();
    one_line.trim().to_string()
}

/// Every point of the outline is a finite number.
fn finite(p: &BezPath) -> bool {
    p.elements().iter().all(|e| {
        let pts: &[Point] = match e {
            PathEl::MoveTo(a) | PathEl::LineTo(a) => std::slice::from_ref(a),
            PathEl::QuadTo(a, b) => &[*a, *b],
            PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
            PathEl::ClosePath => &[],
        };
        pts.iter().all(|q| q.x.is_finite() && q.y.is_finite())
    })
}

/// The shapes a file's bytes hold, or `None` when they are not a set of
/// shapes. Outlines that do not read, are empty or too large are left
/// out, names cleaned and at most `MAX_SHAPES` kept.
pub fn shapes_from_bytes(bytes: &[u8]) -> Option<Vec<MyShape>> {
    let file: StoredFile = serde_json::from_slice(bytes).ok()?;
    if file.kind != KIND {
        return None;
    }
    if file.version > VERSION {
        log::warn!(
            "My shapes was written by a newer TraceDraw (version {}); reading what this one knows",
            file.version
        );
    }
    Some(
        file.shapes
            .into_iter()
            .filter_map(|s| {
                let unit = BezPath::from_svg(&s.path).ok()?;
                let n = unit.elements().len();
                if n == 0 || n > MAX_ELEMENTS || !finite(&unit) {
                    return None;
                }
                let aspect = if s.aspect.is_finite() && s.aspect > 1e-6 && s.aspect < 1e6 {
                    s.aspect
                } else {
                    1.0
                };
                Some(MyShape {
                    name: clean_name(&s.name),
                    unit,
                    aspect,
                    closed: s.closed,
                    uid: 0,
                })
            })
            .take(MAX_SHAPES)
            .collect(),
    )
}

/// A file holding `shapes`.
pub fn shapes_to_bytes(shapes: &[MyShape]) -> Option<Vec<u8>> {
    let file = StoredFile {
        shapes: shapes
            .iter()
            .map(|s| StoredShape {
                name: s.name.clone(),
                path: s.unit.to_svg(),
                aspect: s.aspect,
                closed: s.closed,
            })
            .collect(),
        ..StoredFile::default()
    };
    serde_json::to_vec(&file).ok()
}

/// A shape made of outlines in page millimetres, or `None` when they
/// have no area to draw into (nothing, a point, a straight level or
/// upright line) or are too large to keep.
pub fn make_shape(name: &str, page: &BezPath) -> Option<MyShape> {
    let n = page.elements().len();
    if n == 0 || n > MAX_ELEMENTS || !finite(page) {
        return None;
    }
    let b = page.bounding_box();
    let (w, h) = (b.width(), b.height());
    if !(w > 1e-6 && h > 1e-6) {
        return None;
    }
    let to_unit = Affine::new([1.0 / w, 0.0, 0.0, 1.0 / h, -b.x0 / w, -b.y0 / h]);
    let closed = page
        .elements()
        .iter()
        .any(|e| matches!(e, PathEl::ClosePath));
    Some(MyShape {
        name: clean_name(name),
        unit: to_unit * page.clone(),
        aspect: w / h,
        closed,
        uid: 0,
    })
}

/// The set and whether it was read.
#[derive(Default)]
pub struct MyShapes {
    pub shapes: Vec<MyShape>,
    loaded: bool,
    next_uid: u64,
}

/// Where the set is kept (desktop).
fn path() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("my_shapes.json"))
}

impl MyShapes {
    /// Read the set when it has not been read yet.
    pub fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        // Unit tests never read the preferences folder they share.
        if cfg!(test) {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let stored = crate::web::storage_get(WEB_KEY)
                .and_then(|t| shapes_from_bytes(t.as_bytes()))
                .unwrap_or_default();
            let added = std::mem::take(&mut self.shapes);
            let mut all = stored;
            all.extend(added);
            all.truncate(MAX_SHAPES);
            self.set_shapes(all);
            return;
        }
        #[allow(unreachable_code)]
        if let Some(p) = path() {
            self.load_from(&p);
        }
    }

    /// Replace the set with the one in the file at `p` (empty when it is
    /// missing or unreadable).
    pub fn load_from(&mut self, p: &Path) {
        self.loaded = true;
        let too_big = std::fs::metadata(p).is_ok_and(|m| m.len() > MAX_FILE);
        let shapes = if too_big {
            log::warn!("{} is too large to read", p.display());
            Vec::new()
        } else {
            match std::fs::read(p) {
                Ok(bytes) => shapes_from_bytes(&bytes).unwrap_or_else(|| {
                    log::warn!(
                        "{} is not a file of shapes; My shapes starts empty",
                        p.display()
                    );
                    Vec::new()
                }),
                Err(_) => Vec::new(),
            }
        };
        self.set_shapes(shapes);
    }

    fn set_shapes(&mut self, shapes: Vec<MyShape>) {
        self.shapes = shapes;
        for i in 0..self.shapes.len() {
            let uid = self.new_uid();
            if let Some(s) = self.shapes.get_mut(i) {
                s.uid = uid;
            }
        }
    }

    fn new_uid(&mut self) -> u64 {
        self.next_uid += 1;
        self.next_uid
    }

    /// Keep the set (unit tests keep nothing, see `ensure_loaded`).
    pub fn save(&self) {
        if cfg!(test) {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(bytes) = shapes_to_bytes(&self.shapes) {
                if crate::web::storage_set(WEB_KEY, &String::from_utf8_lossy(&bytes)).is_err() {
                    log::warn!("could not keep My shapes in the browser's storage");
                }
            }
            return;
        }
        #[allow(unreachable_code)]
        if let Some(p) = path() {
            if let Err(e) = self.save_to(&p) {
                log::warn!("could not keep My shapes in {}: {e}", p.display());
            }
        }
    }

    /// Write the file to `p`, through a temporary file so that a failed
    /// write leaves the previous one as it was.
    pub fn save_to(&self, p: &Path) -> std::io::Result<()> {
        let bytes = shapes_to_bytes(&self.shapes)
            .ok_or_else(|| std::io::Error::other("My shapes could not be written"))?;
        let tmp = p.with_extension("json.tmp");
        crate::files::write_creating(&tmp, bytes)?;
        std::fs::rename(&tmp, p)
    }

    pub fn get(&self, uid: u64) -> Option<&MyShape> {
        self.shapes.iter().find(|s| s.uid == uid)
    }

    /// Add a shape at the end; its uid, or `None` when the set is full.
    pub fn push(&mut self, mut shape: MyShape) -> Option<u64> {
        if self.shapes.len() >= MAX_SHAPES {
            return None;
        }
        shape.uid = self.new_uid();
        let uid = shape.uid;
        self.shapes.push(shape);
        Some(uid)
    }

    /// Take a shape out of the set.
    pub fn remove(&mut self, uid: u64) -> Option<MyShape> {
        let i = self.shapes.iter().position(|s| s.uid == uid)?;
        Some(self.shapes.remove(i))
    }
}

impl App {
    /// The shape of My shapes the Common Shapes tool draws, when one of
    /// them is picked.
    pub fn picked_my_shape(&self) -> Option<&MyShape> {
        self.my_shape.and_then(|uid| self.my_shapes.get(uid))
    }

    /// Add Selection as Common Shape: the outlines of the selected
    /// objects as one shape at the end of My shapes, kept at once and
    /// picked for the Common Shapes tool; false when nothing selected has
    /// an outline with an area or the set is full.
    pub fn add_selection_as_common_shape(&mut self) -> bool {
        self.my_shapes.ensure_loaded();
        let shapes = self.selected_shapes();
        let mut outline = BezPath::new();
        for s in &shapes {
            if matches!(s.kind, ShapeKind::Bitmap { .. }) {
                continue;
            }
            let local = s.local_path_with(&self.doc().symbols);
            outline.extend(s.transform * local);
        }
        let name = match shapes.as_slice() {
            [one] => one
                .name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| crate::ui::dockers::kind_name(&one.kind)),
            _ => trf(
                "shapes.mine_n",
                &[("n", &(self.my_shapes.shapes.len() + 1).to_string())],
            ),
        };
        let Some(shape) = make_shape(&name, &outline) else {
            self.status = tr("shapes.mine_nothing");
            return false;
        };
        let name = shape.name.clone();
        match self.my_shapes.push(shape) {
            Some(uid) => {
                self.my_shapes.save();
                self.my_shape = Some(uid);
                self.status = trf("shapes.mine_added", &[("name", &name)]);
                true
            }
            None => {
                self.status = trf("shapes.mine_full", &[("n", &MAX_SHAPES.to_string())]);
                false
            }
        }
    }

    /// Remove a shape from My shapes (kept at once); the built-in shape
    /// picked before is drawn again when it was the one picked.
    pub fn remove_common_shape(&mut self, uid: u64) -> bool {
        let Some(gone) = self.my_shapes.remove(uid) else {
            return false;
        };
        self.my_shapes.save();
        if self.my_shape == Some(uid) {
            self.my_shape = None;
        }
        self.status = trf("shapes.mine_removed", &[("name", &gone.name)]);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use tracedraw_core::{Command, Fill};

    fn triangle(app: &mut App) -> tracedraw_core::ShapeId {
        let mut path = BezPath::new();
        path.move_to((10.0, 10.0));
        path.line_to((50.0, 10.0));
        path.line_to((30.0, 30.0));
        path.close_path();
        let id = app
            .new_shape(ShapeKind::Path { path, closed: true })
            .expect("triangle");
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(tracedraw_core::Color::BLACK),
        });
        id
    }

    #[test]
    fn the_selection_becomes_a_shape_that_is_kept_and_read_back() {
        let mut app = App::headless();
        let id = triangle(&mut app);
        assert!(!app.add_selection_as_common_shape(), "nothing selected");
        app.select(vec![id]);
        assert!(app.add_selection_as_common_shape());
        let s = app.picked_my_shape().expect("picked").clone();
        // 40 by 20: twice as wide as high, in the unit square.
        assert!((s.aspect - 2.0).abs() < 1e-9);
        assert!(s.closed);
        let b = s.unit.bounding_box();
        assert!((b.x0, b.y0, b.x1, b.y1) == (0.0, 0.0, 1.0, 1.0), "{b:?}");
        // Kept in a file and read back the same (as after a restart).
        let dir = std::env::temp_dir().join(format!("tracedraw-my-shapes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("my_shapes.json");
        app.my_shapes.save_to(&file).expect("saved");
        let mut again = MyShapes::default();
        again.load_from(&file);
        assert_eq!(again.shapes.len(), 1);
        let back = &again.shapes[0];
        assert_eq!(back.name, s.name);
        assert_eq!(back.closed, s.closed);
        assert!((back.aspect - s.aspect).abs() < 1e-9);
        let (p, q) = (
            back.path(Rect::new(0.0, 0.0, 40.0, 20.0)),
            s.path(Rect::new(0.0, 0.0, 40.0, 20.0)),
        );
        assert!(
            (p.area() - q.area()).abs() < 1e-6,
            "{} {}",
            p.area(),
            q.area()
        );
        // Removed: the set is empty and the tool draws the built-in shape.
        let uid = s.uid;
        assert!(app.remove_common_shape(uid));
        assert!(app.my_shapes.shapes.is_empty());
        assert!(app.my_shape.is_none());
        assert!(!app.remove_common_shape(uid));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_and_foreign_files_give_an_empty_set() {
        assert!(shapes_from_bytes(b"not json").is_none());
        assert!(shapes_from_bytes(br#"{"kind":"tracedraw-shelf","shapes":[]}"#).is_none());
        let some = br#"{"kind":"tracedraw-my-shapes","version":9,"shapes":[
            {"name":"ok","path":"M0 0L1 0L1 1Z","aspect":2},
            {"name":"bad","path":"not a path"},
            {"name":"empty","path":""},
            {"name":"odd\nname","path":"M0 0L1 1","aspect":-3}
        ]}"#;
        let shapes = shapes_from_bytes(some).expect("read");
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[1].name, "odd name");
        assert_eq!(shapes[1].aspect, 1.0);
        let mut set = MyShapes::default();
        set.load_from(&std::env::temp_dir().join("tracedraw-no-such-dir/my_shapes.json"));
        assert!(set.shapes.is_empty());
        // Lines without an area cannot be drawn into a box.
        let mut line = BezPath::new();
        line.move_to((0.0, 5.0));
        line.line_to((10.0, 5.0));
        assert!(make_shape("line", &line).is_none());
        assert!(make_shape("none", &BezPath::new()).is_none());
    }

    #[test]
    fn a_shape_of_my_own_is_drawn_by_dragging_and_ctrl_keeps_its_proportions() {
        let mut app = App::headless();
        let id = triangle(&mut app);
        app.select(vec![id]);
        assert!(app.add_selection_as_common_shape());
        app.tool = Tool::CommonShapes;
        // The preview follows the shape of my own.
        let preview = app
            .creation_preview(Point::new(0.0, 0.0), Point::new(10.0, 10.0), false)
            .expect("preview");
        assert!((preview.area().abs() - 50.0).abs() < 1e-6);
        let before = app
            .doc()
            .all_layers()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        assert!(app.finish_tools2_box(Point::new(100.0, 100.0), Point::new(160.0, 130.0)));
        let after = app
            .doc()
            .all_layers()
            .map(|l| l.shapes.len())
            .sum::<usize>();
        assert_eq!(after, before + 1);
        let drawn = app
            .doc()
            .find_shape(app.selection[0])
            .expect("drawn")
            .clone();
        let ShapeKind::Path { path, closed } = &drawn.kind else {
            panic!("{:?}", drawn.kind);
        };
        assert!(*closed);
        let b = path.bounding_box();
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (100.0, 100.0, 160.0, 130.0));
        // Half the box: the triangle's area.
        assert!((path.area().abs() - 900.0).abs() < 1e-6);
        // Ctrl keeps the proportions (2 : 1), from the larger side of the drag.
        let s = app.picked_my_shape().expect("picked").clone();
        let q = keep_proportions(s.aspect, Point::new(0.0, 0.0), Point::new(10.0, 30.0));
        assert_eq!((q.x, q.y), (60.0, 30.0));
        let q = keep_proportions(s.aspect, Point::new(0.0, 0.0), Point::new(-40.0, -5.0));
        assert_eq!((q.x, q.y), (-40.0, -20.0));
        // A built-in shape again once mine is removed.
        let uid = s.uid;
        app.remove_common_shape(uid);
        let built_in = app
            .creation_preview(Point::new(0.0, 0.0), Point::new(10.0, 10.0), false)
            .expect("preview");
        assert_eq!(
            built_in,
            app.common_shape.path(Rect::new(0.0, 0.0, 10.0, 10.0))
        );
    }

    #[test]
    fn groups_and_several_objects_make_one_shape() {
        let mut app = App::headless();
        let a = triangle(&mut app);
        let b = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(60.0, 10.0, 70.0, 30.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![a, b]);
        assert!(app.add_selection_as_common_shape());
        let s = app.picked_my_shape().expect("picked");
        assert!((s.aspect - 3.0).abs() < 1e-9);
        assert!(s.name.contains('1'), "{}", s.name);
        let drawn = s.path(Rect::new(10.0, 10.0, 70.0, 30.0));
        // The triangle and the rectangle where they were, one curve.
        let b = drawn.bounding_box();
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (10.0, 10.0, 70.0, 30.0));
        let starts = drawn
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count();
        assert_eq!(starts, 2);
    }
}

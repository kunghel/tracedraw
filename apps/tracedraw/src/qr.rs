//! QR code objects: a group whose children (the background and the dark
//! pixels) are made from its settings ([`QrCode`]), edited in the
//! Properties docker. Object > Insert > QR Code places one at once. See
//! docs/behavior/qr-code.md.

use crate::app::App;
use tracedraw_core::{
    document::ShapeKind,
    geometry::{BezPath, Point, Rect, Shape as _},
    Command, Fill, QrCode, QrKind, QrSymbol, Shape, ShapeId,
};

/// Light pixels around the code on every side (its quiet zone), in
/// pixels: enough for scanners to find the code against what lies around.
pub const QUIET: usize = 2;

/// The pixel grid of a code: `cols` by `rows` pixels, the dark ones row
/// by row from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub dark: Vec<bool>,
}

/// The pixel grid of a code of the kind `symbol` holding `payload`.
pub fn modules(payload: &str, symbol: QrSymbol) -> Result<Grid, String> {
    if payload.trim().is_empty() {
        return Err(crate::i18n::tr("dialog.qr_empty"));
    }
    match symbol {
        QrSymbol::Qr => {
            let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|e| e.to_string())?;
            let n = code.width();
            let mut dark = Vec::with_capacity(n * n);
            for y in 0..n {
                for x in 0..n {
                    dark.push(code[(x, y)] == qrcode::Color::Dark);
                }
            }
            Ok(Grid {
                cols: n,
                rows: n,
                dark,
            })
        }
        // Data Matrix (#4228): the smallest square symbol that holds the
        // text, with ECC 200 error correction.
        QrSymbol::DataMatrix => {
            let code = datamatrix::DataMatrix::encode_str(
                payload,
                datamatrix::SymbolList::default().enforce_square(),
            )
            .map_err(|_| crate::i18n::tr("qr.too_long"))?;
            let bitmap = code.bitmap();
            let (cols, rows) = (bitmap.width(), bitmap.height());
            let mut dark = vec![false; cols * rows];
            for (x, y) in bitmap.pixels() {
                if x < cols && y < rows {
                    dark[y * cols + x] = true;
                }
            }
            Ok(Grid { cols, rows, dark })
        }
        // Rectangular micro QR (#4228): the shortest symbol that holds
        // the text at medium error correction.
        QrSymbol::Rmqr => {
            let code = qrcode_generator::rmqr::Encoder::new(
                qrcode_generator::rmqr::ErrorCorrection::Medium,
            )
            .encode_text(payload)
            .map_err(|_| crate::i18n::tr("qr.too_long"))?;
            let (cols, rows) = (code.width(), code.height());
            let mut dark = Vec::with_capacity(cols * rows);
            for y in 0..rows {
                for x in 0..cols {
                    dark.push(code.module(x, y).unwrap_or(false));
                }
            }
            Ok(Grid { cols, rows, dark })
        }
    }
}

/// The size of the whole object (code and quiet zone) in millimetres.
pub fn size_mm(g: &Grid, qr: &QrCode) -> (f64, f64) {
    let p = qr.pixel_mm.max(0.01);
    (
        (g.cols + 2 * QUIET) as f64 * p,
        (g.rows + 2 * QUIET) as f64 * p,
    )
}

/// The dark pixels as one path, each row's runs of dark pixels as one
/// rectangle, with the object's top-left corner at `top_left` (y up).
pub fn pixels_path(g: &Grid, pixel: f64, top_left: Point) -> BezPath {
    let mut path = BezPath::new();
    let n = g.cols;
    for y in 0..g.rows {
        let mut x = 0;
        while x < n {
            if !g.dark[y * n + x] {
                x += 1;
                continue;
            }
            let start = x;
            while x < n && g.dark[y * n + x] {
                x += 1;
            }
            let x0 = top_left.x + (start + QUIET) as f64 * pixel;
            let x1 = top_left.x + (x + QUIET) as f64 * pixel;
            let y1 = top_left.y - (y + QUIET) as f64 * pixel;
            let y0 = y1 - pixel;
            path.extend(Rect::new(x0, y0, x1, y1).to_path(0.01));
        }
    }
    path
}

/// The outlines of the dark pixels of a `cols` by `rows` grid (row by
/// row from the top): each patch of touching dark pixels as one outline
/// with its holes, its corners only where it turns, so a code has a few
/// nodes per patch instead of four for every pixel (#4398). One pixel is
/// `pixel` millimetres; the grid's top-left corner is at `top_left`
/// (y up). Outer outlines and holes run opposite ways, so either fill
/// rule fills the dark pixels.
pub fn grid_outline(cols: usize, rows: usize, dark: &[bool], pixel: f64, top_left: Point) -> BezPath {
    use std::collections::HashMap;
    let on = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && (x as usize) < cols
            && (y as usize) < rows
            && dark.get(y as usize * cols + x as usize).copied().unwrap_or(false)
    };
    // The edges between a dark and a light pixel, each going round its
    // dark pixel the same way (grid points, y down).
    let mut out: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    let mut edges = 0usize;
    for y in 0..rows as i64 {
        for x in 0..cols as i64 {
            if !on(x, y) {
                continue;
            }
            let mut add = |a: (i64, i64), b: (i64, i64)| {
                out.entry(a).or_default().push(b);
                edges += 1;
            };
            if !on(x - 1, y) {
                add((x, y), (x, y + 1));
            }
            if !on(x, y + 1) {
                add((x, y + 1), (x + 1, y + 1));
            }
            if !on(x + 1, y) {
                add((x + 1, y + 1), (x + 1, y));
            }
            if !on(x, y - 1) {
                add((x + 1, y), (x, y));
            }
        }
    }
    let to_page = |(x, y): (i64, i64)| {
        Point::new(
            top_left.x + x as f64 * pixel,
            top_left.y - y as f64 * pixel,
        )
    };
    let mut starts: Vec<(i64, i64)> = out.keys().copied().collect();
    starts.sort_unstable_by_key(|&(x, y)| (y, x));
    let mut path = BezPath::new();
    for start in starts {
        while let Some(first) = out.get_mut(&start).and_then(|v| v.pop()) {
            // Walk the loop, taking the edges as they come.
            let mut pts = vec![start, first];
            let mut at = first;
            let mut guard = 0;
            while at != start && guard <= edges {
                let Some(next) = out.get_mut(&at).and_then(|v| v.pop()) else {
                    break;
                };
                pts.push(next);
                at = next;
                guard += 1;
            }
            pts.pop();
            // Only the corners: points where the outline turns.
            let n = pts.len();
            let corners: Vec<(i64, i64)> = (0..n)
                .filter(|&i| {
                    let a = pts[(i + n - 1) % n];
                    let b = pts[i];
                    let c = pts[(i + 1) % n];
                    (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0) != 0
                })
                .map(|i| pts[i])
                .collect();
            if corners.len() < 3 {
                continue;
            }
            path.move_to(to_page(corners[0]));
            for &q in &corners[1..] {
                path.line_to(to_page(q));
            }
            path.close_path();
        }
    }
    path
}

/// The children of a QR code object with its top-left corner at
/// `top_left`: the background (light fill, no outline) and the dark pixels
/// (dark fill and the outline).
fn children(app: &mut App, qr: &QrCode, top_left: Point) -> Result<Vec<Shape>, String> {
    code_children(qr, top_left, &mut || app.engine.new_shape_id())
}

/// [`children`] with the new objects' ids from `new_id` (Print Merge
/// makes each record's code with them, #4396).
pub(crate) fn code_children(
    qr: &QrCode,
    top_left: Point,
    new_id: &mut dyn FnMut() -> tracedraw_core::ShapeId,
) -> Result<Vec<Shape>, String> {
    let g = modules(&qr.payload(), qr.symbol)?;
    let pixel = qr.pixel_mm.max(0.01);
    let (w, h) = size_mm(&g, qr);
    let mut back = Shape::new(
        new_id(),
        ShapeKind::Rect {
            rect: Rect::new(top_left.x, top_left.y - h, top_left.x + w, top_left.y),
            radius: 0.0,
            corners: None,
        },
    );
    back.fill = qr.light.clone();
    back.stroke = None;
    back.name = Some(crate::i18n::tr("qr.background"));
    let mut pixels = Shape::new(
        new_id(),
        ShapeKind::Path {
            path: pixels_path(&g, pixel, top_left),
            closed: true,
        },
    );
    pixels.fill = qr.dark.clone();
    pixels.stroke = qr.outline.clone();
    pixels.name = Some(crate::i18n::tr("qr.pixels"));
    Ok(vec![back, pixels])
}

/// Where a QR code object's top-left corner is: its background's.
pub(crate) fn top_left(s: &Shape) -> Option<Point> {
    match &s.kind {
        ShapeKind::Group { children } => children.iter().find_map(|c| match &c.kind {
            ShapeKind::Rect { rect, .. } => Some(Point::new(rect.x0, rect.y1)),
            _ => None,
        }),
        _ => None,
    }
}

impl App {
    /// Object > Insert > QR Code: a QR code of the default settings at
    /// the centre of the page, selected.
    pub fn insert_qr_code(&mut self) -> Option<ShapeId> {
        let layer = self.active_layer()?;
        let qr = QrCode::default();
        let g = modules(&qr.payload(), qr.symbol).ok()?;
        let (w, h) = size_mm(&g, &qr);
        let c = self.page_rect().center();
        let kids = children(self, &qr, Point::new(c.x - w / 2.0, c.y + h / 2.0)).ok()?;
        let id = self.engine.new_shape_id();
        let mut group = Shape::new(id, ShapeKind::Group { children: kids });
        group.fill = Fill::None;
        group.stroke = None;
        group.name = Some(crate::i18n::tr("qr.object_name"));
        group.qr = Some(Box::new(qr));
        self.run(Command::AddShape {
            layer,
            shape: group,
        });
        self.select(vec![id]);
        Some(id)
    }

    /// Give the QR code object `id` new settings: its children are made
    /// again at the same top-left corner, in one undo step. An error (no
    /// content, too much for a code) changes nothing.
    pub fn set_qr_code(&mut self, id: ShapeId, qr: QrCode) -> Result<(), String> {
        let shape = self
            .doc()
            .find_shape(id)
            .cloned()
            .ok_or_else(|| "no such object".to_string())?;
        let anchor = top_left(&shape).unwrap_or_else(|| {
            let b = self.visual_bounds(&shape);
            Point::new(b.x0, b.y1)
        });
        let kids = children(self, &qr, anchor)?;
        self.engine
            .run_batch(
                "QR Code",
                &[
                    Command::SetShapeKind {
                        shape: id,
                        kind: ShapeKind::Group { children: kids },
                    },
                    Command::SetQrCode {
                        shape: id,
                        qr: Some(Box::new(qr)),
                    },
                ],
            )
            .map_err(|e| e.to_string())?;
        self.raster.borrow_mut().invalidate();
        Ok(())
    }
}

/// A kind of content picked in Properties whose fields make no content
/// yet (Email, with nothing to carry over from a URL): Properties shows
/// and edits it while the object keeps its code, until the fields make
/// content (#1845).
#[derive(Debug, Clone, PartialEq)]
pub struct QrDraft {
    pub kind: QrKind,
    pub fields: Vec<String>,
}

impl App {
    /// Properties' kind and fields of the QR code object `id` are now
    /// `kind` and `fields`: when they make content the code is made again
    /// (as [`App::set_qr_code`]) and `Ok(None)` comes back; when they make
    /// none (a kind just picked, a field emptied) the object keeps its
    /// code, nothing goes into the history, and the draft for Properties
    /// to show comes back. An error (too much for a code) changes nothing.
    pub fn edit_qr_content(
        &mut self,
        id: ShapeId,
        kind: QrKind,
        fields: Vec<String>,
    ) -> Result<Option<QrDraft>, String> {
        let qr = self
            .doc()
            .find_shape(id)
            .and_then(|s| s.qr.as_deref().cloned())
            .ok_or_else(|| "not a QR code object".to_string())?;
        let next = QrCode {
            kind,
            fields,
            ..qr.clone()
        };
        if next.payload().trim().is_empty() {
            if next.kind == qr.kind && next.fields == qr.fields {
                return Ok(None);
            }
            return Ok(Some(QrDraft {
                kind: next.kind,
                fields: next.fields,
            }));
        }
        if next != qr {
            self.set_qr_code(id, next)?;
        }
        Ok(None)
    }
}

/// The brightness (0 to 1) a fill shows on average: a gradient's mean
/// over its stops, other fills their representative colour; paper white
/// for no fill.
fn fill_luma(f: &Fill) -> f32 {
    match f {
        Fill::Fountain(g) => {
            let stops = g.resolved_stops();
            if stops.is_empty() {
                g.first_color().luminance()
            } else {
                stops.iter().map(|s| s.color.luminance()).sum::<f32>() / stops.len() as f32
            }
        }
        other => other.preview_color().map_or(1.0, |c| c.luminance()),
    }
}

/// Validate: draw the code as it will look (its dark and light fills'
/// brightness) and read it back with a QR decoder. `Ok` with the text read
/// when it matches what was encoded; `Err` saying why otherwise (too
/// little contrast, nothing found, different content).
pub fn validate(qr: &QrCode) -> Result<String, String> {
    use crate::i18n::tr;
    let payload = qr.payload();
    let g = modules(&payload, qr.symbol)?;
    let (d, l) = (fill_luma(&qr.dark), fill_luma(&qr.light));
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (dv, lv) = (to8(d), to8(l));
    // Scanners need dark pixels clearly darker than the background.
    if (lv as i32 - dv as i32) < 64 {
        return Err(tr("qr.validate_contrast"));
    }
    match qr.symbol {
        QrSymbol::Qr => {}
        // A Data Matrix is read back from its pixels, as a scanner sees
        // them once it has found the symbol (#4228).
        QrSymbol::DataMatrix => {
            let read = datamatrix::DataMatrix::decode(&g.dark, g.cols)
                .map_err(|_| tr("qr.validate_not_found"))?;
            return match String::from_utf8(read) {
                Ok(text) if text == payload => Ok(text),
                _ => Err(tr("qr.validate_mismatch")),
            };
        }
        // No reader for rectangular codes here: the contrast is what can
        // be checked, and the encoder made the symbol to the standard.
        QrSymbol::Rmqr => return Ok(payload),
    }
    let (cols, rows, dark) = (g.cols, g.rows, &g.dark);
    let scale = 6;
    let pad = QUIET + 2;
    let (w, h) = ((cols + 2 * pad) * scale, (rows + 2 * pad) * scale);
    let img = image::GrayImage::from_fn(w as u32, h as u32, |x, y| {
        let (mx, my) = (x as usize / scale, y as usize / scale);
        let on = mx >= pad
            && my >= pad
            && mx - pad < cols
            && my - pad < rows
            && dark[(my - pad) * cols + (mx - pad)];
        // Outside the object, paper white.
        let inside =
            mx >= 2 && my >= 2 && mx < cols + 2 * QUIET + 2 && my < rows + 2 * QUIET + 2;
        image::Luma([if on {
            dv
        } else if inside {
            lv
        } else {
            255
        }])
    });
    let mut prepared = rqrr::PreparedImage::prepare(img);
    let grids = prepared.detect_grids();
    let Some(grid) = grids.first() else {
        return Err(tr("qr.validate_not_found"));
    };
    match grid.decode() {
        Ok((_, text)) if text == payload => Ok(text),
        Ok(_) => Err(tr("qr.validate_mismatch")),
        Err(_) => Err(tr("qr.validate_not_found")),
    }
}

#[cfg(test)]
mod tests {
    /// #4398: the dark pixels' outlines have a node only where they turn,
    /// and fill exactly the dark pixels, holes included.
    #[test]
    fn the_outline_of_dark_pixels_has_few_nodes_and_keeps_holes() {
        use tracedraw_core::geometry::Shape as _;
        // A ring of 3 x 3 with a hole, and a pixel touching it at a corner.
        let dark = [
            true, true, true, false, //
            true, false, true, false, //
            true, true, true, false, //
            false, false, false, true,
        ];
        let path = grid_outline(4, 4, &dark, 1.0, Point::new(0.0, 4.0));
        let nodes = path
            .elements()
            .iter()
            .filter(|e| !matches!(e, tracedraw_core::geometry::PathEl::ClosePath))
            .count();
        // Ring: 4 outer + 4 hole corners; the lone pixel: 4.
        assert_eq!(nodes, 12, "{path:?}");
        for y in 0..4 {
            for x in 0..4 {
                let p = Point::new(x as f64 + 0.5, 4.0 - y as f64 - 0.5);
                assert_eq!(path.winding(p) != 0, dark[y * 4 + x], "pixel {x},{y}");
                assert_eq!(path.winding(p).rem_euclid(2) == 1, dark[y * 4 + x], "even-odd {x},{y}");
            }
        }
        // A real code: far fewer nodes than four a pixel.
        let g = modules("https://example.org/tracedraw?id=12345", QrSymbol::Qr).unwrap();
        let path = grid_outline(g.cols, g.rows, &g.dark, 1.0, Point::ZERO);
        let dark_px = g.dark.iter().filter(|d| **d).count();
        assert!(path.elements().len() < dark_px * 2, "{} for {dark_px}", path.elements().len());
    }

    use super::*;
    use tracedraw_core::{Color, QrKind};

    #[test]
    fn insert_places_a_selected_qr_code_that_reads_back() {
        let mut app = App::headless();
        let id = app.insert_qr_code().expect("placed");
        assert_eq!(app.selection, vec![id]);
        let s = app.doc().find_shape(id).expect("in the drawing").clone();
        let qr = s.qr.clone().expect("a QR code object");
        assert!(matches!(&s.kind, ShapeKind::Group { children } if children.len() == 2));
        assert_eq!(validate(&qr), Ok(qr.payload()));
    }

    #[test]
    fn editing_makes_the_code_again_in_one_step_at_the_same_corner() {
        let mut app = App::headless();
        let id = app.insert_qr_code().expect("placed");
        let before = app.doc().find_shape(id).cloned().expect("shape");
        let corner = top_left(&before).expect("corner");
        let mut qr = (**before.qr.as_ref().expect("qr")).clone();
        qr.kind = QrKind::Phone;
        qr.fields = vec!["+351 220 000 000".into()];
        qr.pixel_mm = 1.0;
        qr.dark = Fill::Solid(Color::rgb8(0, 0, 120));
        app.set_qr_code(id, qr.clone()).expect("set");
        let after = app.doc().find_shape(id).cloned().expect("shape");
        assert_eq!(after.qr.as_deref(), Some(&qr));
        assert_eq!(top_left(&after), Some(corner));
        let ShapeKind::Group { children } = &after.kind else {
            panic!("a group");
        };
        assert_eq!(children[1].fill, qr.dark);
        let g = modules("tel:+351 220 000 000", QrSymbol::Qr).expect("modules");
        let (side, _) = size_mm(&g, &qr);
        let b = app.visual_bounds(&children[0]);
        assert!((b.width() - side).abs() < 1e-6, "{b:?} {side}");
        // One undo step brings the old code back.
        app.undo();
        assert_eq!(
            app.doc().find_shape(id).and_then(|s| s.qr.clone()),
            before.qr
        );
        // Nothing to encode: nothing changes.
        let empty = QrCode {
            fields: vec![String::new()],
            ..QrCode::default()
        };
        assert!(app.set_qr_code(id, empty).is_err());
        assert_eq!(
            app.doc().find_shape(id).and_then(|s| s.qr.clone()),
            before.qr
        );
    }

    #[test]
    fn each_kind_writes_the_form_scanners_read() {
        let qr = |kind: QrKind, fields: &[&str]| QrCode {
            kind,
            fields: fields.iter().map(|s| s.to_string()).collect(),
            ..QrCode::default()
        };
        assert_eq!(qr(QrKind::Phone, &["123"]).payload(), "tel:123");
        assert_eq!(qr(QrKind::Sms, &["123", "hi"]).payload(), "SMSTO:123:hi");
        assert_eq!(
            qr(QrKind::Email, &["a@b.c", "s", "m"]).payload(),
            "MATMSG:TO:a@b.c;SUB:s;BODY:m;;"
        );
        assert_eq!(
            qr(QrKind::Location, &["41.15", "-8.61"]).payload(),
            "geo:41.15,-8.61"
        );
        let v = qr(QrKind::Contact, &["Ana", "Silva", "", "123", "", "", ""]).payload();
        assert!(v.starts_with("BEGIN:VCARD\nVERSION:3.0\nN:Silva;Ana\nFN:Ana Silva\n"));
        assert!(v.contains("TEL:123\n") && v.ends_with("END:VCARD"));
        assert_eq!(qr(QrKind::Location, &["1", ""]).payload(), "");
        // Fields shared by name carry over to another kind.
        assert_eq!(
            QrKind::Sms.carry(QrKind::Phone, &["123".into()]),
            vec!["123".to_string(), String::new()]
        );
        for kind in QrKind::ALL {
            let fields: Vec<&str> = kind.fields().iter().map(|_| "1").collect();
            let q = qr(kind, &fields);
            assert_eq!(validate(&q), Ok(q.payload()), "{kind:?}");
        }
    }

    /// Every kind can be picked from a new URL code and filled in, as
    /// Properties does it: a kind whose fields carry nothing over (Email,
    /// Phone number, SMS, Text, Location) stays picked as a draft while
    /// the object keeps its code, and its fields, filled in one by one,
    /// make the code of that kind. Before, picking it made empty content,
    /// which could not be encoded, so the kind went back to URL (#1845).
    #[test]
    fn every_kind_can_be_picked_and_filled_in() {
        for kind in QrKind::ALL {
            let mut app = App::headless();
            let id = app.insert_qr_code().expect("placed");
            let start = app.doc().find_shape(id).and_then(|s| s.qr.clone());
            let url = start.as_deref().cloned().expect("qr");
            let steps = app.engine.history_labels().0.len();
            let carried = kind.carry(url.kind, &url.fields);
            let mut draft = app
                .edit_qr_content(id, kind, carried.clone())
                .expect("picked");
            if matches!(kind, QrKind::Url | QrKind::Contact) {
                // The address carries over: the code is made at once.
                assert_eq!(draft, None);
            } else {
                assert_eq!(
                    draft,
                    Some(QrDraft {
                        kind,
                        fields: carried.clone()
                    }),
                    "{kind:?}"
                );
                // What Properties did before: the code made at once from
                // the empty fields, which fails, so the kind went back.
                let at_once = QrCode {
                    kind,
                    fields: carried.clone(),
                    ..url.clone()
                };
                assert!(app.set_qr_code(id, at_once).is_err(), "{kind:?}");
                // The object keeps its code; nothing to undo.
                assert_eq!(app.doc().find_shape(id).and_then(|s| s.qr.clone()), start);
                assert_eq!(app.engine.history_labels().0.len(), steps);
            }
            let mut fields = carried;
            for i in 0..fields.len() {
                if draft.is_none() && matches!(kind, QrKind::Url | QrKind::Contact) {
                    break;
                }
                fields[i] = format!("{}", i + 1);
                draft = app
                    .edit_qr_content(id, kind, fields.clone())
                    .expect("edited");
            }
            assert_eq!(draft, None, "{kind:?}");
            let qr = app
                .doc()
                .find_shape(id)
                .and_then(|s| s.qr.as_deref().cloned())
                .expect("qr");
            assert_eq!(qr.kind, kind);
            assert_eq!(validate(&qr), Ok(qr.payload()), "{kind:?}");
            // Emptying the field again keeps the code, shown as a draft.
            if kind == QrKind::Phone {
                let d = app
                    .edit_qr_content(id, kind, vec![String::new()])
                    .expect("draft");
                assert!(d.is_some());
                assert_eq!(
                    app.doc()
                        .find_shape(id)
                        .and_then(|s| s.qr.as_deref().cloned()),
                    Some(qr)
                );
            }
        }
    }

    #[test]
    fn validate_reports_too_little_contrast() {
        let q = QrCode {
            dark: Fill::Solid(Color::rgb8(200, 200, 200)),
            ..QrCode::default()
        };
        assert!(validate(&q).is_err());
        let inverted = QrCode {
            dark: Fill::Solid(Color::WHITE),
            light: Fill::Solid(Color::BLACK),
            ..QrCode::default()
        };
        assert!(validate(&inverted).is_err());
    }

    #[test]
    fn qr_code_objects_survive_the_native_format() {
        let mut app = App::headless();
        let id = app.insert_qr_code().expect("placed");
        let json = app.doc().to_json().expect("json");
        let back = tracedraw_core::Document::from_json(&json).expect("reads");
        assert_eq!(
            back.find_shape(id).and_then(|s| s.qr.clone()),
            app.doc().find_shape(id).and_then(|s| s.qr.clone())
        );
    }

    #[test]
    fn qr_code_objects_are_written_to_cdr_as_their_shapes() {
        let mut app = App::headless();
        app.insert_qr_code().expect("placed");
        let bytes = tracedraw_cdr::write::document_to_cdr_version(app.doc(), 27);
        let (back, _) = tracedraw_cdr::open_bytes(&bytes, "qr").expect("reads back");
        let groups: Vec<&Shape> = back.pages[0]
            .layers
            .iter()
            .flat_map(|l| &l.shapes)
            .filter(|s| matches!(&s.kind, ShapeKind::Group { children } if children.len() == 2))
            .collect();
        assert_eq!(groups.len(), 1);
    }

    /// A Data Matrix and a rectangular micro QR code (#4228): the same
    /// object, its pixels made by the other kind of code; a Data Matrix
    /// reads back, an rMQR is wider than tall.
    #[test]
    fn data_matrix_and_rectangular_codes_are_made_and_checked() {
        let mut app = App::headless();
        let id = app.insert_qr_code().expect("placed");
        let mut qr = app.doc().find_shape(id).and_then(|s| s.qr.as_deref().cloned()).expect("qr");
        qr.symbol = QrSymbol::DataMatrix;
        qr.kind = QrKind::Text;
        qr.fields = vec!["LOT 4228-A".into()];
        app.set_qr_code(id, qr.clone()).expect("data matrix");
        let g = modules(&qr.payload(), qr.symbol).expect("grid");
        assert_eq!(g.cols, g.rows, "square");
        assert_eq!(validate(&qr), Ok("LOT 4228-A".to_string()));
        qr.symbol = QrSymbol::Rmqr;
        app.set_qr_code(id, qr.clone()).expect("rmqr");
        let g = modules(&qr.payload(), qr.symbol).expect("grid");
        assert!(g.cols > g.rows * 2, "{} x {}", g.cols, g.rows);
        let s = app.doc().find_shape(id).cloned().expect("shape");
        let ShapeKind::Group { children } = &s.kind else {
            panic!("a group");
        };
        let b = app.visual_bounds(&children[0]);
        let (w, h) = size_mm(&g, &qr);
        assert!((b.width() - w).abs() < 1e-6 && (b.height() - h).abs() < 1e-6, "{b:?}");
        assert!(validate(&qr).is_ok());
        // Saved and read back, the kind of code stays.
        let json = serde_json::to_string(&qr).expect("json");
        let back: QrCode = serde_json::from_str(&json).expect("back");
        assert_eq!(back.symbol, QrSymbol::Rmqr);
    }
}

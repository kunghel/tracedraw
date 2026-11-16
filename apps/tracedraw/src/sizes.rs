//! Tools > Sizes and Areas (#2098): the sizes of the selected objects,
//! those of the same size counted together (`[3x] 50 x 30 mm`), with
//! the total area of their boxes and the area the closed ones cover;
//! and the size of every page with their total area. The numbers a sign
//! shop or a print shop prices a job by, and that a macro listed in
//! other programs. Sizes are in the document's unit through the drawing
//! scale, areas in square metres (and square feet with imperial units).

use crate::app::{App, Units};
use crate::i18n::{tr, trf};
use tracedraw_core::document::Shape;

/// Sizes closer than this (in page millimetres) count as the same.
const SAME_MM: f64 = 0.005;

/// A size the selection holds and how many objects have it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeRow {
    /// Width and height in page millimetres.
    pub w: f64,
    pub h: f64,
    pub count: usize,
}

/// What Sizes and Areas lists for the selection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectSizes {
    /// The sizes, the most frequent first, then the largest.
    pub rows: Vec<SizeRow>,
    /// Objects measured.
    pub objects: usize,
    /// The total area of their boxes, in square page millimetres.
    pub box_area: f64,
    /// The area the closed objects cover (holes taken out), and how many
    /// of them are closed.
    pub filled_area: f64,
    pub closed: usize,
}

/// One page's size.
#[derive(Debug, Clone, PartialEq)]
pub struct PageRow {
    pub name: String,
    pub w: f64,
    pub h: f64,
}

/// The sizes of `shapes` (each object's box; dimensions are left out).
pub fn object_sizes(shapes: &[Shape]) -> ObjectSizes {
    let mut out = ObjectSizes::default();
    for s in shapes {
        if crate::dimensions::is_dimension(s) {
            continue;
        }
        let b = s.bounds();
        if !b.is_finite() {
            continue;
        }
        let (w, h) = (b.width(), b.height());
        out.objects += 1;
        out.box_area += w * h;
        if let Some(a) = crate::ui::dockers::selection_area_mm2(s) {
            out.filled_area += a;
            out.closed += 1;
        }
        match out
            .rows
            .iter_mut()
            .find(|r| (r.w - w).abs() < SAME_MM && (r.h - h).abs() < SAME_MM)
        {
            Some(r) => r.count += 1,
            None => out.rows.push(SizeRow { w, h, count: 1 }),
        }
    }
    out.rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then((b.w * b.h).total_cmp(&(a.w * a.h)))
    });
    out
}

/// The size of every page of `doc`, named as the page tabs name them.
pub fn page_sizes(app: &App) -> Vec<PageRow> {
    app.doc()
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| PageRow {
            name: if p.name.trim().is_empty() {
                trf("sizes.page_n", &[("n", &(i + 1).to_string())])
            } else {
                p.name.clone()
            },
            w: p.size.width,
            h: p.size.height,
        })
        .collect()
}

/// Page millimetres to the drawing's own millimetres: through its
/// drawing scale for objects (`world`), as printed for pages.
fn mm_scale(units: Units, world: bool) -> f64 {
    if world {
        units.world_from_mm(1.0) * units.mm()
    } else {
        1.0
    }
}

/// `w x h` in the document's unit, through the drawing scale when
/// `world` (objects; pages are as printed).
pub fn size_text(units: Units, w: f64, h: f64, world: bool) -> String {
    let k = mm_scale(units, world) / units.mm();
    format!("{:.2} x {:.2} {}", w * k, h * k, units.short())
}

/// An area given in square page millimetres, in square metres (and in
/// square feet too with imperial units), through the drawing scale when
/// `world`.
pub fn area_text(units: Units, mm2: f64, world: bool) -> String {
    let k = mm_scale(units, world);
    let m2 = mm2 * k * k / 1e6;
    let imperial = matches!(
        units,
        Units::Inches | Units::Feet | Units::Yards | Units::Miles
    );
    if imperial {
        format!("{m2:.4} m\u{b2} ({:.3} ft\u{b2})", m2 / 0.09290304)
    } else {
        format!("{m2:.4} m\u{b2}")
    }
}

/// The selection's sizes as lines of text, as the window shows them.
pub fn objects_report(units: Units, s: &ObjectSizes) -> Vec<String> {
    if s.objects == 0 {
        return vec![tr("sizes.none")];
    }
    let mut lines: Vec<String> = s
        .rows
        .iter()
        .map(|r| format!("[{}x] {}", r.count, size_text(units, r.w, r.h, true)))
        .collect();
    lines.push(String::new());
    lines.push(trf(
        "sizes.count",
        &[
            ("n", &s.objects.to_string()),
            ("sizes", &s.rows.len().to_string()),
        ],
    ));
    lines.push(trf(
        "sizes.box_area",
        &[("area", &area_text(units, s.box_area, true))],
    ));
    if s.closed > 0 {
        lines.push(trf(
            "sizes.filled_area",
            &[
                ("area", &area_text(units, s.filled_area, true)),
                ("n", &s.closed.to_string()),
            ],
        ));
    }
    lines
}

/// The pages' sizes as lines of text.
pub fn pages_report(units: Units, pages: &[PageRow]) -> Vec<String> {
    let mut lines: Vec<String> = pages
        .iter()
        .map(|p| format!("{}: {}", p.name, size_text(units, p.w, p.h, false)))
        .collect();
    let total: f64 = pages.iter().map(|p| p.w * p.h).sum();
    lines.push(String::new());
    lines.push(trf(
        "sizes.page_area",
        &[
            ("n", &pages.len().to_string()),
            ("area", &area_text(units, total, false)),
        ],
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{document::ShapeKind, geometry::Rect};

    fn rect(app: &mut App, r: Rect) -> tracedraw_core::ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: r,
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    }

    /// #2098 (Objects Calc): three objects of 50 x 30 mm count as one
    /// size, the box areas add up in square metres, and the closed
    /// objects' covered area is given too.
    #[test]
    fn same_sizes_count_together_with_the_total_area() {
        let mut app = App::headless();
        let mut ids = Vec::new();
        for i in 0..3 {
            let x = 10.0 + 60.0 * i as f64;
            ids.push(rect(&mut app, Rect::new(x, 10.0, x + 50.0, 40.0)));
        }
        ids.push(rect(&mut app, Rect::new(0.0, 100.0, 100.0, 300.0)));
        app.select(ids);
        let s = object_sizes(&app.selected_shapes());
        assert_eq!(s.objects, 4);
        assert_eq!(s.rows.len(), 2);
        assert_eq!(s.rows[0].count, 3);
        assert!((s.rows[0].w - 50.0).abs() < 1e-9 && (s.rows[0].h - 30.0).abs() < 1e-9);
        assert_eq!(s.rows[1].count, 1);
        // 3 x 1500 + 20000 mm².
        assert!((s.box_area - 24500.0).abs() < 1e-6, "{}", s.box_area);
        assert_eq!(s.closed, 4);
        assert!((s.filled_area - 24500.0).abs() < 1e-3);
        let lines = objects_report(Units::Millimeters, &s);
        assert_eq!(lines[0], "[3x] 50.00 x 30.00 mm");
        assert_eq!(lines[1], "[1x] 100.00 x 200.00 mm");
        assert!(
            lines.iter().any(|l| l.contains("0.0245 m\u{b2}")),
            "{lines:?}"
        );
        // Nothing selected: said so.
        app.select(vec![]);
        let none = object_sizes(&app.selected_shapes());
        assert_eq!(
            objects_report(Units::Millimeters, &none),
            [tr("sizes.none")]
        );
    }

    /// #2098 (Pages Calc): every page's size and their total area.
    #[test]
    fn every_page_is_listed_with_the_total_area() {
        let mut app = App::headless();
        app.insert_page(true);
        let pages = page_sizes(&app);
        assert_eq!(pages.len(), app.doc().pages.len());
        assert!(pages.len() >= 2);
        let total: f64 = app
            .doc()
            .pages
            .iter()
            .map(|p| p.size.width * p.size.height)
            .sum();
        let lines = pages_report(Units::Millimeters, &pages);
        let p = &app.doc().pages[0];
        assert!(
            lines[0].ends_with(&size_text(
                Units::Millimeters,
                p.size.width,
                p.size.height,
                false
            )),
            "{lines:?}"
        );
        assert!(
            lines
                .last()
                .expect("total")
                .contains(&area_text(Units::Millimeters, total, false)),
            "{lines:?}"
        );
    }

    #[test]
    fn imperial_units_give_square_feet_too() {
        // One square metre.
        assert_eq!(area_text(Units::Millimeters, 1e6, true), "1.0000 m\u{b2}");
        assert_eq!(
            area_text(Units::Inches, 1e6, true),
            "1.0000 m\u{b2} (10.764 ft\u{b2})"
        );
    }
}

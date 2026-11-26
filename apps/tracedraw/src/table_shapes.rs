//! Object > Insert > Shapes from Spreadsheet (#4270): the rows of a CSV
//! or Excel file of sizes (a cutting list, a parts list) drawn as
//! rectangles of their width and height, or circles of their diameter,
//! as many of each as its quantity, laid out in rows across the page.

use crate::app::App;
use crate::ui::btn_icons::IconUi;
use crate::i18n::tr;
use std::path::PathBuf;
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::Rect,
    Command, ShapeId,
};

/// The most shapes one file makes, so a wrong column picked for the
/// quantity cannot fill the drawing with millions of objects.
pub const MAX_SHAPES: usize = 5000;

/// The dialog's state: the file read, which column holds what, the gap.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableShapes {
    pub path: Option<PathBuf>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub width: Option<usize>,
    pub height: Option<usize>,
    pub diameter: Option<usize>,
    pub quantity: Option<usize>,
    pub name: Option<usize>,
    /// The space between shapes, mm.
    pub gap_mm: f64,
}

/// One shape to draw: a rectangle `w` by `h` mm, or a circle when
/// `circle` (`w` its diameter), named `name`.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub w: f64,
    pub h: f64,
    pub circle: bool,
    pub name: String,
}

/// A number from a cell: "600", "600.5", "600,5", "600 mm", "Ø 40".
pub fn number(cell: &str) -> Option<f64> {
    let t = cell.trim();
    let start = t.find(|c: char| c.is_ascii_digit() || c == '.' || c == ',')?;
    let digits: String = t[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    let digits = if digits.contains('.') {
        digits.replace(',', "")
    } else {
        digits.replace(',', ".")
    };
    digits.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// The column whose name says it holds `words` (a name that is one of
/// the short `exact` ones counts only when it is the whole name).
fn column(headers: &[String], words: &[&str], exact: &[&str]) -> Option<usize> {
    let names: Vec<String> = headers.iter().map(|h| h.trim().to_lowercase()).collect();
    names
        .iter()
        .position(|n| exact.iter().any(|e| n == e))
        .or_else(|| {
            names
                .iter()
                .position(|n| !n.is_empty() && words.iter().any(|w| n.contains(w)))
        })
}

impl TableShapes {
    /// The file's columns guessed from their names, in the languages
    /// cutting and parts lists are written in.
    pub fn guess(&mut self) {
        let h = &self.headers;
        self.width = column(
            h,
            &["width", "wide", "宽", "寬", "largura", "ancho", "breite", "larg", "szer", "ширин"],
            &["w", "b"],
        );
        self.height = column(
            h,
            &[
                "height", "length", "long", "高", "长", "長", "altura", "alto", "höhe", "länge",
                "compr", "hauteur", "wys", "высот", "длин",
            ],
            &["h", "l"],
        );
        self.diameter = column(
            h,
            &["diam", "直径", "直徑", "durchmesser", "ø", "диаметр", "średnica"],
            &["d", "dia"],
        );
        self.quantity = column(
            h,
            &[
                "qty", "quant", "count", "数量", "數量", "qtd", "cantidad", "anzahl", "pcs",
                "pieces", "peças", "stück", "ilość", "кол",
            ],
            &["n", "q"],
        );
        self.name = column(
            h,
            &[
                "name", "label", "名称", "名稱", "nome", "nombre", "part", "零件", "desc", "item",
                "bezeichnung", "nazwa",
            ],
            &[],
        );
        // A column taken for one thing is not also another.
        if self.height == self.width {
            self.height = None;
        }
        if self.diameter == self.width || self.diameter == self.height {
            self.diameter = None;
        }
        // Without names, the first two numeric columns are the sizes.
        if self.width.is_none() && self.height.is_none() && self.diameter.is_none() {
            let numeric: Vec<usize> = (0..self.headers.len())
                .filter(|&c| {
                    self.rows
                        .iter()
                        .take(20)
                        .any(|r| r.get(c).and_then(|v| number(v)).is_some())
                })
                .collect();
            self.width = numeric.first().copied();
            self.height = numeric.get(1).copied();
        }
    }

    /// Read `path` and guess its columns.
    pub fn load(&mut self, path: PathBuf) -> Result<(), String> {
        let (headers, rows) = crate::print_merge::read_data(&path)?;
        self.headers = headers;
        self.rows = rows;
        self.path = Some(path);
        self.guess();
        Ok(())
    }

    /// The shapes the rows make, sizes in mm (the cells are in `units`):
    /// a row with a diameter is a circle, one with a width and a height a
    /// rectangle, repeated by its quantity; rows with neither are left
    /// out. At most [`MAX_SHAPES`].
    pub fn items(&self, units: crate::app::Units) -> Vec<Item> {
        let cell = |r: &Vec<String>, c: Option<usize>| {
            c.and_then(|c| r.get(c)).and_then(|v| number(v)).filter(|v| *v > 0.0)
        };
        let mut out = Vec::new();
        for r in &self.rows {
            let name = self
                .name
                .and_then(|c| r.get(c))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let item = if let Some(d) = cell(r, self.diameter) {
                let d = units.to_mm(d);
                Item {
                    w: d,
                    h: d,
                    circle: true,
                    name,
                }
            } else if let (Some(w), Some(h)) = (cell(r, self.width), cell(r, self.height)) {
                Item {
                    w: units.to_mm(w),
                    h: units.to_mm(h),
                    circle: false,
                    name,
                }
            } else {
                continue;
            };
            let n = match self.quantity {
                Some(_) => cell(r, self.quantity).map_or(1, |q| q.round().max(1.0) as usize),
                None => 1,
            };
            for _ in 0..n {
                if out.len() >= MAX_SHAPES {
                    return out;
                }
                out.push(item.clone());
            }
        }
        out
    }
}

/// Where each item goes: in rows from `area`'s top-left corner, left to
/// right, a new row when the next would pass the right side, `gap` mm
/// between them (y up).
pub fn layout(items: &[Item], area: Rect, gap: f64) -> Vec<Rect> {
    let gap = gap.max(0.0);
    let (mut x, mut top) = (area.x0, area.y1);
    let mut row_h: f64 = 0.0;
    items
        .iter()
        .map(|it| {
            if x > area.x0 && x + it.w > area.x1 {
                x = area.x0;
                top -= row_h + gap;
                row_h = 0.0;
            }
            let r = Rect::new(x, top - it.h, x + it.w, top);
            x += it.w + gap;
            row_h = row_h.max(it.h);
            r
        })
        .collect()
}

impl App {
    /// Draw the items on the active layer, laid out from the page's
    /// top-left corner, with the default fill and outline of their kind;
    /// one undo step, the new shapes selected.
    pub fn insert_table_shapes(&mut self, items: &[Item], gap_mm: f64) -> Vec<ShapeId> {
        let Some(layer) = self.active_layer() else {
            return Vec::new();
        };
        if items.is_empty() {
            return Vec::new();
        }
        let places = layout(items, self.page_rect(), gap_mm);
        let mut cmds = Vec::with_capacity(items.len());
        let mut ids = Vec::with_capacity(items.len());
        for (it, rect) in items.iter().zip(places) {
            let id = self.engine.new_shape_id();
            let kind = if it.circle {
                ShapeKind::Ellipse { rect, arc: None }
            } else {
                ShapeKind::Rect {
                    rect,
                    radius: 0.0,
                    corners: None,
                }
            };
            let mut shape = Shape::new(id, kind);
            self.doc().defaults.apply_to(&mut shape);
            if !it.name.is_empty() {
                shape.name = Some(it.name.clone());
            }
            cmds.push(Command::AddShape { layer, shape });
            ids.push(id);
        }
        if let Err(e) = self.engine.run_batch("Shapes from Spreadsheet", &cmds) {
            self.status = e.to_string();
            return Vec::new();
        }
        self.select(ids.clone());
        ids
    }
}

/// The Shapes from Spreadsheet dialog.
pub fn dialog(app: &mut App, ctx: &egui::Context, st: &mut TableShapes, close: &mut bool) {
    use crate::ui::tips::RichTip as _;
    let u = app.units;
    if let Some(p) = crate::files::take_picked("table_shapes_file") {
        if let Err(e) = st.load(p) {
            app.status = e;
        }
    }
    let items = st.items(u);
    let mut ok = false;
    crate::ui::dialogs::window(ctx, tr("dialog.table_shapes")).show(ctx, |ui| {
        ui.set_min_width(380.0);
        ui.label(
            egui::RichText::new(tr("dialog.table_shapes_hint"))
                .color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .ibutton("dialog.table_shapes_file")
                .rich_tip(crate::ui::tips::keyed("dialog.table_shapes_file"))
                .clicked()
            {
                crate::files::Dialog::new()
                    .add_filter("CSV, Excel", &["csv", "txt", "xlsx"])
                    .add_filter("Excel", &["xlsx"])
                    .add_filter("CSV", &["csv", "txt"])
                    .pick_into("table_shapes_file");
            }
            let name = st
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "-".into());
            ui.label(name);
        });
        ui.add_space(4.0);
        let headers = st.headers.clone();
        let pick = |ui: &mut egui::Ui, key: &str, salt: &str, v: &mut Option<usize>| {
            ui.label(tr(key));
            let shown = v
                .and_then(|i| headers.get(i).cloned())
                .unwrap_or_else(|| tr("dialog.table_shapes_none"));
            crate::ui::keynav::ComboBox::from_id_salt(("table_shapes", salt))
                .width(200.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(v, None, tr("dialog.table_shapes_none"));
                    for (i, h) in headers.iter().enumerate() {
                        let label = if h.trim().is_empty() {
                            format!("{}", i + 1)
                        } else {
                            h.clone()
                        };
                        ui.selectable_value(v, Some(i), label);
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed(key));
            ui.end_row();
        };
        egui::Grid::new("table_shapes_grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                pick(ui, "dialog.table_shapes_width", "w", &mut st.width);
                pick(ui, "dialog.table_shapes_height", "h", &mut st.height);
                pick(ui, "dialog.table_shapes_diameter", "d", &mut st.diameter);
                pick(ui, "dialog.table_shapes_quantity", "q", &mut st.quantity);
                pick(ui, "dialog.table_shapes_name", "n", &mut st.name);
                ui.label(tr("dialog.table_shapes_gap"));
                let mut g = u.from_mm(st.gap_mm);
                if ui
                    .add(
                        crate::ui::field::NumField::new(&mut g)
                            .range(0.0..=u.from_mm(1000.0))
                            .speed(0.1)
                            .max_decimals(3)
                            .suffix(format!(" {}", u.short())),
                    )
                    .rich_tip(crate::ui::tips::keyed("dialog.table_shapes_gap"))
                    .changed()
                {
                    st.gap_mm = u.to_mm(g).max(0.0);
                }
                ui.end_row();
            });
        ui.add_space(4.0);
        let circles = items.iter().filter(|i| i.circle).count();
        ui.label(
            egui::RichText::new(
                tr("dialog.table_shapes_count")
                    .replace("{rects}", &(items.len() - circles).to_string())
                    .replace("{circles}", &circles.to_string())
                    .replace("{units}", u.short()),
            )
            .color(crate::theme::Tokens::text_dim()),
        );
        ui.add_space(6.0);
        ui.add_enabled_ui(!items.is_empty(), |ui| {
            if crate::ui::dialogs::ok_cancel(ui, close) {
                ok = true;
            }
        });
        if items.is_empty()
            && ui
                .button(tr("dialog.cancel"))
                .on_hover_text(tr("dialog.table_shapes_hint"))
                .clicked()
        {
            *close = true;
        }
    });
    if ok {
        app.insert_table_shapes(&items, st.gap_mm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(headers: &[&str], rows: &[&[&str]]) -> TableShapes {
        let mut t = TableShapes {
            headers: headers.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
            gap_mm: 5.0,
            ..Default::default()
        };
        t.guess();
        t
    }

    /// A cutting list in Chinese: its columns found by name, each row
    /// repeated by its quantity, circles by their diameter; all drawn in
    /// one step, laid out in rows on the page.
    #[test]
    fn a_cutting_list_becomes_rectangles_and_circles() {
        let t = table(
            &["名称", "宽", "长", "直径", "数量"],
            &[
                &["侧板", "300", "600", "", "2"],
                &["圆盘", "", "", "Ø 80", "1"],
                &["", "abc", "", "", ""],
                &["背板", "580,5", "600 mm", "", ""],
            ],
        );
        assert_eq!(t.width, Some(1));
        assert_eq!(t.height, Some(2));
        assert_eq!(t.diameter, Some(3));
        assert_eq!(t.quantity, Some(4));
        assert_eq!(t.name, Some(0));
        let items = t.items(crate::app::Units::Millimeters);
        assert_eq!(items.len(), 4, "{items:?}");
        assert_eq!(items.iter().filter(|i| i.circle).count(), 1);
        assert!((items[3].w - 580.5).abs() < 1e-9);
        let mut app = App::headless();
        let count = |app: &App| app.doc().all_layers().map(|l| l.shapes.len()).sum::<usize>();
        let before = count(&app);
        let ids = app.insert_table_shapes(&items, t.gap_mm);
        assert_eq!(ids.len(), 4);
        assert_eq!(app.selection, ids);
        let s0 = app.doc().find_shape(ids[0]).expect("first").clone();
        assert_eq!(s0.name.as_deref(), Some("侧板"));
        let (b0, b1) = (s0.bounds(), app.doc().find_shape(ids[1]).expect("2").bounds());
        assert!((b0.width() - 300.0).abs() < 1e-6 && (b0.height() - 600.0).abs() < 1e-6);
        // The copy goes beside the first or under it, never over it.
        assert!(b1.x0 >= b0.x1 + 4.9 || b1.y1 <= b0.y0 - 4.9, "{b0:?} {b1:?}");
        assert!(matches!(
            app.doc().find_shape(ids[2]).expect("circle").kind,
            ShapeKind::Ellipse { .. }
        ));
        // One undo takes them all away.
        app.undo();
        assert_eq!(count(&app), before);
    }

    /// Without names the first two numeric columns are width and height.
    #[test]
    fn unnamed_columns_take_the_first_two_numbers() {
        let t = table(&["", "x1", "x2"], &[&["x", "10", "20"]]);
        assert_eq!((t.width, t.height), (Some(1), Some(2)));
        let rects = layout(
            &t.items(crate::app::Units::Millimeters),
            Rect::new(0.0, 0.0, 100.0, 100.0),
            0.0,
        );
        assert_eq!(rects, vec![Rect::new(0.0, 80.0, 10.0, 100.0)]);
        assert_eq!(number("1,250.5"), Some(1250.5));
        assert_eq!(number("-"), None);
    }
}

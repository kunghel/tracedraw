//! Table > Edit in Spreadsheet (#4145): a table's cells go to a workbook
//! file (the workbook it came from while its cells still match, else one
//! written from the cells) that opens in the spreadsheet program
//! (`spreadsheet_editor.rs`); each time the program saves it, the table's
//! cells take the sheet's texts again, one undo step each time, the
//! table's look kept. See docs/behavior/table.md, "Editing a table in a
//! spreadsheet program".

use crate::app::App;
use crate::i18n::{tr, trf};
use tracedraw_core::{
    document::{ShapeKind, Table},
    Command, ShapeId, TextSpan,
};

/// A workbook handed to the spreadsheet program for a table: the table,
/// the file and its time stamp when last read or written.
#[derive(Debug, Clone)]
pub struct TableFile {
    pub shape: ShapeId,
    pub path: std::path::PathBuf,
    pub modified: Option<std::time::SystemTime>,
}

/// Rows with trailing empty cells and rows left out, for comparing a
/// table's cells with a workbook's.
fn trimmed(rows: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut r = r.clone();
            while r.last().is_some_and(|c| c.is_empty()) {
                r.pop();
            }
            r
        })
        .collect();
    while out.last().is_some_and(|r| r.is_empty()) {
        out.pop();
    }
    out
}

/// The workbook to edit table `t` in: the one it keeps while it still
/// holds the table's texts (its formulas and formatting stay), else one
/// written from the cells.
pub fn workbook_for(t: &Table) -> Result<Vec<u8>, String> {
    if let Some(src) = &t.source_xlsx {
        if let Ok(rows) = tracedraw_io::xlsx::read_grid(src) {
            if trimmed(&rows) == trimmed(&t.text_rows()) {
                return Ok(src.to_vec());
            }
        }
    }
    tracedraw_io::xlsx::write_xlsx(t)
}

/// `t` with the sheet's `rows` as its cells' texts: rows and columns
/// added or taken away at the bottom and the right to fit, each cell
/// keeping its first span's look (new cells take the nearest cell's),
/// the workbook `source` kept with it.
pub fn table_with_rows(t: &Table, rows: &[Vec<String>], source: Vec<u8>) -> Table {
    let mut t = t.clone();
    let want_rows = rows.len().max(1) as u32;
    let want_cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1) as u32;
    if want_rows > t.rows() {
        let last = t.rows() - 1;
        t.insert_rows(t.rows(), want_rows - t.rows(), last);
    } else if want_rows < t.rows() {
        t.remove_rows(want_rows, t.rows() - want_rows);
    }
    if want_cols > t.cols() {
        let last = t.cols() - 1;
        t.insert_cols(t.cols(), want_cols - t.cols(), last);
    } else if want_cols < t.cols() {
        t.remove_cols(want_cols, t.cols() - want_cols);
    }
    // The look a cell without text of its own takes: the nearest cell
    // above (else to the left) that has text.
    let looks: Vec<(u32, u32, TextSpan)> = t
        .cells
        .iter()
        .filter_map(|c| c.text.first().map(|s| (c.row, c.col, s.clone())))
        .collect();
    let look_for = |r: u32, c: u32| -> TextSpan {
        looks
            .iter()
            .filter(|(lr, lc, _)| *lc == c && *lr <= r)
            .max_by_key(|(lr, _, _)| *lr)
            .or_else(|| {
                looks
                    .iter()
                    .filter(|(lr, _, _)| *lr == r)
                    .min_by_key(|(_, lc, _)| lc.abs_diff(c))
            })
            .or_else(|| looks.first())
            .map(|(_, _, s)| s.clone())
            .unwrap_or_else(|| TextSpan::new("", "Arial", 10.0))
    };
    let mut updates = Vec::new();
    for cell in &t.cells {
        let text = rows
            .get(cell.row as usize)
            .and_then(|r| r.get(cell.col as usize))
            .cloned()
            .unwrap_or_default();
        let current: String = cell.text.iter().map(|s| s.text.as_str()).collect();
        if current == text {
            continue;
        }
        let look = cell
            .text
            .first()
            .cloned()
            .unwrap_or_else(|| look_for(cell.row, cell.col));
        updates.push((cell.row, cell.col, text, look));
    }
    for (r, c, text, look) in updates {
        if let Some(cell) = t.cells.iter_mut().find(|x| x.row == r && x.col == c) {
            cell.text = if text.is_empty() {
                Vec::new()
            } else {
                vec![TextSpan { text, ..look }]
            };
        }
    }
    t.source_xlsx = Some(source.into());
    t.refit();
    t
}

fn modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl App {
    /// The name of the Edit in Spreadsheet command: "Edit in" and the
    /// spreadsheet program's name when one is known.
    pub fn edit_in_spreadsheet_label(&self) -> String {
        match crate::spreadsheet_editor::display_name(&self.settings.spreadsheet_editor) {
            Some(name) => trf("table.edit_in_named", &[("name", &name)]),
            None => tr("table.edit_in_spreadsheet"),
        }
    }

    /// Table > Edit in Spreadsheet: write the table's workbook next to the
    /// system's temporary files (or in a portable copy's own folder) and
    /// open it in the spreadsheet program; saving it there updates the
    /// table (#4145).
    pub fn edit_table_externally(&mut self) {
        if crate::files::WEB {
            // A browser cannot hand a file to another program.
            self.status = tr("status.not_in_browser");
            return;
        }
        let Some((id, _, t)) = self.table_target() else {
            return;
        };
        let bytes = match workbook_for(&t) {
            Ok(b) => b,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        let dir = crate::settings::work_dir("tracedraw-edit");
        #[cfg(target_arch = "wasm32")]
        let dir = std::path::PathBuf::from("tracedraw-edit");
        let path = dir.join(format!("table-{}.xlsx", id.raw()));
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &bytes)) {
            self.status = e.to_string();
            return;
        }
        self.table_files.retain(|f| f.shape != id);
        self.table_files.push(TableFile {
            shape: id,
            path: path.clone(),
            modified: modified(&path),
        });
        // The cells typed in are kept before the table leaves for the
        // other program.
        self.table_edit = None;
        #[cfg(target_os = "android")]
        {
            self.status = if crate::android::outside::edit(&path, crate::spreadsheet_editor::XLSX_MIME) {
                tr("status.table_sent_to_spreadsheet")
            } else {
                tr("status.no_app_edits_spreadsheets")
            };
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        match crate::export::open_in_editor(&self.settings.spreadsheet_editor, &path) {
            Ok(()) => self.status = tr("status.table_sent_to_spreadsheet"),
            Err(e) => self.status = e,
        }
    }

    /// Each second (with the linked pictures): a table's workbook saved
    /// in the spreadsheet program gives the table its cells again. A file
    /// still being written (unreadable) is tried again the next time; the
    /// watch ends when the table or the file is gone.
    pub fn watch_table_files(&mut self) {
        if self.table_files.is_empty() {
            return;
        }
        let mut files = std::mem::take(&mut self.table_files);
        files.retain(|f| self.doc().find_shape(f.shape).is_some() && f.path.exists());
        for f in &mut files {
            let now = modified(&f.path);
            if now.is_none() || now == f.modified {
                continue;
            }
            let Ok(bytes) = std::fs::read(&f.path) else {
                continue;
            };
            let Ok(rows) = tracedraw_io::xlsx::read_grid(&bytes) else {
                continue;
            };
            f.modified = now;
            self.update_table_from_sheet(f.shape, &rows, bytes);
        }
        self.table_files = files;
    }

    /// Table `id` takes the sheet's `rows` (from workbook `bytes`), one
    /// undo step; false when it is no table.
    pub fn update_table_from_sheet(&mut self, id: ShapeId, rows: &[Vec<String>], bytes: Vec<u8>) -> bool {
        let Some(ShapeKind::Table(t)) = self.doc().find_shape(id).map(|s| s.kind.clone()) else {
            return false;
        };
        let t = table_with_rows(&t, rows, bytes);
        if self.table_edit.as_ref().is_some_and(|e| e.shape == id) {
            self.table_edit = None;
        }
        match self.engine.run_with_label(
            &Command::SetShapeKind {
                shape: id,
                kind: ShapeKind::Table(t),
            },
            "Update from Spreadsheet",
        ) {
            Ok(_) => {
                self.raster.borrow_mut().invalidate();
                self.status = tr("status.table_updated_from_spreadsheet");
                true
            }
            Err(e) => {
                self.status = e.to_string();
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    fn table() -> Table {
        let mut t = Table::new(Rect::new(0.0, 0.0, 40.0, 20.0), 2, 2, None);
        let mut head = TextSpan::new("Item", "Arial", 10.0);
        head.bold = true;
        let i = t.cell_index(0, 0).unwrap();
        t.cells[i].text = vec![head];
        let i = t.cell_index(1, 0).unwrap();
        t.cells[i].text = vec![TextSpan::new("Pen", "Arial", 9.0)];
        t
    }

    #[test]
    fn the_kept_workbook_is_edited_while_it_matches() {
        let mut t = table();
        let own = tracedraw_io::xlsx::write_rows(
            &[vec!["Item".to_string()], vec!["Pen".into()]],
            &[],
        )
        .unwrap();
        t.source_xlsx = Some(own.clone().into());
        assert_eq!(workbook_for(&t).unwrap(), own);
        // A cell changed here: a workbook of the cells instead.
        let i = t.cell_index(1, 0).unwrap();
        t.cells[i].text[0].text = "Ink".into();
        let made = workbook_for(&t).unwrap();
        assert_ne!(made, own);
        assert_eq!(
            tracedraw_io::xlsx::read_grid(&made).unwrap()[1][0],
            "Ink"
        );
    }

    #[test]
    fn a_saved_sheet_gives_the_table_its_cells_and_keeps_the_look() {
        let t = table();
        let rows = vec![
            vec!["Item".to_string(), "Qty".into(), "Price".into()],
            vec!["Pen".into(), "3".into(), "".into()],
            vec!["Ink".into(), "12".into(), "4.5".into()],
        ];
        let out = table_with_rows(&t, &rows, vec![1, 2, 3]);
        assert_eq!((out.rows(), out.cols()), (3, 3));
        assert_eq!(out.text_rows(), rows);
        let cell = |r, c| out.cells[out.cell_index(r, c).unwrap()].text[0].clone();
        assert!(cell(0, 0).bold, "the head keeps its look");
        assert!(!cell(2, 0).bold && (cell(2, 0).size_pt - 9.0).abs() < 1e-9, "a new row looks like the one above");
        assert_eq!(out.source_xlsx.as_deref(), Some(&[1u8, 2, 3][..]));
        // Fewer rows and columns: the table shrinks.
        let out = table_with_rows(&out, &[vec!["Only".to_string()]], vec![]);
        assert_eq!((out.rows(), out.cols()), (1, 1));
    }

    #[test]
    fn a_workbook_saved_again_updates_the_table() {
        let mut app = App::headless();
        let layer = app.active_layer().unwrap();
        let id = app.engine.new_shape_id();
        app.run(Command::AddShape {
            layer,
            shape: tracedraw_core::Shape::new(id, ShapeKind::Table(table())),
        });
        let dir = std::env::temp_dir().join(format!("td-table-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.xlsx");
        std::fs::write(&path, tracedraw_io::xlsx::write_xlsx(&table()).unwrap()).unwrap();
        app.table_files.push(TableFile {
            shape: id,
            path: path.clone(),
            modified: Some(std::time::SystemTime::UNIX_EPOCH),
        });
        let rows = vec![vec!["Item".to_string(), "Qty".into()], vec!["Ink".into(), "7".into()]];
        std::fs::write(&path, tracedraw_io::xlsx::write_rows(&rows, &[]).unwrap()).unwrap();
        app.watch_table_files();
        let ShapeKind::Table(t) = &app.doc().find_shape(id).unwrap().kind else {
            panic!("a table");
        };
        assert_eq!(t.text_rows(), rows);
        assert!(t.source_xlsx.is_some());
        assert_eq!(app.engine.undo_label(), Some("Update from Spreadsheet"));
        // Unchanged since: nothing more happens.
        let rev = app.engine.revision();
        app.watch_table_files();
        assert_eq!(app.engine.revision(), rev);
        // The table gone: the watch ends.
        app.run(Command::DeleteShapes { shapes: vec![id] });
        app.watch_table_files();
        assert!(app.table_files.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}

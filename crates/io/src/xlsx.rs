//! Office Open XML spreadsheets (`.xlsx`): the first worksheet's cells as
//! text, for Print Merge data (#377, #378). Read from the public
//! ECMA-376 structure: `xl/workbook.xml` lists the sheets, its
//! relationships part names each sheet's file, `xl/sharedStrings.xml` holds
//! the shared strings, and a sheet's `sheetData` holds rows of cells whose
//! `r` attribute gives the column (`B3`), `t` the type (`s` a shared string
//! index, `inlineStr`, `str`, `b`, or a number) and `v` (or `is/t`) the
//! value.

use std::io::Read;

/// The most bytes one part may have when unpacked.
const MAX_PART: u64 = 64 * 1024 * 1024;

/// The first sheet as rows of cell texts, empty rows and trailing empty
/// cells left out; ragged rows padded to the widest one.
pub fn read_rows(bytes: &[u8]) -> Result<Vec<Vec<String>>, String> {
    read_sheet(bytes, false)
}

/// The first sheet as a grid (#4145): each row at its own place, empty
/// rows between filled ones kept empty, so a table read back from it
/// keeps its rows where they were; trailing empty rows left out.
pub fn read_grid(bytes: &[u8]) -> Result<Vec<Vec<String>>, String> {
    read_sheet(bytes, true)
}

fn read_sheet(bytes: &[u8], grid: bool) -> Result<Vec<Vec<String>>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("not an XLSX file: {e}"))?;
    let part = |zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str| {
        let mut f = zip.by_name(name).map_err(|_| format!("{name} missing"))?;
        if f.size() > MAX_PART {
            return Err(format!("{name} too large"));
        }
        let mut raw = Vec::new();
        f.read_to_end(&mut raw).map_err(|e| e.to_string())?;
        Ok::<String, String>(decode_xml_bytes(&raw))
    };
    let sheet = first_sheet_path(
        &part(&mut zip, "xl/workbook.xml")?,
        &part(&mut zip, "xl/_rels/workbook.xml.rels").unwrap_or_default(),
    )
    .unwrap_or_else(|| "xl/worksheets/sheet1.xml".to_string());
    let shared = match part(&mut zip, "xl/sharedStrings.xml") {
        Ok(xml) => shared_strings(&xml)?,
        Err(_) => Vec::new(),
    };
    sheet_rows(&part(&mut zip, &sheet)?, &shared, grid)
}

/// The first sheet's part name, from the workbook and its relationships.
fn first_sheet_path(workbook: &str, rels: &str) -> Option<String> {
    let wb = roxmltree::Document::parse(workbook).ok()?;
    let sheet = wb.descendants().find(|n| n.has_tag_name("sheet"))?;
    let rid = sheet
        .attributes()
        .find(|a| a.name() == "id")
        .map(|a| a.value().to_string())?;
    let rels = roxmltree::Document::parse(rels).ok()?;
    let target = rels
        .descendants()
        .filter(|n| n.has_tag_name("Relationship"))
        .find(|n| n.attribute("Id") == Some(rid.as_str()))?
        .attribute("Target")?;
    // Targets are relative to `xl/`, or absolute from the package root.
    Some(match target.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None => format!("xl/{target}"),
    })
}

/// The column letters of a column number (0 `A`, 27 `AB`).
pub fn column_name(mut c: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (c % 26) as u8);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn xml_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Characters XML 1.0 cannot carry go as OOXML escapes.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {
                out.push_str(&format!("_x{:04X}_", c as u32))
            }
            c => out.push(c),
        }
    }
    out
}

/// Whether a cell's text goes in as a number: plain decimal notation
/// that reads back the same, so the spreadsheet can count with it.
fn plain_number(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && t == s
        && t.chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
        && t.parse::<f64>().is_ok_and(f64::is_finite)
        && !leading_zero(t.trim_start_matches('-'))
        && t.len() <= 15
}

/// "007" keeps its zeros only as text.
fn leading_zero(t: &str) -> bool {
    t.len() > 1 && t.starts_with('0') && !t.starts_with("0.")
}

/// A workbook (`.xlsx`) of one sheet holding `rows` of cell texts (#4143):
/// plain numbers as numbers, the rest as text; `merged` lists blocks of
/// cells joined into one as (first row, first column, last row, last
/// column), counted from 0. Written from the public ECMA-376 structure.
pub fn write_rows(rows: &[Vec<String>], merged: &[(u32, u32, u32, u32)]) -> Result<Vec<u8>, String> {
    use std::fmt::Write as _;
    use std::io::Write as _;
    let mut sheet = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheetData>",
    );
    for (r, row) in rows.iter().enumerate() {
        let _ = write!(sheet, "<row r=\"{}\">", r + 1);
        for (c, text) in row.iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            let at = format!("{}{}", column_name(c), r + 1);
            if plain_number(text) {
                let _ = write!(sheet, "<c r=\"{at}\"><v>{text}</v></c>");
            } else {
                let _ = write!(
                    sheet,
                    "<c r=\"{at}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                    xml_text(text)
                );
            }
        }
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData>");
    if !merged.is_empty() {
        let _ = write!(sheet, "<mergeCells count=\"{}\">", merged.len());
        for &(r0, c0, r1, c1) in merged {
            let _ = write!(
                sheet,
                "<mergeCell ref=\"{}{}:{}{}\"/>",
                column_name(c0 as usize),
                r0 + 1,
                column_name(c1 as usize),
                r1 + 1
            );
        }
        sheet.push_str("</mergeCells>");
    }
    sheet.push_str("</worksheet>");
    let parts: [(&str, String); 5] = [
        (
            "[Content_Types].xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
             <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
             <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
             <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
             <Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\
             </Types>"
                .into(),
        ),
        (
            "_rels/.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
             </Relationships>"
                .into(),
        ),
        (
            "xl/workbook.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
             xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
             <sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>"
                .into(),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/>\
             </Relationships>"
                .into(),
        ),
        ("xl/worksheets/sheet1.xml", sheet),
    ];
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, xml) in parts {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(xml.as_bytes()).map_err(|e| e.to_string())?;
    }
    zip.finish()
        .map(|c| c.into_inner())
        .map_err(|e| e.to_string())
}

/// A workbook of `table`'s cells (#4143): each cell's text in its place,
/// merged cells merged, for a table that has no workbook of its own.
pub fn write_xlsx(table: &tracedraw_core::document::Table) -> Result<Vec<u8>, String> {
    let merged: Vec<(u32, u32, u32, u32)> = table
        .cells
        .iter()
        .filter(|c| c.row_span > 1 || c.col_span > 1)
        .map(|c| (c.row, c.col, c.row + c.row_span - 1, c.col + c.col_span - 1))
        .collect();
    write_rows(&table.text_rows(), &merged)
}

/// Decode an XML part from UTF-8, UTF-16 with BOM, or the character set
/// named in its `<?xml ... encoding="..."?>` prolog (`windows-1251`,
/// `gbk`, `shift_jis`, `windows-1250`, etc.), stripping any non-UTF-8
/// `encoding=` declaration so `roxmltree` parses the resulting UTF-8.
fn decode_xml_bytes(raw: &[u8]) -> String {
    if let Some(rest) = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = raw.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return strip_xml_encoding(&String::from_utf16_lossy(&units));
    }
    if let Some(rest) = raw.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return strip_xml_encoding(&String::from_utf16_lossy(&units));
    }
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.to_string();
    }
    let head = &raw[..raw.len().min(200)];
    let head_ascii: String = head.iter().map(|&b| b as char).collect();
    let enc = xml_declared_encoding(&head_ascii)
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::WINDOWS_1251);
    let (cow, _, _) = enc.decode(raw);
    strip_xml_encoding(&cow)
}

fn xml_declared_encoding(head: &str) -> Option<&str> {
    let end = head.find("?>")?;
    let decl = &head[..end];
    let pos = decl.find("encoding")?;
    let after = decl[pos + "encoding".len()..].trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    let q = after.chars().next().filter(|&c| c == '"' || c == '\'')?;
    let rest = &after[1..];
    let close = rest.find(q)?;
    Some(&rest[..close])
}

fn strip_xml_encoding(xml: &str) -> String {
    match xml.find("?>").filter(|&end| xml[..end].starts_with("<?xml")) {
        Some(end) => format!("<?xml version=\"1.0\"?>{}", &xml[end + 2..]),
        None => xml.to_string(),
    }
}

fn ooxml_escape_at(bytes: &[u8], i: usize) -> Option<u16> {
    let slice = bytes.get(i..i.checked_add(7)?)?;
    if slice[0] != b'_' || !matches!(slice[1], b'x' | b'X') || slice[6] != b'_' {
        return None;
    }
    let hex = std::str::from_utf8(&slice[2..6]).ok()?;
    u16::from_str_radix(hex, 16).ok()
}

/// Unescape OOXML `_xHHHH_` hex escapes (`ECMA-376` 22.4.2.4), including
/// surrogate pairs.
fn unescape_ooxml(s: &str) -> String {
    if !s.contains("_x") && !s.contains("_X") {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut units: Vec<u16> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some(u) = ooxml_escape_at(bytes, i) {
            units.push(u);
            i += 7;
            continue;
        }
        let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
        let mut buf = [0u16; 2];
        for u in ch.encode_utf16(&mut buf) {
            units.push(*u);
        }
        i += ch.len_utf8();
    }
    String::from_utf16_lossy(&units)
}

/// The text of a string item: its `t`, or its runs' `t`s together
/// (phonetic runs left out).
fn item_text(n: roxmltree::Node) -> String {
    let mut s = String::new();
    for t in n.descendants().filter(|d| d.has_tag_name("t")) {
        if t.ancestors().any(|a| a.has_tag_name("rPh")) {
            continue;
        }
        s.push_str(t.text().unwrap_or(""));
    }
    unescape_ooxml(&s)
}

fn shared_strings(xml: &str) -> Result<Vec<String>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("sharedStrings.xml: {e}"))?;
    Ok(doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("si"))
        .map(item_text)
        .collect())
}

/// The column of a cell reference (`A1` 0, `AB7` 27).
pub fn column_of(r: &str) -> Option<usize> {
    let letters: String = r.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let mut n = 0usize;
    for c in letters.chars() {
        n = n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    Some(n - 1)
}

/// A number as a cell shows it plainly: whole numbers without a decimal
/// point, others without trailing zeros.
fn number_text(v: &str) -> String {
    match v.parse::<f64>() {
        Ok(x) if x.is_finite() && x.fract() == 0.0 && x.abs() < 1e15 => format!("{}", x as i64),
        Ok(x) if x.is_finite() => format!("{x}"),
        _ => v.to_string(),
    }
}

fn sheet_rows(xml: &str, shared: &[String], grid: bool) -> Result<Vec<Vec<String>>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("worksheet: {e}"))?;
    let Some(data) = doc.descendants().find(|n| n.has_tag_name("sheetData")) else {
        return Ok(Vec::new());
    };
    let mut rows: Vec<Vec<String>> = Vec::new();
    for row in data.children().filter(|n| n.has_tag_name("row")) {
        // Its place (`r`, from 1), for a grid.
        let at = row
            .attribute("r")
            .and_then(|r| r.trim().parse::<usize>().ok())
            .filter(|r| (1..=1_048_576).contains(r))
            .map(|r| r - 1);
        let mut cells: Vec<String> = Vec::new();
        let mut next = 0usize;
        for c in row.children().filter(|n| n.has_tag_name("c")) {
            let col = c.attribute("r").and_then(column_of).unwrap_or(next);
            next = col + 1;
            if col > 16_383 {
                continue;
            }
            let v = c
                .children()
                .find(|n| n.has_tag_name("v"))
                .and_then(|n| n.text())
                .unwrap_or("");
            let text = match c.attribute("t") {
                Some("s") => v
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| shared.get(i).cloned())
                    .unwrap_or_default(),
                Some("inlineStr") => c
                    .children()
                    .find(|n| n.has_tag_name("is"))
                    .map(item_text)
                    .unwrap_or_default(),
                Some("b") => (if v.trim() == "1" { "TRUE" } else { "FALSE" }).to_string(),
                Some("str") | Some("e") => unescape_ooxml(v),
                _ => number_text(v.trim()),
            };
            if cells.len() <= col {
                cells.resize(col + 1, String::new());
            }
            cells[col] = text;
        }
        while cells.last().is_some_and(|s| s.is_empty()) {
            cells.pop();
        }
        if grid {
            let at = at.unwrap_or(rows.len()).max(rows.len());
            if !cells.is_empty() {
                rows.resize(at, Vec::new());
                rows.push(cells);
            }
        } else if !cells.is_empty() {
            rows.push(cells);
        }
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    for r in &mut rows {
        r.resize(width, String::new());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A workbook made here, as a spreadsheet program writes one.
    fn book(sheet: &str, shared: &str) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o: zip::write::SimpleFileOptions = Default::default();
            for (name, body) in [
                (
                    "xl/workbook.xml",
                    r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Cards" sheetId="1" r:id="rId7"/></sheets></workbook>"#,
                ),
                (
                    "xl/_rels/workbook.xml.rels",
                    r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="worksheet" Target="worksheets/cards.xml"/></Relationships>"#,
                ),
                ("xl/worksheets/cards.xml", sheet),
                ("xl/sharedStrings.xml", shared),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn the_first_sheet_reads_as_rows_of_text() {
        let shared = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>Name</t></si><si><t>Photo</t></si><si><r><t>Ana </t></r><r><t>Silva</t></r></si><si><t>ana.jpg</t></si></sst>"#;
        let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
            <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="inlineStr"><is><t>ID</t></is></c></row>
            <row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2" t="s"><v>3</v></c><c r="C2"><v>1042</v></c></row>
            <row r="4"><c r="A4" t="str"><v>Bo</v></c><c r="C4"><v>2.50</v></c><c r="D4"/></row>
        </sheetData></worksheet>"#;
        let rows = read_rows(&book(sheet, shared)).unwrap();
        assert_eq!(
            rows,
            vec![
                vec!["Name".to_string(), "Photo".into(), "ID".into()],
                vec!["Ana Silva".into(), "ana.jpg".into(), "1042".into()],
                vec!["Bo".into(), "".into(), "2.5".into()],
            ]
        );
    }

    #[test]
    fn column_letters_and_bad_input() {
        assert_eq!(column_of("A1"), Some(0));
        assert_eq!(column_of("Z9"), Some(25));
        assert_eq!(column_of("AB7"), Some(27));
        assert_eq!(column_of("12"), None);
        assert!(read_rows(b"not a zip").is_err());
    }

    #[test]
    fn written_workbooks_read_back() {
        // #4143: a table's cells as a workbook, read back the same.
        let rows = vec![
            vec!["Item".to_string(), "Qty".into(), "Note".into()],
            vec!["Paper <A4> & co".into(), "12".into(), "".into()],
            vec!["Ink".into(), "007".into(), "line\none".into()],
        ];
        let bytes = write_rows(&rows, &[(0, 0, 0, 1)]).unwrap();
        assert_eq!(read_rows(&bytes).unwrap(), rows);
        // An empty row keeps its place in a grid.
        let gap = vec![
            vec!["a".to_string(), "b".into()],
            vec!["".into(), "".into()],
            vec!["c".into(), "".into()],
        ];
        let bytes = write_rows(&gap, &[]).unwrap();
        assert_eq!(read_grid(&bytes).unwrap(), gap);
        assert_eq!(read_rows(&bytes).unwrap().len(), 2);
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
        assert!(plain_number("12") && plain_number("-3.5") && plain_number("0.25"));
        assert!(!plain_number("007") && !plain_number("1,5") && !plain_number(" 4"));
    }

    #[test]
    fn multilingual_cells_legacy_xml_encodings_and_ooxml_escapes_read() {
        let shared = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>Позиция</t></si><si><t>日本語</t></si><si><t>مرحبا</t></si><si><t>A_x0020_B_x0416_</t></si></sst>"#;
        let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
            <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="s"><v>2</v></c><c r="D1" t="s"><v>3</v></c></row>
        </sheetData></worksheet>"#;
        let rows = read_rows(&book(sheet, shared)).unwrap();
        assert_eq!(
            rows,
            vec![vec![
                "Позиция".to_string(),
                "日本語".into(),
                "مرحبا".into(),
                "A BЖ".into()
            ]]
        );

        // Non-UTF-8 sharedStrings.xml declared as windows-1251.
        let xml_1251 = "<?xml version=\"1.0\" encoding=\"windows-1251\"?><sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><si><t>Бумага</t></si></sst>";
        let (raw_1251, _, _) = encoding_rs::WINDOWS_1251.encode(xml_1251);
        let decoded = decode_xml_bytes(&raw_1251);
        assert_eq!(shared_strings(&decoded).unwrap(), vec!["Бумага".to_string()]);
    }
}

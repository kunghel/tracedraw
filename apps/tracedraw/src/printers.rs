//! The system's printers for the Print dialog's Printer list: the
//! printers CUPS knows (Linux and macOS), listed by `lpstat` and printed
//! to with `lp`, which takes the print PDF as it is; on Windows every
//! printer Windows has, local ones and network connections alike, listed
//! and printed to through PowerShell (the sheets drawn by TraceDraw and
//! handed to the printer as pictures, so no PDF program is needed). In
//! the browser the list is empty and the print goes through the PDF
//! viewer, as before. See docs/behavior/printing.md.

use std::path::Path;

/// What a printer is doing, as the system reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrinterState {
    #[default]
    Unknown,
    Ready,
    Printing,
    WarmingUp,
    Stopped,
    Offline,
}

impl PrinterState {
    /// The i18n key of the Print dialog's Status row.
    pub fn key(self) -> &'static str {
        match self {
            PrinterState::Unknown => "print.status_unknown",
            PrinterState::Ready => "print.status_ready",
            PrinterState::Printing => "print.status_printing",
            PrinterState::WarmingUp => "print.status_warming_up",
            PrinterState::Stopped => "print.status_stopped",
            PrinterState::Offline => "print.status_offline",
        }
    }
}

/// A printer the system has.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Printer {
    /// The name `lp -d` takes.
    pub name: String,
    /// The system's default destination.
    pub default: bool,
    /// The Print dialog's Status, Location and Comment rows.
    pub state: PrinterState,
    pub location: String,
    pub comment: String,
}

/// `printers` in name order (ignoring case); the default stays where its
/// name puts it (#614).
pub fn by_name(mut printers: Vec<Printer>) -> Vec<Printer> {
    printers.sort_by_cached_key(|p| (p.name.to_lowercase(), p.name.clone()));
    printers
}

/// The printers `lpstat -l -p -d` lists ("printer NAME is idle..." lines,
/// each followed by indented details such as "Description:" and
/// "Location:", and "system default destination: NAME"), in name order.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn parse_lpstat(output: &str) -> Vec<Printer> {
    let mut out: Vec<Printer> = Vec::new();
    let mut default: Option<String> = None;
    // The printer the indented lines below belong to.
    let mut current: Option<usize> = None;
    for raw in output.lines() {
        let indented = raw.starts_with([' ', '\t']);
        let line = raw.trim();
        if indented {
            let Some(p) = current.and_then(|i| out.get_mut(i)) else {
                continue;
            };
            if let Some(v) = line.strip_prefix("Description:") {
                p.comment = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("Location:") {
                p.location = v.trim().to_string();
            }
            continue;
        }
        current = None;
        if let Some(rest) = line.strip_prefix("printer ") {
            let mut words = rest.splitn(2, char::is_whitespace);
            let name = words.next().unwrap_or_default();
            let after = words.next().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let state = if after.contains("now printing") {
                PrinterState::Printing
            } else if after.contains("is idle") {
                PrinterState::Ready
            } else if after.contains("disabled") {
                PrinterState::Stopped
            } else {
                PrinterState::Unknown
            };
            match out.iter().position(|p| p.name == name) {
                Some(i) => current = Some(i),
                None => {
                    out.push(Printer {
                        name: name.to_string(),
                        state,
                        ..Default::default()
                    });
                    current = Some(out.len() - 1);
                }
            }
        } else if let Some(rest) = line.strip_prefix("system default destination:") {
            let name = rest.trim();
            if !name.is_empty() {
                default = Some(name.to_string());
            }
        }
    }
    if let Some(name) = default {
        if let Some(p) = out.iter_mut().find(|p| p.name == name) {
            p.default = true;
        }
    }
    by_name(out)
}

/// The PowerShell command that lists Windows' printers, one per line:
/// name, default (True or False), PrinterStatus, WorkOffline, location
/// and comment, separated by tabs. `Win32_Printer` holds the printers
/// installed on the computer and the network printers connected for the
/// user (#614).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub const WINDOWS_LIST: &str = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
Get-CimInstance Win32_Printer | ForEach-Object { \
\"$($_.Name)`t$($_.Default)`t$($_.PrinterStatus)`t$($_.WorkOffline)`t\
$($_.Location -replace '\\s+', ' ')`t$($_.Comment -replace '\\s+', ' ')\" }";

/// The state of a `Win32_Printer`'s PrinterStatus (3 idle, 4 printing,
/// 5 warming up, 6 stopped, 7 offline), offline whatever it says when
/// WorkOffline is set.
fn windows_state(status: &str, offline: &str) -> PrinterState {
    if offline.trim().eq_ignore_ascii_case("true") {
        return PrinterState::Offline;
    }
    match status.trim().parse::<u32>() {
        Ok(3) => PrinterState::Ready,
        Ok(4) => PrinterState::Printing,
        Ok(5) => PrinterState::WarmingUp,
        Ok(6) => PrinterState::Stopped,
        Ok(7) => PrinterState::Offline,
        _ => PrinterState::Unknown,
    }
}

/// The printers [`WINDOWS_LIST`] printed, in name order.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn parse_windows_list(output: &str) -> Vec<Printer> {
    let mut out: Vec<Printer> = Vec::new();
    for line in output.lines() {
        let line = line
            .trim_end_matches(['\r', '\n'])
            .trim_start_matches('\u{feff}');
        let fields: Vec<&str> = line.split('\t').collect();
        let field = |i: usize| fields.get(i).map(|f| f.trim()).unwrap_or("");
        let name = field(0);
        if name.is_empty() || out.iter().any(|p| p.name == name) {
            continue;
        }
        out.push(Printer {
            name: name.to_string(),
            default: field(1).eq_ignore_ascii_case("true"),
            state: windows_state(field(2), field(3)),
            location: field(4).to_string(),
            comment: field(5).to_string(),
        });
    }
    by_name(out)
}

/// The printers the system has, in name order; empty where none is set
/// up or there is no CUPS. It can take seconds (a print server that does
/// not answer): the Print dialog calls it off the interface's thread.
#[cfg(not(any(target_arch = "wasm32", target_os = "windows")))]
pub fn system_printers() -> Vec<Printer> {
    let out = std::process::Command::new("lpstat")
        .args(["-l", "-p", "-d"])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .output();
    match out {
        Ok(o) => parse_lpstat(&String::from_utf8_lossy(&o.stdout)),
        Err(e) => {
            log::info!("no printer list: lpstat: {e}");
            Vec::new()
        }
    }
}

#[cfg(target_os = "windows")]
pub fn system_printers() -> Vec<Printer> {
    match powershell().args(["-Command", WINDOWS_LIST]).output() {
        Ok(o) => parse_windows_list(&String::from_utf8_lossy(&o.stdout)),
        Err(e) => {
            log::info!("no printer list: powershell: {e}");
            Vec::new()
        }
    }
}

/// PowerShell without a profile, a prompt or a console window.
#[cfg(target_os = "windows")]
fn powershell() -> std::process::Command {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut c = std::process::Command::new("powershell.exe");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
    ])
    .creation_flags(CREATE_NO_WINDOW);
    c
}

#[cfg(target_arch = "wasm32")]
pub fn system_printers() -> Vec<Printer> {
    Vec::new()
}

/// The printer the dialog starts on: the one remembered when the system
/// still has it, else the system's default, else the first listed, else
/// none (the print goes through the PDF viewer).
pub fn starting_printer(remembered: Option<&str>, printers: &[Printer]) -> Option<String> {
    if let Some(r) = remembered {
        if printers.iter().any(|p| p.name == r) {
            return Some(r.to_string());
        }
    }
    printers
        .iter()
        .find(|p| p.default)
        .or_else(|| printers.first())
        .map(|p| p.name.clone())
}

/// The program and arguments that open `printer`'s own settings (the
/// Print dialog's Properties button): on Windows the printer's printing
/// preferences, on macOS the system's Printers & Scanners settings, on
/// Linux and other CUPS systems the printer's page of the CUPS web
/// interface in the browser.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn properties_command(printer: &str) -> (String, Vec<String>) {
    if cfg!(target_os = "windows") {
        (
            "rundll32.exe".into(),
            vec![
                "printui.dll,PrintUIEntry".into(),
                "/e".into(),
                "/n".into(),
                printer.into(),
            ],
        )
    } else if cfg!(target_os = "macos") {
        (
            "open".into(),
            vec!["x-apple.systempreferences:com.apple.Print-Scan-Settings.extension".into()],
        )
    } else {
        let mut url = String::from("http://localhost:631/printers/");
        for b in printer.bytes() {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                url.push(b as char);
            } else {
                url.push_str(&format!("%{b:02X}"));
            }
        }
        ("xdg-open".into(), vec![url])
    }
}

/// Open `printer`'s settings ([`properties_command`]); Err says why not.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_properties(printer: &str) -> Result<(), String> {
    let (program, args) = properties_command(printer);
    let mut c = std::process::Command::new(&program);
    c.args(&args);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.spawn().map(|_| ()).map_err(|e| format!("{program}: {e}"))
}

#[cfg(target_arch = "wasm32")]
pub fn open_properties(printer: &str) -> Result<(), String> {
    Err(format!("no printer {printer} here"))
}

/// The `lp` command that prints `pdf` on `printer`: `copies` copies,
/// collated as whole sets or sheet by sheet, under `title` in the queue.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn lp_args(printer: &str, pdf: &Path, copies: u32, collate: bool, title: &str) -> Vec<String> {
    let mut args = vec!["-d".to_string(), printer.to_string()];
    if copies > 1 {
        args.push("-n".into());
        args.push(copies.min(999).to_string());
        args.push("-o".into());
        args.push(format!(
            "collate={}",
            if collate { "true" } else { "false" }
        ));
    }
    let title = title.trim();
    if !title.is_empty() {
        args.push("-t".into());
        args.push(title.to_string());
    }
    args.push("--".into());
    args.push(pdf.display().to_string());
    args
}

/// Send `pdf` to `printer` through `lp`. Ok with what `lp` said (the
/// request id), Err with its complaint.
#[cfg(not(any(target_arch = "wasm32", target_os = "windows")))]
pub fn print_pdf(
    printer: &str,
    pdf: &Path,
    copies: u32,
    collate: bool,
    title: &str,
) -> Result<String, String> {
    let out = std::process::Command::new("lp")
        .args(lp_args(printer, pdf, copies, collate, title))
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("lp: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("lp: {}", out.status)
        } else {
            err
        })
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "windows"))]
#[allow(dead_code)]
pub fn print_pdf(
    printer: &str,
    _pdf: &Path,
    _copies: u32,
    _collate: bool,
    _title: &str,
) -> Result<String, String> {
    Err(format!("no printer {printer} here"))
}

/// `s` as a PowerShell string in single quotes (the quote marks
/// PowerShell takes for one, typographic ones too, doubled).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// The PowerShell script that prints `sheets` (picture files with the
/// sheet's width and height in mm) on the Windows printer `printer`:
/// `copies` copies, collated or not, under `title` in the queue. Each
/// sheet takes the printer's paper of its size when it has one (either
/// way round), turns the paper for a wide sheet, and is drawn at its
/// size from the paper's corner, the printer's unprintable edge taken
/// off, as a PDF viewer prints a page at actual size. Exits with 2 when
/// the printer is gone.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn windows_print_script(
    printer: &str,
    sheets: &[(std::path::PathBuf, f64, f64)],
    copies: u32,
    collate: bool,
    title: &str,
) -> String {
    let files: Vec<String> = sheets
        .iter()
        .map(|(p, _, _)| ps_quote(&p.display().to_string()))
        .collect();
    let sizes: Vec<String> = sheets
        .iter()
        .map(|(_, w, h)| format!("@({w:.3}, {h:.3})"))
        .collect();
    format!(
        r#"$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$files = @({files})
$sizes = @({sizes})
$doc = New-Object System.Drawing.Printing.PrintDocument
$doc.PrinterSettings.PrinterName = {printer}
if (-not $doc.PrinterSettings.IsValid) {{ [Console]::Error.WriteLine('Printer not found: ' + {printer}); exit 2 }}
$doc.DocumentName = {title}
$doc.PrinterSettings.Copies = {copies}
$doc.PrinterSettings.Collate = ${collate}
$doc.OriginAtMargins = $false
$state = @{{ i = 0 }}
$doc.add_QueryPageSettings({{
    param($sender, $e)
    $w = $sizes[$state.i][0]; $h = $sizes[$state.i][1]
    foreach ($p in $doc.PrinterSettings.PaperSizes) {{
        $pw = $p.Width * 0.254; $ph = $p.Height * 0.254
        if (([Math]::Abs($pw - [Math]::Min($w, $h)) -lt 2) -and ([Math]::Abs($ph - [Math]::Max($w, $h)) -lt 2)) {{
            $e.PageSettings.PaperSize = $p
            break
        }}
    }}
    $e.PageSettings.Landscape = ($w -gt $h)
}})
$doc.add_PrintPage({{
    param($sender, $e)
    $w = $sizes[$state.i][0]; $h = $sizes[$state.i][1]
    $img = [System.Drawing.Image]::FromFile($files[$state.i])
    $g = $e.Graphics
    $g.PageUnit = [System.Drawing.GraphicsUnit]::Millimeter
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $mx = $e.PageSettings.HardMarginX * 0.254; $my = $e.PageSettings.HardMarginY * 0.254
    $g.DrawImage($img, [single](-$mx), [single](-$my), [single]$w, [single]$h)
    $img.Dispose()
    $state.i++
    $e.HasMorePages = $state.i -lt $files.Count
}})
$doc.Print()
"#,
        files = files.join(", "),
        sizes = sizes.join(", "),
        printer = ps_quote(printer),
        title = ps_quote(title),
        copies = copies.clamp(1, 999),
        collate = if collate { "true" } else { "false" },
    )
}

/// Print `sheets` (PNG pictures with their size in mm) on the Windows
/// printer `printer` through [`windows_print_script`]. Ok when Windows
/// took the job, Err with what went wrong.
#[cfg(target_os = "windows")]
pub fn print_sheets_windows(
    printer: &str,
    sheets: &[(Vec<u8>, f64, f64)],
    copies: u32,
    collate: bool,
    title: &str,
) -> Result<String, String> {
    let dir = std::env::temp_dir().join("tracedraw-print");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut files = Vec::with_capacity(sheets.len());
    for (k, (png, w, h)) in sheets.iter().enumerate() {
        let path = dir.join(format!("sheet-{}.png", k + 1));
        std::fs::write(&path, png).map_err(|e| e.to_string())?;
        files.push((path, *w, *h));
    }
    let script = dir.join("print.ps1");
    // With its byte order mark PowerShell reads the script as UTF-8, so
    // printer and file names in any script survive.
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(
        windows_print_script(printer, &files, copies, collate, title).as_bytes(),
    );
    std::fs::write(&script, bytes).map_err(|e| e.to_string())?;
    let out = powershell()
        .arg("-File")
        .arg(&script)
        .output()
        .map_err(|e| format!("powershell: {e}"))?;
    for (path, _, _) in &files {
        let _ = std::fs::remove_file(path);
    }
    if out.status.success() {
        Ok(String::new())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("powershell: {}", out.status)
        } else {
            err
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Printer list comes from `lpstat`: every printer in name order,
    /// the default marked where it stands, with its state, description
    /// and location (#300, #614).
    #[test]
    fn the_printer_list_is_read_from_lpstat() {
        let out = "printer Office_HP disabled since Mon 20 Oct 2026 09:00:00 AM -\n\
                   \tPaused\n\
                   \tDescription: HP LaserJet 4250\n\
                   \tLocation: Room 3-102\n\
                   printer EPSON-L3250-Series is idle.  enabled since Tue 21 Oct 2026 10:00:00 AM\n\
                   \tDescription: EPSON L3250 Series\n\
                   \tLocation:\n\
                   printer PDF now printing PDF-12.  enabled since Tue 21 Oct 2026 10:00:00 AM\n\
                   system default destination: Office_HP\n";
        let p = parse_lpstat(out);
        let names: Vec<&str> = p.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["EPSON-L3250-Series", "Office_HP", "PDF"]);
        assert!(!p[0].default && p[1].default && !p[2].default);
        assert_eq!(p[1].state, PrinterState::Stopped);
        assert_eq!(p[1].comment, "HP LaserJet 4250");
        assert_eq!(p[1].location, "Room 3-102");
        assert_eq!(p[0].state, PrinterState::Ready);
        assert_eq!(p[0].location, "");
        assert_eq!(p[2].state, PrinterState::Printing);
        // No printers, or no default: the list as given.
        assert!(parse_lpstat("lpstat: No destinations added.\n").is_empty());
        assert!(parse_lpstat("no system default destination\n").is_empty());
        let p =
            parse_lpstat("printer A is idle.\nprinter B is idle.\nno system default destination\n");
        assert_eq!(p.len(), 2);
        assert!(!p[0].default);
    }

    /// Windows lists its printers through PowerShell, network ones too,
    /// in name order with the default marked in place, their state,
    /// location and comment (#614).
    #[test]
    fn the_windows_printer_list_holds_network_printers() {
        let out = "\u{feff}Microsoft Print to PDF\tFalse\t3\tFalse\t\t\r\n\
                   \\\\office-srv\\HP LaserJet 4250\tTrue\t4\tFalse\tRoom 3-102\tColour, A3\r\n\
                   \\\\office-srv\\Canon iR-ADV C5535\tFalse\t3\tTrue\t\t\r\n\
                   \r\n\
                   Fax\tFalse\r\n";
        let p = parse_windows_list(out);
        let names: Vec<&str> = p.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "\\\\office-srv\\Canon iR-ADV C5535",
                "\\\\office-srv\\HP LaserJet 4250",
                "Fax",
                "Microsoft Print to PDF",
            ]
        );
        assert!(p[1].default && !p[0].default && !p[3].default);
        assert_eq!(p[1].state, PrinterState::Printing);
        assert_eq!(p[1].location, "Room 3-102");
        assert_eq!(p[1].comment, "Colour, A3");
        assert_eq!(p[0].state, PrinterState::Offline, "WorkOffline");
        assert_eq!(p[3].state, PrinterState::Ready);
        assert_eq!(p[2].state, PrinterState::Unknown, "an older listing");
        assert!(WINDOWS_LIST.contains("PrinterStatus") && WINDOWS_LIST.contains("Comment"));
        assert!(parse_windows_list("").is_empty());
        // The listing command asks for every printer Windows has.
        assert!(WINDOWS_LIST.contains("Win32_Printer"));
    }

    /// The Windows print script names the printer, the copies and the
    /// sheets, quoting what it is given.
    #[test]
    fn the_windows_print_script_names_the_printer_copies_and_sheets() {
        let sheets = [
            (
                std::path::PathBuf::from(r"C:\Temp\sheet-1.png"),
                210.0,
                297.0,
            ),
            (
                std::path::PathBuf::from(r"C:\Temp\sheet-2.png"),
                297.0,
                210.0,
            ),
        ];
        let s = windows_print_script(r"\\srv\Bob's printer", &sheets, 3, false, "Poster");
        assert!(
            s.contains(r"$doc.PrinterSettings.PrinterName = '\\srv\Bob''s printer'"),
            "{s}"
        );
        assert!(s.contains("$doc.PrinterSettings.Copies = 3"));
        assert!(s.contains("$doc.PrinterSettings.Collate = $false"));
        assert!(
            s.contains(r"$files = @('C:\Temp\sheet-1.png', 'C:\Temp\sheet-2.png')"),
            "{s}"
        );
        assert!(
            s.contains("$sizes = @(@(210.000, 297.000), @(297.000, 210.000))"),
            "{s}"
        );
        assert!(s.contains("$doc.DocumentName = 'Poster'"));
        // Typographic quotes close a PowerShell string too.
        assert_eq!(ps_quote("a\u{2019}b"), "'a\u{2019}\u{2019}b'");
        assert!(windows_print_script("P", &sheets, 0, true, "").contains("Copies = 1"));
    }

    #[test]
    fn the_dialog_starts_on_the_remembered_printer_else_the_default() {
        let printers =
            parse_lpstat("printer A is idle.\nprinter B is idle.\nsystem default destination: B\n");
        assert_eq!(starting_printer(Some("A"), &printers).as_deref(), Some("A"));
        assert_eq!(
            starting_printer(Some("Gone"), &printers).as_deref(),
            Some("B")
        );
        assert_eq!(starting_printer(None, &printers).as_deref(), Some("B"));
        assert_eq!(starting_printer(Some("A"), &[]), None);
        // The default is chosen where it stands, not moved first.
        let printers = parse_lpstat(
            "printer A is idle.\nprinter B is idle.\nprinter C is idle.\nsystem default destination: C\n",
        );
        assert_eq!(printers[2].name, "C");
        assert_eq!(starting_printer(None, &printers).as_deref(), Some("C"));
    }

    /// Properties opens the printer's own settings: its printing
    /// preferences on Windows, Printers & Scanners on macOS, its CUPS web
    /// page elsewhere (#614).
    #[test]
    fn properties_open_the_printers_own_settings() {
        let (program, args) = properties_command("Office HP/2");
        if cfg!(target_os = "windows") {
            assert_eq!(program, "rundll32.exe");
            assert_eq!(
                args,
                ["printui.dll,PrintUIEntry", "/e", "/n", "Office HP/2"]
            );
        } else if cfg!(target_os = "macos") {
            assert_eq!(program, "open");
            assert_eq!(
                args,
                ["x-apple.systempreferences:com.apple.Print-Scan-Settings.extension"]
            );
        } else {
            assert_eq!(program, "xdg-open");
            assert_eq!(args, ["http://localhost:631/printers/Office%20HP%2F2"]);
        }
    }

    /// `lp` gets the printer, the copies with their collation, the title
    /// and the file; one copy needs no count.
    #[test]
    fn lp_is_told_the_printer_copies_and_title() {
        let pdf = Path::new("/tmp/tracedraw-print/Poster.pdf");
        assert_eq!(
            lp_args("EPSON-L3250-Series", pdf, 1, true, "Poster"),
            [
                "-d",
                "EPSON-L3250-Series",
                "-t",
                "Poster",
                "--",
                "/tmp/tracedraw-print/Poster.pdf"
            ]
        );
        assert_eq!(
            lp_args("A", pdf, 3, false, ""),
            [
                "-d",
                "A",
                "-n",
                "3",
                "-o",
                "collate=false",
                "--",
                "/tmp/tracedraw-print/Poster.pdf"
            ]
        );
        assert_eq!(
            lp_args("A", pdf, 2, true, " ")[4..6],
            ["-o", "collate=true"]
        );
    }
}

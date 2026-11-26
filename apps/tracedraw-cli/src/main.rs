//! tracedraw-cli: headless tools.
//!
//! ```text
//! tracedraw-cli inspect file.cdr          dump the RIFF chunk tree
//! tracedraw-cli info file.cdr             version, pages, objects, warnings
//! tracedraw-cli convert in.cdr out.svg    export the first page (or .tdraw)
//! tracedraw-cli check file.cdr            findings of a written file
//! ```

use std::process::ExitCode;

/// Damage a file deterministically: truncate, flip bytes, cut a span, or
/// overwrite a few 32-bit words.
/// `println!` that ends the program quietly when the reader of its output
/// went away (`tracedraw-cli inspect file | head`) instead of panicking.
macro_rules! outln {
    () => {
        outln!("")
    };
    ($($t:tt)*) => {{
        use std::io::Write as _;
        if let Err(e) = writeln!(std::io::stdout(), $($t)*) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                std::process::exit(0);
            }
        }
    }};
}

/// `print!` with the same handling as [`outln!`].
macro_rules! out {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        if let Err(e) = write!(std::io::stdout(), $($t)*) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                std::process::exit(0);
            }
        }
    }};
}

fn mutate(data: &[u8], seed: u64) -> Vec<u8> {
    let mut v = data.to_vec();
    let mut s = seed | 1;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let len = v.len().max(1);
    match rnd() % 4 {
        0 => {
            let n = (rnd() as usize) % len;
            v.truncate(n);
        }
        1 => {
            for _ in 0..(1 + rnd() % 64) {
                let i = (rnd() as usize) % len;
                if i < v.len() {
                    v[i] = rnd() as u8;
                }
            }
        }
        2 => {
            let i = (rnd() as usize) % len;
            let j = (rnd() as usize) % len;
            let (a, b) = (i.min(j), i.max(j).min(v.len()));
            v.drain(a..b);
        }
        _ => {
            for _ in 0..(1 + rnd() % 8) {
                let i = (rnd() as usize) % len;
                if i + 4 <= v.len() {
                    v[i..i + 4].copy_from_slice(&(rnd() as u32).to_le_bytes());
                }
            }
        }
    }
    v
}

fn usage() -> ExitCode {
    eprintln!("usage:\n  tracedraw-cli inspect <file.cdr> [chunk id]\n  tracedraw-cli info <file.cdr|file.cmx|file.tdraw> [--unknown]\n  tracedraw-cli check <file.cdr>   findings of a written file (curves, boxes, ids, text, references)\n  tracedraw-cli icc <profile.icc>\n  tracedraw-cli thumbnail <file> <out.png> [size]   the first page as a picture, for file managers\n  tracedraw-cli stress <file> [iterations]   mutation test of the file's reader\n  tracedraw-cli convert <in.cdr|in.cmx|in.tdraw|in.svg|in.pdf|in.ai|in.eps|in.ps|in.prn|in.dxf|in.psd|in.emf|in.wmf|in.plt|in.txt|in.rtf|in.docx> <out.svg|out.pdf|out.eps|out.ps|out.prn|out.dxf|out.emf|out.wmf|out.plt|out.psd|out.html|out.png|out.tdraw|out.cdr>");
    ExitCode::from(2)
}

/// How text goes into a PDF: `curves`, `named` (fonts left out of the
/// file) or, by default, text with embedded fonts.
fn pdf_text(choice: Option<&str>) -> tracedraw_io::pdf::PdfText {
    let mut t = tracedraw_io::pdf::PdfText::default();
    match choice.map(str::to_ascii_lowercase).as_deref() {
        Some("curves") => t.as_curves = true,
        Some("named") => t.embed_fonts = false,
        _ => {}
    }
    t
}

/// Protection for a written PDF: a password to open it, a password for
/// the permissions and the permissions to deny (a comma-separated list of
/// print, hq-print, copy, modify, annotate, forms, assemble and
/// accessibility). Nothing set: no protection.
fn pdf_security(
    open: Option<String>,
    owner: Option<String>,
    deny: Option<String>,
) -> Option<tracedraw_io::pdf_crypt::Security> {
    let open = open.unwrap_or_default();
    let owner = owner.unwrap_or_default();
    if open.is_empty() && owner.is_empty() {
        return None;
    }
    let mut p = tracedraw_io::pdf_crypt::Permissions::default();
    if !owner.is_empty() {
        for item in deny.unwrap_or_default().split(',') {
            match item.trim().to_ascii_lowercase().as_str() {
                "print" => p.print = false,
                "hq-print" => p.print_high_quality = false,
                "copy" => p.copy = false,
                "modify" => p.modify = false,
                "annotate" => p.annotate = false,
                "forms" => p.fill_forms = false,
                "assemble" => p.assemble = false,
                "accessibility" => p.accessibility = false,
                "" => {}
                other => eprintln!("warning: unknown permission {other}"),
            }
        }
    }
    let owner = if owner.is_empty() {
        open.clone()
    } else {
        owner
    };
    Some(tracedraw_io::pdf_crypt::Security {
        user_password: open,
        owner_password: owner,
        permissions: p,
    })
}

/// What the .cdr reader skipped or guessed, one line each, as the other
/// readers report it while they read.
fn reader_warnings(report: &tracedraw_cdr::ParseReport) -> Vec<String> {
    let mut lines: Vec<String> = report
        .warnings
        .iter()
        .map(|w| format!("warning: {w}"))
        .collect();
    if report.skipped_objects > 0 {
        lines.push(format!(
            "warning: {} object(s) skipped",
            report.skipped_objects
        ));
    }
    lines
}

/// Read a document of any kind the CLI knows; the fonts it carries are
/// loaded for its text.
fn load(
    path: &str,
) -> Result<(tracedraw_core::Document, Option<tracedraw_cdr::ParseReport>), String> {
    let mut loaded = read_document(path)?;
    if !loaded.0.fonts.is_empty() {
        tracedraw_text::fonts().add_document_fonts(&loaded.0.fonts);
    }
    // The fonts the app was given, and the stand-in for each missing
    // one, as the app finds it (#3818).
    static APP_FONTS: std::sync::Once = std::sync::Once::new();
    APP_FONTS.call_once(|| {
        if let Some(settings) = app_settings_path() {
            load_app_fonts(tracedraw_text::fonts(), &settings);
        }
    });
    for line in stand_ins(tracedraw_text::fonts(), &loaded.0) {
        eprintln!("{path}: {line}");
    }
    // The app set to take the automatic fonts and keep them in the
    // drawing (Missing Fonts > Always, #3725): the CLI writes the
    // drawing with them too, so the files it makes name fonts that are
    // installed (#3965).
    static KEEP_STAND_INS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let keep = *KEEP_STAND_INS.get_or_init(|| {
        app_settings_path()
            .map(|s| keeps_stand_ins(&s.join("settings.json")) || keeps_stand_ins(&s))
            .unwrap_or(false)
    });
    if keep {
        let n = replace_missing_fonts(tracedraw_text::fonts(), &mut loaded.0);
        if n > 0 {
            eprintln!("{path}: {n} missing font(s) replaced in the drawing by their stand-ins");
        }
    }
    // Paragraph text flows around the objects marked to wrap it.
    loaded.0.refresh_text_flow();
    // CMYK colours render through the profile the drawing carries.
    if let Some(p) = &loaded.0.cmyk_profile {
        match tracedraw_core::icc::Profile::parse(&p.data) {
            Ok(profile) if profile.color_space() == tracedraw_core::icc::ColorSpace::Cmyk => {
                tracedraw_core::color::engine::install(tracedraw_core::color::IccEngine::new(
                    None,
                    Some(profile),
                    tracedraw_core::icc::Intent::default(),
                    false,
                ));
            }
            _ => eprintln!("{path}: the colour profile it carries could not be used"),
        }
    }
    tracedraw_render::bitmap_effects::prepare(&mut loaded.0);
    Ok(loaded)
}

/// The app's preferences file: `settings.json` in its configuration
/// folder (the `tracedraw-settings` folder beside the program for a
/// portable copy, or as `TRACEDRAW_CONFIG_DIR` names).
fn app_settings_path() -> Option<std::path::PathBuf> {
    let dir = if let Ok(dir) = std::env::var("TRACEDRAW_CONFIG_DIR") {
        Some(std::path::PathBuf::from(dir))
    } else if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join("tracedraw-settings")))
        .filter(|d| d.is_dir())
    {
        Some(dir)
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|p| std::path::PathBuf::from(p).join("TraceDraw"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|p| std::path::PathBuf::from(p).join("Library/Application Support/TraceDraw"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".config"))
            })
            .map(|p| p.join("tracedraw"))
    };
    dir.map(|d| d.join("settings.json"))
}

/// Load the font files installed for the app in its Font Manager and
/// those of the folders added there, as the app does at start (#3818):
/// the CLI then draws with the fonts the app draws with. The
/// families added.
fn load_app_fonts(fonts: &tracedraw_text::FontSystem, settings: &std::path::Path) -> usize {
    let Ok(text) = std::fs::read_to_string(settings) else {
        return 0;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return 0;
    };
    let paths = |key: &str| -> Vec<std::path::PathBuf> {
        v["font_manager"][key]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str())
                    .map(std::path::PathBuf::from)
                    .collect()
            })
            .unwrap_or_default()
    };
    let folders = paths("folders");
    let mut files = paths("installed");
    files.extend(
        folders
            .iter()
            .flat_map(|d| tracedraw_text::catalog::font_files(d)),
    );
    if files.is_empty() {
        return 0;
    }
    fonts.set_font_roots(&folders);
    fonts.install_paths(&files).len()
}

/// A line for each font of `doc` that is not installed, naming the
/// installed font that draws its text instead, as the app's Missing
/// Fonts dialog finds it (#3818).
fn stand_ins(fonts: &tracedraw_text::FontSystem, doc: &tracedraw_core::Document) -> Vec<String> {
    doc.font_families_used()
        .into_iter()
        .filter_map(|family| {
            let with = fonts.substitute(&family)?;
            Some(format!(
                "font \"{family}\" is not installed: its text is drawn with \"{with}\""
            ))
        })
        .collect()
}

/// Whether the app's settings file `settings` says to take the automatic
/// font for each missing one and keep it in the drawing (#3725).
fn keeps_stand_ins(settings: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string(settings) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    let w = &v["warnings"];
    w["accept_missing_fonts"].as_bool() == Some(true)
        && w["accept_missing_fonts_keep"].as_bool() == Some(true)
}

/// Each font of `doc` that is not installed replaced in the drawing by
/// the installed font that draws its text instead, as the app's Missing
/// Fonts dialog does with Always and Replace in the drawing (#3965);
/// how many families were replaced.
fn replace_missing_fonts(
    fonts: &tracedraw_text::FontSystem,
    doc: &mut tracedraw_core::Document,
) -> usize {
    let mut n = 0;
    for family in doc.font_families_used() {
        if let Some(with) = fonts.substitute(&family) {
            if with != family && doc.replace_font_family(&family, &with) > 0 {
                n += 1;
            }
        }
    }
    n
}

fn read_document(
    path: &str,
) -> Result<(tracedraw_core::Document, Option<tracedraw_cdr::ParseReport>), String> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".cdr")
        || lower.ends_with(".cdt")
        || lower.ends_with(".cdrt")
        || lower.ends_with(".cmx")
    {
        tracedraw_cdr::open(path)
            .map(|(d, r)| (d, Some(r)))
            .map_err(|e| e.to_string())
    } else if lower.ends_with(".pdf")
        || (lower.ends_with(".ai") && !std::fs::read(path).is_ok_and(|b| b.starts_with(b"%!")))
    {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        // TRACEDRAW_PASSWORD opens a protected file.
        let password = std::env::var("TRACEDRAW_PASSWORD").unwrap_or_default();
        // TRACEDRAW_PDF_IMPORT_TEXT=curves reads the text as curves from
        // the fonts the file embeds; =paragraphs joins its lines into
        // paragraph text, as the app's Join Lines choice does.
        let text = std::env::var("TRACEDRAW_PDF_IMPORT_TEXT").unwrap_or_default();
        let options = tracedraw_io::pdf_import::ImportOptions {
            text_as_curves: text == "curves",
            text_as_paragraphs: text == "paragraphs",
            ..Default::default()
        };
        let imported = tracedraw_io::pdf_import::parse_with_options(
            &bytes,
            &mut tracedraw_core::id::IdSource::default(),
            &password,
            options,
        )
        .map_err(|e| e.to_string())?;
        for w in &imported.warnings {
            eprintln!("warning: {w}");
        }
        let title = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok((
            tracedraw_io::pdf_import::to_document(imported, &title),
            None,
        ))
    } else if [".eps", ".ps", ".prn", ".ai"]
        .iter()
        .any(|e| lower.ends_with(e))
    {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let imported =
            tracedraw_io::eps_import::parse(&bytes, &mut tracedraw_core::id::IdSource::default())?;
        for w in &imported.warnings {
            eprintln!("warning: {w}");
        }
        // Every page of the file, each at its own size.
        let pages = imported.pages();
        let first = pages
            .first()
            .map(|p| p.1)
            .unwrap_or(tracedraw_core::document::paper::A4);
        let mut doc = tracedraw_core::Document::new("eps", first);
        let mut ids = doc.ids().clone();
        for (i, (shapes, size)) in pages.into_iter().enumerate() {
            if i == 0 {
                doc.pages[0].layers[0].shapes = shapes;
                continue;
            }
            let mut layer = tracedraw_core::Layer::new(ids.layer(), "Layer 1");
            layer.shapes = shapes;
            doc.pages.push(tracedraw_core::Page {
                id: ids.page(),
                name: format!("Page {}", i + 1),
                size,
                layers: vec![layer],
                guides: Vec::new(),
                background: None,
            });
        }
        doc.set_ids(ids);
        Ok((doc, None))
    } else if lower.ends_with(".psd") || lower.ends_with(".psb") {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let imported =
            tracedraw_io::psd::parse(&bytes, &mut tracedraw_core::id::IdSource::default())?;
        for w in &imported.warnings {
            eprintln!("warning: {w}");
        }
        let mut doc = tracedraw_core::Document::new("psd", imported.size);
        doc.pages[0].layers[0].shapes = imported.shapes;
        Ok((doc, None))
    } else if lower.ends_with(".emf") || lower.ends_with(".wmf") {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let imported =
            tracedraw_io::emf::parse(&bytes, &mut tracedraw_core::id::IdSource::default())?;
        for w in &imported.warnings {
            eprintln!("warning: {w}");
        }
        let title = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok((tracedraw_io::emf::to_document(imported, &title), None))
    } else if lower.ends_with(".plt") || lower.ends_with(".hpgl") || lower.ends_with(".hgl") {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let imported =
            tracedraw_io::plt::parse(&bytes, &mut tracedraw_core::id::IdSource::default())?;
        for w in &imported.warnings {
            eprintln!("warning: {w}");
        }
        let title = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok((tracedraw_io::plt::to_document(imported, &title), None))
    } else if lower.ends_with(".txt") || lower.ends_with(".rtf") || lower.ends_with(".docx") {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let imported = if lower.ends_with(".rtf") {
            tracedraw_io::text_import::parse_rtf(&bytes)?
        } else if lower.ends_with(".docx") {
            tracedraw_io::text_import::parse_docx(&bytes)?
        } else {
            tracedraw_io::text_import::parse_txt(&bytes)
        };
        for w in &imported.warnings {
            eprintln!("warning: {w}");
        }
        let mut doc = tracedraw_core::Document::new("text", tracedraw_core::document::paper::A4);
        let mut ids = doc.ids().clone();
        let mut shape = imported.to_shape(tracedraw_io::text_import::Placement::a4(), &mut ids);
        shape.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK);
        shape.stroke = None;
        doc.pages[0].layers[0].shapes.push(shape);
        doc.set_ids(ids);
        Ok((doc, None))
    } else if lower.ends_with(".dxf") {
        let text = std::fs::read(path)
            .map_err(|e| e.to_string())
            .map(|b| String::from_utf8_lossy(&b).into_owned())?;
        let imported =
            tracedraw_io::dxf::parse(&text, &mut tracedraw_core::id::IdSource::default())?;
        for w in &imported.warnings {
            eprintln!("warning: {w}");
        }
        let title = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok((tracedraw_io::dxf::to_document(imported, &title), None))
    } else if lower.ends_with(".svg") || lower.ends_with(".svgz") {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let text = if bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
            use std::io::Read;
            let mut out = String::new();
            flate2::read::GzDecoder::new(&bytes[..])
                .read_to_string(&mut out)
                .map_err(|e| e.to_string())?;
            out
        } else {
            String::from_utf8(bytes).map_err(|e| e.to_string())?
        };
        let imported =
            tracedraw_io::svg_import::parse(&text, &mut tracedraw_core::id::IdSource::default())?;
        let mut doc = tracedraw_core::Document::new("svg", imported.size);
        doc.pages[0].layers[0].shapes = imported.shapes;
        Ok((doc, None))
    } else {
        tracedraw_io::load_native(path)
            .map(|d| (d, None))
            .map_err(|e| e.to_string())
    }
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    tracedraw_text::install();
    tracedraw_io::emf::install();
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("inspect") => {
            let Some(path) = args.get(2) else {
                return usage();
            };
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let container = match tracedraw_cdr::container::detect(&bytes) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            outln!(
                "container: {:?}, {}",
                container.kind,
                container.version.name()
            );
            let riff = match tracedraw_cdr::container::riff_stream(&bytes, &container) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let externals = tracedraw_cdr::container::external_streams(&bytes, &container);
            match tracedraw_cdr::riff::parse_with_externals(&riff, externals, container.version.0) {
                Ok(tree) => match args.get(3) {
                    // `inspect file.cdr arrw`: the data of every chunk with
                    // that id, in hex.
                    Some(id) => {
                        let mut name = [b' '; 4];
                        for (k, b) in id.bytes().take(4).enumerate() {
                            name[k] = b;
                        }
                        tree.root.walk(&mut |c, _| {
                            if c.is(&name) {
                                let d = tree.data(&riff, c);
                                outln!("{} {} bytes", c.id_str(), d.len());
                                for (i, row) in d.chunks(16).enumerate() {
                                    let hex: Vec<String> =
                                        row.iter().map(|b| format!("{b:02x}")).collect();
                                    outln!("{:6} {}", i * 16, hex.join(" "));
                                }
                            }
                        });
                    }
                    None => out!("{}", tree.dump()),
                },
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Some("check") => {
            // Checks a .cdr file for what a reader would stumble on
            // (curves, boxes, ids, text records, references); one line per
            // finding, exit code 1 when there are any.
            let Some(path) = args.get(2) else {
                return usage();
            };
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            match tracedraw_cdr::check::check_bytes(&bytes) {
                Ok(findings) if findings.is_empty() => {
                    outln!("{path}: no findings");
                    ExitCode::SUCCESS
                }
                Ok(findings) => {
                    for f in &findings {
                        outln!("{path}: {f}");
                    }
                    outln!("{path}: {} finding(s)", findings.len());
                    ExitCode::FAILURE
                }
                Err(e) => {
                    eprintln!("{path}: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("info") => {
            let Some(path) = args.get(2) else {
                return usage();
            };
            match load(path) {
                Ok((doc, report)) => {
                    outln!("title: {}", doc.title);
                    for p in &doc.pages {
                        let n: usize = p.layers.iter().map(|l| l.shapes.len()).sum();
                        outln!(
                            "{}: {:.1} x {:.1} mm, {} layer(s), {} object(s)",
                            p.name,
                            p.size.width,
                            p.size.height,
                            p.layers.len(),
                            n
                        );
                    }
                    for f in &doc.fonts {
                        outln!("carries font: {} ({} bytes)", f.family, f.data.len());
                    }
                    if let Some(p) = &doc.cmyk_profile {
                        outln!("carries CMYK profile: {} ({} bytes)", p.name, p.data.len());
                    }
                    // TRACEDRAW_OBJECTS=1 lists every object, for checking a
                    // reader against what the file holds.
                    if std::env::var("TRACEDRAW_OBJECTS").is_ok_and(|v| v != "0") {
                        for p in &doc.pages {
                            outln!("{}:", p.name);
                            for l in &p.layers {
                                outln!("  layer {:?}", l.name);
                                for s in &l.shapes {
                                    out!("{}", describe_shape(s, 2));
                                }
                            }
                        }
                    }
                    if let Some(r) = report {
                        outln!(
                            "version: {}",
                            r.version.map(|v| v.name()).unwrap_or_else(|| {
                                if path.to_ascii_lowercase().ends_with(".cmx") {
                                    "CMX".into()
                                } else {
                                    String::new()
                                }
                            })
                        );
                        outln!("skipped objects: {}", r.skipped_objects);
                        for w in &r.warnings {
                            outln!("warning: {w}");
                        }
                        // --unknown: the chunks and `loda` arguments the
                        // reader left uninterpreted, most frequent first.
                        if args.iter().skip(3).any(|a| a == "--unknown") {
                            outln!(
                                "unknown records: {} ({} kind(s))",
                                r.unknown.records(),
                                r.unknown.entries.len()
                            );
                            for line in r.unknown.lines() {
                                outln!("  {line}");
                            }
                        }
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{path}: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("icc") => {
            // Describe an ICC profile and show a few conversions through it.
            let Some(path) = args.get(2) else {
                usage();
                return ExitCode::FAILURE;
            };
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let profile = match tracedraw_core::icc::Profile::parse(&bytes) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            outln!(
                "{}: {:?}, {:?} -> {:?}, version {:?}, {} channel(s), device->PCS {}, PCS->device {}",
                profile.description().unwrap_or("(no description)"),
                profile.class(),
                profile.color_space(),
                profile.pcs(),
                profile.version(),
                profile.channels(),
                profile.has_device_to_pcs(),
                profile.has_pcs_to_device()
            );
            let srgb = tracedraw_core::icc::Profile::srgb();
            let intent = tracedraw_core::icc::Intent::default();
            if let Some(t) = tracedraw_core::icc::Transform::new(&profile, &srgb, intent, true) {
                let n = profile.channels();
                let samples: Vec<Vec<f32>> = if n == 4 {
                    vec![
                        vec![0.0, 0.0, 0.0, 0.0],
                        vec![1.0, 0.0, 0.0, 0.0],
                        vec![0.0, 1.0, 0.0, 0.0],
                        vec![0.0, 0.0, 1.0, 0.0],
                        vec![0.0, 0.0, 0.0, 1.0],
                    ]
                } else {
                    vec![vec![1.0; n], vec![0.0; n]]
                };
                for s in samples {
                    let rgb = t.transform(&s);
                    outln!("  {s:?} -> sRGB {rgb:?}");
                }
            }
            ExitCode::SUCCESS
        }
        Some("stress") => {
            // Mutation test: feed damaged copies of a file to its reader and
            // report panics (there must be none).
            let Some(input) = args.get(2) else {
                return usage();
            };
            let iters: u64 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(200);
            let data = match std::fs::read(input) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("{input}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let lower = input.to_ascii_lowercase();
            let mut panics = 0;
            std::panic::set_hook(Box::new(|info| eprintln!("panic: {info}")));
            for seed in 0..iters {
                let m = mutate(&data, seed * 7919 + 13);
                let lower = lower.clone();
                let ok = std::panic::catch_unwind(move || {
                    let mut ids = tracedraw_core::id::IdSource::default();
                    if lower.ends_with(".cdr")
                        || lower.ends_with(".cdt")
                        || lower.ends_with(".cdrt")
                        || lower.ends_with(".cmx")
                    {
                        let _ = tracedraw_cdr::open_bytes(&m, "stress");
                    } else if lower.ends_with(".pdf") || lower.ends_with(".ai") {
                        let _ = tracedraw_io::pdf_import::parse(&m, &mut ids);
                    } else if lower.ends_with(".eps") || lower.ends_with(".ps") {
                        let _ = tracedraw_io::eps_import::parse(&m, &mut ids);
                    } else if lower.ends_with(".dxf") {
                        let _ = tracedraw_io::dxf::parse(&String::from_utf8_lossy(&m), &mut ids);
                    } else if lower.ends_with(".psd") || lower.ends_with(".psb") {
                        let _ = tracedraw_io::psd::parse(&m, &mut ids);
                    } else if lower.ends_with(".emf") || lower.ends_with(".wmf") {
                        let _ = tracedraw_io::emf::parse(&m, &mut ids);
                    } else if lower.ends_with(".plt") {
                        let _ = tracedraw_io::plt::parse(&m, &mut ids);
                    } else if lower.ends_with(".rtf") {
                        let _ = tracedraw_io::text_import::parse_rtf(&m);
                    } else if lower.ends_with(".docx") {
                        let _ = tracedraw_io::text_import::parse_docx(&m);
                    } else if lower.ends_with(".svg") {
                        let _ =
                            tracedraw_io::svg_import::parse(&String::from_utf8_lossy(&m), &mut ids);
                    } else {
                        let _ = tracedraw_core::Document::from_tdraw_bytes(&m);
                    }
                })
                .is_ok();
                if !ok {
                    panics += 1;
                    eprintln!("seed {seed} panicked");
                }
            }
            outln!("{input}: {iters} mutated copies, {panics} panic(s)");
            if panics == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        // A picture of the first page for the file manager (the Linux
        // thumbnailer runs it): `thumbnail <in> <out.png> [size]`, the
        // long side `size` pixels (256 by default), white paper.
        Some("thumbnail") => {
            let (Some(input), Some(output)) = (args.get(2), args.get(3)) else {
                return usage();
            };
            let size = args
                .get(4)
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|s| s.is_finite() && *s >= 16.0)
                .unwrap_or(256.0)
                .min(2048.0);
            let doc = match load(input) {
                Ok((doc, _)) => doc,
                Err(e) => {
                    eprintln!("{input}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let Some(page) = doc.pages.first() else {
                eprintln!("{input}: no page");
                return ExitCode::FAILURE;
            };
            // The page with what lies beside it on the desktop, as the
            // Windows thumbnails and the Welcome Screen show it (#3082).
            let Some(pm) = tracedraw_render::render_whole_page_image(&doc, page.id, size) else {
                eprintln!("{input}: nothing to draw");
                return ExitCode::FAILURE;
            };
            match pm.save_png(output) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{output}: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("convert") => {
            let (Some(input), Some(output)) = (args.get(2), args.get(3)) else {
                return usage();
            };
            let (doc, report) = match load(input) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{input}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            for line in report.as_ref().map(reader_warnings).unwrap_or_default() {
                eprintln!("{line}");
            }
            // Conversion to a drawing/native file retains source state;
            // exports omit content excluded from output, as PDF does.
            let ext = std::path::Path::new(output)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let doc = if matches!(
                ext.as_str(),
                "svg"
                    | "pdf"
                    | "eps"
                    | "ps"
                    | "prn"
                    | "dxf"
                    | "emf"
                    | "wmf"
                    | "plt"
                    | "psd"
                    | "html"
                    | "png"
            ) {
                tracedraw_io::for_output(&doc)
            } else {
                std::borrow::Cow::Borrowed(&doc)
            };
            let res = if output.to_ascii_lowercase().ends_with(".svg") {
                tracedraw_io::save_svg(&doc, 0, output)
            } else if output.to_ascii_lowercase().ends_with(".pdf") {
                // TRACEDRAW_PDFX=x1a|x3|x4 selects a PDF/X level; TRACEDRAW_ICC
                // names the output profile (default: the CMYK profile the
                // drawing carries) and TRACEDRAW_BLEED the bleed in mm.
                // TRACEDRAW_PDF_BOOKMARKS=1 adds a bookmark per page; the
                // TRACEDRAW_PDF_*PASSWORD and _DENY variables protect it.
                // TRACEDRAW_PDF_TEXT=curves writes text as curves, =named
                // leaves the fonts out of the file.
                let standard = match std::env::var("TRACEDRAW_PDFX")
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "x1a" | "x-1a" => tracedraw_io::pdf::PdfStandard::X1a,
                    "x3" | "x-3" => tracedraw_io::pdf::PdfStandard::X3,
                    "x4" | "x-4" => tracedraw_io::pdf::PdfStandard::X4,
                    _ => tracedraw_io::pdf::PdfStandard::None,
                };
                let opts = tracedraw_io::pdf::PdfOptions {
                    standard,
                    output_profile: std::env::var("TRACEDRAW_ICC")
                        .ok()
                        .and_then(|p| std::fs::read(p).ok()),
                    output_condition: std::env::var("TRACEDRAW_CONDITION").unwrap_or_default(),
                    bleed_mm: std::env::var("TRACEDRAW_BLEED")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0.0),
                    title: None,
                    flatten_dpi: 300.0,
                    bookmarks: std::env::var("TRACEDRAW_PDF_BOOKMARKS")
                        .is_ok_and(|v| !matches!(v.as_str(), "" | "0" | "no" | "false")),
                    security: pdf_security(
                        std::env::var("TRACEDRAW_PDF_OPEN_PASSWORD").ok(),
                        std::env::var("TRACEDRAW_PDF_OWNER_PASSWORD").ok(),
                        std::env::var("TRACEDRAW_PDF_DENY").ok(),
                    ),
                    text: pdf_text(std::env::var("TRACEDRAW_PDF_TEXT").ok().as_deref()),
                    ..Default::default()
                };
                std::fs::write(output, tracedraw_io::pdf::document_to_pdf_with(&doc, &opts))
                    .map_err(tracedraw_io::Error::from)
            } else if output.to_ascii_lowercase().ends_with(".eps") {
                std::fs::write(output, tracedraw_io::eps::page_to_eps(&doc, 0))
                    .map_err(tracedraw_io::Error::from)
            } else if [".ps", ".prn"]
                .iter()
                .any(|e| output.to_ascii_lowercase().ends_with(e))
            {
                std::fs::write(output, tracedraw_io::eps::document_to_ps(&doc))
                    .map_err(tracedraw_io::Error::from)
            } else if output.to_ascii_lowercase().ends_with(".cdr") {
                // TRACEDRAW_CDR_VERSION picks the version written (7 to 27).
                let asked: u16 = std::env::var("TRACEDRAW_CDR_VERSION")
                    .ok()
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(tracedraw_cdr::write::DEFAULT_VERSION);
                let allowed = tracedraw_cdr::write::checked_versions();
                if !allowed.contains(&asked) {
                    eprintln!(
                        "cdr version {asked} is not saved: use {} to {}",
                        allowed.start(),
                        allowed.end()
                    );
                    std::process::exit(2);
                }
                let version = asked;
                std::fs::write(
                    output,
                    tracedraw_cdr::write::document_to_cdr_version_with(&doc, version, &{
                        let pages: Vec<tracedraw_cdr::write::Png> =
                            tracedraw_io::page_pictures(&doc, 256)
                                .into_iter()
                                .map(|(data, width, height)| tracedraw_cdr::write::Png {
                                    data,
                                    width,
                                    height,
                                })
                                .collect();
                        // The text's faces go in the font table (from 16);
                        // TRACEDRAW_CDR_EMBED_FONTS=1 embeds them.
                        let fonts = tracedraw_text::fonts();
                        tracedraw_cdr::write::Previews {
                            thumbnail: pages.first().cloned(),
                            pages,
                            painted_bitmaps: tracedraw_io::bitmap_fill_pictures(&doc),
                            fonts: doc
                                .font_faces_used()
                                .into_iter()
                                .map(|(family, bold, italic)| tracedraw_cdr::write::FontFace {
                                    data: fonts.face_file(&family, bold, italic),
                                    family,
                                    bold,
                                    italic,
                                })
                                .collect(),
                            embed_fonts: std::env::var("TRACEDRAW_CDR_EMBED_FONTS")
                                .is_ok_and(|v| v.trim() == "1"),
                            // TRACEDRAW_CDR_PARAGRAPH_FRAMES=1 writes paragraph
                            // text as paragraph frames, off by default (#1698).
                            paragraph_frames: std::env::var("TRACEDRAW_CDR_PARAGRAPH_FRAMES")
                                .is_ok_and(|v| v.trim() == "1"),
                            // Drop shadows go out as pictures beneath
                            // their objects (#1031).
                            shadows: tracedraw_io::shadow_pictures(
                                &doc,
                                tracedraw_io::SHADOW_PICTURE_DPI,
                            )
                            .into_iter()
                            .map(|(shape, rect, data, width, height)| {
                                tracedraw_cdr::write::ShadowPicture {
                                    shape,
                                    rect,
                                    png: tracedraw_cdr::write::Png {
                                        data,
                                        width,
                                        height,
                                    },
                                }
                            })
                            .collect(),
                        }
                    }),
                )
                .map_err(tracedraw_io::Error::from)
            } else if output.to_ascii_lowercase().ends_with(".emf") {
                tracedraw_io::save_emf(&doc, page_index(), output)
            } else if output.to_ascii_lowercase().ends_with(".wmf") {
                // TRACEDRAW_WMF_SOLID_DASHES=1 writes dashed outlines as
                // solid lines, as the export dialog's box does (#3819).
                let opts = tracedraw_io::emf::WmfOptions {
                    solid_dashes: std::env::var("TRACEDRAW_WMF_SOLID_DASHES")
                        .is_ok_and(|v| !v.is_empty() && v != "0"),
                };
                std::fs::write(
                    output,
                    tracedraw_io::emf::page_to_wmf_with(&doc, page_index(), &opts),
                )
                .map_err(tracedraw_io::Error::from)
            } else if output.to_ascii_lowercase().ends_with(".plt") {
                tracedraw_io::save_plt(&doc, page_index(), output)
            } else if output.to_ascii_lowercase().ends_with(".psd") {
                let dpi = std::env::var("TRACEDRAW_DPI")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(96.0);
                tracedraw_io::save_psd(&doc, page_index(), dpi, output)
            } else if output.to_ascii_lowercase().ends_with(".dxf") {
                tracedraw_io::save_dxf(&doc, page_index(), output)
            } else if output.to_ascii_lowercase().ends_with(".html") {
                std::fs::write(output, tracedraw_io::html::document_to_html(&doc))
                    .map_err(tracedraw_io::Error::from)
            } else if output.to_ascii_lowercase().ends_with(".png") {
                // Rasterise the first page (or TRACEDRAW_PAGE) at 96 dpi
                // (or TRACEDRAW_DPI).
                let dpi = std::env::var("TRACEDRAW_DPI")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(96.0);
                match doc
                    .pages
                    .get(page_index())
                    .and_then(|p| tracedraw_render::render_page_export_image(&doc, p.id, dpi))
                {
                    Some(pm) => pm
                        .save_png(output)
                        .map_err(|e| tracedraw_io::Error::Io(std::io::Error::other(e))),
                    None => Err(tracedraw_io::Error::Io(std::io::Error::other(
                        "render failed",
                    ))),
                }
            } else {
                tracedraw_io::save_native(&doc, output)
            };
            match res {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{output}: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => usage(),
    }
}

/// One line per object (and its children), indented by `depth`: kind,
/// geometry, transform, fill, outline and text.
fn describe_shape(s: &tracedraw_core::Shape, depth: usize) -> String {
    use tracedraw_core::document::ShapeKind;
    let pad = "  ".repeat(depth);
    let kind = match &s.kind {
        ShapeKind::Rect {
            rect,
            radius,
            corners,
        } => format!(
            "rect {:.2}x{:.2} at ({:.2},{:.2}) radius {:.2}{}",
            rect.width(),
            rect.height(),
            rect.x0,
            rect.y0,
            radius,
            corners
                .as_ref()
                .map(|c| format!(" corners {c:?}"))
                .unwrap_or_default()
        ),
        ShapeKind::Ellipse { rect, arc } => format!(
            "ellipse {:.2}x{:.2} at ({:.2},{:.2}){}",
            rect.width(),
            rect.height(),
            rect.x0,
            rect.y0,
            arc.as_ref()
                .map(|a| format!(" arc {a:?}"))
                .unwrap_or_default()
        ),
        ShapeKind::Polygon { rect, points, .. } => {
            format!(
                "polygon {points} points {:.2}x{:.2}",
                rect.width(),
                rect.height()
            )
        }
        ShapeKind::Path { path, closed } => format!(
            "path {} elements{}",
            path.elements().len(),
            if *closed { " closed" } else { "" }
        ),
        ShapeKind::Text {
            spans,
            origin,
            frame,
            align,
            para,
            on_path,
        } => {
            let runs: Vec<String> = spans
                .iter()
                .map(|r| {
                    format!(
                        "{:?} [{} {:.1}pt{}{}]",
                        r.text,
                        r.font_family,
                        r.size_pt,
                        if r.bold { " bold" } else { "" },
                        if r.italic { " italic" } else { "" }
                    )
                })
                .collect();
            format!(
                "text at ({:.2},{:.2}) frame {:?} align {:?} leading {:.0}%{} {}",
                origin.x,
                origin.y,
                frame.map(|f| (f.width, f.height)),
                align,
                para.leading_pct,
                if on_path.is_some() { " on path" } else { "" },
                runs.join(" + ")
            )
        }
        ShapeKind::Group { children } => format!("group of {}", children.len()),
        ShapeKind::ClipFrame { contents, .. } => format!("clip frame of {}", contents.len()),
        ShapeKind::Table(_) => "table".to_string(),
        ShapeKind::SymbolInstance { index } => format!("symbol {index}"),
        ShapeKind::Bitmap {
            rect,
            width_px,
            height_px,
            ..
        } => format!(
            "bitmap {width_px}x{height_px} px in {:.2}x{:.2} mm",
            rect.width(),
            rect.height()
        ),
    };
    let b = s.bounds();
    let t = s.transform.as_coeffs();
    let fill = match &s.fill {
        tracedraw_core::Fill::None => "none".to_string(),
        tracedraw_core::Fill::Solid(c) => format!("{c:?}"),
        other => format!("{:?}", std::mem::discriminant(other)),
    };
    let stroke = s
        .stroke
        .as_ref()
        .map(|o| format!("{:.3} mm {:?}", o.width, o.color))
        .unwrap_or_else(|| "none".to_string());
    let mut out = format!(
        "{pad}{kind}\n{pad}  bounds ({:.2},{:.2})-({:.2},{:.2}) transform [{:.3} {:.3} {:.3} {:.3} {:.2} {:.2}] opacity {:.2}\n{pad}  fill {fill}; outline {stroke}\n",
        b.x0, b.y0, b.x1, b.y1, t[0], t[1], t[2], t[3], t[4], t[5], s.opacity
    );
    match &s.kind {
        ShapeKind::Group { children } => {
            for c in children {
                out.push_str(&describe_shape(c, depth + 1));
            }
        }
        ShapeKind::ClipFrame { frame, contents } => {
            out.push_str(&describe_shape(frame, depth + 1));
            for c in contents {
                out.push_str(&describe_shape(c, depth + 1));
            }
        }
        _ => {}
    }
    out
}

/// The page the single-page exports write: TRACEDRAW_PAGE, counted from
/// 1 (the first page when unset or unreadable).
fn page_index() -> usize {
    page_from(std::env::var("TRACEDRAW_PAGE").ok().as_deref())
}

fn page_from(v: Option<&str>) -> usize {
    v.and_then(|v| v.trim().parse::<usize>().ok())
        .map(|n| n.saturating_sub(1))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fonts installed in the app's Font Manager, file by file and
    /// by folder, are loaded from its settings, and a font the drawing
    /// names that is not installed is reported with its stand-in (#3818).
    #[test]
    fn the_apps_fonts_load_and_missing_ones_name_their_stand_in() {
        let dir = std::env::temp_dir().join(format!("td-cli-fonts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("folder")).expect("dir");
        let assets =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tracedraw/assets/fonts");
        std::fs::copy(
            assets.join("Selawik-Bold.ttf"),
            dir.join("folder/Selawik-Bold.ttf"),
        )
        .expect("copy");
        let settings = dir.join("settings.json");
        let v = serde_json::json!({
            "font_manager": {
                "installed": [assets.join("Selawik-Regular.ttf")],
                "folders": [dir.join("folder")],
            }
        });
        std::fs::write(&settings, v.to_string()).expect("settings");
        let fonts = tracedraw_text::FontSystem::empty();
        assert_eq!(load_app_fonts(&fonts, &dir.join("none.json")), 0);
        assert!(load_app_fonts(&fonts, &settings) >= 1);
        assert!(fonts.has_family("Selawik"));
        assert_eq!(
            fonts.installed_paths().len(),
            2,
            "the file and the folder's"
        );
        let svg = dir.join("t.svg");
        std::fs::write(
            &svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><text x="5" y="20" font-family="Selawik">A</text><text x="5" y="40" font-family="Nowhere Grotesk 3818">B</text></svg>"#,
        )
        .expect("svg");
        let (mut doc, _) = read_document(svg.to_str().expect("utf-8")).expect("read");
        // The reader may name a family it does not know otherwise: the
        // second text is set in one no computer has.
        let mut texts = 0;
        for layer in doc.all_layers_mut() {
            for shape in &mut layer.shapes {
                if let tracedraw_core::document::ShapeKind::Text { spans, .. } = &mut shape.kind {
                    texts += 1;
                    if texts == 2 {
                        spans
                            .iter_mut()
                            .for_each(|sp| sp.font_family = "Nowhere Grotesk 3818".into());
                    }
                }
            }
        }
        assert_eq!(texts, 2);
        let lines = stand_ins(&fonts, &doc);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("\"Nowhere Grotesk 3818\"") && lines[0].contains("\"Selawik\""),
            "{lines:?}"
        );
        // Set to keep the stand-ins (#3965): the drawing names them.
        let keep = dir.join("keep.json");
        std::fs::write(
            &keep,
            r#"{"warnings":{"accept_missing_fonts":true,"accept_missing_fonts_keep":true}}"#,
        )
        .expect("settings");
        assert!(keeps_stand_ins(&keep));
        assert!(!keeps_stand_ins(&settings));
        assert_eq!(replace_missing_fonts(&fonts, &mut doc), 1);
        assert!(!doc
            .font_families_used()
            .contains(&"Nowhere Grotesk 3818".to_string()));
        assert!(stand_ins(&fonts, &doc).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_page_to_export_counts_from_one() {
        assert_eq!(page_from(None), 0);
        assert_eq!(page_from(Some("1")), 0);
        assert_eq!(page_from(Some(" 3 ")), 2);
        assert_eq!(page_from(Some("0")), 0);
        assert_eq!(page_from(Some("two")), 0);
    }

    #[test]
    fn pdf_text_comes_from_the_environment_value() {
        assert!(pdf_text(Some("curves")).as_curves);
        assert!(!pdf_text(Some("named")).embed_fonts);
        let t = pdf_text(None);
        assert!(!t.as_curves && t.embed_fonts && t.subset);
    }

    #[test]
    fn pdf_protection_comes_from_the_environment_values() {
        assert!(pdf_security(None, None, Some("print".into())).is_none());
        // A password to open it only: everything allowed, one password.
        let s = pdf_security(Some("open".into()), None, Some("print".into())).expect("set");
        assert_eq!(
            (s.user_password.as_str(), s.owner_password.as_str()),
            ("open", "open")
        );
        assert!(s.permissions.print);
        // A permissions password with what to deny.
        let s =
            pdf_security(None, Some("boss".into()), Some("print, Copy,forms".into())).expect("set");
        assert!(s.user_password.is_empty());
        assert_eq!(s.owner_password, "boss");
        assert!(!s.permissions.print && !s.permissions.copy && !s.permissions.fill_forms);
        assert!(s.permissions.modify && s.permissions.accessibility);
    }

    #[test]
    fn reader_warnings_name_what_was_skipped_or_guessed() {
        let clean = tracedraw_cdr::ParseReport::default();
        assert!(reader_warnings(&clean).is_empty());
        let report = tracedraw_cdr::ParseReport {
            skipped_objects: 2,
            warnings: vec!["no readable mcfg chunk; assuming A4".into()],
            ..Default::default()
        };
        assert_eq!(
            reader_warnings(&report),
            [
                "warning: no readable mcfg chunk; assuming A4",
                "warning: 2 object(s) skipped"
            ]
        );
    }
}

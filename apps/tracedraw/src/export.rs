//! Export (vector and raster formats), print (through a PDF handed to
//! the system viewer), print merge, find and replace.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::ui::dialogs::{
    ExportColorMode, ExportState, FindReplaceState, PrintState, PropertyFind, EXPORT_FORMATS,
};
use std::path::Path;
use tracedraw_core::find::{self as objfind, ColorQuery, Hit, ObjectQuery, ObjectType};
use tracedraw_core::{
    document::ShapeKind,
    geometry::{Affine, Rect, Size},
    Command, Document, ShapeId, TextSpan,
};

/// Document limited to the selection, sized to what it covers, and that
/// box. Empty selection: the page, as it was.
///
/// One reading of what "the selection" means for an export, used by the
/// Export dialog and by the MCP server's `export` with `selection`, so the
/// two cannot come out different.
pub(crate) fn selection_document(app: &App) -> (Document, Rect) {
    let doc = app.engine.document();
    let bounds = selection_export_bounds(app).unwrap_or(app.page_rect());
    let mut tmp = Document::new(
        doc.title.clone(),
        Size::new(bounds.width().max(0.1), bounds.height().max(0.1)),
    );
    tmp.symbols = doc.symbols.clone();
    // The drawing's spot inks and colour profile go along: without
    // its ink, a spot colour exports as its process stand-in.
    tmp.inks = doc.inks.clone();
    tmp.cmyk_profile = doc.cmyk_profile.clone();
    tmp.color_styles = doc.color_styles.clone();
    let l = tmp.pages[0].layers[0].id;
    for s in app.selected_shapes() {
        let mut c = s.clone();
        let shift = Affine::translate((-bounds.x0, -bounds.y0));
        c.transform_by(shift);
        carry_blend_ends(&mut c, shift);
        if let Ok(layer) = tmp.layer_mut(l) {
            layer.shapes.push(c);
        }
    }
    (tmp, bounds)
}

/// The box a selection exported alone is sized to: what each selected
/// object draws, its outline included. The selection's own box leaves the
/// outline out (an outline straddles the path), so a selected rectangle
/// came out of an SVG, PDF or PNG export with half its outline cut off at
/// every edge (#2290).
fn selection_export_bounds(app: &App) -> Option<Rect> {
    app.selected_shapes()
        .iter()
        .map(|s| {
            let r = outline_reach_within(s, s.transform);
            app.own_visual_bounds(s).inflate(r, r)
        })
        .reduce(|a, b| a.union(b))
}

/// How far the outline of `s`, or of any object inside it (a group's),
/// reaches past its path on the page ([`crate::interaction::outline_reach`]).
fn outline_reach_within(s: &tracedraw_core::Shape, to_page: Affine) -> f64 {
    let own = crate::interaction::outline_reach(s, to_page);
    match &s.kind {
        ShapeKind::Group { children } => children
            .iter()
            .map(|c| outline_reach_within(c, to_page * c.transform))
            .fold(own, f64::max),
        _ => own,
    }
}

/// Document limited to the selection or the current page, for exports.
pub(crate) fn export_document(app: &App, st: &ExportState) -> (Document, Rect) {
    let doc = app.engine.document();
    if st.selection_only {
        selection_document(app)
    } else {
        // Facing pages (#1972): each spread as one page, or each page
        // with what its facing page holds across the spine (and in the
        // bleed beside it).
        if exports_spreads(app, st) {
            return (
                doc.spreads_document(&export_page_indexes(app, st)),
                app.page_rect(),
            );
        }
        // The pages chosen, in the order chosen (#1382).
        let mut tmp = doc.with_spread_overhangs(st.bleed_mm.max(doc.metadata.bleed));
        let mut all: Vec<_> = std::mem::take(&mut tmp.pages)
            .into_iter()
            .map(Some)
            .collect();
        tmp.pages = export_page_indexes(app, st)
            .into_iter()
            .filter_map(|i| all.get_mut(i).and_then(Option::take))
            .collect();
        (tmp, app.page_rect())
    }
}

/// `st` exports the spreads of facing pages as pages (#1972).
pub(crate) fn exports_spreads(app: &App, st: &ExportState) -> bool {
    st.spreads && !st.selection_only && app.engine.document().facing().is_some()
}

/// The indexes of the pages `st` exports from the drawing (#1382).
pub(crate) fn export_page_indexes(app: &App, st: &ExportState) -> Vec<usize> {
    let doc = app.engine.document();
    let current = doc.pages.iter().position(|p| p.id == app.page).unwrap_or(0);
    st.page_indexes(doc.pages.len(), current)
}

/// Where page `number` of a export to `path` that writes one file per
/// page goes: "name-3.png" for page 3 (#1382).
pub fn numbered_path(path: &Path, number: usize, ext: &str) -> std::path::PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map_or(ext.to_string(), |e| e.to_string_lossy().to_string());
    path.with_file_name(format!("{stem}-{number}.{ext}"))
}

pub fn export(app: &mut App, path: &Path, st: &ExportState) -> Result<String, String> {
    let (_, ext) = EXPORT_FORMATS[st.format];
    let (mut doc, _) = export_document(app, st);
    if ext == "ai" && st.classic_ai() {
        // The classic AI file's `%%Title`, as the PDF-compatible one's.
        doc.title = if st.title.trim().is_empty() {
            app.document_title().trim_end_matches('*').to_string()
        } else {
            st.title.trim().to_string()
        };
    }
    if st.one_file_holds_the_pages(ext) {
        let bytes = document_file(app, &doc, ext, st)?;
        crate::files::write(path, bytes).map_err(|e| e.to_string())?;
        return Ok(trf(
            "status.exported",
            &[("path", &path.display().to_string())],
        ));
    }
    // The other formats hold one page: one file per page, numbered by
    // the page it shows when there are several (#1382).
    let numbers: Vec<usize> = if exports_spreads(app, st) {
        (1..=doc.pages.len()).collect()
    } else {
        export_page_indexes(app, st)
            .into_iter()
            .map(|i| i + 1)
            .collect()
    };
    let many = doc.pages.len() > 1;
    let mut written = Vec::with_capacity(doc.pages.len());
    // A CMYK bitmap carries the profile its inks are in (#1763).
    let icc = (st.color_mode_for(ext) == ExportColorMode::Cmyk)
        .then(|| export_cmyk_profile(app))
        .flatten();
    for k in 0..doc.pages.len() {
        let bytes = page_file(&doc, k, ext, st, icc.as_deref())?;
        let target = if many {
            numbered_path(path, numbers.get(k).copied().unwrap_or(k + 1), ext)
        } else {
            path.to_path_buf()
        };
        crate::files::write(&target, bytes).map_err(|e| e.to_string())?;
        written.push(target);
    }
    Ok(match (written.first(), written.last()) {
        (Some(first), Some(last)) if many => trf(
            "status.exported_pages",
            &[
                ("count", &written.len().to_string()),
                ("first", &first.display().to_string()),
                ("last", &last.display().to_string()),
            ],
        ),
        _ => trf("status.exported", &[("path", &path.display().to_string())]),
    })
}

/// The file of a format that holds every page (PDF, AI, PostScript, PRN,
/// HTML) for the pages of `doc`.
fn document_file(
    app: &App,
    doc: &Document,
    ext: &str,
    st: &ExportState,
) -> Result<Vec<u8>, String> {
    let output = tracedraw_io::for_output(doc);
    let doc = output.as_ref();
    Ok(match ext {
        "pdf" | "ai" => {
            use tracedraw_io::pdf::{PdfOptions, PdfStandard};
            let standard = match (ext, st.pdf_standard) {
                ("pdf", 1) => PdfStandard::X1a,
                ("pdf", 2) => PdfStandard::X3,
                ("pdf", 3) => PdfStandard::X4,
                _ => PdfStandard::None,
            };
            let opts = PdfOptions {
                standard,
                output_profile: pdfx_profile(app, standard),
                output_condition: st.output_condition.clone(),
                bleed_mm: if ext == "pdf" { st.bleed_mm } else { 0.0 },
                title: Some(if st.title.trim().is_empty() {
                    app.document_title().trim_end_matches('*').to_string()
                } else {
                    st.title.trim().to_string()
                }),
                keywords: (!st.tags.trim().is_empty()).then(|| st.tags.trim().to_string()),
                flatten_dpi: st.dpi.max(72.0),
                text: tracedraw_io::pdf::PdfText {
                    as_curves: ext == "pdf" && st.text_as_curves,
                    ..Default::default()
                },
                ..Default::default()
            };
            tracedraw_io::pdf::document_to_pdf_with(doc, &opts)
        }
        // Every exported page; a print file is the same PostScript.
        "ps" | "prn" => tracedraw_io::eps::document_to_ps(doc).into_bytes(),
        "html" => tracedraw_io::html::document_to_html(doc).into_bytes(),
        other => return Err(format!("{other}: not a format of several pages")),
    })
}

/// The file of page `k` of `doc` in a format that holds one page; `icc`
/// is the profile a CMYK bitmap carries.
fn page_file(
    doc: &Document,
    k: usize,
    ext: &str,
    st: &ExportState,
    icc: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let output = tracedraw_io::for_output(doc);
    let doc = output.as_ref();
    Ok(match ext {
        "svg" => tracedraw_io::svg::page_to_svg_with(doc, k, &st.svg_options()).into_bytes(),
        "eps" => tracedraw_io::eps::page_to_eps(doc, k).into_bytes(),
        // The classic AI file: one artboard, so one page per file (#1728).
        "ai" if st.classic_ai() => tracedraw_io::ai::page_to_ai8(doc, k).into_bytes(),
        "dxf" => tracedraw_io::dxf::page_to_dxf(doc, k).into_bytes(),
        "emf" => tracedraw_io::emf::page_to_emf(doc, k),
        "wmf" => tracedraw_io::emf::page_to_wmf(doc, k),
        "plt" => tracedraw_io::plt::page_to_plt_with(doc, k, &st.plt_options()).into_bytes(),
        "psd" => tracedraw_io::psd::page_to_psd(doc, k, st.render_dpi()).ok_or("render failed")?,
        _ => {
            let pid = doc.pages.get(k).ok_or("no page")?.id;
            match st.color_mode_for(ext) {
                // The inks of the page, as its separations print them
                // (#1763).
                ExportColorMode::Cmyk => {
                    let (w, h, ink) =
                        cmyk_pixels(doc, pid, st.render_dpi()).ok_or("render failed")?;
                    let bytes = if ext == "jpg" {
                        tracedraw_io::cmyk_bitmap::cmyk_jpeg(w, h, &ink, st.quality, st.dpi, icc)
                    } else {
                        tracedraw_io::cmyk_bitmap::cmyk_tiff(w, h, &ink, st.dpi, icc)
                    };
                    return bytes.ok_or_else(|| "CMYK encoding failed".to_string());
                }
                ExportColorMode::Gray => {
                    let pm = tracedraw_render::render_page_export_image(doc, pid, st.render_dpi())
                        .ok_or("render failed")?;
                    let bytes =
                        encode_gray(&on_white(&pm).ok_or("render failed")?, ext, st.quality)?;
                    return Ok(record_dpi(bytes, ext, st.dpi));
                }
                ExportColorMode::Rgb => {}
            }
            let p = doc.page(pid).map_err(|e| e.to_string())?;
            let fit = tracedraw_render::ViewTransform::whole_page(
                p.size.width,
                p.size.height,
                st.render_dpi() / 25.4,
            );
            let pixels = u64::from(fit.0) * u64::from(fit.1);
            // A large picture is drawn and written a strip at a time, so
            // it is never whole in memory (#3039).
            if matches!(ext, "png" | "tif") && pixels > STREAMED_PIXELS {
                let transparent = st.transparent && ext == "png";
                return streamed_picture(doc, pid, fit, ext, transparent, st.dpi, STRIP_PIXELS);
            }
            if pixels > tracedraw_render::MAX_PIXELS {
                return Err(too_large(fit.0, fit.1));
            }
            let mut pm = tracedraw_render::render_page_export_image(doc, pid, st.render_dpi())
                .ok_or("render failed")?;
            if st.transparent && matches!(ext, "png" | "webp" | "gif" | "jxl") {
                // Re-render on a transparent background.
                let p = doc.page(pid).map_err(|e| e.to_string())?;
                let (w, h, view) = tracedraw_render::ViewTransform::whole_page(
                    p.size.width,
                    p.size.height,
                    st.render_dpi() / 25.4,
                );
                pm = tracedraw_render::render_page(
                    doc,
                    pid,
                    &tracedraw_render::RenderOptions {
                        width: w,
                        height: h,
                        view,
                        preview: None,
                        wireframe: false,
                        ..tracedraw_render::RenderOptions::default()
                    },
                )
                .ok_or("render failed")?;
            }
            let bytes = encode_raster_with(&pm, ext, st.quality, st.webp_lossless)?;
            // The file says its resolution, so it opens at the size
            // chosen in the dialog (#617).
            record_dpi(bytes, ext, st.dpi)
        }
    })
}

/// Pages of more pixels than this are exported as PNG or TIFF a strip at
/// a time (#3039).
const STREAMED_PIXELS: u64 = 1 << 26;

/// The most pixels of one strip of a picture written a strip at a time
/// (64 MB of RGBA).
const STRIP_PIXELS: u64 = 1 << 24;

/// Why a picture of `w` x `h` pixels was not made.
fn too_large(w: u32, h: u32) -> String {
    format!(
        "The picture ({w} x {h} pixels) is too large for this format: \
         export it as PNG or TIFF, or at a lower resolution"
    )
}

/// The page `pid` as a PNG or TIFF (`ext`) file of `w` x `h` pixels drawn
/// through `view`, saying it was made at `dpi`: drawn and encoded strips
/// of at most `strip_pixels` pixels at a time, the same pixels as the page
/// drawn whole, on white or (`transparent`, PNG only) on nothing. The
/// page drawn whole took four times the picture's size in memory and
/// failed above a billion pixels, so a banner of 180 ft exported at 50 or
/// 72 dpi was not made (#3039). A TIFF of more than 4 GB is a BigTIFF.
fn streamed_picture(
    doc: &Document,
    pid: tracedraw_core::PageId,
    (w, h, view): (u32, u32, tracedraw_render::ViewTransform),
    ext: &str,
    transparent: bool,
    dpi: f64,
    strip_pixels: u64,
) -> Result<Vec<u8>, String> {
    let opts = tracedraw_render::RenderOptions {
        width: w,
        height: h,
        view,
        ..Default::default()
    };
    // Whole multiples of 16 rows, as the coarse grid of procedural fills.
    let rows =
        ((strip_pixels / u64::from(w)) / 16 * 16).clamp(16, u64::from(h).div_ceil(16) * 16) as u32;
    let channels: usize = if transparent { 4 } else { 3 };
    // Each strip's pixels, demultiplied, `channels` bytes each.
    let strip = |y0: u32| -> Result<Vec<u8>, String> {
        let band = rows.min(h - y0);
        let objects = tracedraw_render::render_page_rows(doc, pid, &opts, y0, y0 + band)
            .ok_or_else(|| too_large(w, h))?;
        let pm = if transparent {
            objects
        } else {
            let mut pm = tiny_skia::Pixmap::new(w, band).ok_or_else(|| too_large(w, h))?;
            pm.fill(tiny_skia::Color::WHITE);
            pm.draw_pixmap(
                0,
                0,
                objects.as_ref(),
                &Default::default(),
                tiny_skia::Transform::identity(),
                None,
            );
            pm
        };
        let mut out = Vec::with_capacity(pm.pixels().len() * channels);
        for p in pm.pixels() {
            let d = p.demultiply();
            out.extend_from_slice(&[d.red(), d.green(), d.blue(), d.alpha()][..channels]);
        }
        Ok(out)
    };
    let dpi = if dpi.is_finite() && dpi >= 1.0 {
        dpi
    } else {
        96.0
    };
    let mut file = std::io::Cursor::new(Vec::new());
    if ext == "png" {
        let mut enc = png::Encoder::new(&mut file, w, h);
        enc.set_color(if transparent {
            png::ColorType::Rgba
        } else {
            png::ColorType::Rgb
        });
        enc.set_depth(png::BitDepth::Eight);
        let per_metre = (dpi / 0.0254).round().min(f64::from(i32::MAX)) as u32;
        enc.set_pixel_dims(Some(png::PixelDimensions {
            xppu: per_metre,
            yppu: per_metre,
            unit: png::Unit::Meter,
        }));
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        let mut stream = writer.stream_writer().map_err(|e| e.to_string())?;
        let mut y0 = 0;
        while y0 < h {
            std::io::Write::write_all(&mut stream, &strip(y0)?).map_err(|e| e.to_string())?;
            y0 += rows;
        }
        stream.finish().map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
    } else {
        use tiff::encoder::{colortype, Rational, TiffEncoder, TiffKind};
        fn write<K: TiffKind>(
            mut enc: TiffEncoder<&mut std::io::Cursor<Vec<u8>>, K>,
            (w, h, rows, dpi): (u32, u32, u32, f64),
            strip: &dyn Fn(u32) -> Result<Vec<u8>, String>,
        ) -> Result<(), String> {
            let mut image = enc
                .new_image::<colortype::RGB8>(w, h)
                .map_err(|e| e.to_string())?;
            image.resolution(
                tiff::tags::ResolutionUnit::Inch,
                Rational {
                    n: (dpi * 100.0).round() as u32,
                    d: 100,
                },
            );
            image.rows_per_strip(rows).map_err(|e| e.to_string())?;
            let mut y0 = 0;
            while y0 < h {
                image.write_strip(&strip(y0)?).map_err(|e| e.to_string())?;
                y0 += rows;
            }
            image.finish().map_err(|e| e.to_string())
        }
        let dims = (w, h, rows, dpi);
        // Classic TIFF counts its bytes in 32 bits.
        if u64::from(w) * u64::from(h) * 3 > u64::from(u32::MAX) - (1 << 20) {
            write(
                TiffEncoder::new_big(&mut file).map_err(|e| e.to_string())?,
                dims,
                &strip,
            )?;
        } else {
            write(
                TiffEncoder::new(&mut file).map_err(|e| e.to_string())?,
                dims,
                &strip,
            )?;
        }
    }
    Ok(file.into_inner())
}

/// The output intent's profile for a PDF/X file: the CMYK profile picked
/// under Colour Management, when there is one and it can be read.
pub(crate) fn pdfx_profile(app: &App, standard: tracedraw_io::pdf::PdfStandard) -> Option<Vec<u8>> {
    if standard == tracedraw_io::pdf::PdfStandard::None {
        return None;
    }
    let p = app.settings.color.cmyk_profile_path.trim().to_string();
    (!p.is_empty())
        .then(|| crate::files::read(&p).ok())
        .flatten()
}

/// The CMYK profile a CMYK bitmap export carries (#1763): the one the
/// colour management converts its colours with, the drawing's own while
/// it is used, else the one picked under Colour Management; none with
/// the built-in conversions.
pub(crate) fn export_cmyk_profile(app: &App) -> Option<Vec<u8>> {
    let c = &app.settings.color;
    if c.builtin_engine {
        return None;
    }
    if c.open_cmyk == 0 && crate::app::carried_cmyk_profile(app.doc()).is_some() {
        return app.doc().cmyk_profile.as_ref().map(|p| p.data.to_vec());
    }
    app.settings_cmyk_profile().map(|p| p.data.to_vec())
}

/// `pm` drawn over white, as a page is printed.
fn on_white(pm: &tiny_skia::Pixmap) -> Option<tiny_skia::Pixmap> {
    let mut white = tiny_skia::Pixmap::new(pm.width(), pm.height())?;
    white.fill(tiny_skia::Color::WHITE);
    white.draw_pixmap(
        0,
        0,
        pm.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    Some(white)
}

/// Page `pid` of `doc` at `dpi` as ink coverages (#1763): cyan, magenta,
/// yellow and black of each pixel, 255 = full ink, with its size in
/// pixels. Each process ink is the page's separation plate for it
/// ([`plate_page`]) drawn on white: CMYK colours keep their exact
/// values, other colours go through the drawing's colour management as
/// on the plates, pictures give the inks they keep (#1659) or their
/// pixels converted, and spot colours print as their process mix.
pub(crate) fn cmyk_pixels(
    doc: &Document,
    pid: tracedraw_core::PageId,
    dpi: f64,
) -> Option<(u32, u32, Vec<u8>)> {
    let mut page = doc.page(pid).ok()?.clone();
    for layer in page.layers.iter_mut() {
        for s in layer.shapes.iter_mut() {
            map_colors(s, &spot_as_cmyk);
        }
    }
    let mut one = doc.clone();
    one.pages.retain(|p| p.id == pid);
    let mut out: Option<(u32, u32, Vec<u8>)> = None;
    for ink in 0..4 {
        *one.pages.first_mut()? = plate_page(&page, Plate::Process(ink));
        let pm = on_white(&tracedraw_render::render_page_export_image(&one, pid, dpi)?)?;
        let (w, h) = (pm.width(), pm.height());
        let (ow, oh, buf) =
            out.get_or_insert_with(|| (w, h, vec![0u8; w as usize * h as usize * 4]));
        if (*ow, *oh) != (w, h) {
            return None;
        }
        for (px, p) in buf.chunks_exact_mut(4).zip(pm.pixels()) {
            // The plate is grey: black is full ink.
            px[ink] = 255 - p.red();
        }
    }
    out
}

/// The grey of each pixel of an opaque `pm`, through the colour
/// management's Grayscale conversion, each distinct colour once.
fn gray_pixels(pm: &tiny_skia::Pixmap) -> Vec<u8> {
    let mut known: std::collections::HashMap<[u8; 3], u8> = std::collections::HashMap::new();
    pm.pixels()
        .iter()
        .map(|p| {
            let rgb = [p.red(), p.green(), p.blue()];
            *known.entry(rgb).or_insert_with(|| {
                match tracedraw_core::Color::rgb8(rgb[0], rgb[1], rgb[2]).convert_to("Grayscale") {
                    tracedraw_core::Color::Gray { v } => (v.clamp(0.0, 1.0) * 255.0).round() as u8,
                    _ => 0,
                }
            })
        })
        .collect()
}

/// An opaque rendered page as a one-channel JPEG (at `quality`) or TIFF.
fn encode_gray(pm: &tiny_skia::Pixmap, ext: &str, quality: u8) -> Result<Vec<u8>, String> {
    let img = image::GrayImage::from_raw(pm.width(), pm.height(), gray_pixels(pm))
        .ok_or("image buffer")?;
    let mut out = std::io::Cursor::new(Vec::new());
    if ext == "jpg" {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
            .encode_image(&img)
            .map_err(|e| e.to_string())?;
    } else {
        img.write_to(&mut out, image::ImageFormat::Tiff)
            .map_err(|e| e.to_string())?;
    }
    Ok(out.into_inner())
}

/// The ink coverages of an opaque `pm` through the colour management's
/// CMYK conversion, each distinct colour once: for the estimate of a
/// CMYK file's size from the preview.
fn ink_pixels(pm: &tiny_skia::Pixmap) -> Vec<u8> {
    let mut known: std::collections::HashMap<[u8; 3], [u8; 4]> = std::collections::HashMap::new();
    pm.pixels()
        .iter()
        .flat_map(|p| {
            let rgb = [p.red(), p.green(), p.blue()];
            *known.entry(rgb).or_insert_with(|| {
                match tracedraw_core::Color::rgb8(rgb[0], rgb[1], rgb[2]).convert_to("CMYK") {
                    tracedraw_core::Color::Cmyk { c, m, y, k } => {
                        [c, m, y, k].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                    }
                    _ => [0; 4],
                }
            })
        })
        .collect()
}

/// `bytes`, a PNG, JPEG or BMP file, saying it was made at `dpi` dots per
/// inch: a PNG gets a `pHYs` chunk after its header, a JPEG's JFIF header
/// and a BMP's header get the density (#617). Other formats, and files not
/// laid out as the encoder writes them, come back unchanged.
pub fn record_dpi(mut bytes: Vec<u8>, ext: &str, dpi: f64) -> Vec<u8> {
    if !(dpi.is_finite() && dpi >= 1.0) {
        return bytes;
    }
    let per_metre = (dpi / 0.0254).round().min(f64::from(i32::MAX)) as u32;
    match ext {
        "png" => {
            // Signature (8) and the IHDR chunk (4 + 4 + 13 + 4).
            if bytes.len() < 33
                || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                || bytes.get(12..16) != Some(b"IHDR")
            {
                return bytes;
            }
            let mut chunk = Vec::with_capacity(21);
            chunk.extend_from_slice(&9u32.to_be_bytes());
            chunk.extend_from_slice(b"pHYs");
            chunk.extend_from_slice(&per_metre.to_be_bytes());
            chunk.extend_from_slice(&per_metre.to_be_bytes());
            chunk.push(1);
            let crc = crc32(&chunk[4..]);
            chunk.extend_from_slice(&crc.to_be_bytes());
            bytes.splice(33..33, chunk);
            bytes
        }
        "jpg" => {
            // SOI, then the JFIF APP0 segment: units at 13, densities at
            // 14 and 16.
            if bytes.len() < 18
                || bytes.get(2..4) != Some(&[0xFF, 0xE0])
                || bytes.get(6..11) != Some(b"JFIF\0")
            {
                return bytes;
            }
            let d = (dpi.round().min(f64::from(u16::MAX)) as u16).to_be_bytes();
            bytes[13] = 1;
            bytes[14..16].copy_from_slice(&d);
            bytes[16..18].copy_from_slice(&d);
            bytes
        }
        "bmp" => {
            // File header (14), then the info header with the pixels per
            // metre at 24 and 28.
            if bytes.len() < 46 || !bytes.starts_with(b"BM") {
                return bytes;
            }
            let m = per_metre.to_le_bytes();
            bytes[38..42].copy_from_slice(&m);
            bytes[42..46].copy_from_slice(&m);
            bytes
        }
        _ => bytes,
    }
}

/// The CRC-32 a PNG chunk ends with.
fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// True when the export format `ext` keeps transparency.
pub fn keeps_alpha(ext: &str) -> bool {
    matches!(ext, "png" | "webp" | "gif" | "jxl")
}

/// What the export will look like: the page (or the selection) of `st`
/// rendered to fit `max_px` on its longer side, on a transparent
/// background when the format keeps one, else on white.
pub fn preview(app: &App, st: &ExportState, max_px: u32) -> Option<tiny_skia::Pixmap> {
    let (_, ext) = EXPORT_FORMATS[st.format.min(EXPORT_FORMATS.len() - 1)];
    let (doc, _) = export_document(app, st);
    let page = doc.pages.first()?;
    let longest = page.size.width.max(page.size.height).max(0.1);
    let zoom = max_px.max(16) as f64 / longest;
    let (w, h, view) =
        tracedraw_render::ViewTransform::whole_page(page.size.width, page.size.height, zoom);
    let mut pm = tracedraw_render::render_page(
        &doc,
        page.id,
        &tracedraw_render::RenderOptions {
            width: w,
            height: h,
            view,
            preview: None,
            wireframe: false,
            ..tracedraw_render::RenderOptions::default()
        },
    )?;
    if !(st.transparent && keeps_alpha(ext)) {
        let mut white = tiny_skia::Pixmap::new(w, h)?;
        white.fill(tiny_skia::Color::WHITE);
        white.draw_pixmap(
            0,
            0,
            pm.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
        pm = white;
    }
    // A CMYK file shows as it will print, a grey one in grey (#1763).
    match st.color_mode_for(ext) {
        ExportColorMode::Rgb => {}
        ExportColorMode::Cmyk => {
            let mut known: std::collections::HashMap<[u8; 3], [u8; 3]> =
                std::collections::HashMap::new();
            for p in pm.pixels_mut() {
                let rgb = [p.red(), p.green(), p.blue()];
                let [r, g, b] = *known.entry(rgb).or_insert_with(|| {
                    tracedraw_core::color::proof_rgb(rgb.map(|v| f32::from(v) / 255.0))
                        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                });
                *p = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, 255)?;
            }
        }
        ExportColorMode::Gray => {
            let grey = gray_pixels(&pm);
            for (p, v) in pm.pixels_mut().iter_mut().zip(grey) {
                *p = tiny_skia::PremultipliedColorU8::from_rgba(v, v, v, 255)?;
            }
        }
    }
    Some(pm)
}

/// About how many bytes the export of `st` takes: a vector format is
/// written in memory; a bitmap format is estimated from `preview` (the
/// output of [`preview`]) encoded the same way and scaled to the export's
/// pixel count.
pub fn estimated_size(app: &App, st: &ExportState, preview: &tiny_skia::Pixmap) -> Option<usize> {
    let (_, ext) = EXPORT_FORMATS[st.format.min(EXPORT_FORMATS.len() - 1)];
    let (doc, bounds) = export_document(app, st);
    // One file per page: the first page's size times the pages (#1382).
    let pages = doc.pages.len().max(1);
    match ext {
        "svg" => Some(tracedraw_io::svg::page_to_svg(&doc, 0).len() * pages),
        "eps" => Some(tracedraw_io::eps::page_to_eps(&doc, 0).len() * pages),
        "ps" | "prn" => Some(tracedraw_io::eps::document_to_ps(&doc).len()),
        "ai" if st.classic_ai() => Some(tracedraw_io::ai::page_to_ai8(&doc, 0).len() * pages),
        "pdf" | "ai" => Some(tracedraw_io::pdf::document_to_pdf(&doc).len()),
        "png" | "jpg" | "bmp" | "tif" | "webp" | "gif" | "psd" | "jxl" => {
            let enc = if ext == "psd" { "tif" } else { ext };
            let (w, h) = (preview.width(), preview.height());
            let bytes = match st.color_mode_for(ext) {
                ExportColorMode::Rgb => {
                    encode_raster_with(preview, enc, st.quality, st.webp_lossless).ok()?
                }
                ExportColorMode::Gray => encode_gray(preview, ext, st.quality).ok()?,
                ExportColorMode::Cmyk if ext == "jpg" => tracedraw_io::cmyk_bitmap::cmyk_jpeg(
                    w,
                    h,
                    &ink_pixels(preview),
                    st.quality,
                    st.dpi,
                    None,
                )?,
                ExportColorMode::Cmyk => {
                    tracedraw_io::cmyk_bitmap::cmyk_tiff(w, h, &ink_pixels(preview), st.dpi, None)?
                }
            }
            .len() as f64;
            let dpi = st.render_dpi();
            let full = (bounds.width() / 25.4 * dpi) * (bounds.height() / 25.4 * dpi);
            let shown = (preview.width() as f64 * preview.height() as f64).max(1.0);
            Some((bytes * full / shown * pages as f64) as usize)
        }
        _ => None,
    }
}

/// A byte count for people: "820 KB", "1.4 MB".
pub fn human_size(bytes: usize) -> String {
    let b = bytes as f64;
    if b < 1000.0 * 1024.0 {
        format!("{:.0} KB", (b / 1024.0).max(1.0))
    } else {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    }
}

/// Round each colour channel (not alpha) to fewer levels for WebP's
/// near-lossless quality: 100 keeps every value, lower qualities drop up
/// to 4 low bits.
fn near_lossless(img: &mut image::RgbaImage, quality: u8) {
    let bits: u32 = match quality {
        100.. => 0,
        90..=99 => 1,
        75..=89 => 2,
        50..=74 => 3,
        _ => 4,
    };
    if bits == 0 {
        return;
    }
    let half = 1u16 << (bits - 1);
    for p in img.pixels_mut() {
        for v in &mut p.0[..3] {
            let q = ((*v as u16 + half) >> bits) << bits;
            *v = q.min(255) as u8;
        }
    }
}

/// Encode a rendered page in the bitmap format `ext` (WebP lossless).
#[cfg(test)]
fn encode_raster(pm: &tiny_skia::Pixmap, ext: &str, quality: u8) -> Result<Vec<u8>, String> {
    encode_raster_with(pm, ext, quality, true)
}

/// A WebP file with lossy compression at `quality` (1 to 100), alpha
/// kept; `None` where no lossy encoder is built in (the browser).
#[cfg(not(target_arch = "wasm32"))]
fn webp_lossy(img: &image::RgbaImage, quality: u8) -> Option<Vec<u8>> {
    let enc = webp::Encoder::from_rgba(img.as_raw(), img.width(), img.height());
    Some(enc.encode(f32::from(quality.clamp(1, 100))).to_vec())
}

/// The browser's lossy WebP (#592): libwebp is C and not built for
/// wasm32, so a pure-Rust VP8 encoder writes the file, at `quality` as
/// libwebp takes it, alpha kept. `None` (lossless with rounding, as
/// before) for a size WebP cannot hold or when the encoder fails.
#[cfg(target_arch = "wasm32")]
fn webp_lossy(img: &image::RgbaImage, quality: u8) -> Option<Vec<u8>> {
    webp_lossy_pure(img, quality)
}

/// See the browser's `webp_lossy`; the tests run it natively.
#[cfg(any(target_arch = "wasm32", test))]
fn webp_lossy_pure(img: &image::RgbaImage, quality: u8) -> Option<Vec<u8>> {
    use vaam_webp::{ColorType, EncoderParams, WebPEncoder};
    let (w, h) = img.dimensions();
    // WebP's largest side.
    if w == 0 || h == 0 || w > 16383 || h > 16383 {
        return None;
    }
    let opaque = img.pixels().all(|p| p.0[3] == 255);
    let mut out = Vec::new();
    let mut enc = WebPEncoder::new(&mut out);
    let mut params = EncoderParams::default();
    params.use_lossy = true;
    params.lossy_quality = quality.clamp(1, 100);
    enc.set_params(params);
    let done = if opaque {
        // No alpha chunk for a picture with no transparency.
        let rgb: Vec<u8> = img
            .pixels()
            .flat_map(|p| [p.0[0], p.0[1], p.0[2]])
            .collect();
        enc.encode(&rgb, w, h, ColorType::Rgb8)
    } else {
        enc.encode(img.as_raw(), w, h, ColorType::Rgba8)
    };
    match done {
        Ok(()) => Some(out),
        Err(e) => {
            log::warn!("lossy WebP: {e}; writing it lossless");
            None
        }
    }
}

/// A JPEG XL file of `img`, lossless (#3063): RGB when it is opaque,
/// RGBA otherwise. The encoder takes no picture of one pixel across or
/// down, so such a picture is widened to two by repeating it.
pub fn encode_jxl(img: &image::RgbaImage) -> Result<Vec<u8>, String> {
    use zune_core::{bit_depth::BitDepth, colorspace::ColorSpace, options::EncoderOptions};
    let img = if img.width() < 2 || img.height() < 2 {
        image::imageops::resize(
            img,
            img.width().max(2),
            img.height().max(2),
            image::imageops::FilterType::Nearest,
        )
    } else {
        img.clone()
    };
    let (w, h) = (img.width() as usize, img.height() as usize);
    let opaque = img.pixels().all(|p| p.0[3] == 255);
    let (data, space) = if opaque {
        let rgb: Vec<u8> = img
            .pixels()
            .flat_map(|p| [p.0[0], p.0[1], p.0[2]])
            .collect();
        (rgb, ColorSpace::RGB)
    } else {
        (img.into_raw(), ColorSpace::RGBA)
    };
    let options = EncoderOptions::new(w, h, space, BitDepth::Eight);
    let mut out = Vec::new();
    zune_jpegxl::JxlSimpleEncoder::new(&data, options)
        .encode(&mut out)
        .map_err(|e| format!("JPEG XL: {e:?}"))?;
    Ok(out)
}

/// Encode a rendered page in the bitmap format `ext`; WebP lossless
/// (`lossless`, colours rounded below quality 100) or lossy at the
/// quality (#592).
fn encode_raster_with(
    pm: &tiny_skia::Pixmap,
    ext: &str,
    quality: u8,
    lossless: bool,
) -> Result<Vec<u8>, String> {
    let (w, h) = (pm.width(), pm.height());
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for p in pm.pixels() {
        let d = p.demultiply();
        rgba.extend_from_slice(&[d.red(), d.green(), d.blue(), d.alpha()]);
    }
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("image buffer")?;
    if ext == "jxl" {
        return encode_jxl(&img);
    }
    let dynimg = image::DynamicImage::ImageRgba8(img);
    let format = match ext {
        "png" => image::ImageFormat::Png,
        "jpg" => image::ImageFormat::Jpeg,
        "bmp" => image::ImageFormat::Bmp,
        "tif" => image::ImageFormat::Tiff,
        "webp" => image::ImageFormat::WebP,
        "gif" => image::ImageFormat::Gif,
        _ => image::ImageFormat::Png,
    };
    let mut out = std::io::Cursor::new(Vec::new());
    match format {
        image::ImageFormat::Jpeg => {
            let rgb = dynimg.to_rgb8();
            let mut enc =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
            enc.encode_image(&rgb).map_err(|e| e.to_string())?;
        }
        image::ImageFormat::Bmp => {
            dynimg
                .to_rgb8()
                .write_to(&mut out, format)
                .map_err(|e| e.to_string())?;
        }
        image::ImageFormat::WebP => {
            let mut rgba = dynimg.to_rgba8();
            // Lossy: much smaller, the quality as the encoder takes it
            // (#592). Lossless (or no lossy encoder, in the browser): below
            // 100 the colours are first rounded to fewer levels, WebP's
            // near-lossless way, so quality still shrinks the file (#544).
            if let Some(bytes) = (!lossless).then(|| webp_lossy(&rgba, quality)).flatten() {
                return Ok(bytes);
            }
            near_lossless(&mut rgba, quality);
            rgba.write_to(&mut out, format).map_err(|e| e.to_string())?;
        }
        _ => dynimg
            .write_to(&mut out, format)
            .map_err(|e| e.to_string())?,
    }
    Ok(out.into_inner())
}

/// Keeps the sheet at `n` alone (the last one when `n` is past the end).
pub fn keep_one_sheet(doc: &mut Document, n: usize) {
    if doc.pages.is_empty() {
        return;
    }
    let n = n.min(doc.pages.len() - 1);
    let page = doc.pages.swap_remove(n);
    doc.pages = vec![page];
}

/// Print: build a PDF with the print options and send it to the chosen
/// system printer, or open it in the system viewer, which owns the
/// printer dialog.
pub fn print(app: &mut App, st: &PrintState) -> Result<String, String> {
    if st.manual_duplex {
        return print_duplex_side(app, st, false);
    }
    print_sheet(app, st, None)
}

/// The sheets of one side of a manual two-sided print of `n` sheets, in
/// printing order: the fronts are the odd sheets (1, 3, 5...), the backs
/// the even ones. Backs in reverse order run from the last; then an odd
/// count's last front, which has no back, gets a blank one first
/// (`None`), so each back meets its front in the turned stack.
pub fn duplex_sides(n: usize, back: bool, reverse: bool) -> Vec<Option<usize>> {
    if !back {
        return (0..n).step_by(2).map(Some).collect();
    }
    let mut v: Vec<Option<usize>> = (1..n).step_by(2).map(Some).collect();
    if reverse {
        if n % 2 == 1 {
            v.push(None);
        }
        v.reverse();
    }
    v
}

/// Keeps the sheets `picked` of `doc` in that order, a blank sheet of the
/// first one's size for each `None`.
pub fn keep_sheets(doc: &mut Document, picked: &[Option<usize>]) {
    let pages = std::mem::take(&mut doc.pages);
    let blank = |like: &tracedraw_core::document::Page| {
        let mut p = like.clone();
        p.background = None;
        for l in p.layers.iter_mut() {
            l.shapes.clear();
        }
        p
    };
    doc.pages = picked
        .iter()
        .filter_map(|i| match i {
            Some(i) => pages.get(*i).cloned(),
            None => pages.first().map(blank),
        })
        .collect();
}

/// Manual two-sided printing: the fronts (`back` false), after which the
/// prompt to turn the stack over waits (`App::duplex_backs`), or the
/// backs. One sheet alone prints as it is.
pub fn print_duplex_side(app: &mut App, st: &PrintState, back: bool) -> Result<String, String> {
    let n = print_sheets(app, st).pages.len();
    if n < 2 {
        return print_sheet(app, st, None);
    }
    let picked = duplex_sides(n, back, st.duplex_reverse);
    // The sides set their own order; Reverse order would undo it.
    let side = PrintState {
        reverse_order: false,
        ..st.clone()
    };
    let r = print_picked(app, &side, &picked);
    if !back && r.is_ok() {
        app.duplex_backs = Some(st.clone());
    }
    r
}

/// File > Print with `only` set to one sheet's index: that sheet alone
/// (with its plates, under separations) goes out, as Print Preview's
/// Print this sheet sends it; `None` prints every sheet.
pub fn print_sheet(app: &mut App, st: &PrintState, only: Option<usize>) -> Result<String, String> {
    let mut doc = print_sheets(app, st);
    if let Some(n) = only {
        keep_one_sheet(&mut doc, n);
    }
    print_doc(app, st, doc)
}

/// File > Print with only the sheets `picked` (in that order, `None` a
/// blank sheet): one side of a manual two-sided print.
fn print_picked(
    app: &mut App,
    st: &PrintState,
    picked: &[Option<usize>],
) -> Result<String, String> {
    let mut doc = print_sheets(app, st);
    keep_sheets(&mut doc, picked);
    print_doc(app, st, doc)
}

/// The sheets `doc` printed as `st` asks: separations, copies, and the
/// destination.
fn print_doc(app: &mut App, st: &PrintState, mut doc: Document) -> Result<String, String> {
    // Each plate in a file of its own, named after its ink, beside the
    // name chosen.
    if st.separations && st.plate_files && matches!(st.destination, 1 | 2) {
        return print_plate_files(app, st, &doc);
    }
    let plate_names: Vec<String> = plates(&doc).into_iter().map(|(_, n)| n).collect();
    if st.separations {
        doc = separations_doc_without(&doc, &st.plates_off, st.skip_empty_plates);
    }
    let stem = app.document_title().trim_end_matches('*').to_string();
    // Files get their copies inside; the printer (lp) or the system
    // viewer handles the count.
    if st.destination != 0 && st.copies > 1 {
        repeat_copies(&mut doc, st.copies.min(999) as usize, st.collate);
    }
    match st.destination {
        1 | 2 => {
            let ps = st.destination == 2;
            let (ext, filter) = if ps {
                ("ps", "PostScript")
            } else {
                ("pdf", "PDF")
            };
            let bytes = if ps {
                let mut out = tracedraw_io::eps::document_to_ps(&doc);
                if st.separations && st.screens {
                    out = with_screens(&out, &page_screens(&doc, &plate_names, st));
                }
                postscript_level(out, st.ps_level).into_bytes()
            } else {
                tracedraw_io::pdf::document_to_pdf(&doc)
            };
            crate::files::Dialog::new()
                .add_filter(filter, &[ext])
                .set_file_name(format!("{stem}.{ext}"))
                .save_file(app, move |app, path| {
                    app.status = match crate::files::write(&path, bytes) {
                        Ok(()) => trf("status.exported", &[("path", &path.display().to_string())]),
                        Err(e) => format!("{}: {e}", tr("dialog.print_failed")),
                    };
                });
            Ok(String::new())
        }
        _ => {
            let name = format!("{stem}.pdf");
            if let (Some(printer), false) = (&st.printer, crate::files::WEB) {
                // The sheets are drawn and spooled off the interface's
                // thread: a slow or stuck printer leaves the window usable
                // (#810).
                let (printer, copies, collate) = (printer.clone(), st.copies, st.collate);
                // Print as bitmap: the sheets are already pictures of
                // that resolution, sent as they are.
                let bitmap_dpi = st.as_bitmap.then(|| st.bitmap_dpi.clamp(36.0, 1200.0));
                let msg = trf("status.printing_to", &[("name", &printer)]);
                app.start_print_job(move || {
                    print_on(&printer, &name, &doc, copies, collate, &stem, bitmap_dpi)
                });
                return Ok(msg);
            }
            // Android's print service, its Save as PDF and the printers'
            // plugins draw the PDF on a transparent bitmap, and some drop
            // its alpha: what the sheet leaves unpainted came out black
            // (#2663). The sheets go out on white paper.
            if cfg!(target_os = "android") {
                on_white_paper(&mut doc);
            }
            let pdf = tracedraw_io::pdf::document_to_pdf(&doc);
            if crate::files::WEB {
                // In a browser the PDF is downloaded; its viewer prints it.
                let path = std::path::PathBuf::from("/download").join(&name);
                crate::files::write(&path, pdf).map_err(|e| e.to_string())?;
                return Ok(trf("status.print_sent", &[("path", &name)]));
            }
            print_through_viewer(&name, pdf)
        }
    }
}

/// Each sheet of `doc` without a background of its own on opaque white
/// paper: a reader that draws the PDF on a transparent bitmap and drops
/// its alpha shows white, not black, where nothing is drawn (#2663).
fn on_white_paper(doc: &mut Document) {
    for page in doc.pages.iter_mut().filter(|p| p.background.is_none()) {
        page.background = Some(tracedraw_core::Fill::Solid(tracedraw_core::Color::WHITE));
    }
}

/// Separations with each plate in a file of its own (PDF or PostScript):
/// the file name chosen gives each its name, the ink's name after it
/// ("poster - Cyan.pdf").
fn print_plate_files(app: &mut App, st: &PrintState, sheets: &Document) -> Result<String, String> {
    let ps = st.destination == 2;
    let (ext, filter) = if ps {
        ("ps", "PostScript")
    } else {
        ("pdf", "PDF")
    };
    let names: Vec<String> = plates(sheets).into_iter().map(|(_, n)| n).collect();
    let files: Vec<(String, Vec<u8>)> = plate_files(sheets, st)
        .into_iter()
        .map(|(name, mut doc)| {
            if st.copies > 1 {
                repeat_copies(&mut doc, st.copies.min(999) as usize, st.collate);
            }
            let bytes = if ps {
                let mut out = tracedraw_io::eps::document_to_ps(&doc);
                if st.screens {
                    out = with_screens(&out, &page_screens(&doc, &names, st));
                }
                postscript_level(out, st.ps_level).into_bytes()
            } else {
                tracedraw_io::pdf::document_to_pdf(&doc)
            };
            (name, bytes)
        })
        .collect();
    let stem = app.document_title().trim_end_matches('*').to_string();
    crate::files::Dialog::new()
        .add_filter(filter, &[ext])
        .set_file_name(format!("{stem}.{ext}"))
        .save_file(app, move |app, path| {
            let mut written = Vec::new();
            for (name, bytes) in files {
                let to = plate_file_path(&path, &name);
                if let Err(e) = crate::files::write(&to, bytes) {
                    app.status = format!("{}: {e}", tr("dialog.print_failed"));
                    return;
                }
                written.push(to.display().to_string());
            }
            app.status = trf("status.exported", &[("path", &written.join(", "))]);
        });
    Ok(String::new())
}

/// The plates of `sheets` as documents of their own, each named after
/// its ink, in [`plates`] order: the plates unticked in Print Preview
/// left out, and with Leave out empty plates each plate no sheet puts
/// ink on. A sheet with no ink on a plate kept prints blank on it, so
/// every file keeps the sheets in step.
pub fn plate_files(sheets: &Document, st: &PrintState) -> Vec<(String, Document)> {
    let all = plates(sheets);
    let mut out = Vec::new();
    for (i, (plate, name)) in all.iter().enumerate() {
        if st.plates_off.contains(&i) {
            continue;
        }
        if st.skip_empty_plates && !sheets.pages.iter().any(|p| plate_has_ink(p, *plate)) {
            continue;
        }
        let others: Vec<usize> = (0..all.len()).filter(|&j| j != i).collect();
        let mut doc = separations_doc_without(sheets, &others, false);
        doc.title = format!("{} {name}", sheets.title);
        out.push((name.clone(), doc));
    }
    out
}

/// The file of the plate `ink` beside `chosen`: its name with the ink's
/// after it ("poster.pdf" and Cyan give "poster - Cyan.pdf").
pub fn plate_file_path(chosen: &std::path::Path, ink: &str) -> std::path::PathBuf {
    let stem = chosen
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = chosen
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    chosen.with_file_name(shared_file_name(&format!("{stem} - {ink}{ext}")))
}

/// The usual screen angle of plate `i` of [`plates`]: cyan 15°, magenta
/// 75°, yellow 0°, black and the spot inks 45°, angles 30° apart for
/// the strong inks so their dots make no moiré.
pub fn default_screen_angle(i: usize) -> f64 {
    match i {
        0 => 15.0,
        1 => 75.0,
        2 => 0.0,
        _ => 45.0,
    }
}

/// The screen angle of plate `i`, named `name`: the one set by hand in
/// the Print dialog, or its usual one.
pub fn screen_angle(st: &PrintState, i: usize, name: &str) -> f64 {
    st.screen_angles
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, a)| *a)
        .unwrap_or_else(|| default_screen_angle(i))
}

/// The halftone screen (lines per inch, angle) of each page of the
/// separations `sep`, whose pages are named after their ink (see
/// `separations_doc_without`); `names` are the ink names of [`plates`].
/// A page of no ink known gets none.
pub fn page_screens(sep: &Document, names: &[String], st: &PrintState) -> Vec<Option<(f64, f64)>> {
    let lpi = st.screen_lpi.clamp(10.0, 600.0);
    sep.pages
        .iter()
        .map(|p| {
            names
                .iter()
                .enumerate()
                .filter(|(_, n)| p.name == **n || p.name.ends_with(&format!(" {n}")))
                .max_by_key(|(_, n)| n.len())
                .map(|(i, n)| (lpi, screen_angle(st, i, n)))
        })
        .collect()
}

/// PostScript `ps` with the halftone screen of each page set at its
/// start, a round dot at the frequency and angle given.
pub fn with_screens(ps: &str, screens: &[Option<(f64, f64)>]) -> String {
    const MARK: &str = "%%EndPageSetup\n";
    let mut out = String::with_capacity(ps.len() + screens.len() * 80);
    let mut rest = ps;
    let mut n = 0;
    while let Some(i) = rest.find(MARK) {
        out.push_str(&rest[..i + MARK.len()]);
        rest = &rest[i + MARK.len()..];
        if let Some(Some((lpi, angle))) = screens.get(n) {
            out.push_str(&format!(
                "{lpi} {angle} {{dup mul exch dup mul add 1 exch sub}} setscreen\n"
            ));
        }
        n += 1;
    }
    out.push_str(rest);
    out
}

/// The print PDF sent to the system printer `printer` (`lp`), with the
/// copies the dialog asked for.
#[cfg(not(target_os = "windows"))]
fn print_on(
    printer: &str,
    name: &str,
    doc: &Document,
    copies: u32,
    collate: bool,
    title: &str,
    _bitmap_dpi: Option<f64>,
) -> Result<String, String> {
    let pdf = tracedraw_io::pdf::document_to_pdf(doc);
    let dir = std::env::temp_dir().join("tracedraw-print");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(shared_file_name(name));
    crate::files::write(&path, pdf).map_err(|e| e.to_string())?;
    crate::printers::print_pdf(printer, &path, copies, collate, title)?;
    Ok(trf("status.print_sent_printer", &[("name", printer)]))
}

/// The sheets drawn at the printer's own resolution and sent to the
/// Windows printer `printer`, local or on the network (#614), with the
/// copies the dialog asked for. Each sheet goes as strips of pixels with
/// its blank parts left out, so fine lines and small text come out as
/// sharp as from a PDF sent through `lp` and blank paper costs nothing in
/// the job (#1404); `bitmap_dpi` (Print as bitmap) sends that resolution
/// instead.
#[cfg(target_os = "windows")]
fn print_on(
    printer: &str,
    _name: &str,
    doc: &Document,
    copies: u32,
    collate: bool,
    title: &str,
    bitmap_dpi: Option<f64>,
) -> Result<String, String> {
    let dpi = bitmap_dpi
        .unwrap_or_else(|| windows_print_dpi(crate::printers::printer_resolution_windows(printer)));
    let sheets: Vec<crate::printers::Sheet> = doc
        .pages
        .iter()
        .map(|p| crate::printers::Sheet {
            width: p.size.width,
            height: p.size.height,
            tiles: sheet_tiles(doc, p.id, dpi, PRINT_TILE_PIXELS).unwrap_or_default(),
        })
        .collect();
    crate::printers::print_sheets_windows(printer, &sheets, dpi, copies, collate, title)?;
    Ok(trf("status.print_sent_printer", &[("name", printer)]))
}

/// The resolution sheets are drawn at for a Windows printer that says
/// `reported` dots per inch: its own, kept between 300 and 1200 (a
/// higher one would only make the job larger), 600 when it does not say.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn windows_print_dpi(reported: Option<f64>) -> f64 {
    match reported {
        Some(d) if d.is_finite() && d > 0.0 => d.clamp(300.0, 1200.0),
        _ => 600.0,
    }
}

/// The most pixels drawn at once for a printed sheet (64 MB of RGBA).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const PRINT_TILE_PIXELS: u64 = 1 << 24;

/// A sheet drawn on white at `dpi` as strips of at most `max_pixels`
/// pixels each, every strip cut to the part of it that is not blank
/// (white) and left out when all of it is: the PNGs with where they go on
/// the sheet. The strips put together give the same pixels as the whole
/// sheet drawn at once, with no sheet-sized picture in memory.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn sheet_tiles(
    doc: &Document,
    id: tracedraw_core::PageId,
    dpi: f64,
    max_pixels: u64,
) -> Option<Vec<crate::printers::Tile>> {
    let page = doc.page(id).ok()?;
    let zoom = dpi / 25.4;
    let w = ((page.size.width * zoom).ceil() as u32).max(1);
    let h = ((page.size.height * zoom).ceil() as u32).max(1);
    // Whole multiples of 16 rows, as the coarse grid of procedural fills.
    let rows = ((max_pixels / w as u64) / 16 * 16).clamp(16, (h as u64).div_ceil(16) * 16) as u32;
    let opts = tracedraw_render::RenderOptions {
        width: w,
        height: h,
        view: tracedraw_render::ViewTransform {
            zoom,
            origin_x: 0.0,
            origin_y: h as f64,
            stretch_y: 1.0,
        },
        ..Default::default()
    };
    let mut tiles = Vec::new();
    let mut y0 = 0u32;
    while y0 < h {
        let band = rows.min(h - y0);
        let objects = tracedraw_render::render_page_rows(doc, id, &opts, y0, y0 + band)?;
        let mut strip = tiny_skia::Pixmap::new(w, band)?;
        strip.fill(tiny_skia::Color::WHITE);
        strip.draw_pixmap(
            0,
            0,
            objects.as_ref(),
            &Default::default(),
            tiny_skia::Transform::identity(),
            None,
        );
        if let Some((x, y, cw, ch)) = inked_area(&strip) {
            let part =
                strip.clone_rect(tiny_skia::IntRect::from_xywh(x as i32, y as i32, cw, ch)?)?;
            tiles.push(crate::printers::Tile {
                png: part.encode_png().ok()?,
                x: x as f64 / zoom,
                y: (y0 + y) as f64 / zoom,
                width: cw as f64 / zoom,
                height: ch as f64 / zoom,
            });
        }
        y0 += band;
    }
    Some(tiles)
}

/// The smallest box (x, y, width, height in pixels) holding every pixel of
/// `pm` that is not white; `None` when all of them are.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn inked_area(pm: &tiny_skia::Pixmap) -> Option<(u32, u32, u32, u32)> {
    let w = pm.width() as usize;
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (y, row) in pm.data().chunks_exact(w * 4).enumerate() {
        let mut first = None;
        let mut last = 0;
        for (x, px) in row.chunks_exact(4).enumerate() {
            if px != [255, 255, 255, 255] {
                first.get_or_insert(x);
                last = x;
            }
        }
        if let Some(f) = first {
            x0 = x0.min(f);
            x1 = x1.max(last);
            y0 = y0.min(y);
            y1 = y;
        }
    }
    (x0 != usize::MAX).then(|| {
        (
            x0 as u32,
            y0 as u32,
            (x1 - x0 + 1) as u32,
            (y1 - y0 + 1) as u32,
        )
    })
}

/// Prints sent to the system viewer in this session.
#[cfg(not(target_arch = "wasm32"))]
static VIEWER_PRINTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Where the `count`th print of `name` to the viewer goes in `dir`: the
/// first takes the name itself, later ones "name (2).pdf" and on, so a
/// viewer still showing (and holding) an earlier print opens the new one
/// in a page of its own (#809). Prints of the name before the last one
/// are removed once no viewer holds them.
#[cfg(not(target_arch = "wasm32"))]
fn viewer_print_path(dir: &Path, name: &str, count: u32) -> std::path::PathBuf {
    let p = Path::new(name);
    let stem = p.file_stem().map_or(name.into(), |s| s.to_string_lossy());
    let ext = p.extension().map_or("pdf".into(), |s| s.to_string_lossy());
    let nth = |n: u32| match n {
        0 | 1 => dir.join(name),
        n => dir.join(format!("{stem} ({n}).{ext}")),
    };
    for n in 1..count.saturating_sub(1) {
        let _ = std::fs::remove_file(nth(n));
    }
    // A file of that name left by another session and still held.
    let mut n = count.max(1);
    while n < count.max(1) + 100 && nth(n).exists() && std::fs::remove_file(nth(n)).is_err() {
        n += 1;
    }
    nth(n)
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn print_through_viewer(name: &str, pdf: Vec<u8>) -> Result<String, String> {
    let dir = std::env::temp_dir().join("tracedraw-print");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let count = VIEWER_PRINTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let path = viewer_print_path(&dir, name, count);
    crate::files::write(&path, pdf).map_err(|e| e.to_string())?;
    open_with_system(&path)?;
    Ok(trf(
        "status.print_sent",
        &[("path", &path.display().to_string())],
    ))
}

/// On Android the print PDF goes to the system's print service, whose
/// dialog picks the printer, the copies and the paper (#1398). The file
/// stays in the app's temporary folder: the service reads it later.
#[cfg(target_os = "android")]
fn print_through_viewer(name: &str, pdf: Vec<u8>) -> Result<String, String> {
    let dir = std::env::temp_dir().join("tracedraw-print");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(shared_file_name(name));
    std::fs::write(&path, pdf).map_err(|e| e.to_string())?;
    let job = name.trim_end_matches(".pdf");
    if crate::android::outside::print_pdf(&path, job) {
        Ok(trf("status.print_sent_android", &[("name", job)]))
    } else {
        Err(tr("status.print_service_failed"))
    }
}

#[cfg(target_arch = "wasm32")]
fn print_through_viewer(name: &str, _pdf: Vec<u8>) -> Result<String, String> {
    Ok(trf("status.print_sent", &[("path", name)]))
}

/// The sheets repeated for `copies` copies: whole sets one after the other
/// (collated) or each sheet `copies` times.
fn repeat_copies(doc: &mut Document, copies: usize, collate: bool) {
    let original = std::mem::take(&mut doc.pages);
    let mut ids = doc.ids().clone();
    let mut push = |p: &tracedraw_core::document::Page, out: &mut Vec<_>| {
        let mut q = p.clone();
        q.id = ids.page();
        out.push(q);
    };
    let mut out = Vec::with_capacity(original.len() * copies);
    if collate {
        for _ in 0..copies {
            for p in &original {
                push(p, &mut out);
            }
        }
    } else {
        for p in &original {
            for _ in 0..copies {
                push(p, &mut out);
            }
        }
    }
    doc.set_ids(ids);
    doc.pages = out;
}

/// The PostScript file's language level in its header comments.
fn postscript_level(ps: String, level: u8) -> String {
    let level = level.clamp(2, 3);
    match ps.find('\n') {
        Some(i) => format!("{}\n%%LanguageLevel: {level}{}", &ps[..i], &ps[i..]),
        None => ps,
    }
}

/// The sheets the printer gets for the Print dialog's settings: the pages
/// imposed with their marks, mirrored and inverted when asked. Print and
/// Print Preview both start here, so the preview shows what prints.
/// The objects `page` shows (every one with `all`) scaled down and moved
/// as one to fit inside it, centred, when they run off it; objects wholly
/// off the page stay out of it.
fn fit_contents_to_page(page: &mut tracedraw_core::document::Page, all: bool) {
    const EPS: f64 = 1e-6;
    let area = page.rect();
    let shows = |b: &Rect| {
        all || (b.x0 <= area.x1 && b.x1 >= area.x0 && b.y0 <= area.y1 && b.y1 >= area.y0)
    };
    let Some(b) = page
        .layers
        .iter()
        .flat_map(|l| &l.shapes)
        .map(|s| s.bounds())
        .filter(|b| b.width().is_finite() && b.height().is_finite() && shows(b))
        .reduce(|a, b| a.union(b))
    else {
        return;
    };
    let inside = b.x0 >= area.x0 - EPS
        && b.y0 >= area.y0 - EPS
        && b.x1 <= area.x1 + EPS
        && b.y1 <= area.y1 + EPS;
    if inside {
        return;
    }
    let k = (area.width() / b.width().max(EPS))
        .min(area.height() / b.height().max(EPS))
        .min(1.0);
    let t = Affine::translate(area.center().to_vec2())
        * Affine::scale(k)
        * Affine::translate(-b.center().to_vec2());
    for layer in page.layers.iter_mut() {
        for s in layer.shapes.iter_mut() {
            if shows(&s.bounds()) {
                s.transform_by(t);
            }
        }
    }
}

/// The area `shapes` print on: their outlines (half the outline's width
/// beyond the path) and live effects included, at least a sliver each
/// way (a line has no height). `None` for no shapes.
pub fn print_area<'a>(shapes: impl Iterator<Item = &'a tracedraw_core::Shape>) -> Option<Rect> {
    const MIN: f64 = 0.01;
    let area = shapes
        .map(|s| {
            let half = s.stroke.as_ref().map_or(0.0, |st| st.width.max(0.0) / 2.0);
            s.visual_bounds().inflate(half, half)
        })
        .filter(|b| b.x0.is_finite() && b.y0.is_finite() && b.x1.is_finite() && b.y1.is_finite())
        .reduce(|a, b| a.union(b))?;
    let c = area.center();
    let (w, h) = (area.width().max(MIN), area.height().max(MIN));
    Some(Rect::new(
        c.x - w / 2.0,
        c.y - h / 2.0,
        c.x + w / 2.0,
        c.y + h / 2.0,
    ))
}

/// The copies of their end objects that the blends of `s` (and of the
/// objects inside it) keep, in page space, moved by `t` along with `s`:
/// a page-space transformation of the start object alone, with no end
/// object to bring the copy up to date from (the end was not exported
/// with the start), would otherwise leave the steps running to where the
/// end used to be (#1541).
fn carry_blend_ends(s: &mut tracedraw_core::Shape, t: Affine) {
    for e in s.effects.iter_mut() {
        if let tracedraw_core::live::Effect::Blend { end, .. } = e {
            end.transform_by(t);
        }
    }
    match &mut s.kind {
        ShapeKind::Group { children } => children.iter_mut().for_each(|c| carry_blend_ends(c, t)),
        ShapeKind::ClipFrame { frame, contents } => {
            carry_blend_ends(frame, t);
            contents.iter_mut().for_each(|c| carry_blend_ends(c, t));
        }
        _ => {}
    }
}

/// The copies of their end objects that the blends of `doc` keep brought
/// up to date from the end objects themselves, as the editor does after
/// every change (`App::refresh_blends`). The sheets are made from a copy
/// of the drawing whose objects are moved, shrunk and recoloured outside
/// the editor (Fit to page, the selection as the page, a negative, text
/// in black, spot inks as CMYK), and a blend evaluated from a stale copy
/// ran its steps to where the end object used to be, in the colour it
/// used to have (#1541).
fn refresh_blend_ends(doc: &mut Document) {
    use tracedraw_core::live::Effect;
    fn starts(s: &tracedraw_core::Shape, out: &mut Vec<(ShapeId, ShapeId)>) {
        for e in &s.effects {
            if let Effect::Blend {
                end_id: Some(end), ..
            } = e
            {
                out.push((s.id, *end));
            }
        }
        match &s.kind {
            ShapeKind::Group { children } => children.iter().for_each(|c| starts(c, out)),
            ShapeKind::ClipFrame { frame, contents } => {
                starts(frame, out);
                contents.iter().for_each(|c| starts(c, out));
            }
            _ => {}
        }
    }
    let mut pairs = Vec::new();
    for layer in doc.all_layers() {
        for s in &layer.shapes {
            starts(s, &mut pairs);
        }
    }
    for (start, end) in pairs {
        // An end object that is not printed (not selected, say) leaves the
        // copy as it is, as in the editor.
        let Some(mut copy) = doc.shape_on_page(end) else {
            continue;
        };
        copy.effects.clear();
        // In the space of the group or frame holding the start object,
        // where its effects live (#2023).
        let parent = doc.parent_transform(start).unwrap_or(Affine::IDENTITY);
        let det = parent.determinant();
        if det.is_finite() && det.abs() > 1e-12 && parent != Affine::IDENTITY {
            copy.transform = parent.inverse() * copy.transform;
        }
        let Some(s) = doc.find_shape_mut(start) else {
            continue;
        };
        for e in s.effects.iter_mut() {
            if let Effect::Blend {
                end: kept,
                end_id: Some(id),
                ..
            } = e
            {
                if *id == end {
                    **kept = copy.clone();
                }
            }
        }
    }
}

/// The area the objects left on `page` and in the `master` layers cover
/// (what prints of them: visible objects of visible, printable layers,
/// their outlines and live effects included) made the page: the page
/// takes its size and the objects move with it, so they print as they
/// are placed among themselves. Nothing changes when nothing prints.
fn selection_as_page(
    page: &mut tracedraw_core::document::Page,
    master: &mut [tracedraw_core::Layer],
) {
    let printed = |l: &tracedraw_core::Layer| l.visible && l.printable;
    let shapes = page
        .layers
        .iter()
        .chain(master.iter())
        .filter(|l| printed(l))
        .flat_map(|l| &l.shapes)
        .filter(|s| s.visible);
    let Some(area) = print_area(shapes) else {
        return;
    };
    let (w, h) = (area.width(), area.height());
    let to_page = Affine::translate((-area.x0, -area.y0));
    for layer in page.layers.iter_mut().chain(master.iter_mut()) {
        for s in layer.shapes.iter_mut() {
            s.transform_by(to_page);
        }
    }
    page.size = Size::new(w, h);
}

/// The indexes of the drawing's pages the Print dialog's settings print,
/// in printing order: the whole drawing, the current page, the pages
/// listed or the selection's page, odd or even ones only when asked
/// (`PrintState::page_indexes`). `print_sheets` imposes these pages, and
/// the Layout sketch with the booklet summary under it counts the same
/// ones, so the counts match what prints (#788).
pub fn printed_pages(app: &App, st: &PrintState) -> Vec<usize> {
    let doc = app.engine.document();
    let current = doc.pages.iter().position(|p| p.id == app.page).unwrap_or(0);
    st.page_indexes(doc.pages.len(), current)
}

pub fn print_sheets(app: &App, st: &PrintState) -> Document {
    let src = app.engine.document();
    // Facing pages: each page printed with what its facing page holds
    // across the spine and in the bleed beside it (#1972); not for the
    // selection alone.
    let selection = st.range == 3 && !app.selection.is_empty();
    let mut doc = if selection {
        src.clone()
    } else {
        src.with_spread_overhangs(src.metadata.bleed)
    };
    let keep = printed_pages(app, st);
    let mut i = 0;
    doc.pages.retain(|_| {
        i += 1;
        keep.contains(&(i - 1))
    });
    // Selection: only the selected objects of the current page (none of
    // the master layers' others, no page background), and the area they
    // cover is the page printed: the layout, the sheet the size of the
    // page, Keep size and the marks work on it (#1001).
    if st.range == 3 && !app.selection.is_empty() {
        let picked = |s: &tracedraw_core::Shape| app.selection.contains(&s.id);
        for layer in doc.master.iter_mut() {
            layer.shapes.retain(picked);
        }
        for page in doc.pages.iter_mut() {
            page.background = None;
            for layer in page.layers.iter_mut() {
                layer.shapes.retain(picked);
            }
            selection_as_page(page, &mut doc.master);
        }
    }
    // The pages' background colour stays on screen when it is not to be
    // printed (#1651).
    if !st.page_background {
        for page in doc.pages.iter_mut() {
            page.background = None;
        }
    }
    // Fit to page with the selection printed: the selection shrinks to
    // fit the page, centred, rather than being cut at its edge (#534).
    // Printing pages, Fit to page scales the page itself to the paper
    // (`imposition::Options::fit`) and what runs off a page stays cut at
    // its edge, as on screen: pulling it in shrank the page's own objects
    // and moved them off their place (#2678).
    if st.fit_to_page && selection {
        for page in doc.pages.iter_mut() {
            fit_contents_to_page(page, true);
        }
    }
    // The master layers' objects print on the pages too, and the pages'
    // backgrounds: they were left out, so a spot ink there stayed a spot
    // ink and text there kept its colour.
    if st.spot_to_cmyk {
        for page in doc.pages.iter_mut() {
            if let Some(bg) = &mut page.background {
                map_fill_colors(bg, &spot_as_cmyk);
            }
        }
    }
    let layers = doc
        .pages
        .iter_mut()
        .flat_map(|p| p.layers.iter_mut())
        .chain(doc.master.iter_mut());
    for layer in layers {
        for s in layer.shapes.iter_mut() {
            if st.spot_to_cmyk {
                map_colors(s, &spot_as_cmyk);
            }
            if st.text_black {
                text_in_black(s);
            }
            if st.overprint_black {
                overprint_black(s);
            }
        }
    }
    // The blends' steps follow the objects as printed (#1541).
    refresh_blend_ends(&mut doc);
    let options = print_layout(app, st, &doc);
    let mut doc = crate::imposition::impose(&doc, &options);
    for page in doc.pages.iter_mut() {
        let w = page.size.width;
        for layer in page.layers.iter_mut() {
            for s in layer.shapes.iter_mut() {
                // Emulsion down: the whole sheet, marks too, mirrored.
                if st.mirror {
                    s.transform = Affine::translate((w, 0.0))
                        * Affine::new([-1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
                        * s.transform;
                }
                if st.invert {
                    invert_colors(s);
                }
            }
        }
    }
    // A negative turns the paper black too, so that what was black ink
    // (now white) shows as the clear part of the film.
    if st.invert {
        let mut ids = doc.ids().clone();
        for page in doc.pages.iter_mut() {
            let size = page.size;
            let mut ground = tracedraw_core::Shape::new(
                ids.shape(),
                ShapeKind::Rect {
                    rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, size.width, size.height),
                    radius: 0.0,
                    corners: None,
                },
            );
            ground.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK);
            ground.stroke = None;
            if page.layers.is_empty() {
                page.layers
                    .push(tracedraw_core::Layer::new(ids.layer(), "Negative"));
            }
            if let Some(layer) = page.layers.first_mut() {
                layer.shapes.insert(0, ground);
            }
        }
        doc.set_ids(ids);
    }
    // Grayscale composite output (#1657): every colour and picture of
    // the sheets, marks included, as its grey.
    if st.grayscale && !st.separations {
        grayscale_doc(&mut doc);
    }
    // The printer's colour profile (#3374): composite colours and
    // pictures as the printer needs them, for printers whose colour the
    // driver does not manage (sublimation, for one).
    if !st.grayscale && !st.separations {
        if let Some(t) = st
            .output_profile
            .as_deref()
            .and_then(|p| printer_transform(p, st.output_intent))
        {
            printer_profile_doc(&mut doc, &t);
        }
    }
    // Tile: each sheet (fitted to the pages, mirrored or inverted as a
    // whole) cut into tiles of the paper, a poster.
    if st.tile {
        doc = crate::imposition::tile_sheets(&doc, &st.tiling());
    }
    // Reverse order (#1394, #1571): the last sheet first, so a stack
    // printed face up, or the backs of printed sheets, come out in order.
    if st.reverse_order {
        doc.pages.reverse();
    }
    if st.as_bitmap {
        sheets_as_bitmaps(&mut doc, st.bitmap_dpi.clamp(36.0, 1200.0));
    }
    doc
}

/// The grey a colour prints as in grayscale output: its luminance
/// (`0.299 R + 0.587 G + 0.114 B`, as Bitmaps > Mode > Grayscale);
/// the registration colour stays, it prints on every plate.
fn gray_of(c: tracedraw_core::Color) -> tracedraw_core::Color {
    use tracedraw_core::Color;
    match c {
        Color::Gray { .. } | Color::Registration => c,
        _ => {
            let [r, g, b] = c.to_rgb8();
            let v = (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)) / 255.0;
            Color::Gray {
                v: v.clamp(0.0, 1.0),
            }
        }
    }
}

/// Grayscale output (#1657): every colour the sheets hold (fills,
/// outlines, text, gradients, meshes, patterns, effects, shadows and
/// lenses) turned to its grey, and every picture to its grey pixels (a
/// CMYK picture's own inks are left out, so it prints from the grey
/// pixels). The colours are found wherever the document keeps one, by
/// walking the objects' serialized form, so no kind of object is missed.
pub fn grayscale_doc(doc: &mut Document) {
    map_doc_colors(doc, &gray_of, &|img| crate::bitmap_fx::encode_gray(img));
}

/// Every colour the sheets hold (fills, outlines, text, gradients,
/// meshes, patterns, effects, shadows and lenses) through `color`, and
/// every picture through `picture` (its new PNG; `None` leaves it). A
/// CMYK picture's own inks are left out, so it prints from the new
/// pixels. The colours are found wherever the document keeps one, by
/// walking the objects' serialized form, so no kind of object is missed.
pub fn map_doc_colors(
    doc: &mut Document,
    color: &dyn Fn(tracedraw_core::Color) -> tracedraw_core::Color,
    picture: &dyn Fn(&image::RgbaImage) -> Option<Vec<u8>>,
) {
    type Maps<'a> = (
        &'a dyn Fn(tracedraw_core::Color) -> tracedraw_core::Color,
        &'a dyn Fn(&image::RgbaImage) -> Option<Vec<u8>>,
    );
    fn walk(v: &mut serde_json::Value, f: Maps) {
        use serde_json::Value;
        match v {
            Value::Array(a) => a.iter_mut().for_each(|x| walk(x, f)),
            Value::Object(m) => {
                if m.get("model").is_some_and(Value::is_string) {
                    if let Ok(c) =
                        serde_json::from_value::<tracedraw_core::Color>(Value::Object(m.clone()))
                    {
                        // A colour can share its object with the tag of
                        // what holds it (a solid fill is `{"fill":
                        // "solid", "model": ...}`): only its own keys go.
                        if let (Ok(Value::Object(was)), Ok(Value::Object(new))) =
                            (serde_json::to_value(c), serde_json::to_value((f.0)(c)))
                        {
                            for k in was.keys() {
                                m.remove(k);
                            }
                            m.extend(new);
                        }
                        return;
                    }
                }
                m.remove("picture_inks");
                m.remove("cmyk_bitmap");
                if let Some(Value::String(b64)) = m.get_mut("png") {
                    use base64::Engine;
                    let engine = base64::engine::general_purpose::STANDARD;
                    let new = engine
                        .decode(b64.as_bytes())
                        .ok()
                        .and_then(|png| crate::bitmap_fx::decode(&png))
                        .and_then(|img| (f.1)(&img));
                    if let Some(new) = new {
                        *b64 = engine.encode(new);
                        // The picture's effects stay as printed, not
                        // worked out again from the original.
                        m.remove("fx");
                    }
                }
                m.values_mut().for_each(|x| walk(x, f));
            }
            _ => {}
        }
    }
    fn map<T: serde::Serialize + serde::de::DeserializeOwned>(x: &mut T, f: Maps) {
        let Ok(mut v) = serde_json::to_value(&*x) else {
            return;
        };
        walk(&mut v, f);
        match serde_json::from_value(v) {
            Ok(back) => *x = back,
            Err(e) => log::warn!("print colours: {e}"),
        }
    }
    for page in doc.pages.iter_mut() {
        map(page, (color, picture));
    }
    map(&mut doc.master, (color, picture));
}

/// The colour profile of the printer the composite output is converted
/// to (#3374): from the drawing's colours, as sRGB, to the device values
/// of the RGB or CMYK profile at `path`. `None` when the file is not a
/// profile that can be printed to.
pub fn printer_transform(
    path: &str,
    intent: tracedraw_core::icc::Intent,
) -> Option<tracedraw_core::icc::Transform> {
    use tracedraw_core::icc::{ColorSpace, Profile, Transform};
    let bytes = std::fs::read(path).ok()?;
    let dst = Profile::parse(&bytes).ok()?;
    if !matches!(dst.color_space(), ColorSpace::Rgb | ColorSpace::Cmyk) {
        return None;
    }
    Transform::new(&Profile::srgb(), &dst, intent, true)
}

/// Every colour and picture of the sheets converted through the
/// printer's profile `t` (#3374): a colour becomes the RGB or CMYK
/// values the printer needs for it; pictures go through it pixel by
/// pixel with an RGB profile (with a CMYK one they go as RGB, back from
/// the printer's CMYK, so they print as the profile would have them).
/// The registration colour stays: it prints on every plate.
pub fn printer_profile_doc(doc: &mut Document, t: &tracedraw_core::icc::Transform) {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use tracedraw_core::icc::ColorSpace;
    use tracedraw_core::Color;
    let cmyk = t.output_space() == ColorSpace::Cmyk;
    let device = |rgb: [u8; 3]| -> Vec<f32> {
        t.transform(&rgb.map(|v| f32::from(v) / 255.0))
            .into_iter()
            .map(|v| v.clamp(0.0, 1.0))
            .collect()
    };
    let color = |c: Color| -> Color {
        if matches!(c, Color::Registration) {
            return c;
        }
        let d = device(c.to_rgb8());
        match (cmyk, d.as_slice()) {
            (true, [c, m, y, k, ..]) => Color::Cmyk {
                c: *c,
                m: *m,
                y: *y,
                k: *k,
            },
            (false, [r, g, b, ..]) => {
                let v = |x: f32| (x * 255.0).round() as u8;
                Color::rgb8(v(*r), v(*g), v(*b))
            }
            _ => c,
        }
    };
    // A picture's pixels, each colour worked out once.
    let seen: RefCell<HashMap<[u8; 3], [u8; 3]>> = RefCell::new(HashMap::new());
    let pixel = |rgb: [u8; 3]| -> [u8; 3] {
        if let Some(out) = seen.borrow().get(&rgb) {
            return *out;
        }
        let d = device(rgb);
        let v = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
        let out = match (cmyk, d.as_slice()) {
            (false, [r, g, b, ..]) => [v(*r), v(*g), v(*b)],
            (true, [c, m, y, k, ..]) => {
                let [r, g, b] = Color::Cmyk {
                    c: *c,
                    m: *m,
                    y: *y,
                    k: *k,
                }
                .to_rgb8();
                [r, g, b]
            }
            _ => rgb,
        };
        seen.borrow_mut().insert(rgb, out);
        out
    };
    let picture = |img: &image::RgbaImage| -> Option<Vec<u8>> {
        let mut out = img.clone();
        for p in out.pixels_mut() {
            let [r, g, b] = pixel([p[0], p[1], p[2]]);
            p.0 = [r, g, b, p[3]];
        }
        crate::bitmap_fx::encode(&out)
    };
    map_doc_colors(doc, &color, &picture);
}

/// A spot ink's colour as its CMYK mix at the tint.
fn spot_as_cmyk(c: tracedraw_core::Color) -> tracedraw_core::Color {
    match c {
        tracedraw_core::Color::Spot {
            tint, c, m, y, k, ..
        } => tracedraw_core::Color::Cmyk {
            c: c * tint,
            m: m * tint,
            y: y * tint,
            k: k * tint,
        },
        other => other,
    }
}

/// Composite > Print text in black: every text, in groups too, filled and
/// outlined in black.
fn text_in_black(s: &mut tracedraw_core::Shape) {
    use tracedraw_core::{Color, Fill};
    match &mut s.kind {
        ShapeKind::Text { spans, .. } => {
            if !matches!(s.fill, Fill::None) {
                s.fill = Fill::Solid(Color::BLACK);
            }
            if let Some(st) = &mut s.stroke {
                st.color = Color::BLACK;
            }
            for sp in spans.iter_mut() {
                if sp.fill.is_some() {
                    sp.fill = Some(Fill::Solid(Color::BLACK));
                }
                // A character's own outline too: it kept its colour.
                if let Some(st) = &mut sp.outline {
                    st.color = Color::BLACK;
                }
            }
        }
        ShapeKind::Group { children } => children.iter_mut().for_each(text_in_black),
        ShapeKind::ClipFrame { contents, .. } => contents.iter_mut().for_each(text_in_black),
        _ => {}
    }
}

/// Whether `c` is black ink only (100 % black, no other ink).
fn is_pure_black(c: &tracedraw_core::Color) -> bool {
    match c.convert_to("CMYK") {
        tracedraw_core::Color::Cmyk { c, m, y, k } => {
            k >= 0.999 && c <= 0.001 && m <= 0.001 && y <= 0.001
        }
        _ => false,
    }
}

/// Composite > Always overprint black: black fills and outlines overprint.
fn overprint_black(s: &mut tracedraw_core::Shape) {
    use tracedraw_core::Fill;
    if let Fill::Solid(c) = &s.fill {
        if is_pure_black(c) {
            s.overprint_fill = true;
        }
    }
    if s.stroke.as_ref().is_some_and(|st| is_pure_black(&st.color)) {
        s.overprint_outline = true;
    }
    match &mut s.kind {
        ShapeKind::Group { children } => children.iter_mut().for_each(overprint_black),
        ShapeKind::ClipFrame { frame, contents } => {
            overprint_black(frame);
            contents.iter_mut().for_each(overprint_black);
        }
        _ => {}
    }
}

/// A sheet drawn on white at `dpi`: its PNG and size in pixels.
fn sheet_pixels(
    doc: &Document,
    id: tracedraw_core::PageId,
    dpi: f64,
) -> Option<(Vec<u8>, u32, u32)> {
    let pm = tracedraw_render::render_page_export_image(doc, id, dpi)?;
    let mut white = tiny_skia::Pixmap::new(pm.width(), pm.height())?;
    white.fill(tiny_skia::Color::WHITE);
    white.draw_pixmap(
        0,
        0,
        pm.as_ref(),
        &Default::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    let png = white.encode_png().ok()?;
    Some((png, white.width(), white.height()))
}

/// General > Print as bitmap: each sheet becomes one picture of itself.
fn sheets_as_bitmaps(doc: &mut Document, dpi: f64) {
    let pages: Vec<_> = doc.pages.iter().map(|p| (p.id, p.size)).collect();
    for (id, size) in pages {
        let Some((png, w, h)) = sheet_pixels(doc, id, dpi) else {
            continue;
        };
        let shape_id = doc.ids_mut().shape();
        let mut shape = tracedraw_core::Shape::new(
            shape_id,
            ShapeKind::Bitmap {
                rect: tracedraw_core::geometry::Rect::new(0.0, 0.0, size.width, size.height),
                width_px: w,
                height_px: h,
                png: png.into(),
                fx: None,
            },
        );
        shape.fill = tracedraw_core::Fill::None;
        shape.stroke = None;
        if let Ok(page) = doc.page_mut(id) {
            if let Some(first) = page.layers.first_mut() {
                first.shapes = vec![shape];
            }
            page.layers.truncate(1);
        }
    }
}

/// A separation plate: a process ink (0 cyan to 3 black) or a spot ink by
/// its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plate {
    Process(usize),
    Spot(u32),
}

/// The plates of a drawing: the four process inks, then its spot inks,
/// each with its name.
pub fn plates(doc: &Document) -> Vec<(Plate, String)> {
    let mut plates: Vec<(Plate, String)> = vec![
        (Plate::Process(0), "Cyan".into()),
        (Plate::Process(1), "Magenta".into()),
        (Plate::Process(2), "Yellow".into()),
        (Plate::Process(3), "Black".into()),
    ];
    plates.extend(doc.inks.iter().map(|i| (Plate::Spot(i.id), i.name.clone())));
    plates
}

/// One plate of a page: every colour turned into the grey of the ink's
/// coverage (black = full ink); registration colour prints on all plates.
/// Pictures show the coverage of the plate's ink, pixel by pixel, with
/// their alpha (#1659, `picture_plates`).
pub fn plate_page(
    page: &tracedraw_core::document::Page,
    plate: Plate,
) -> tracedraw_core::document::Page {
    use tracedraw_core::{Color, Fill};
    let mut p = page.clone();
    let coverage = move |c: Color| -> Color {
        if matches!(c, Color::Registration) {
            return Color::Gray { v: 0.0 };
        }
        let v = match (plate, c) {
            (Plate::Spot(id), Color::Spot { ink, tint, .. }) if ink == id => tint,
            (Plate::Spot(_), _) | (Plate::Process(_), Color::Spot { .. }) => 0.0,
            (Plate::Process(i), _) => match c.convert_to("CMYK") {
                Color::Cmyk { c, m, y, k } => [c, m, y, k].get(i).copied().unwrap_or(0.0),
                _ => 0.0,
            },
        };
        Color::Gray {
            v: 1.0 - v.clamp(0.0, 1.0),
        }
    };
    for layer in p.layers.iter_mut() {
        for s in layer.shapes.iter_mut() {
            // Fills the separations cannot split show as their average
            // colour.
            if matches!(
                s.fill,
                Fill::Texture(_) | Fill::Pattern(_) | Fill::Procedural(_)
            ) {
                if let Some(c) = s.fill.preview_color() {
                    s.fill = Fill::Solid(c);
                }
            }
            map_colors(s, &coverage);
            crate::picture_plates::swap_pictures(s, &|pic| {
                crate::picture_plates::plate_picture(pic, plate)
            });
        }
    }
    p
}

/// A page as printed with only some inks (#571): process colours keep
/// the channels of the inks on, spot colours show when their ink is on
/// and are paper otherwise; registration colour prints with every ink.
/// Pictures keep the inks on, pixel by pixel (#1659); `inks` are the
/// drawing's spot inks, for the colour a picture's spot channel shows.
pub fn inks_page(
    page: &tracedraw_core::document::Page,
    process: [bool; 4],
    spots: &[u32],
    inks: &[tracedraw_core::document::Ink],
) -> tracedraw_core::document::Page {
    use tracedraw_core::{Color, Fill};
    let spot_colors: Vec<(u32, [u8; 3])> = inks
        .iter()
        .filter(|i| spots.contains(&i.id))
        .map(|i| (i.id, i.color(1.0).to_rgb8()))
        .collect();
    let spots = spots.to_vec();
    let keep = move |c: Color| -> Color {
        let on = |i: usize, v: f32| if process[i] { v } else { 0.0 };
        match c {
            Color::Registration => Color::Cmyk {
                c: on(0, 1.0),
                m: on(1, 1.0),
                y: on(2, 1.0),
                k: on(3, 1.0),
            },
            Color::Spot { ink, .. } if spots.contains(&ink) => c,
            Color::Spot { .. } => Color::Gray { v: 1.0 },
            other => match other.convert_to("CMYK") {
                Color::Cmyk { c, m, y, k } => Color::Cmyk {
                    c: on(0, c),
                    m: on(1, m),
                    y: on(2, y),
                    k: on(3, k),
                },
                same => same,
            },
        }
    };
    let mut p = page.clone();
    for layer in p.layers.iter_mut() {
        for s in layer.shapes.iter_mut() {
            if matches!(
                s.fill,
                Fill::Texture(_) | Fill::Pattern(_) | Fill::Procedural(_)
            ) {
                if let Some(c) = s.fill.preview_color() {
                    s.fill = Fill::Solid(c);
                }
            }
            map_colors(s, &keep);
            crate::picture_plates::swap_pictures(s, &|pic| {
                crate::picture_plates::inks_picture(pic, process, &spot_colors)
            });
        }
    }
    p
}

/// The print layout for the Print dialog's settings: the imposition, the
/// sheet, the bleed and the marks.
pub fn print_layout(app: &App, st: &PrintState, doc: &Document) -> crate::imposition::Options {
    let bleed = if st.bleed {
        st.bleed_limit.clamp(0.0, 50.0)
    } else {
        0.0
    };
    let first = doc
        .pages
        .first()
        .map(|p| p.size)
        .unwrap_or(Size::new(210.0, 297.0));
    let tz = crate::time_tracker::local_zone();
    crate::imposition::Options {
        layout: st.layout(),
        // Tiled, the sheet fits the pages and the paper's size and
        // margins go to the tiles.
        sheet: if st.tile {
            None
        } else {
            st.sheet_size_for(first)
        },
        gutter: st.gutter.max(0.0),
        gutter_y: Some(st.gutter_y.max(0.0)),
        order: crate::imposition::PageOrder::from_index(st.page_order),
        bleed,
        scale: if st.fit_to_page {
            1.0
        } else {
            st.scale / 100.0
        },
        back_side: st.back_side,
        keep_size: st.keep_size,
        info_offset: st.info_offset,
        paper_margins: if st.tile { [0.0; 4] } else { st.margins },
        // Moved and stretched in Print Preview, the whole page with its
        // clip, clear of the printer's margins (#536, #595).
        page_offset: st.art_offset,
        page_stretch: st.art_stretch,
        page_rotation: st.art_rotation,
        cell_rotation: st.cell_rotation.clone(),
        cell_order: st.cell_order.clone(),
        stretch_fit: st.stretch_fit,
        fit: st.fit_to_page,
        booklet: st.booklet(),
        marks: crate::imposition::Marks {
            crop: st.crop_marks,
            bleed: false,
            registration: st.registration_marks,
            color_bars: st.color_bars,
            densitometer: st.densitometer,
            page_info: st.page_info,
            page_numbers: st.page_numbers,
        },
        crop: crate::imposition::CropStyle {
            length: st.crop_length,
            offset: st.crop_offset,
            width: st.crop_width,
            between: st.crop_between,
        },
        title: if st.info_text.trim().is_empty() {
            app.document_name()
        } else {
            st.info_text.trim().to_string()
        },
        when: crate::time_tracker::date_time(crate::time_tracker::unix_now(), &tz),
    }
}

/// Map every colour of `s` and what it holds through `f`: fills of every
/// kind, outlines, character fills, groups and clip frames.
pub fn map_colors(
    s: &mut tracedraw_core::Shape,
    f: &dyn Fn(tracedraw_core::Color) -> tracedraw_core::Color,
) {
    let fill = map_fill_colors;
    fill(&mut s.fill, f);
    if let Some(st) = &mut s.stroke {
        st.color = f(st.color);
    }
    // The copy of its end object a blend keeps: the steps run to the
    // end's colour as mapped (#1541). The colours of the other effects
    // (a contour's last step, an extrusion's shades, glows, shadows and
    // the rest) and of the drop shadow go too: they were left as they
    // were, so a contour or a shadow printed in colour on every plate of
    // the separations and kept its spot ink with Convert spot colours to
    // CMYK.
    for e in s.effects.iter_mut() {
        if let tracedraw_core::live::Effect::Blend { end, .. } = e {
            map_colors(end, f);
        } else {
            map_colors_within(e, f);
        }
    }
    if let Some(sh) = &mut s.shadow {
        sh.color = f(sh.color);
    }
    match &mut s.kind {
        ShapeKind::Group { children } => children.iter_mut().for_each(|c| map_colors(c, f)),
        ShapeKind::ClipFrame { frame, contents } => {
            map_colors(frame, f);
            contents.iter_mut().for_each(|c| map_colors(c, f));
        }
        ShapeKind::Text { spans, .. } => {
            for sp in spans.iter_mut() {
                if let Some(x) = &mut sp.fill {
                    fill(x, f);
                }
                if let Some(x) = &mut sp.background {
                    fill(x, f);
                }
                if let Some(st) = &mut sp.outline {
                    st.color = f(st.color);
                }
            }
        }
        _ => {}
    }
}

/// The colours of a fill mapped through `f`.
fn map_fill_colors(
    x: &mut tracedraw_core::Fill,
    f: &dyn Fn(tracedraw_core::Color) -> tracedraw_core::Color,
) {
    use tracedraw_core::{Fill, Pattern};
    match x {
        Fill::Solid(c) => *c = f(*c),
        Fill::Fountain(g) => g.stops.iter_mut().for_each(|s| s.color = f(s.color)),
        Fill::Mesh(m) => m.nodes.iter_mut().for_each(|n| n.color = f(n.color)),
        Fill::Pattern(Pattern::TwoColor { front, back, .. }) => {
            *front = f(*front);
            *back = f(*back);
        }
        _ => {}
    }
}

/// Every colour `x` holds, wherever it keeps one, mapped through `f`,
/// found by walking its serialized form (as `grayscale_doc` does), so no
/// effect's colour is missed.
fn map_colors_within<T: serde::Serialize + serde::de::DeserializeOwned>(
    x: &mut T,
    f: &dyn Fn(tracedraw_core::Color) -> tracedraw_core::Color,
) {
    fn walk(v: &mut serde_json::Value, f: &dyn Fn(tracedraw_core::Color) -> tracedraw_core::Color) {
        use serde_json::Value;
        match v {
            Value::Array(a) => a.iter_mut().for_each(|v| walk(v, f)),
            Value::Object(m) => {
                if m.get("model").is_some_and(Value::is_string) {
                    if let Ok(c) =
                        serde_json::from_value::<tracedraw_core::Color>(Value::Object(m.clone()))
                    {
                        // Only the colour's own keys go: it can share its
                        // object with the tag of what holds it.
                        if let (Ok(Value::Object(was)), Ok(Value::Object(new))) =
                            (serde_json::to_value(c), serde_json::to_value(f(c)))
                        {
                            for k in was.keys() {
                                m.remove(k);
                            }
                            m.extend(new);
                        }
                        return;
                    }
                }
                m.values_mut().for_each(|v| walk(v, f));
            }
            _ => {}
        }
    }
    let Ok(mut v) = serde_json::to_value(&*x) else {
        return;
    };
    walk(&mut v, f);
    match serde_json::from_value(v) {
        Ok(back) => *x = back,
        Err(e) => log::warn!("colours of an effect: {e}"),
    }
}

fn invert_colors(s: &mut tracedraw_core::Shape) {
    use tracedraw_core::Color;
    map_colors(s, &|c: Color| {
        let [r, g, b] = c.to_rgb8();
        Color::rgb8(255 - r, 255 - g, 255 - b)
    });
}

/// Colour separations with every plate.
#[cfg(test)]
fn separations_doc(doc: &Document) -> Document {
    separations_doc_without(doc, &[], false)
}

/// Whether `plate` of `page` takes any ink: a colour with coverage on
/// that plate, or a picture (its inks not looked into, so it counts on
/// every plate); the registration colour the marks are drawn in does not
/// count.
pub fn plate_has_ink(page: &tracedraw_core::document::Page, plate: Plate) -> bool {
    use tracedraw_core::{Color, Fill};
    let inked = std::cell::Cell::new(false);
    let look = |c: Color| -> Color {
        let v = match (plate, c) {
            (_, Color::Registration) => 0.0,
            (Plate::Spot(id), Color::Spot { ink, tint, .. }) if ink == id => tint,
            (Plate::Spot(_), _) | (Plate::Process(_), Color::Spot { .. }) => 0.0,
            (Plate::Process(i), _) => match c.convert_to("CMYK") {
                Color::Cmyk { c, m, y, k } => [c, m, y, k].get(i).copied().unwrap_or(0.0),
                _ => 0.0,
            },
        };
        if v > 1e-6 {
            inked.set(true);
        }
        c
    };
    fn has_picture(s: &tracedraw_core::Shape) -> bool {
        match &s.kind {
            ShapeKind::Bitmap { .. } => true,
            ShapeKind::Group { children } => children.iter().any(has_picture),
            ShapeKind::ClipFrame { frame, contents } => {
                has_picture(frame) || contents.iter().any(has_picture)
            }
            _ => false,
        }
    }
    for layer in &page.layers {
        for s in &layer.shapes {
            if has_picture(s) {
                return true;
            }
            let mut s = s.clone();
            if matches!(
                s.fill,
                Fill::Texture(_) | Fill::Pattern(_) | Fill::Procedural(_)
            ) {
                if let Some(c) = s.fill.preview_color() {
                    s.fill = Fill::Solid(c);
                }
            }
            map_colors(&mut s, &look);
            if inked.get() {
                return true;
            }
        }
    }
    false
}

/// Colour separations: one grey page per sheet and plate (the four
/// process inks, then each spot ink), each the grey of that ink's
/// coverage; a spot colour prints on its own plate only, at its tint.
/// The plates at the indices `off` (in [`plates`] order), those unticked
/// in Print Preview, are left out (#846); with `skip_empty` so is each
/// sheet's plate that takes no ink ([`plate_has_ink`]), a sheet with no
/// ink at all keeping its first plate.
fn separations_doc_without(doc: &Document, off: &[usize], skip_empty: bool) -> Document {
    let size = doc
        .pages
        .first()
        .map(|p| p.size)
        .unwrap_or(Size::new(210.0, 297.0));
    let mut sep = Document::new(format!("{} (separations)", doc.title), size);
    sep.pages.clear();
    sep.symbols = doc.symbols.clone();
    let plates: Vec<(Plate, String)> = plates(doc)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !off.contains(i))
        .map(|(_, p)| p)
        .collect();
    for page in &doc.pages {
        let inked: Vec<bool> = plates
            .iter()
            .map(|(plate, _)| !skip_empty || plate_has_ink(page, *plate))
            .collect();
        let any = inked.iter().any(|i| *i);
        for (n, (plate, name)) in plates.iter().enumerate() {
            if !inked[n] && (any || n > 0) {
                continue;
            }
            let mut p = plate_page(page, *plate);
            p.name = format!("{} {}", page.name, name);
            let mut ids = sep.ids().clone();
            p.id = ids.page();
            sep.set_ids(ids);
            sep.pages.push(p);
        }
    }
    sep
}

/// A file name that stays one name inside a folder: no folder
/// separators, no leading dots, never empty.
pub fn shared_file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|&c| !crate::files::is_direction_mark(c))
        .map(|c| {
            if matches!(c, '/' | '\\' | '\0') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_start_matches('.').to_string();
    if clean.is_empty() {
        "document".into()
    } else {
        clean
    }
}

/// Whether `target` is an address with a scheme (`https:`, `mailto:`,
/// `market:`) rather than a file path.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn is_address(target: &str) -> bool {
    target.split_once(':').is_some_and(|(scheme, _)| {
        scheme.len() > 1
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// Open a file or a web address in the app the system has for it. On
/// Android (#1398) an address opens in its app, a file of the app (or a
/// document the picker returned) in the app picked for its type; a
/// folder cannot be shown there.
#[cfg(target_os = "android")]
pub fn open_with_system(path: &Path) -> Result<(), String> {
    let target = path.to_string_lossy();
    let opened = if is_address(&target) {
        crate::android::outside::view(&target, "")
    } else {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mime = crate::files::mime_for_extension(ext);
        match crate::files::document_uri(path) {
            Some(uri) => crate::android::outside::view(&uri, mime),
            None if path.is_dir() => return Err(tr("status.no_folders_on_android")),
            None => crate::android::outside::view(&target, mime),
        }
    };
    if opened {
        Ok(())
    } else {
        Err(tr("status.could_not_open_viewer"))
    }
}

#[cfg(not(target_os = "android"))]
pub fn open_with_system(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("cmd")
        .args(["/C", "start", "", &path.display().to_string()])
        .spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(path).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(path).spawn();
    r.map(|_| ())
        .map_err(|e| format!("{}: {e}", tr("status.could_not_open_viewer")))
}

/// The program and arguments that open `path` in `editor`: the
/// program itself, or on macOS an application bundle through `open -a`.
pub fn editor_command(editor: &str, path: &Path) -> (String, Vec<String>) {
    let editor = editor.trim().trim_matches('"');
    let file = path.display().to_string();
    if cfg!(target_os = "macos") && editor.trim_end_matches('/').ends_with(".app") {
        return ("open".into(), vec!["-a".into(), editor.into(), file]);
    }
    (editor.into(), vec![file])
}

/// Open `path` in the bitmap editor of the preferences, or with the
/// system's program when none is set (#593).
#[cfg(not(target_arch = "wasm32"))]
pub fn open_in_editor(editor: &str, path: &Path) -> Result<(), String> {
    if editor.trim().is_empty() {
        return open_with_system(path);
    }
    let (program, args) = editor_command(editor, path);
    std::process::Command::new(&program)
        .args(&args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{}: {program}: {e}", tr("status.could_not_open_viewer")))
}

impl App {
    /// Open a web page in the system browser (a new tab in the browser
    /// build).
    pub fn open_url(&mut self, url: &str) {
        #[cfg(target_arch = "wasm32")]
        {
            crate::web::open_url(url);
            return;
        }
        #[allow(unreachable_code)]
        if let Err(e) = open_with_system(Path::new(url)) {
            self.status = e;
        }
    }

    /// File name for a PDF sent somewhere: the document title, cleaned.
    fn send_to_file_name(&self) -> String {
        let title = self.document_title();
        // The name in its own script, accents and vowel marks included.
        let stem = crate::files::clean_file_name_for(title.trim_end_matches('*'), true);
        let stem = stem.trim_start_matches('.').trim();
        let stem = if stem.is_empty() { "Graphic1" } else { stem };
        format!("{stem}.pdf")
    }

    /// File > Send To: write the document as PDF into `target` and report
    /// where it went.
    pub fn send_to(&mut self, target: SendTarget) {
        if crate::files::ANDROID {
            self.send_to_android(target);
            return;
        }
        if crate::files::WEB {
            // No folders in a browser: every target is a PDF download.
            let name = self.send_to_file_name();
            let pdf = tracedraw_io::pdf::document_to_pdf(self.engine.document());
            let path = std::path::PathBuf::from("/download").join(&name);
            self.status = match crate::files::write(&path, pdf) {
                Ok(()) => trf("status.sent_to", &[("path", &name)]),
                Err(e) => trf("status.export_failed", &[("e", &e.to_string())]),
            };
            let _ = target;
            return;
        }
        let Some(dir) = target.dir() else {
            self.status = tr("status.send_to_no_folder");
            return;
        };
        let path = dir.join(self.send_to_file_name());
        match tracedraw_io::save_pdf(self.engine.document(), &path) {
            Ok(()) => {
                self.status = trf("status.sent_to", &[("path", &path.display().to_string())]);
                if target == SendTarget::Mail {
                    // The platform's mail client gets the PDF as an
                    // attachment; without a mailto attachment standard,
                    // open the file's folder so it can be dragged in.
                    let _ = open_with_system(&dir);
                }
            }
            Err(e) => self.status = trf("status.export_failed", &[("e", &e.to_string())]),
        }
    }
}

impl App {
    /// Send To on Android (#1398): Documents saves the PDF where the
    /// system's picker says, Mail Recipient hands it to a mail app through
    /// the share sheet; there is no desktop folder ([`SendTarget::why_not`]).
    fn send_to_android(&mut self, target: SendTarget) {
        if let Some(why) = target.why_not() {
            self.status = tr(why);
            return;
        }
        let name = self.send_to_file_name();
        let pdf = tracedraw_io::pdf::document_to_pdf(self.engine.document());
        if target == SendTarget::Documents {
            crate::files::Dialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name(name)
                .save_file(self, move |app, path| {
                    app.status = match crate::files::write(&path, pdf) {
                        Ok(()) => trf("status.sent_to", &[("path", &path.display().to_string())]),
                        Err(e) => trf("status.export_failed", &[("e", &e.to_string())]),
                    };
                });
            return;
        }
        let dir = std::env::temp_dir().join("tracedraw-send");
        let path = dir.join(shared_file_name(&name));
        self.status = match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, pdf))
        {
            Err(e) => trf("status.export_failed", &[("e", &e.to_string())]),
            Ok(()) if share_pdf(&path, name.trim_end_matches(".pdf")) => {
                trf("status.shared", &[("name", &name)])
            }
            Ok(()) => tr("status.share_failed"),
        };
    }
}

/// Hand the PDF at `path` to a mail app through the share sheet (Android);
/// false where there is none.
fn share_pdf(path: &Path, subject: &str) -> bool {
    #[cfg(target_os = "android")]
    return crate::android::outside::share(path, "application/pdf", subject, true);
    #[cfg(not(target_os = "android"))]
    {
        let _ = (path, subject);
        false
    }
}

/// Destinations of File > Send To.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendTarget {
    Desktop,
    Documents,
    Mail,
}

impl SendTarget {
    /// Why this destination cannot be used here, as an i18n key for the
    /// greyed menu row's tip: Android has no desktop folder.
    pub fn why_not(self) -> Option<&'static str> {
        why_not_send_to(self, crate::files::ANDROID)
    }

    /// The folder the PDF is written to; the mail recipient gets it from
    /// the system's temporary folder.
    pub fn dir(self) -> Option<std::path::PathBuf> {
        match self {
            SendTarget::Desktop => user_dir("XDG_DESKTOP_DIR", "Desktop"),
            SendTarget::Documents => user_dir("XDG_DOCUMENTS_DIR", "Documents"),
            SendTarget::Mail => {
                let d = std::env::temp_dir().join("tracedraw-send");
                std::fs::create_dir_all(&d).ok()?;
                Some(d)
            }
        }
    }
}

/// [`SendTarget::why_not`] on Android (`android`) or elsewhere.
pub fn why_not_send_to(target: SendTarget, android: bool) -> Option<&'static str> {
    (android && target == SendTarget::Desktop).then_some("menu.file.send_to_desktop_android")
}

/// `$XDG_*_DIR`, then `~/<name>` (created when missing), then the home
/// folder itself.
fn user_dir(env: &str, name: &str) -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var_os(env) {
        let p = std::path::PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)?;
    let p = home.join(name);
    if p.is_dir() || std::fs::create_dir_all(&p).is_ok() {
        Some(p)
    } else {
        Some(home)
    }
}

// ----- print merge ---------------------------------------------------------------

/// The text of a data file as spreadsheets save it: UTF-8 (with or
/// without the byte order mark), UTF-16 with its mark (Excel's Unicode
/// Text), else code page 1252 (Excel's CSV on Windows), which failed to
/// read as UTF-8 (#495).
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |rest: &[u8], le: bool| {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes
            .iter()
            .map(|&b| crate::ui::glyphs_docker::from_cp1252(b).unwrap_or('\u{FFFD}'))
            .collect(),
    }
}

/// Records of a CSV text: the first row names the fields; the separator
/// is the comma, semicolon or tab the first row holds most of.
pub fn parse_csv(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let first = text.lines().next().unwrap_or_default();
    let delim = b",;\t"
        .iter()
        .copied()
        .max_by_key(|d| first.matches(*d as char).count())
        .filter(|d| first.contains(*d as char))
        .unwrap_or(b',');
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map(|h| h.iter().map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    let rows: Vec<Vec<String>> = rdr
        .records()
        .filter_map(|r| r.ok())
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    (headers, rows)
}

// ----- find and replace ----------------------------------------------------------

pub(crate) fn matches_text(hay: &str, needle: &str, match_case: bool, whole_word: bool) -> bool {
    if needle.is_empty() {
        return false;
    }
    let (h, n) = if match_case {
        (hay.to_string(), needle.to_string())
    } else {
        (hay.to_lowercase(), needle.to_lowercase())
    };
    if !whole_word {
        return h.contains(&n);
    }
    h.split(|c: char| !c.is_alphanumeric()).any(|w| w == n)
}

/// Find text objects containing `needle` (kind 0) or objects whose kind or
/// name matches it (kind 1).
pub fn find_shapes(
    app: &App,
    needle: &str,
    kind: usize,
    match_case: bool,
    whole_word: bool,
) -> Vec<ShapeId> {
    let Ok(page) = app.doc().page(app.page) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    fn walk(
        s: &tracedraw_core::Shape,
        needle: &str,
        kind: usize,
        mc: bool,
        ww: bool,
        out: &mut Vec<ShapeId>,
    ) {
        let hit = match (kind, &s.kind) {
            (0, ShapeKind::Text { spans, .. }) => {
                let t: String = spans.iter().map(|x| x.text.as_str()).collect();
                matches_text(&t, needle, mc, ww)
            }
            (1, k) => {
                let name = s.name.clone().unwrap_or_default();
                let kind_name = crate::ui::dockers::kind_name(k);
                matches_text(&name, needle, mc, ww) || matches_text(&kind_name, needle, mc, false)
            }
            _ => false,
        };
        if hit {
            out.push(s.id);
        }
        match &s.kind {
            ShapeKind::Group { children } => children
                .iter()
                .for_each(|c| walk(c, needle, kind, mc, ww, out)),
            ShapeKind::ClipFrame { frame, contents } => {
                walk(frame, needle, kind, mc, ww, out);
                contents
                    .iter()
                    .for_each(|c| walk(c, needle, kind, mc, ww, out));
            }
            _ => {}
        }
    }
    // Only top-level objects are selectable: a text found inside a group
    // or a frame selects the object it is in (it was left out, so Find
    // found no grouped text).
    for s in page.layers.iter().flat_map(|l| &l.shapes) {
        let mut hits = Vec::new();
        walk(s, needle, kind, match_case, whole_word, &mut hits);
        if !hits.is_empty() {
            out.push(s.id);
        }
    }
    out
}

pub fn replace_text(
    app: &mut App,
    needle: &str,
    replacement: &str,
    match_case: bool,
    whole_word: bool,
    all: bool,
) {
    let Ok(page) = app.doc().page(app.page) else {
        return;
    };
    // The texts that hold `needle`, inside groups and frames too, each
    // with its runs changed: Find and Replace found the texts of a group
    // and replaced nothing in them.
    let mut changed: Vec<(ShapeId, ShapeKind)> = Vec::new();
    fn walk(
        s: &tracedraw_core::Shape,
        find: &dyn Fn(&[TextSpan]) -> Option<Vec<TextSpan>>,
        out: &mut Vec<(ShapeId, ShapeKind)>,
    ) {
        match &s.kind {
            ShapeKind::Text {
                spans,
                origin,
                frame,
                align,
                para,
                on_path,
            } => {
                if let Some(spans) = find(spans) {
                    out.push((
                        s.id,
                        ShapeKind::Text {
                            spans,
                            origin: *origin,
                            frame: *frame,
                            align: *align,
                            para: para.clone(),
                            on_path: on_path.clone(),
                        },
                    ));
                }
            }
            ShapeKind::Group { children } => children.iter().for_each(|c| walk(c, find, out)),
            ShapeKind::ClipFrame { frame, contents } => {
                walk(frame, find, out);
                contents.iter().for_each(|c| walk(c, find, out));
            }
            _ => {}
        }
    }
    let find =
        |spans: &[TextSpan]| replace_in_spans(spans, needle, replacement, match_case, whole_word);
    for s in page.layers.iter().flat_map(|l| &l.shapes) {
        walk(s, &find, &mut changed);
    }
    if !all {
        changed.truncate(1);
    }
    let cmds: Vec<Command> = changed
        .into_iter()
        .map(|(shape, kind)| Command::SetShapeKind { shape, kind })
        .collect();
    if !cmds.is_empty() {
        if let Err(e) = app.engine.run_batch("Replace", &cmds) {
            app.status = e.to_string();
        }
    }
}

/// `text` with `needle` replaced (see [`replace_in_spans`]).
#[cfg(test)]
fn replace_in(
    text: &str,
    needle: &str,
    replacement: &str,
    match_case: bool,
    whole_word: bool,
) -> String {
    let span = [TextSpan::new(text, "", 12.0)];
    match replace_in_spans(&span, needle, replacement, match_case, whole_word) {
        Some(mut spans) => spans.pop().map(|s| s.text).unwrap_or_default(),
        None => text.to_string(),
    }
}

/// The runs of a text with every `needle` in them replaced, `None` when
/// there is none. The runs are searched as one text, so a word in two
/// looks (half of it bold) is found and replaced too; the replacement
/// takes the look of the run its match starts in, and a run left with no
/// letters goes. Letters match one for one, ignoring case unless
/// `match_case`; with `whole_word` a match has no letter or digit right
/// before or after it. The positions were taken from the text made lower
/// case, which can be longer or shorter than the text ("İ" is three bytes
/// lower case and two as typed), so a Turkish "İ" before a match cut the
/// text in the wrong place, or past its end, which closed TraceDraw.
pub(crate) fn replace_in_spans(
    spans: &[TextSpan],
    needle: &str,
    replacement: &str,
    match_case: bool,
    whole_word: bool,
) -> Option<Vec<TextSpan>> {
    let want: Vec<char> = needle.chars().collect();
    if want.is_empty() {
        return None;
    }
    // Every letter of the text with the run it is in.
    let letters: Vec<(char, usize)> = spans
        .iter()
        .enumerate()
        .flat_map(|(k, sp)| sp.text.chars().map(move |c| (c, k)))
        .collect();
    let same = |a: char, b: char| a == b || (!match_case && a.to_lowercase().eq(b.to_lowercase()));
    let word = |at: Option<&(char, usize)>| at.is_some_and(|(c, _)| c.is_alphanumeric());
    let mut found: Vec<usize> = Vec::new();
    let mut i = 0;
    while i + want.len() <= letters.len() {
        let hit = want
            .iter()
            .enumerate()
            .all(|(k, w)| same(letters[i + k].0, *w))
            && !(whole_word
                && (word(i.checked_sub(1).and_then(|p| letters.get(p)))
                    || word(letters.get(i + want.len()))));
        if hit {
            found.push(i);
            i += want.len();
        } else {
            i += 1;
        }
    }
    if found.is_empty() {
        return None;
    }
    let mut texts: Vec<String> = vec![String::new(); spans.len()];
    let mut next = found.into_iter().peekable();
    let mut i = 0;
    while i < letters.len() {
        if next.next_if_eq(&i).is_some() {
            texts[letters[i].1].push_str(replacement);
            i += want.len();
        } else {
            texts[letters[i].1].push(letters[i].0);
            i += 1;
        }
    }
    let mut out: Vec<TextSpan> = spans
        .iter()
        .zip(texts)
        .filter(|(sp, t)| !t.is_empty() || sp.text.is_empty())
        .map(|(sp, text)| TextSpan { text, ..sp.clone() })
        .collect();
    if out.is_empty() {
        // All of it replaced by nothing: the first run stays, empty.
        out.extend(spans.first().map(|sp| TextSpan {
            text: String::new(),
            ..sp.clone()
        }));
    }
    Some(out)
}

// ----- find and replace by properties -----------------------------------------

impl PropertyFind {
    /// The criteria ticked.
    pub fn query(&self) -> ObjectQuery {
        let color = |none: bool, c| {
            if none {
                ColorQuery::Nothing
            } else {
                ColorQuery::Color(c)
            }
        };
        let font = self.font.trim();
        ObjectQuery {
            object_type: self
                .use_type
                .then(|| ObjectType::ALL.get(self.object_type).copied())
                .flatten(),
            fill: self.use_fill.then(|| color(self.fill_none, self.fill)),
            outline: self
                .use_outline
                .then(|| color(self.outline_none, self.outline)),
            outline_width: self.use_width.then_some(self.width),
            font: (self.use_font && !font.is_empty()).then(|| font.to_string()),
        }
    }

    /// The replacements ticked.
    pub fn replacement(&self) -> objfind::Replacement {
        let font = self.new_font.trim();
        objfind::Replacement {
            fill: self.set_fill.then_some(self.new_fill),
            outline: self.set_outline.then_some(self.new_outline),
            outline_width: self.set_width.then_some(self.new_width),
            font: (self.set_font && !font.is_empty()).then(|| font.to_string()),
            font_chars: self.font_chars,
            western_digits: self.western_digits,
        }
    }

    /// Fill in the criteria from `s` (From Selection): its type, fill,
    /// outline, outline width and font, ticking the ones it has.
    pub fn take_from(&mut self, s: &tracedraw_core::Shape) {
        let q = objfind::query_from(s);
        self.use_type = false;
        if let Some(t) = q.object_type {
            self.object_type = ObjectType::ALL.iter().position(|x| *x == t).unwrap_or(0);
        }
        self.use_fill = q.fill.is_some();
        match q.fill {
            Some(ColorQuery::Nothing) => self.fill_none = true,
            Some(ColorQuery::Color(c)) => {
                self.fill_none = false;
                self.fill = c;
            }
            None => {}
        }
        self.use_outline = q.outline.is_some();
        match q.outline {
            Some(ColorQuery::Nothing) => self.outline_none = true,
            Some(ColorQuery::Color(c)) => {
                self.outline_none = false;
                self.outline = c;
            }
            None => {}
        }
        self.use_width = false;
        if let Some(w) = q.outline_width {
            self.width = w;
        }
        self.use_font = q.font.is_some();
        if let Some(f) = q.font {
            self.font = f;
        }
    }
}

/// The objects that meet the ticked criteria, on the current page or in
/// the whole drawing.
pub fn find_by_properties(app: &App, pf: &PropertyFind) -> Vec<Hit> {
    let scope = if pf.whole_drawing {
        objfind::Scope::Document
    } else {
        objfind::Scope::Page
    };
    objfind::find(app.doc(), &pf.query(), scope, app.page_index())
}

/// Find All by properties: select the objects found on the current page.
/// Returns how many of them are on it.
pub fn select_found(app: &mut App, st: &mut FindReplaceState) -> usize {
    st.props.hits = find_by_properties(app, &st.props);
    // No object shown alone yet: Next starts at the first one.
    st.cursor = st.props.hits.len();
    let here = Some(app.page_index());
    let ids: Vec<ShapeId> = st
        .props
        .hits
        .iter()
        .filter(|h| h.page.is_none_or(|p| Some(p) == here))
        .map(|h| h.id)
        .collect();
    let n = ids.len();
    app.select(ids);
    n
}

/// The next (or previous) object found: its page comes into view and it
/// is selected alone. The search runs again first, so it follows edits.
pub fn step_found(app: &mut App, st: &mut FindReplaceState, forward: bool) {
    let before = st.props.hits.get(st.cursor).copied();
    st.props.hits = find_by_properties(app, &st.props);
    let n = st.props.hits.len();
    if n == 0 {
        return;
    }
    // Carry on from the object shown last, wherever it is now.
    let at = before.and_then(|b| st.props.hits.iter().position(|h| h.id == b.id));
    st.cursor = match (at, forward) {
        (None, true) => 0,
        (None, false) => n - 1,
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
    };
    let Some(hit) = st.props.hits.get(st.cursor).copied() else {
        return;
    };
    if let Some(p) = hit.page {
        if p != app.page_index() {
            app.goto_page(p);
        }
    }
    app.select(vec![hit.id]);
}

/// Replace All by properties, as one undo step. Returns how many
/// objects changed.
pub fn replace_properties(app: &mut App, st: &mut FindReplaceState) -> usize {
    st.props.hits = find_by_properties(app, &st.props);
    let cmds = objfind::replace_commands(
        app.doc(),
        &st.props.hits,
        &st.props.query(),
        &st.props.replacement(),
    );
    if cmds.is_empty() {
        return 0;
    }
    let changed: std::collections::HashSet<ShapeId> = cmds
        .iter()
        .filter_map(|c| match c {
            Command::SetFill { shapes, .. } | Command::SetStroke { shapes, .. } => {
                shapes.first().copied()
            }
            Command::SetShapeKind { shape, .. } => Some(*shape),
            _ => None,
        })
        .collect();
    match app.engine.run_batch("Replace", &cmds) {
        Ok(()) => changed.len(),
        Err(e) => {
            app.status = e.to_string();
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_address, shared_file_name, why_not_send_to, SendTarget};

    #[test]
    fn page_exports_omit_excluded_layer_contents_without_changing_the_drawing() {
        use tracedraw_core::{Color, Document, Fill, Rect, Shape, ShapeKind, Size};
        let mut doc = Document::new("Excluded", Size::new(10.0, 10.0));
        let mut shape = Shape::new(
            doc.ids_mut().shape(),
            ShapeKind::Rect {
                rect: Rect::new(1.0, 1.0, 4.0, 4.0),
                radius: 0.0,
                corners: None,
            },
        );
        shape.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        shape.stroke = None;
        doc.pages[0].layers[0].shapes.push(shape);
        doc.pages[0].layers[0].printable = false;
        let before = doc.to_json().unwrap();
        let st = super::ExportState {
            dpi: 25.4,
            transparent: false,
            ..Default::default()
        };
        let png = super::page_file(&doc, 0, "png", &st, None).unwrap();
        let pm = tiny_skia::Pixmap::decode_png(&png).unwrap();
        let p = pm.pixel(2, 8).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (255, 255, 255));
        let svg = super::page_file(&doc, 0, "svg", &st, None).unwrap();
        assert!(!String::from_utf8(svg).unwrap().contains("#ff0000"));
        assert_eq!(doc.to_json().unwrap(), before);
    }

    /// Android (#1398): Send To has no desktop folder there (the row is
    /// greyed with the reason), while Documents and Mail stay; every
    /// destination works elsewhere. The file handed to another app keeps
    /// one name inside its folder, and addresses are told from paths.
    #[test]
    fn android_sends_and_opens_through_the_system() {
        assert_eq!(
            why_not_send_to(SendTarget::Desktop, true),
            Some("menu.file.send_to_desktop_android")
        );
        assert_eq!(why_not_send_to(SendTarget::Documents, true), None);
        assert_eq!(why_not_send_to(SendTarget::Mail, true), None);
        for t in [SendTarget::Desktop, SendTarget::Documents, SendTarget::Mail] {
            assert_eq!(why_not_send_to(t, false), None);
            // Not on Android here.
            assert_eq!(t.why_not(), None);
        }
        assert!(crate::i18n::has_english(
            "menu.file.send_to_desktop_android"
        ));
        assert_eq!(shared_file_name("Poster.pdf"), "Poster.pdf");
        assert_eq!(shared_file_name("a/b\\c.pdf"), "a_b_c.pdf");
        assert_eq!(shared_file_name("../x.pdf"), "_x.pdf");
        assert_eq!(shared_file_name(" .. "), "document");
        assert_eq!(shared_file_name("\u{200F}שלום.pdf"), "שלום.pdf");
        // Send To names the PDF after the drawing in its own script.
        let mut app = App::headless();
        for name in ["שָׁלוֹם", "café", "文件", "a: b?"] {
            app.file = Some(std::path::PathBuf::from(format!("/x/{name}.tdraw")));
            let want = name.replace([':', '?'], "_");
            assert_eq!(app.send_to_file_name(), format!("{want}.pdf"));
        }
        for a in [
            "https://tracedraw.dev/help",
            "mailto:someone@example.com",
            "market://details?id=dev.tracedraw.app",
            "content://com.android.providers/document/1",
        ] {
            assert!(is_address(a), "{a}");
        }
        for p in [
            "/data/user/0/dev.tracedraw.app/files/tmp/a.pdf",
            "C:\\Users\\a.pdf",
            "notes.txt",
            "",
        ] {
            assert!(!is_address(p), "{p}");
        }
        // The Print dialog names the destination after the system's
        // service: the PDF viewer here, the print service on Android.
        assert_eq!(crate::ui::dialogs::print_service_key(), "print.dest_viewer");
        for key in [
            "print.dest_android",
            "status.print_sent_android",
            "status.print_service_failed",
            "status.shared",
            "status.share_failed",
            "status.bitmap_sent_to_app",
            "status.no_app_edits_pictures",
            "status.no_folders_on_android",
            "options.bitmap_editor_android",
            "options.open_folder_android",
        ] {
            assert!(crate::i18n::has_english(key), "{key}");
        }
    }

    /// Each print to the viewer gets a name of its own, so the viewer
    /// opens it in a new page while the last one is still shown; older
    /// ones are cleared away (#809).
    #[test]
    fn each_print_to_the_viewer_gets_its_own_file() {
        let dir = std::env::temp_dir().join(format!("tracedraw-viewer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut paths = Vec::new();
        for count in 1..=3 {
            let p = super::viewer_print_path(&dir, "Poster.pdf", count);
            std::fs::write(&p, b"%PDF").unwrap();
            paths.push(p);
        }
        let names: Vec<_> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["Poster.pdf", "Poster (2).pdf", "Poster (3).pdf"]);
        assert!(!paths[0].exists(), "the oldest is cleared away");
        assert!(paths[1].exists() && paths[2].exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    /// Separations to files: each plate in a document of its own named
    /// after its ink, every sheet on it; the empty ones left out when asked.
    #[test]
    fn each_plate_goes_to_a_file_of_its_own() {
        use tracedraw_core::Fill;
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(20.0, 20.0, 80.0, 60.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.run(tracedraw_core::Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(tracedraw_core::Color::Cmyk {
                c: 0.0,
                m: 1.0,
                y: 1.0,
                k: 0.0,
            }),
        });
        let mut st = PrintState {
            separations: true,
            plate_files: true,
            ..PrintState::default()
        };
        let sheets = print_sheets(&app, &st);
        let names = |files: &[(String, Document)]| -> Vec<String> {
            files.iter().map(|(n, _)| n.clone()).collect()
        };
        let files = plate_files(&sheets, &st);
        assert_eq!(names(&files), ["Cyan", "Magenta", "Yellow", "Black"]);
        for (name, doc) in &files {
            assert_eq!(doc.pages.len(), sheets.pages.len(), "{name}");
            assert!(doc.pages[0].name.ends_with(name.as_str()), "{name}");
        }
        // Unticked in Print Preview, or with no ink when empty plates
        // are left out (the black outline keeps the black plate).
        st.plates_off = vec![1];
        st.skip_empty_plates = true;
        assert_eq!(names(&plate_files(&sheets, &st)), ["Yellow", "Black"]);
        let p = plate_file_path(std::path::Path::new("/tmp/out/poster.pdf"), "Spot Red");
        assert_eq!(
            p,
            std::path::PathBuf::from("/tmp/out/poster - Spot Red.pdf")
        );
        let p = plate_file_path(std::path::Path::new("card"), "Black/K");
        assert_eq!(p.parent(), Some(std::path::Path::new("")));
        assert!(!p.to_string_lossy().contains('/'), "{p:?}");
    }

    /// Separations to PostScript with screens: each plate's page sets its
    /// ink's screen, the usual angles unless one is set by hand.
    #[test]
    fn each_plate_gets_its_halftone_screen() {
        let mut app = App::headless();
        app.new_shape(ShapeKind::Rect {
            rect: tracedraw_core::geometry::Rect::new(20.0, 20.0, 80.0, 60.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
        let mut st = PrintState {
            separations: true,
            screens: true,
            screen_lpi: 133.0,
            ..PrintState::default()
        };
        st.screen_angles.push(("Yellow".into(), 90.0));
        let sheets = print_sheets(&app, &st);
        let names: Vec<String> = plates(&sheets).into_iter().map(|(_, n)| n).collect();
        let sep = separations_doc_without(&sheets, &[], false);
        let screens = page_screens(&sep, &names, &st);
        assert_eq!(
            screens,
            [
                Some((133.0, 15.0)),
                Some((133.0, 75.0)),
                Some((133.0, 90.0)),
                Some((133.0, 45.0))
            ]
        );
        let ps = with_screens(&tracedraw_io::eps::document_to_ps(&sep), &screens);
        assert_eq!(ps.matches(" setscreen").count(), 4);
        let at = ps.find("133 75 {").expect("magenta screen");
        assert!(
            ps[..at].matches("%%Page: ").count() == 2,
            "on the second page"
        );
    }

    #[test]
    fn manual_two_sided_prints_fronts_then_backs_each_meeting_its_front() {
        let s = |v: Vec<Option<usize>>| -> Vec<i64> {
            v.iter().map(|i| i.map_or(-1, |i| i as i64)).collect()
        };
        assert_eq!(s(duplex_sides(5, false, false)), [0, 2, 4]);
        assert_eq!(s(duplex_sides(5, true, false)), [1, 3]);
        // From the last: the odd count's last front gets a blank back first.
        assert_eq!(s(duplex_sides(5, true, true)), [-1, 3, 1]);
        assert_eq!(s(duplex_sides(4, true, true)), [3, 1]);
        let mut doc = Document::new("t", Size::new(50.0, 50.0));
        let mut ids = doc.ids().clone();
        let mut second = doc.pages[0].clone();
        second.id = ids.page();
        second.name = "B".into();
        doc.set_ids(ids);
        doc.pages[0].name = "A".into();
        doc.pages.push(second);
        keep_sheets(&mut doc, &[None, Some(1), Some(0)]);
        let names: Vec<_> = doc.pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["A", "B", "A"]);
        assert!(doc.pages[0].layers.iter().all(|l| l.shapes.is_empty()));
        assert!(doc.pages[0].background.is_none());
    }

    /// A spot colour on a rectangle: printed with separations, the sheets
    /// give the four process plates and one for the spot ink, where the
    /// rectangle is full ink; the print preview offers its box (#779).
    #[test]
    fn separations_print_a_plate_for_each_spot_ink() {
        use tracedraw_core::Fill;
        let mut app = App::headless();
        app.run(tracedraw_core::Command::SetInks {
            inks: vec![tracedraw_core::document::Ink {
                id: 7,
                name: "Spot Red".into(),
                cmyk: [0.0, 1.0, 0.8, 0.0],
            }],
        });
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(20.0, 20.0, 80.0, 60.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.run(tracedraw_core::Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(tracedraw_core::Color::Spot {
                ink: 7,
                tint: 1.0,
                c: 0.0,
                m: 1.0,
                y: 0.8,
                k: 0.0,
            }),
        });
        let st = PrintState {
            separations: true,
            ..PrintState::default()
        };
        let sheets = print_sheets(&app, &st);
        let names: Vec<String> = plates(&sheets).into_iter().map(|(_, n)| n).collect();
        assert_eq!(names, ["Cyan", "Magenta", "Yellow", "Black", "Spot Red"]);
        let seps = separations_doc(&sheets);
        assert_eq!(seps.pages.len(), 5);
        // Plates unticked in Print Preview do not print (#846).
        let some = separations_doc_without(&sheets, &[0, 2], false);
        let kept: Vec<&str> = some.pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(kept.len(), 3, "{kept:?}");
        assert!(
            kept[0].ends_with("Magenta") && kept[2].ends_with("Spot Red"),
            "{kept:?}"
        );
        let spot = &seps.pages[4];
        assert!(spot.name.ends_with("Spot Red"), "{}", spot.name);
        // The rectangle is full ink on its plate and absent on cyan.
        let fills = |page: &tracedraw_core::document::Page| -> Vec<Fill> {
            let mut out = Vec::new();
            fn walk(s: &tracedraw_core::Shape, out: &mut Vec<Fill>) {
                if let ShapeKind::Group { children } = &s.kind {
                    children.iter().for_each(|c| walk(c, out));
                } else if let ShapeKind::ClipFrame { contents, .. } = &s.kind {
                    contents.iter().for_each(|c| walk(c, out));
                } else {
                    out.push(s.fill.clone());
                }
            }
            for l in &page.layers {
                l.shapes.iter().for_each(|s| walk(s, &mut out));
            }
            out
        };
        // Empty plates left out: the spot red rectangle with its black
        // outline takes no cyan, magenta or yellow, so those plates go;
        // the marks' registration colour does not count.
        let st_marks = PrintState {
            separations: true,
            crop_marks: true,
            registration_marks: true,
            ..PrintState::default()
        };
        let marked = print_sheets(&app, &st_marks);
        let inked = separations_doc_without(&marked, &[], true);
        let kept: Vec<&str> = inked.pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(kept, ["1 Black", "1 Spot Red"]);
        // With nothing on any plate the sheet keeps its first plate.
        let blank = print_sheets(&App::headless(), &st_marks);
        let left = separations_doc_without(&blank, &[], true);
        assert_eq!(left.pages.len(), 1);
        assert!(
            left.pages[0].name.ends_with("Cyan"),
            "{}",
            left.pages[0].name
        );
        let full = Fill::Solid(tracedraw_core::Color::Gray { v: 0.0 });
        assert!(fills(spot).contains(&full), "{:?}", fills(spot));
        assert!(!fills(&seps.pages[0]).contains(&full));
    }

    /// A Windows print goes at the printer's resolution in strips, blank
    /// ones left out and each cut to its inked part; put back together
    /// they are the sheet drawn whole, pixel for pixel (#1404).
    #[test]
    fn printed_sheet_strips_put_together_are_the_whole_sheet() {
        use tracedraw_core::Fill;
        let mut doc = Document::new("s", tracedraw_core::Size::new(50.0, 40.0));
        let layer = doc.pages[0].layers[0].id;
        let page = doc.pages[0].id;
        for (rect, colour) in [
            (
                tracedraw_core::Rect::new(3.0, 2.0, 21.5, 9.3),
                tracedraw_core::Color::rgb8(200, 0, 0),
            ),
            (
                tracedraw_core::Rect::new(10.2, 25.0, 47.0, 38.0),
                tracedraw_core::Color::rgb8(0, 90, 200),
            ),
        ] {
            let id = doc.ids_mut().shape();
            let mut s = tracedraw_core::Shape::new(id, ShapeKind::Ellipse { rect, arc: None });
            s.fill = Fill::Solid(colour);
            doc.layer_mut(layer).expect("layer").shapes.push(s);
        }
        let dpi = 200.0;
        let whole = tracedraw_render::render_page_image(&doc, page, dpi).expect("whole");
        let (w, h) = (whole.width(), whole.height());
        // Strips of 16 rows: many, some blank, some cut.
        let tiles = sheet_tiles(&doc, page, dpi, w as u64 * 16).expect("tiles");
        assert!(tiles.len() > 4, "{}", tiles.len());
        assert!(
            tiles.len() < h.div_ceil(16) as usize,
            "the blank strips between the two shapes are left out"
        );
        let mut joined = tiny_skia::Pixmap::new(w, h).expect("joined");
        joined.fill(tiny_skia::Color::WHITE);
        let zoom = dpi / 25.4;
        for t in &tiles {
            assert!(t.width < 50.0 - 1.0, "cut to its inked part");
            let pm = tiny_skia::Pixmap::decode_png(&t.png).expect("png");
            assert!(pm.height() <= 16);
            assert!(((t.width * zoom) - pm.width() as f64).abs() < 1e-6);
            assert!(((t.height * zoom) - pm.height() as f64).abs() < 1e-6);
            joined.draw_pixmap(
                (t.x * zoom).round() as i32,
                (t.y * zoom).round() as i32,
                pm.as_ref(),
                &Default::default(),
                tiny_skia::Transform::identity(),
                None,
            );
        }
        // The same pixels, but for an edge's smoothing now and then by a
        // step of its sixteen samples (each strip's shapes are placed a
        // whole number of rows apart, which rounds a coordinate the other
        // way at times): no marks where the strips meet, where outlines
        // cut at a strip's edge used to leave caps.
        let differ: Vec<u8> = whole
            .data()
            .iter()
            .zip(joined.data())
            .map(|(a, b)| a.abs_diff(*b))
            .filter(|d| *d > 0)
            .collect();
        let worst = differ.iter().copied().max().unwrap_or(0);
        assert!(worst <= 40, "{worst}");
        assert!(differ.len() < whole.data().len() / 200, "{}", differ.len());
        // A blank sheet sends nothing but the paper.
        let blank = Document::new("b", tracedraw_core::Size::new(50.0, 40.0));
        let tiles = sheet_tiles(&blank, blank.pages[0].id, 300.0, 1 << 24).expect("blank");
        assert!(tiles.is_empty());
    }

    /// The printer's own resolution, kept within reason (#1404).
    #[test]
    fn windows_prints_at_the_printers_resolution() {
        assert_eq!(windows_print_dpi(Some(600.0)), 600.0);
        assert_eq!(windows_print_dpi(Some(1200.0)), 1200.0);
        assert_eq!(windows_print_dpi(Some(2400.0)), 1200.0);
        assert_eq!(windows_print_dpi(Some(150.0)), 300.0);
        assert_eq!(windows_print_dpi(None), 600.0);
        assert_eq!(windows_print_dpi(Some(f64::NAN)), 600.0);
    }

    #[test]
    fn spot_inks_get_plates_of_their_own() {
        use tracedraw_core::{document::Ink, Fill};
        let mut doc = Document::new("s", tracedraw_core::Size::new(50.0, 50.0));
        doc.inks.push(Ink {
            id: 7,
            name: "Gold".into(),
            cmyk: [0.0, 0.2, 0.8, 0.1],
        });
        let layer = doc.pages[0].layers[0].id;
        let id = doc.ids_mut().shape();
        let mut s = tracedraw_core::Shape::new(
            id,
            ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(doc.inks[0].color(0.6));
        s.stroke = None;
        doc.layer_mut(layer).expect("layer").shapes.push(s);
        let pdf = tracedraw_io::pdf::document_to_pdf(&separations_doc(&doc));
        let text = String::from_utf8_lossy(&pdf);
        // C, M, Y, K and the spot plate.
        assert!(
            text.contains("/Count 5"),
            "{}",
            &text[..text.len().min(400)]
        );
    }

    /// A drawing of one picture of `rgba` pixels (`w` wide), with `inks`
    /// as its ink channels when given, and a spot ink with id 7.
    fn picture_doc(
        w: u32,
        rgba: &[u8],
        inks: Option<(Vec<tracedraw_core::picture_inks::PictureInk>, Vec<u8>)>,
    ) -> Document {
        use tracedraw_core::picture_inks::PictureInks;
        let h = (rgba.len() / 4) as u32 / w;
        let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).expect("pixels");
        let png: tracedraw_core::SharedBytes = crate::bitmap_fx::encode(&img).expect("png").into();
        let mut doc = Document::new("p", tracedraw_core::Size::new(50.0, 50.0));
        doc.inks.push(tracedraw_core::document::Ink {
            id: 7,
            name: "Gold".into(),
            cmyk: [0.0, 0.2, 0.8, 0.1],
        });
        let id = doc.ids_mut().shape();
        let mut s = tracedraw_core::Shape::new(
            id,
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                width_px: w,
                height_px: h,
                png: png.clone(),
                fx: None,
            },
        );
        s.fill = tracedraw_core::Fill::None;
        s.stroke = None;
        if let Some((inks, planes)) = inks {
            let n = inks.len() as u32;
            let gray = image::GrayImage::from_raw(w, h * n, planes).expect("planes");
            let mut out = std::io::Cursor::new(Vec::new());
            gray.write_to(&mut out, image::ImageFormat::Png)
                .expect("png");
            s.cmyk_bitmap = true;
            s.picture_inks = Some(Box::new(PictureInks {
                inks,
                png: out.into_inner().into(),
                of: tracedraw_core::picture_inks::fingerprint(&png),
            }));
        }
        doc.pages[0].layers[0].shapes.push(s);
        doc
    }

    /// The pixels of the picture on each plate (C, M, Y, K, then the spot
    /// inks), found inside groups and clip frames.
    fn plate_pixels(doc: &Document) -> Vec<Vec<[u8; 4]>> {
        fn find(s: &tracedraw_core::Shape) -> Option<&tracedraw_core::SharedBytes> {
            match &s.kind {
                ShapeKind::Bitmap { png, .. } => Some(png),
                ShapeKind::Group { children } => children.iter().find_map(find),
                ShapeKind::ClipFrame { contents, .. } => contents.iter().find_map(find),
                _ => None,
            }
        }
        separations_doc(doc)
            .pages
            .iter()
            .map(|p| {
                let png = p.layers[0].shapes.iter().find_map(find).expect("picture");
                let img = crate::bitmap_fx::decode(png).expect("decodes");
                img.pixels().map(|p| p.0).collect()
            })
            .collect()
    }

    /// Separations split pictures into their inks (#1659): a CMYK
    /// picture prints each ink it holds on its plate, an RGB or grey one
    /// is converted to CMYK, a spot channel goes to its spot plate, and
    /// the picture's alpha stays on every plate.
    #[test]
    fn separations_split_pictures_into_their_inks() {
        use tracedraw_core::picture_inks::PictureInk;
        let ink = |p: &[u8; 4]| 255 - p[0];
        // A CMYK picture of cyan ink alone (its screen pixels a cyan
        // that converts back to some magenta too): only the cyan plate.
        let process: Vec<PictureInk> = (0..4).map(PictureInk::Process).collect();
        let doc = picture_doc(
            1,
            &[0, 174, 239, 255],
            Some((process.clone(), vec![255, 0, 0, 0])),
        );
        let plates = plate_pixels(&doc);
        assert_eq!(plates.len(), 5, "C, M, Y, K and the spot plate");
        let inks: Vec<u8> = plates.iter().map(|p| ink(&p[0])).collect();
        assert_eq!(inks, [255, 0, 0, 0, 0]);
        // Black ink alone stays on the black plate.
        let doc = picture_doc(1, &[35, 31, 32, 255], Some((process, vec![0, 0, 0, 255])));
        let inks: Vec<u8> = plate_pixels(&doc).iter().map(|p| ink(&p[0])).collect();
        assert_eq!(inks, [0, 0, 0, 255, 0]);
        // RGB red: magenta and yellow.
        let doc = picture_doc(1, &[255, 0, 0, 255], None);
        let inks: Vec<u8> = plate_pixels(&doc).iter().map(|p| ink(&p[0])).collect();
        assert!(inks[0] < 26 && inks[3] < 26 && inks[4] == 0, "{inks:?}");
        assert!(inks[1] > 230 && inks[2] > 230, "{inks:?}");
        // A grey picture lands on the black plate.
        let doc = picture_doc(2, &[128, 128, 128, 255, 0, 0, 0, 255], None);
        let plates = plate_pixels(&doc);
        for (i, plate) in plates.iter().enumerate() {
            let inks: Vec<u8> = plate.iter().map(ink).collect();
            if i == 3 {
                assert!(inks[0] > 100 && inks[1] == 255, "{inks:?}");
            } else {
                assert_eq!(inks, [0, 0], "plate {i}");
            }
        }
        // Transparency: the alpha of every pixel on every plate.
        let doc = picture_doc(3, &[255, 0, 0, 0, 255, 0, 0, 128, 0, 0, 0, 255], None);
        for plate in plate_pixels(&doc) {
            assert_eq!(
                plate.iter().map(|p| p[3]).collect::<Vec<_>>(),
                [0, 128, 255]
            );
        }
        // A spot channel prints on its spot plate only.
        let doc = picture_doc(
            1,
            &[200, 160, 60, 255],
            Some((vec![PictureInk::Spot(7)], vec![204])),
        );
        let inks: Vec<u8> = plate_pixels(&doc).iter().map(|p| ink(&p[0])).collect();
        assert_eq!(inks, [0, 0, 0, 0, 204]);
        // Channels that no longer fit the pixels (the picture changed)
        // give way to the pixels: this red is magenta and yellow.
        let mut doc = picture_doc(
            1,
            &[255, 0, 0, 255],
            Some((
                (0..4).map(PictureInk::Process).collect(),
                vec![255, 0, 0, 0],
            )),
        );
        if let Some(inks) = &mut doc.pages[0].layers[0].shapes[0].picture_inks {
            inks.of ^= 1;
        }
        let inks: Vec<u8> = plate_pixels(&doc).iter().map(|p| ink(&p[0])).collect();
        assert!(inks[0] < 26 && inks[1] > 230, "{inks:?}");
        // Inside a clip frame (clipped, as it shows) the picture splits too.
        let mut doc = picture_doc(1, &[255, 0, 0, 255], None);
        let pic = doc.pages[0].layers[0].shapes.remove(0);
        let id = doc.ids_mut().shape();
        let frame = tracedraw_core::Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 5.0, 5.0),
                radius: 0.0,
                corners: None,
            },
        );
        let id = doc.ids_mut().shape();
        doc.pages[0].layers[0]
            .shapes
            .push(tracedraw_core::Shape::new(
                id,
                ShapeKind::ClipFrame {
                    frame: Box::new(frame),
                    contents: vec![pic],
                },
            ));
        let inks: Vec<u8> = plate_pixels(&doc).iter().map(|p| ink(&p[0])).collect();
        assert!(inks[0] < 26 && inks[2] > 230, "{inks:?}");
    }

    /// File > Export of the current page, every page or a list of pages:
    /// one numbered file per page for the formats of one page, one file
    /// with every page chosen for PDF, its text as curves when asked
    /// (#1382).
    #[test]
    fn export_writes_the_pages_chosen() {
        let mut app = App::headless();
        // Three pages, each with a rectangle of its own width.
        for k in 0..3 {
            if k > 0 {
                app.add_page();
            }
            app.new_shape(ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 20.0 + 10.0 * k as f64, 30.0),
                radius: 0.0,
                corners: None,
            });
        }
        let ids: Vec<_> = app.doc().pages.iter().map(|p| p.id).collect();
        app.page = ids[1];
        let dir = std::env::temp_dir().join(format!("tracedraw-1382-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let files = |dir: &Path| {
            let mut names: Vec<String> = std::fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
                .collect();
            names.sort();
            names
        };
        let png = |range: (bool, bool, &str)| ExportState {
            format: 2,
            dpi: 25.4,
            all_pages: range.0,
            page_list: range.1,
            pages: range.2.into(),
            transparent: false,
            ..ExportState::default()
        };
        // The current page: one file under the name given.
        let one = dir.join("one");
        std::fs::create_dir_all(&one).unwrap();
        export(&mut app, &one.join("a.png"), &png((false, false, ""))).expect("current");
        assert_eq!(files(&one), ["a.png"]);
        let img = image::open(one.join("a.png")).unwrap();
        assert_eq!(img.width(), 210, "page 2, 25.4 dpi: one pixel per mm");
        // Every page: numbered files.
        let all = dir.join("all");
        std::fs::create_dir_all(&all).unwrap();
        let msg = export(&mut app, &all.join("a.png"), &png((true, false, ""))).expect("all");
        assert_eq!(files(&all), ["a-1.png", "a-2.png", "a-3.png"]);
        assert!(msg.contains('3'), "{msg}");
        // A list: the pages listed, numbered by their page.
        let list = dir.join("list");
        std::fs::create_dir_all(&list).unwrap();
        export(&mut app, &list.join("a.png"), &png((true, true, "3, 1"))).expect("list");
        assert_eq!(files(&list), ["a-1.png", "a-3.png"]);
        // Page 3's rectangle is 40 mm wide, page 1's 20 mm: each file
        // shows its own page.
        let dark = |p: &Path| {
            let img = image::open(p).unwrap().to_rgb8();
            let y = img.height() - 20;
            let inked: Vec<u32> = (0..img.width())
                .filter(|x| img.get_pixel(*x, y).0 != [255, 255, 255])
                .collect();
            inked.last().unwrap_or(&0) - inked.first().unwrap_or(&0)
        };
        assert!(dark(&list.join("a-3.png")) > dark(&list.join("a-1.png")) + 10);
        // SVG too writes one file per page.
        let svg = dir.join("svg");
        std::fs::create_dir_all(&svg).unwrap();
        let st = ExportState {
            format: 0,
            ..png((true, true, "2-3"))
        };
        export(&mut app, &svg.join("b.svg"), &st).expect("svg");
        assert_eq!(files(&svg), ["b-2.svg", "b-3.svg"]);
        // PDF: one file holding the pages chosen.
        let pdf = dir.join("pdf");
        std::fs::create_dir_all(&pdf).unwrap();
        let st = ExportState {
            format: 1,
            ..png((true, true, "1, 3"))
        };
        export(&mut app, &pdf.join("c.pdf"), &st).expect("pdf");
        assert_eq!(files(&pdf), ["c.pdf"]);
        let text = String::from_utf8_lossy(&std::fs::read(pdf.join("c.pdf")).unwrap()).to_string();
        assert!(text.contains("/Count 2"), "two pages");
        let st = ExportState {
            format: 1,
            ..png((true, false, ""))
        };
        export(&mut app, &pdf.join("c.pdf"), &st).expect("pdf");
        let text = String::from_utf8_lossy(&std::fs::read(pdf.join("c.pdf")).unwrap()).to_string();
        assert!(text.contains("/Count 3"), "every page");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Facing pages (#1972): Export > Spreads makes each spread one page
    /// as wide as both, page 1 alone; every page on its own carries
    /// what its facing page holds across the spine, in exports and in
    /// print; a PDF of the spreads has a page per spread.
    #[test]
    fn facing_pages_export_spreads_or_pages_with_the_spine_crossing() {
        let mut app = App::headless();
        for _ in 0..2 {
            app.add_page();
        }
        app.page = app.doc().pages[1].id;
        let size = app.doc().pages[1].size;
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(size.width - 20.0, 10.0, size.width + 30.0, 40.0),
            radius: 0.0,
            corners: None,
        });
        let st = ExportState {
            format: 1,
            all_pages: true,
            ..ExportState::default()
        };
        let objects = |d: &Document, k: usize| -> usize {
            d.pages[k].layers.iter().map(|l| l.shapes.len()).sum()
        };
        // Facing pages off: as before.
        let (plain, _) = export_document(&app, &st);
        assert_eq!(plain.pages.len(), 3);
        assert_eq!(objects(&plain, 2), 0);
        app.set_facing_pages(true, tracedraw_core::spreads::PageSide::Right);
        // Every page alone: page 3 carries the rectangle's part on it.
        let (pages, _) = export_document(&app, &st);
        assert_eq!(pages.pages.len(), 3);
        assert_eq!(objects(&pages, 2), 1);
        assert_eq!(objects(&pages, 0), 0);
        let printed = print_sheets(&app, &PrintState::default());
        assert_eq!(printed.pages.len(), 3);
        // Spreads: page 1, then pages 2-3 as one page twice as wide.
        let spreads = ExportState {
            spreads: true,
            ..st.clone()
        };
        let (doc, _) = export_document(&app, &spreads);
        assert_eq!(doc.pages.len(), 2);
        assert_eq!(doc.pages[0].size, size);
        assert!((doc.pages[1].size.width - 2.0 * size.width).abs() < 1e-9);
        assert_eq!(objects(&doc, 1), 1);
        let dir = std::env::temp_dir().join(format!("td-spreads-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.pdf");
        export(&mut app, &path, &spreads).expect("pdf");
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).to_string();
        assert!(text.contains("/Count 2"), "a page per spread");
        // One file per spread for a format of one page.
        let png = ExportState {
            format: 2,
            dpi: 20.0,
            ..spreads.clone()
        };
        export(&mut app, &dir.join("s.png"), &png).expect("png");
        assert!(dir.join("s-1.png").exists() && dir.join("s-2.png").exists());
        assert!(!dir.join("s-3.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The pages an export takes: the current one, every page, or the
    /// list in its order (every page when it names none); the selection
    /// comes from the current page (#1382).
    #[test]
    fn export_page_choices_pick_pages() {
        let mut st = ExportState::default();
        assert_eq!(st.page_indexes(4, 2), vec![2]);
        st.all_pages = true;
        assert_eq!(st.page_indexes(4, 2), vec![0, 1, 2, 3]);
        st.page_list = true;
        st.pages = "4, 1-2".into();
        assert_eq!(st.page_indexes(4, 2), vec![3, 0, 1]);
        st.pages = "9".into();
        assert_eq!(st.page_indexes(4, 2), vec![0, 1, 2, 3]);
        st.selection_only = true;
        assert_eq!(st.page_indexes(4, 2), vec![2]);
        assert!(ExportState::one_file_for_all_pages("pdf"));
        assert!(!ExportState::one_file_for_all_pages("svg"));
        assert!(!ExportState::one_file_for_all_pages("png"));
        assert_eq!(
            numbered_path(Path::new("/x/name.jpeg"), 3, "jpg"),
            Path::new("/x/name-3.jpeg")
        );
        assert_eq!(
            numbered_path(Path::new("/x/name"), 2, "png"),
            Path::new("/x/name-2.png")
        );
    }

    /// "AI" writes the PDF-compatible file by default, and the classic
    /// version 8 file (PostScript, one page per file) when chosen, for
    /// programs that read only those (#1728).
    #[test]
    fn export_ai_writes_pdf_compatible_or_classic_version_8() {
        let mut app = App::headless();
        for k in 0..2 {
            if k > 0 {
                app.add_page();
            }
            app.new_shape(ShapeKind::Rect {
                rect: Rect::new(10.0, 10.0, 40.0, 30.0),
                radius: 0.0,
                corners: None,
            });
        }
        let dir = std::env::temp_dir().join(format!("tracedraw-1728-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ai = EXPORT_FORMATS.iter().position(|f| f.1 == "ai").unwrap();
        let st = |version: usize| ExportState {
            format: ai,
            all_pages: true,
            ai_version: version,
            title: "Logo (curves)".into(),
            ..ExportState::default()
        };
        // The default: one PDF-compatible file with both pages, as before.
        assert!(!ExportState::default().classic_ai());
        export(&mut app, &dir.join("pdf.ai"), &st(0)).expect("pdf-compatible");
        let bytes = std::fs::read(dir.join("pdf.ai")).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        // Classic: one PostScript file per page, each with the AI header.
        assert!(st(1).classic_ai() && !st(1).one_file_holds_the_pages("ai"));
        export(&mut app, &dir.join("old.ai"), &st(1)).expect("classic");
        for name in ["old-1.ai", "old-2.ai"] {
            let text = std::fs::read_to_string(dir.join(name)).expect(name);
            assert!(text.starts_with("%!PS-Adobe-3.0\n"), "{name}");
            assert!(text.contains("\n%%AI8_CreatorVersion: 8.0\n"));
            assert!(text.contains("\n%AI5_FileFormat 4.0\n"));
            assert!(text.contains("\n%%BoundingBox: 0 0 596 842\n"));
            assert!(text.contains("\n%%Title: (Logo \\(curves\\))\n"), "{text}");
        }
        // It opens again as a PostScript-based .ai, with the rectangle.
        assert_eq!(App::file_kind(&dir.join("old-1.ai")), "eps");
        let mut ids = tracedraw_core::id::IdSource::default();
        let bytes = std::fs::read(dir.join("old-1.ai")).unwrap();
        let back = tracedraw_io::eps_import::parse(&bytes, &mut ids).expect("reads");
        assert!(!back.shapes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// SVG's Text choice (#2289): as curves by default, as `<text>`
    /// naming the font, or with the font file inside.
    #[test]
    fn export_svg_text_as_curves_text_or_with_fonts() {
        tracedraw_text::install();
        let fonts = tracedraw_text::fonts();
        let Some(family) = ["DejaVu Sans", "Liberation Sans", "Noto Sans"]
            .into_iter()
            .find(|f| fonts.has_family(f))
        else {
            return;
        };
        let mut app = App::headless();
        app.new_shape(ShapeKind::Text {
            spans: vec![TextSpan::new("Editable words", family, 24.0)],
            origin: tracedraw_core::Point::new(20.0, 50.0),
            frame: None,
            align: Default::default(),
            para: Default::default(),
            on_path: None,
        });
        let dir = std::env::temp_dir().join(format!("tracedraw-2289-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.svg");
        let mut st = ExportState {
            format: 0,
            ..ExportState::default()
        };
        let mut out = Vec::new();
        for choice in 0..3 {
            st.svg_text = choice;
            export(&mut app, &path, &st).expect("svg");
            out.push(std::fs::read_to_string(&path).unwrap());
        }
        assert!(!out[0].contains("<text"), "{}", out[0]);
        assert!(out[1].contains(">Editable words</tspan>"), "{}", out[1]);
        assert!(out[1].contains(family), "{}", out[1]);
        assert!(!out[1].contains("@font-face"));
        assert!(out[2].contains("@font-face"));
        assert!(out[2].contains(">Editable words</tspan>"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Export to PDF with text as curves holds no font (#1382).
    #[test]
    fn export_pdf_text_as_curves_holds_no_font() {
        tracedraw_text::install();
        let fonts = tracedraw_text::fonts();
        let Some(family) = ["DejaVu Sans", "Liberation Sans", "Noto Sans"]
            .into_iter()
            .find(|f| fonts.has_family(f))
        else {
            return;
        };
        let mut app = App::headless();
        app.new_shape(ShapeKind::Text {
            spans: vec![TextSpan::new("Searchable words", family, 24.0)],
            origin: tracedraw_core::Point::new(20.0, 50.0),
            frame: None,
            align: Default::default(),
            para: Default::default(),
            on_path: None,
        });
        let dir = std::env::temp_dir().join(format!("tracedraw-1382c-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.pdf");
        let mut st = ExportState {
            format: 1,
            ..ExportState::default()
        };
        export(&mut app, &path, &st).expect("pdf");
        let live = std::fs::read(&path).unwrap();
        st.text_as_curves = true;
        export(&mut app, &path, &st).expect("pdf");
        let curves = std::fs::read(&path).unwrap();
        let words = |bytes: &[u8]| {
            lopdf::Document::load_mem(bytes)
                .ok()
                .and_then(|d| d.extract_text(&[1]).ok())
                .unwrap_or_default()
        };
        assert!(String::from_utf8_lossy(&live).contains("/FontDescriptor"));
        assert!(words(&live).contains("Searchable"), "live text");
        assert!(
            !String::from_utf8_lossy(&curves).contains("/Font"),
            "no font"
        );
        assert!(!words(&curves).contains("Searchable"), "curves");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_export_preview_shows_the_page_and_estimates_the_size() {
        let mut app = App::headless();
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(10.0, 10.0, 100.0, 60.0),
            radius: 0.0,
            corners: None,
        });
        // PNG at 300 dpi: an A4 preview keeps the page's proportions.
        let st = ExportState {
            format: 2,
            dpi: 300.0,
            ..ExportState::default()
        };
        let pm = preview(&app, &st, 200).expect("preview");
        assert_eq!(pm.height(), 200);
        assert!((pm.width() as f64 - 200.0 * 210.0 / 297.0).abs() <= 1.0);
        let png = estimated_size(&app, &st, &pm).expect("estimate");
        assert!(png > 0);
        // The same page at 72 dpi is estimated smaller.
        let small = ExportState {
            dpi: 72.0,
            ..st.clone()
        };
        assert!(estimated_size(&app, &small, &pm).unwrap_or(usize::MAX) < png);
        // A vector format is measured, not estimated.
        let svg = ExportState {
            format: 0,
            ..st.clone()
        };
        assert!(estimated_size(&app, &svg, &pm).is_some());
        assert!(keeps_alpha("png") && !keeps_alpha("jpg"));
    }

    /// #3039: a page drawn and written a strip at a time is the same
    /// picture as the page drawn whole, as PNG (on white or transparent)
    /// and TIFF, with the resolution recorded; blurred shadows that reach
    /// across strips included.
    #[test]
    fn a_picture_written_a_strip_at_a_time_is_the_page_drawn_whole() {
        use tracedraw_core::{Color, Fill};
        let mut app = App::headless();
        let circle = app
            .new_shape(ShapeKind::Ellipse {
                rect: Rect::new(20.0, 20.0, 120.0, 90.0),
                arc: None,
            })
            .expect("circle");
        app.run(Command::SetFill {
            shapes: vec![circle],
            fill: Fill::Solid(Color::rgb8(220, 0, 30)),
        });
        app.run(Command::SetShadow {
            shapes: vec![circle],
            shadow: Some(tracedraw_core::Shadow {
                offset: tracedraw_core::geometry::Vec2::new(6.0, -6.0),
                opacity: 0.8,
                blur: 3.0,
                ..Default::default()
            }),
        });
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(60.0, 100.0, 180.0, 160.0),
            radius: 4.0,
            corners: None,
        });
        let (doc, _) = export_document(&app, &ExportState::default());
        let pid = doc.pages[0].id;
        for (format, ext, transparent) in [(2, "png", false), (2, "png", true), (5, "tif", false)] {
            let st = ExportState {
                format,
                dpi: 40.0,
                transparent,
                ..ExportState::default()
            };
            let whole = page_file(&doc, 0, ext, &st, None).expect(ext);
            let p = doc.page(pid).unwrap();
            let fit = tracedraw_render::ViewTransform::whole_page(
                p.size.width,
                p.size.height,
                st.render_dpi() / 25.4,
            );
            // Strips of 16 rows: many of them, each blurred shadow drawn
            // across several.
            let strips = streamed_picture(&doc, pid, fit, ext, transparent, st.dpi, 16).expect(ext);
            let a = image::load_from_memory(&whole).expect(ext).to_rgba8();
            let b = image::load_from_memory(&strips).expect(ext).to_rgba8();
            assert_eq!(a.dimensions(), (fit.0, fit.1), "{ext}");
            assert_eq!(a.dimensions(), b.dimensions(), "{ext}");
            // Only the smoothing of some edge pixels may differ a little,
            // as between the bands the page is drawn in on several cores.
            let diff = |x: &image::Rgba<u8>, y: &image::Rgba<u8>| {
                x.0.iter()
                    .zip(y.0)
                    .map(|(u, v)| u.abs_diff(v))
                    .max()
                    .unwrap_or(0)
            };
            let worst = a.pixels().zip(b.pixels()).map(|(x, y)| diff(x, y)).max();
            let off = a
                .pixels()
                .zip(b.pixels())
                .filter(|(x, y)| diff(x, y) > 2)
                .count();
            assert!(
                worst <= Some(32),
                "{ext} transparent {transparent}: {worst:?}"
            );
            assert!(
                off * 100 < a.pixels().len(),
                "{ext} transparent {transparent}: {off} pixels differ"
            );
            assert!(
                a.pixels().any(|q| q.0 == [220, 0, 30, 255]),
                "{ext}: the circle"
            );
            if ext == "png" {
                let (x, y) = crate::image_dpi::resolution(&strips).expect("dpi");
                assert!((x - 40.0).abs() < 0.1 && (y - 40.0).abs() < 0.1, "{x} {y}");
            }
        }
        assert!(too_large(1, 2).contains("PNG or TIFF"));
    }

    /// A bitmap export records the resolution of the dialog, and the
    /// scale changes how many pixels it has but not that resolution, so it
    /// opens at the size chosen (#617).
    #[test]
    fn bitmap_exports_record_their_resolution_and_scale() {
        let mut app = App::headless();
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(10.0, 10.0, 100.0, 60.0),
            radius: 0.0,
            corners: None,
        });
        let b = app.page_rect();
        let dir = std::env::temp_dir().join(format!("tracedraw-617-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (format, ext) in [(2, "png"), (3, "jpg"), (4, "bmp")] {
            let st = ExportState {
                format,
                dpi: 50.0,
                scale: 2.0,
                transparent: false,
                ..ExportState::default()
            };
            let path = dir.join(format!("out.{ext}"));
            export(&mut app, &path, &st).expect(ext);
            let bytes = std::fs::read(&path).unwrap();
            let (x, y) = crate::image_dpi::resolution(&bytes).expect(ext);
            assert!(
                (x - 50.0).abs() < 0.1 && (y - 50.0).abs() < 0.1,
                "{ext}: {x} {y}"
            );
            // Still a file every reader opens, twice the pixels of 50 dpi.
            let img = image::load_from_memory(&bytes).expect(ext);
            let want = crate::ui::dialogs::export_pixels(b.width(), b.height(), 100.0);
            assert_eq!((img.width(), img.height()), want, "{ext}");
        }
        let _ = std::fs::remove_dir_all(&dir);
        // Files not laid out as expected come back as they were.
        assert_eq!(record_dpi(vec![1, 2, 3], "png", 300.0), vec![1, 2, 3]);
        assert_eq!(record_dpi(b"BM".to_vec(), "bmp", 300.0), b"BM".to_vec());
        assert_eq!(
            record_dpi(vec![0xFF, 0xD8], "jpg", f64::NAN),
            vec![0xFF, 0xD8]
        );
    }

    /// The pixel at `(x, y)` of an uncompressed chunky TIFF, read with
    /// the fields of TIFF 6.0 (image width, strip offsets, rows per
    /// strip, samples per pixel); also the photometric interpretation.
    fn tiff_pixel(b: &[u8], x: usize, y: usize) -> (Vec<u8>, u32) {
        let le16 = |at: usize| u16::from_le_bytes([b[at], b[at + 1]]) as u32;
        let le32 = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        assert_eq!(&b[..4], b"II*\0");
        let ifd = le32(4) as usize;
        let field = |tag: u32| -> Vec<u32> {
            for i in 0..le16(ifd) as usize {
                let e = ifd + 2 + 12 * i;
                if le16(e) == tag {
                    let (kind, n) = (le16(e + 2), le32(e + 4) as usize);
                    let size = if kind == 3 { 2 } else { 4 };
                    let at = if size * n <= 4 {
                        e + 8
                    } else {
                        le32(e + 8) as usize
                    };
                    return (0..n)
                        .map(|k| {
                            if size == 2 {
                                le16(at + 2 * k)
                            } else {
                                le32(at + 4 * k)
                            }
                        })
                        .collect();
                }
            }
            panic!("no tag {tag}");
        };
        let (w, spp, rows) = (field(256)[0], field(277)[0], field(278)[0]);
        let offsets = field(273);
        let row = (w * spp) as usize;
        let strip = y / rows as usize;
        let at = offsets[strip] as usize + (y % rows as usize) * row + x * spp as usize;
        (b[at..at + spp as usize].to_vec(), field(262)[0])
    }

    /// File > Export to JPEG or TIFF in CMYK (#1763): a rectangle filled
    /// with pure cyan comes out as cyan ink alone, the paper as no ink,
    /// an RGB red through the colour management as magenta and yellow;
    /// RGB exports are as before, Grayscale writes one channel, and the
    /// drawing's CMYK profile goes into the files.
    #[test]
    fn cmyk_bitmap_exports_keep_the_inks() {
        use tracedraw_core::{Color, Fill};
        let mut app = App::headless();
        for (rect, color) in [
            (
                Rect::new(10.0, 10.0, 100.0, 100.0),
                Color::cmyk8(255, 0, 0, 0),
            ),
            (Rect::new(110.0, 10.0, 200.0, 100.0), Color::rgb8(255, 0, 0)),
        ] {
            app.new_shape_styled(
                ShapeKind::Rect {
                    rect,
                    radius: 0.0,
                    corners: None,
                },
                |s| {
                    s.fill = Fill::Solid(color);
                    s.stroke = None;
                },
            );
        }
        let mut doc = app.doc().clone();
        let profile = tracedraw_core::icc::test_profiles::lut16_cmyk_profile();
        doc.cmyk_profile = Some(tracedraw_core::DocumentProfile::new(
            "Press",
            profile.clone(),
        ));
        app.engine.replace(doc);
        let dir = std::env::temp_dir().join(format!("tracedraw-1763-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let st = |format: usize, color_mode: usize| ExportState {
            format,
            dpi: 25.4,
            quality: 95,
            color_mode,
            transparent: false,
            ..ExportState::default()
        };
        // Page coordinates run up from the bottom: the rectangles lie
        // near the bottom of the 297 mm page, one pixel per mm.
        let (cyan_at, red_at, paper_at) = ((55, 297 - 55), (155, 297 - 55), (55, 150));

        // CMYK JPEG, read by the decoder TraceDraw opens files with.
        let jpg = dir.join("cmyk.jpg");
        export(&mut app, &jpg, &st(3, 1)).expect("cmyk jpeg");
        let bytes = std::fs::read(&jpg).unwrap();
        assert!(bytes.windows(5).any(|w| w == b"Adobe"), "Adobe marker");
        let sof = bytes
            .windows(2)
            .position(|w| w == [0xFF, 0xC0])
            .expect("SOF0");
        assert_eq!(bytes[sof + 9], 4, "four components");
        let icc_at = bytes
            .windows(12)
            .position(|w| w == b"ICC_PROFILE\0")
            .expect("profile embedded");
        assert_eq!(&bytes[icc_at + 14..icc_at + 14 + 64], &profile[..64]);
        assert_eq!(
            crate::image_dpi::resolution(&bytes).map(|(x, _)| x.round()),
            Some(25.0)
        );
        let img = image::load_from_memory(&bytes).expect("decodes").to_rgb8();
        assert_eq!((img.width(), img.height()), (210, 297));
        // The decoder shows (255 - ink) x (255 - black): only no red, full
        // green and blue is cyan 100 with magenta, yellow and black 0.
        let p = img.get_pixel(cyan_at.0, cyan_at.1).0;
        assert!(p[0] < 6 && p[1] > 249 && p[2] > 249, "cyan: {p:?}");
        let p = img.get_pixel(paper_at.0, paper_at.1).0;
        assert!(p.iter().all(|v| *v > 249), "paper: {p:?}");

        // CMYK TIFF: the inks as they are.
        let tif = dir.join("cmyk.tif");
        export(&mut app, &tif, &st(5, 1)).expect("cmyk tiff");
        let bytes = std::fs::read(&tif).unwrap();
        let (cyan, photometric) = tiff_pixel(&bytes, cyan_at.0 as usize, cyan_at.1 as usize);
        assert_eq!(photometric, 5, "separated");
        assert!(
            cyan[0] > 252 && cyan[1..].iter().all(|v| *v < 3),
            "{cyan:?}"
        );
        let (paper, _) = tiff_pixel(&bytes, paper_at.0 as usize, paper_at.1 as usize);
        assert_eq!(paper, [0, 0, 0, 0]);
        let (red, _) = tiff_pixel(&bytes, red_at.0 as usize, red_at.1 as usize);
        let want = match Color::rgb8(255, 0, 0).convert_to("CMYK") {
            Color::Cmyk { c, m, y, k } => [c, m, y, k].map(|v| (v * 255.0).round() as i32),
            other => panic!("{other:?}"),
        };
        for (got, want) in red.iter().zip(want) {
            assert!((*got as i32 - want).abs() <= 2, "red: {red:?} {want:?}");
        }
        assert!(
            red[1] > 200 && red[2] > 200,
            "red prints magenta and yellow"
        );
        assert!(bytes.windows(64).any(|w| w == &profile[..64]), "profile");
        assert_eq!(
            crate::image_dpi::resolution(&bytes).map(|(x, _)| x.round()),
            Some(25.0)
        );

        // RGB, the default: JFIF with three components, as before.
        assert_eq!(ExportState::default().color_mode, 0);
        let rgb = dir.join("rgb.jpg");
        export(&mut app, &rgb, &st(3, 0)).expect("rgb jpeg");
        let bytes = std::fs::read(&rgb).unwrap();
        assert_eq!(&bytes[6..11], b"JFIF\0");
        assert!(!bytes.windows(5).any(|w| w == b"Adobe"));
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!(img.color(), image::ColorType::Rgb8);
        let p = img.to_rgb8().get_pixel(cyan_at.0, cyan_at.1).0;
        let want = Color::cmyk8(255, 0, 0, 0).to_rgb8();
        for (a, b) in p.iter().zip(want) {
            assert!((*a as i32 - b as i32).abs() < 6, "{p:?} {want:?}");
        }
        let rgb_tif = dir.join("rgb.tif");
        export(&mut app, &rgb_tif, &st(5, 0)).expect("rgb tiff");
        let (_, photometric) = tiff_pixel(&std::fs::read(&rgb_tif).unwrap(), 0, 0);
        assert_eq!(photometric, 2, "RGB TIFF as before");

        // Grayscale: one channel, the colour's grey.
        for (format, ext) in [(3, "jpg"), (5, "tif")] {
            let path = dir.join(format!("gray.{ext}"));
            export(&mut app, &path, &st(format, 2)).expect(ext);
            let img = image::open(&path).unwrap();
            assert_eq!(img.color(), image::ColorType::L8, "{ext}");
            let v = img.to_luma8().get_pixel(cyan_at.0, cyan_at.1).0[0];
            let want = Color::cmyk8(255, 0, 0, 0).convert_to("Grayscale");
            let want = match want {
                Color::Gray { v } => (v * 255.0).round() as i32,
                other => panic!("{other:?}"),
            };
            assert!((v as i32 - want).abs() < 6, "{ext}: {v} {want}");
        }
        // Other formats ignore the colour mode.
        let png = dir.join("x.png");
        export(&mut app, &png, &st(2, 1)).expect("png");
        assert!(matches!(
            image::open(&png).unwrap().color(),
            image::ColorType::Rgb8 | image::ColorType::Rgba8
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The bitmap size in the Export dialog: typed in pixels or in a
    /// length it sets the scale, the resolution staying (#2640: pixels
    /// set the resolution); the other side follows and nonsense is
    /// ignored (#617).
    #[test]
    fn the_export_size_is_typed_in_pixels_or_lengths() {
        use crate::ui::dialogs::{export_size_in_unit, set_export_size};
        let mut st = ExportState {
            dpi: 100.0,
            size_unit: 3,
            ..ExportState::default()
        };
        let near =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
        assert!(near(export_size_in_unit(254.0, 127.0, &st), (10.0, 5.0)));
        set_export_size(&mut st, 254.0, 20.0);
        assert!((st.scale - 2.0).abs() < 1e-9 && st.dpi == 100.0);
        assert!(near(export_size_in_unit(254.0, 127.0, &st), (20.0, 10.0)));
        assert!((st.render_dpi() - 200.0).abs() < 1e-9);
        st.size_unit = 1;
        assert!(near(export_size_in_unit(254.0, 127.0, &st), (508.0, 254.0)));
        st.size_unit = 0;
        assert!(near(
            export_size_in_unit(254.0, 127.0, &st),
            (2000.0, 1000.0)
        ));
        set_export_size(&mut st, 254.0, 1000.0);
        assert!(st.dpi == 100.0 && (st.scale - 1.0).abs() < 1e-6, "{st:?}");
        assert!(near(
            export_size_in_unit(254.0, 127.0, &st),
            (1000.0, 500.0)
        ));
        // Never past the highest resolution, and nonsense changes nothing.
        st.size_unit = 3;
        set_export_size(&mut st, 254.0, 1.0e9);
        assert!(st.render_dpi() <= 2400.0 + 1e-6);
        let before = st.clone();
        for typed in [0.0, -5.0, f64::NAN, f64::INFINITY] {
            set_export_size(&mut st, 254.0, typed);
        }
        set_export_size(&mut st, 0.0, 10.0);
        assert_eq!(st, before);
        st.scale = f64::NAN;
        assert_eq!(st.scale_or_one(), 1.0);
    }

    /// A resolution typed or picked in the Export dialog leaves the size
    /// shown: in pixels as many pixels come out, at the new resolution;
    /// in a length the printed size stays and the pixels follow. A size
    /// typed leaves the resolution (#2640).
    #[test]
    fn the_export_resolution_and_size_change_one_at_a_time() {
        use crate::ui::dialogs::{export_size_in_unit, set_export_dpi, set_export_size};
        let mut st = ExportState {
            dpi: 300.0,
            size_unit: 0,
            ..ExportState::default()
        };
        // An A4 page: 2481 x 3508 pixels at 300 dpi.
        let (w, h) = (210.0, 297.0);
        let before = export_size_in_unit(w, h, &st);
        assert_eq!(before, (2481.0, 3508.0));
        set_export_dpi(&mut st, 72.0);
        assert_eq!(st.dpi, 72.0);
        assert_eq!(export_size_in_unit(w, h, &st), before, "{st:?}");
        // A size typed: the resolution stays.
        set_export_size(&mut st, w, 1000.0);
        assert_eq!(st.dpi, 72.0);
        assert_eq!(export_size_in_unit(w, h, &st).0, 1000.0);
        // In millimetres the printed size stays and the pixels follow.
        st = ExportState {
            dpi: 300.0,
            size_unit: 1,
            ..ExportState::default()
        };
        set_export_dpi(&mut st, 150.0);
        assert_eq!(export_size_in_unit(w, h, &st), (210.0, 297.0));
        assert!((st.render_dpi() - 150.0).abs() < 1e-9);
        // Nonsense changes nothing.
        let kept = st.clone();
        for d in [0.0, -1.0, f64::NAN] {
            set_export_dpi(&mut st, d);
        }
        assert_eq!(st, kept);
    }

    #[test]
    fn colours_are_mapped_inside_groups_frames_and_text() {
        use tracedraw_core::{document::TextSpan, Color, Fill, Rect, Shape, ShapeId};
        let red = Color::Rgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        };
        let mut inner = Shape::new(
            ShapeId(2),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 1.0, 1.0),
                radius: 0.0,
                corners: None,
            },
        );
        inner.fill = Fill::Solid(red);
        let mut span = TextSpan::new("a", "Inter", 10.0);
        span.fill = Some(Fill::Solid(red));
        let text = Shape::new(
            ShapeId(3),
            ShapeKind::Text {
                spans: vec![span],
                origin: tracedraw_core::Point::ZERO,
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            },
        );
        let group = Shape::new(
            ShapeId(4),
            ShapeKind::Group {
                children: vec![inner, text],
            },
        );
        let frame = Shape::new(
            ShapeId(5),
            ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 2.0, 2.0),
                radius: 0.0,
                corners: None,
            },
        );
        let mut clip = Shape::new(
            ShapeId(1),
            ShapeKind::ClipFrame {
                frame: Box::new(frame),
                contents: vec![group],
            },
        );
        let green = Color::Gray { v: 0.5 };
        map_colors(&mut clip, &|_| green);
        let ShapeKind::ClipFrame { contents, frame } = &clip.kind else {
            panic!("clip frame");
        };
        assert_eq!(frame.stroke.as_ref().map(|s| s.color), Some(green));
        let ShapeKind::Group { children } = &contents[0].kind else {
            panic!("group");
        };
        assert_eq!(children[0].fill, Fill::Solid(green));
        let ShapeKind::Text { spans, .. } = &children[1].kind else {
            panic!("text");
        };
        assert_eq!(spans[0].fill, Some(Fill::Solid(green)));
    }

    #[test]
    fn page_lists_and_ranges_pick_pages() {
        use crate::ui::dialogs::parse_page_list;
        assert_eq!(parse_page_list("1-3, 5", 6), vec![0, 1, 2, 4]);
        assert_eq!(parse_page_list("4-", 5), vec![3, 4]);
        assert_eq!(parse_page_list("9, x, 2", 3), vec![1]);
        let mut st = PrintState::default();
        assert_eq!(st.page_indexes(3, 1), vec![0, 1, 2]);
        st.range = 1;
        assert_eq!(st.page_indexes(3, 1), vec![1]);
        st.range = 2;
        st.pages = "3".into();
        assert_eq!(st.page_indexes(3, 0), vec![2]);
        // Odd or even pages of the document or the list (#951); the
        // current page stays, and a choice leaving no page is let go.
        st.parity = 1;
        assert_eq!(st.page_indexes(3, 0), vec![2]);
        st.pages = "1-6".into();
        assert_eq!(st.page_indexes(6, 0), vec![0, 2, 4]);
        st.parity = 2;
        assert_eq!(st.page_indexes(6, 0), vec![1, 3, 5]);
        st.pages = "3".into();
        assert_eq!(st.page_indexes(6, 0), vec![2], "nothing left: as listed");
        st.range = 0;
        assert_eq!(st.page_indexes(5, 0), vec![1, 3]);
        st.range = 1;
        assert_eq!(st.page_indexes(5, 0), vec![0]);
    }

    /// A drop shadow prints: the print PDF holds its picture with its soft
    /// mask, as the exported PDF does (#1029).
    #[test]
    fn a_drop_shadow_prints() {
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(20.0, 20.0, 60.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        let pdf_text = |app: &App| {
            let bytes =
                tracedraw_io::pdf::document_to_pdf(&print_sheets(app, &PrintState::default()));
            String::from_utf8_lossy(&bytes).into_owned()
        };
        assert!(!pdf_text(&app).contains("/SMask"));
        app.run(tracedraw_core::Command::SetShadow {
            shapes: vec![id],
            shadow: Some(tracedraw_core::Shadow::default()),
        });
        let text = pdf_text(&app);
        assert!(text.contains("/Subtype /Image"), "the shadow's picture");
        assert!(text.contains("/SMask"), "with its transparency");
        assert!(text.contains("/BM /Multiply"), "multiplied");
    }

    /// Black text prints black, and a negative (Invert) prints it as the
    /// clear part of a black sheet rather than white on white paper.
    #[test]
    fn black_text_prints_black_and_a_negative_blackens_the_sheet() {
        use tracedraw_core::{Color, Fill};
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Text {
                spans: vec![TextSpan::new("Card", "Sans", 24.0)],
                origin: tracedraw_core::geometry::Point::new(20.0, 100.0),
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            })
            .expect("text");
        let k100 = Color::cmyk_pct(0.0, 0.0, 0.0, 100.0);
        app.run(tracedraw_core::Command::SetFill {
            shapes: vec![id],
            fill: Fill::Solid(k100),
        });
        // The sheet may hold the page in groups or clip frames.
        fn texts(s: &tracedraw_core::Shape, out: &mut Vec<Fill>) {
            match &s.kind {
                ShapeKind::Text { .. } => out.push(s.fill.clone()),
                ShapeKind::Group { children } => children.iter().for_each(|c| texts(c, out)),
                ShapeKind::ClipFrame { frame, contents } => {
                    texts(frame, out);
                    contents.iter().for_each(|c| texts(c, out));
                }
                _ => {}
            }
        }
        let text_fill = |doc: &Document| -> Vec<Fill> {
            let mut out = Vec::new();
            for l in doc.all_layers() {
                l.shapes.iter().for_each(|s| texts(s, &mut out));
            }
            out
        };
        let st = PrintState::default();
        let sheets = print_sheets(&app, &st);
        assert_eq!(text_fill(&sheets), vec![Fill::Solid(k100)]);
        // The PDF as text with its zlib streams inflated.
        let bytes = tracedraw_io::pdf::document_to_pdf(&sheets);
        let mut pdf = String::from_utf8_lossy(&bytes).into_owned();
        let mut rest: &[u8] = &bytes;
        while let Some(i) = rest.windows(7).position(|w| w == b"stream\n") {
            let data = &rest[i + 7..];
            use std::io::Read;
            let mut out = Vec::new();
            if flate2::read::ZlibDecoder::new(data)
                .read_to_end(&mut out)
                .is_ok()
            {
                pdf.push_str(&String::from_utf8_lossy(&out));
            }
            rest = data;
        }
        assert!(pdf.contains("0 0 0 1 k"), "black ink in the PDF");
        let st = PrintState {
            invert: true,
            ..PrintState::default()
        };
        let sheets = print_sheets(&app, &st);
        match text_fill(&sheets).as_slice() {
            [Fill::Solid(c)] => assert!(c.to_rgb8().iter().all(|v| *v > 180), "{c:?}"),
            other => panic!("{other:?}"),
        }
        let first = &sheets.pages[0].layers[0].shapes[0];
        assert_eq!(first.fill, Fill::Solid(Color::BLACK));
        let b = first.bounds();
        let size = sheets.pages[0].size;
        assert!(b.x0 <= 0.0 && b.y0 <= 0.0 && b.x1 >= size.width && b.y1 >= size.height);
    }

    #[test]
    fn print_colour_options_change_the_sheets_objects() {
        use tracedraw_core::{Color, Fill};
        let spot = Color::Spot {
            ink: 9,
            tint: 0.5,
            c: 1.0,
            m: 0.0,
            y: 0.0,
            k: 0.0,
        };
        assert_eq!(
            spot_as_cmyk(spot),
            Color::Cmyk {
                c: 0.5,
                m: 0.0,
                y: 0.0,
                k: 0.0
            }
        );
        let mut doc = Document::new("t", Size::new(50.0, 50.0));
        let id = doc.ids_mut().shape();
        let mut text = tracedraw_core::Shape::new(
            id,
            ShapeKind::Text {
                spans: vec![TextSpan::new("a", "Arial", 12.0)],
                origin: tracedraw_core::geometry::Point::ZERO,
                frame: None,
                align: Default::default(),
                para: Default::default(),
                on_path: None,
            },
        );
        text.fill = Fill::Solid(Color::cmyk_pct(0.0, 100.0, 0.0, 0.0));
        text_in_black(&mut text);
        assert_eq!(text.fill, Fill::Solid(Color::BLACK));
        let mut black = text.clone();
        black.fill = Fill::Solid(Color::cmyk_pct(0.0, 0.0, 0.0, 100.0));
        overprint_black(&mut black);
        assert!(black.overprint_fill);
        let mut red = text;
        red.fill = Fill::Solid(Color::cmyk_pct(0.0, 100.0, 100.0, 0.0));
        overprint_black(&mut red);
        assert!(!red.overprint_fill);
    }

    #[test]
    fn copies_repeat_sheets_collated_or_not_and_ps_gets_its_level() {
        let mut doc = Document::new("t", Size::new(50.0, 50.0));
        let mut ids = doc.ids().clone();
        let mut second = doc.pages[0].clone();
        second.id = ids.page();
        second.name = "B".into();
        doc.set_ids(ids);
        doc.pages[0].name = "A".into();
        doc.pages.push(second);
        let mut a = doc.clone();
        repeat_copies(&mut a, 2, true);
        let names: Vec<_> = a.pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["A", "B", "A", "B"]);
        repeat_copies(&mut doc, 2, false);
        let names: Vec<_> = doc.pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["A", "A", "B", "B"]);
        let ids: std::collections::BTreeSet<_> = doc.pages.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 4);
        let ps = postscript_level("%!PS-Adobe-3.0\n%%EOF\n".into(), 3);
        assert!(ps.starts_with("%!PS-Adobe-3.0\n%%LanguageLevel: 3\n"));
    }

    /// #3374: with a printer's colour profile chosen, the composite
    /// colours and the pictures print as that profile's device values
    /// (here a linear RGB profile: sRGB 128 grey is about 55); none
    /// leaves them, and grayscale or separations pass it over.
    #[test]
    fn a_printer_profile_converts_the_printed_colours() {
        use tracedraw_core::icc::test_profiles::{curv_gamma, matrix_trc_profile};
        use tracedraw_core::{Color, Fill};
        let mut app = App::headless();
        let grey = Color::rgb8(128, 128, 128);
        app.new_shape_styled(
            ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 60.0, 40.0),
                radius: 0.0,
                corners: None,
            },
            |s| {
                s.name = Some("grey".into());
                s.fill = Fill::Solid(grey);
                s.stroke = None;
            },
        )
        .expect("rect");
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([128, 128, 128, 200]));
        app.new_shape(ShapeKind::Bitmap {
            rect: tracedraw_core::geometry::Rect::new(70.0, 10.0, 100.0, 30.0),
            width_px: 3,
            height_px: 2,
            png: crate::bitmap_fx::encode(&img).expect("png").into(),
            fx: None,
        })
        .expect("bitmap");
        let dir = std::env::temp_dir().join(format!("td-print-icc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("linear.icc");
        let g = || curv_gamma(1.0);
        std::fs::write(&path, matrix_trc_profile(g(), g(), g())).expect("profile");
        let printed = |st: &PrintState| {
            let doc = print_sheets(&app, st);
            fn all(shapes: &[tracedraw_core::Shape], out: &mut Vec<tracedraw_core::Shape>) {
                for s in shapes {
                    out.push(s.clone());
                    match &s.kind {
                        ShapeKind::Group { children } => all(children, out),
                        ShapeKind::ClipFrame { frame, contents } => {
                            all(std::slice::from_ref(&**frame), out);
                            all(contents, out);
                        }
                        _ => {}
                    }
                }
            }
            let mut shapes = Vec::new();
            for l in doc.pages.iter().flat_map(|p| p.layers.iter()) {
                all(&l.shapes, &mut shapes);
            }
            let rect = shapes
                .iter()
                .find(|s| s.name.as_deref() == Some("grey"))
                .expect("rect printed");
            let fill = match &rect.fill {
                Fill::Solid(c) => c.to_rgb8(),
                other => panic!("{other:?}"),
            };
            let px = shapes
                .iter()
                .find_map(|s| match &s.kind {
                    ShapeKind::Bitmap { png, .. } => crate::bitmap_fx::decode(png),
                    _ => None,
                })
                .expect("picture printed")
                .get_pixel(0, 0)
                .0;
            (fill, px)
        };
        let mut st = PrintState::default();
        assert_eq!(printed(&st), ([128, 128, 128], [128, 128, 128, 200]));
        st.output_profile = Some(path.display().to_string());
        let (fill, px) = printed(&st);
        for v in fill.into_iter().chain(px[..3].iter().copied()) {
            assert!((50..=60).contains(&v), "{fill:?} {px:?}");
        }
        assert_eq!(px[3], 200, "the alpha stays");
        st.grayscale = true;
        assert_eq!(printed(&st).0, [128, 128, 128]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Grayscale output (#1657): every colour on the sheets, an effect's
    /// too, prints as its grey and a picture as grey pixels; the drawing
    /// keeps its colours. Reverse order (#1394, #1571): the last sheet
    /// comes first.
    #[test]
    fn grayscale_and_reverse_order_change_the_printed_sheets() {
        use tracedraw_core::{Color, Fill};
        let mut app = App::headless();
        let size = app.doc().pages[0].size;
        let red = Color::rgb8(220, 20, 20);
        app.new_shape_styled(
            ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 60.0, 40.0),
                radius: 0.0,
                corners: None,
            },
            |s| {
                s.name = Some("first-page".into());
                s.fill = Fill::Solid(red);
                s.effects.push(tracedraw_core::live::Effect::BlockShadow {
                    offset: tracedraw_core::geometry::Vec2::new(2.0, -2.0),
                    color: Color::cmyk_pct(100.0, 0.0, 0.0, 0.0),
                    gap: 0.0,
                });
            },
        )
        .expect("rect");
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([220, 20, 20, 255]));
        app.new_shape(ShapeKind::Bitmap {
            rect: tracedraw_core::geometry::Rect::new(70.0, 10.0, 100.0, 30.0),
            width_px: 3,
            height_px: 2,
            png: crate::bitmap_fx::encode(&img).expect("png").into(),
            fx: None,
        })
        .expect("bitmap");
        app.run(tracedraw_core::Command::AddPage {
            name: None,
            size,
            layer: None,
        });
        app.page = app.doc().pages[1].id;
        app.new_shape_styled(
            ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 30.0, 30.0),
                radius: 0.0,
                corners: None,
            },
            |s| {
                s.name = Some("second-page".into());
                s.fill = Fill::Solid(Color::rgb8(20, 200, 20));
            },
        )
        .expect("rect");

        let json = |doc: &Document| serde_json::to_string(&doc.pages).expect("json");
        let models = |doc: &Document| {
            let text = json(doc);
            let mut found: Vec<String> = text
                .match_indices("\"model\":\"")
                .map(|(i, m)| {
                    text[i + m.len()..]
                        .split('"')
                        .next()
                        .unwrap_or_default()
                        .to_string()
                })
                .collect();
            found.sort();
            found.dedup();
            found
        };
        let pictures = |doc: &Document| {
            use base64::Engine;
            let text = json(doc);
            text.match_indices("\"png\":\"")
                .filter_map(|(i, m)| {
                    let b64 = text[i + m.len()..].split('"').next()?;
                    base64::engine::general_purpose::STANDARD.decode(b64).ok()
                })
                .collect::<Vec<_>>()
        };

        let plain = print_sheets(&app, &PrintState::default());
        assert!(models(&plain)
            .iter()
            .any(|m| m != "gray" && m != "registration"));
        let names: Vec<bool> = plain
            .pages
            .iter()
            .map(|p| {
                serde_json::to_string(p)
                    .expect("json")
                    .contains("first-page")
            })
            .collect();
        assert_eq!(names, [true, false], "in the drawing's order");

        let st = PrintState {
            grayscale: true,
            crop_marks: true,
            color_bars: true,
            ..PrintState::default()
        };
        let gray = print_sheets(&app, &st);
        for m in models(&gray) {
            assert!(m == "gray" || m == "registration", "{m} left in grayscale");
        }
        let pics = pictures(&gray);
        assert_eq!(pics.len(), 1);
        assert!(crate::bitmap_fx::png_is_gray(&pics[0]), "a grey picture");
        let px = image::load_from_memory(&pics[0]).expect("png").to_luma8();
        let want = (0.299f32 * 220.0 + 0.587 * 20.0 + 0.114 * 20.0).round() as i32;
        assert!((i32::from(px.get_pixel(1, 1).0[0]) - want).abs() <= 1);
        // The red fill as its luminance.
        assert!(json(&gray).contains("first-page"));
        assert!(
            models(&app.doc().clone()).iter().any(|m| m == "rgb"),
            "the drawing keeps its colours"
        );
        // Separations print from the colours, whatever the box says.
        let sep = print_sheets(
            &app,
            &PrintState {
                grayscale: true,
                separations: true,
                ..PrintState::default()
            },
        );
        assert!(models(&sep).iter().any(|m| m == "rgb"));

        let st = PrintState {
            reverse_order: true,
            ..PrintState::default()
        };
        let rev = print_sheets(&app, &st);
        let names: Vec<bool> = rev
            .pages
            .iter()
            .map(|p| {
                serde_json::to_string(p)
                    .expect("json")
                    .contains("first-page")
            })
            .collect();
        assert_eq!(names, [false, true], "the last page first");
        let both = print_sheets(
            &app,
            &PrintState {
                reverse_order: true,
                grayscale: true,
                ..PrintState::default()
            },
        );
        assert_eq!(both.pages.len(), 2);
        assert!(serde_json::to_string(&both.pages[1])
            .expect("json")
            .contains("first-page"));
    }

    #[test]
    fn the_print_dialog_settings_make_the_layout() {
        let app = App::headless();
        let mut st = PrintState {
            layout_kind: 3,
            cols: 4,
            rows: 0,
            sheet: 2,
            landscape: true,
            crop_marks: true,
            ..Default::default()
        };
        let o = print_layout(&app, &st, app.doc());
        assert_eq!(
            o.layout,
            crate::imposition::Layout::Repeat { cols: 4, rows: 1 }
        );
        assert_eq!(o.sheet, Some(Size::new(297.0, 210.0)));
        assert!(o.marks.crop && !o.marks.registration);
        // Bleed only when asked for, as far as the bleed limit.
        st.bleed_limit = 3.0;
        assert_eq!(print_layout(&app, &st, app.doc()).bleed, 0.0);
        st.bleed = true;
        assert_eq!(print_layout(&app, &st, app.doc()).bleed, 3.0);
        // Orientation overrides the Landscape box.
        st.orientation = 1;
        assert_eq!(
            print_layout(&app, &st, app.doc()).sheet,
            Some(Size::new(210.0, 297.0))
        );
        st.orientation = 0;
        st.sheet = 0;
        assert_eq!(print_layout(&app, &st, app.doc()).sheet, None);
    }

    #[test]
    fn sizes_read_in_kilobytes_and_megabytes() {
        assert_eq!(human_size(10), "1 KB");
        assert_eq!(human_size(820 * 1024), "820 KB");
        assert_eq!(human_size(3 * 1024 * 1024 + 400 * 1024), "3.4 MB");
    }

    #[test]
    fn csv_parses_headers_and_rows() {
        let (h, r) = parse_csv("Name,City\nAna,Porto\nBob,Lisboa\n");
        assert_eq!(h, vec!["Name", "City"]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1][1], "Lisboa");
        let (h2, _) = parse_csv("a;b\n1;2\n");
        assert_eq!(h2, vec!["a", "b"]);
        let (h3, r3) = parse_csv("a\tb, c\n1\t2\n");
        assert_eq!(h3, vec!["a", "b, c"]);
        assert_eq!(r3[0], vec!["1", "2"]);
    }

    /// Data files as spreadsheets save them read: UTF-8 with its mark (no
    /// stray mark in the first field), UTF-16 (Unicode Text) and code page
    /// 1252, which Excel writes CSV in on Windows (#495).
    #[test]
    fn data_files_in_every_encoding_read() {
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend("Nome,Cidade\nJoão,Évora\n".as_bytes());
        let (h, r) = parse_csv(&decode_text(&bom));
        assert_eq!(h, vec!["Nome", "Cidade"]);
        assert_eq!(r[0], vec!["João", "Évora"]);
        let mut le = vec![0xFF, 0xFE];
        for u in "Name\tCity\nAna\tPorto\n".encode_utf16() {
            le.extend(u.to_le_bytes());
        }
        let (h, r) = parse_csv(&decode_text(&le));
        assert_eq!(h, vec!["Name", "City"]);
        assert_eq!(r[0], vec!["Ana", "Porto"]);
        // "João" and the euro sign in code page 1252 are not UTF-8.
        let ansi = b"Name;Price\nJo\xe3o;5 \x80\n";
        let (h, r) = parse_csv(&decode_text(ansi));
        assert_eq!(h, vec!["Name", "Price"]);
        assert_eq!(r[0], vec!["João", "5 €"]);
    }

    /// Find and Replace searches the runs of a text as one text, finds and
    /// replaces texts inside groups, and ignores case letter for letter: a
    /// word in two looks was found and left as it was, a grouped text was
    /// not found and not replaced, and a Turkish "İ" (longer lower case)
    /// before a match cut the text in the wrong place or past its end,
    /// which closed TraceDraw.
    #[test]
    fn replace_finds_words_across_runs_in_groups_and_after_a_dotted_capital_i() {
        assert_eq!(
            replace_in("\u{130}\u{130}x", "x", "y", false, false),
            "\u{130}\u{130}y"
        );
        assert_eq!(
            replace_in("\u{130}stanbul", "STAN", "-", false, false),
            "\u{130}-bul"
        );
        let mut app = App::headless();
        let mut bold = TextSpan::new("lo wor", "Arial", 12.0);
        bold.bold = true;
        let text = |spans: Vec<TextSpan>| ShapeKind::Text {
            spans,
            origin: tracedraw_core::Point::new(10.0, 10.0),
            frame: None,
            align: tracedraw_core::TextAlign::Left,
            para: Default::default(),
            on_path: None,
        };
        let split = app
            .new_shape(text(vec![
                TextSpan::new("hel", "Arial", 12.0),
                bold,
                TextSpan::new("ld", "Arial", 12.0),
            ]))
            .expect("text");
        let inner = app
            .new_shape(text(vec![TextSpan::new("say hello", "Arial", 12.0)]))
            .expect("text");
        let other = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        app.select(vec![inner, other]);
        app.group_selection();
        let group = app.selection[0];
        assert_eq!(
            find_shapes(&app, "HELLO", 0, false, true),
            vec![split, group]
        );
        replace_text(&mut app, "hello", "bye", false, true, true);
        let runs = |id| match &app.doc().find_shape(id).expect("text").kind {
            ShapeKind::Text { spans, .. } => spans
                .iter()
                .map(|s| (s.text.clone(), s.bold))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        assert_eq!(
            runs(split),
            vec![
                ("bye".to_string(), false),
                (" wor".to_string(), true),
                ("ld".to_string(), false)
            ]
        );
        assert_eq!(runs(inner), vec![("say bye".to_string(), false)]);
        // Whole words only: "hello" in "helloworld" stays.
        assert!(replace_in_spans(
            &[TextSpan::new("helloworld", "A", 1.0)],
            "hello",
            "x",
            false,
            true
        )
        .is_none());
    }

    #[test]
    fn replace_variants() {
        assert_eq!(replace_in("Hello hello", "hello", "x", false, false), "x x");
        assert_eq!(
            replace_in("Hello hello", "hello", "x", true, false),
            "Hello x"
        );
        assert_eq!(
            replace_in("cat concat", "cat", "dog", false, true),
            "dog concat"
        );
    }

    /// WebP takes the quality: lower values make smaller files (colours
    /// rounded, near-lossless), 100 keeps the image exact (#544).
    /// The browser's lossy WebP (#592): the pure-Rust encoder writes a
    /// lossy (VP8) file that follows Quality, far smaller than the
    /// lossless file the browser wrote before with Lossless off, which
    /// decodes to the picture with its transparency; an empty picture
    /// falls back to lossless.
    #[test]
    fn the_browsers_lossy_webp_follows_the_quality() {
        // A photograph-like picture: smooth shades with grain.
        let (w, h) = (256u32, 128u32);
        let mut img = image::RgbaImage::new(w, h);
        let mut seed = 12345u32;
        for (x, y, p) in img.enumerate_pixels_mut() {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let grain = ((seed >> 16) % 24) as i32 - 12;
            let c = |v: i32| (v + grain).clamp(0, 255) as u8;
            *p = image::Rgba([c(x as i32), c(2 * y as i32), c(((x + y) / 2) as i32), 255]);
        }
        let low = webp_lossy_pure(&img, 10).expect("lossy");
        let high = webp_lossy_pure(&img, 90).expect("lossy");
        assert!(low.len() < high.len(), "{} vs {}", low.len(), high.len());
        // A lossy bitstream ("VP8 "), not a lossless one ("VP8L").
        assert_eq!(&low[0..4], b"RIFF");
        assert_eq!(&low[8..12], b"WEBP");
        assert!(low.windows(4).any(|c| c == b"VP8 "));
        assert!(!low.windows(4).any(|c| c == b"VP8L"));
        let back = image::load_from_memory(&high).expect("decodes").to_rgba8();
        assert_eq!(back.dimensions(), (w, h));
        let (a, b) = (back.get_pixel(100, 60).0, img.get_pixel(100, 60).0);
        for k in 0..3 {
            assert!((a[k] as i32 - b[k] as i32).abs() < 40, "{a:?} vs {b:?}");
        }
        // What the browser wrote before with Lossless off: near-lossless.
        let mut rounded = img.clone();
        near_lossless(&mut rounded, 10);
        let mut before = std::io::Cursor::new(Vec::new());
        rounded
            .write_to(&mut before, image::ImageFormat::WebP)
            .expect("lossless");
        let before = before.into_inner();
        assert!(
            low.len() * 3 < before.len(),
            "{} vs {}",
            low.len(),
            before.len()
        );
        // Transparency is kept.
        let mut clear = img.clone();
        for (x, _, p) in clear.enumerate_pixels_mut() {
            if x < 64 {
                p.0[3] = 0;
            }
        }
        let t = webp_lossy_pure(&clear, 75).expect("lossy with alpha");
        let back = image::load_from_memory(&t).expect("decodes").to_rgba8();
        assert_eq!(back.get_pixel(10, 10).0[3], 0);
        assert_eq!(back.get_pixel(200, 10).0[3], 255);
        // Nothing to encode: no lossy file.
        assert!(webp_lossy_pure(&image::RgbaImage::new(0, 0), 50).is_none());
    }

    /// #3063: JPEG XL is in the Export list, keeps transparency, and a
    /// page written as JPEG XL reads back with the same pixels.
    #[test]
    fn a_page_exports_as_lossless_jpeg_xl() {
        assert!(crate::ui::dialogs::EXPORT_FORMATS
            .iter()
            .any(|(_, e)| *e == "jxl"));
        assert!(keeps_alpha("jxl"));
        let mut pm = tiny_skia::Pixmap::new(40, 30).expect("pixmap");
        for (i, p) in pm.pixels_mut().iter_mut().enumerate() {
            let (x, y) = ((i % 40) as u8, (i / 40) as u8);
            *p = tiny_skia::PremultipliedColorU8::from_rgba(x * 6, y * 8, 200, 255).expect("px");
        }
        let bytes = encode_raster_with(&pm, "jxl", 80, false).expect("jxl");
        let back = crate::files::decode_picture(&bytes)
            .expect("reads")
            .to_rgba8();
        assert_eq!(back.dimensions(), (40, 30));
        assert_eq!(back.get_pixel(39, 29).0, [39 * 6, 29 * 8, 200, 255]);
    }

    #[test]
    fn webp_quality_shrinks_the_file() {
        let mut pm = tiny_skia::Pixmap::new(256, 128).expect("pixmap");
        for (i, p) in pm.pixels_mut().iter_mut().enumerate() {
            let (x, y) = ((i % 256) as u8, (i / 256) as u8);
            let n = ((x as u32 * 7 + y as u32 * 13) % 5) as u8;
            *p = tiny_skia::PremultipliedColorU8::from_rgba(x, y.wrapping_mul(2), x ^ y ^ n, 255)
                .expect("pixel");
        }
        // Lossy: far smaller at a low quality, and still a WebP (#592).
        let lossy = encode_raster_with(&pm, "webp", 10, false).expect("webp");
        let lossy_high = encode_raster_with(&pm, "webp", 90, false).expect("webp");
        assert!(
            lossy.len() < lossy_high.len(),
            "{} vs {}",
            lossy.len(),
            lossy_high.len()
        );
        assert!(image::load_from_memory(&lossy).is_ok());
        let full = encode_raster(&pm, "webp", 100).expect("webp");
        assert!(
            lossy.len() * 3 < full.len(),
            "{} vs {}",
            lossy.len(),
            full.len()
        );
        let low = encode_raster(&pm, "webp", 40).expect("webp");
        assert!(
            low.len() < full.len() * 3 / 4,
            "{} vs {}",
            low.len(),
            full.len()
        );
        let back = image::load_from_memory(&full).expect("decode").to_rgba8();
        assert_eq!(back.get_pixel(37, 21).0, {
            let d = pm.pixel(37, 21).expect("pixel").demultiply();
            [d.red(), d.green(), d.blue(), d.alpha()]
        });
    }

    /// The artwork moved in Print Preview prints moved by that much on
    /// the sheet; at first it stays (#536).
    #[test]
    fn the_artwork_moved_in_print_preview_prints_moved() {
        let mut app = App::headless();
        app.new_shape(ShapeKind::Rect {
            rect: tracedraw_core::geometry::Rect::new(20.0, 20.0, 60.0, 50.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
        let st = PrintState {
            fit_to_page: false,
            ..PrintState::default()
        };
        // The page goes on the sheet in a frame that clips it: the
        // rectangle is inside, with the frame's transform.
        let bounds = |doc: &Document| {
            doc.pages[0]
                .layers
                .iter()
                .flat_map(|l| &l.shapes)
                .find_map(|s| match &s.kind {
                    ShapeKind::ClipFrame { contents, .. } => contents
                        .iter()
                        .find(|c| matches!(c.kind, ShapeKind::Rect { .. }))
                        .map(|c| {
                            let mut c = c.clone();
                            c.transform = s.transform * c.transform;
                            c.bounds()
                        }),
                    _ => None,
                })
                .expect("the rectangle")
        };
        let at_first = bounds(&print_sheets(&app, &st));
        let moved = PrintState {
            art_offset: (5.0, -3.0),
            ..st.clone()
        };
        let b = bounds(&print_sheets(&app, &moved));
        assert!(
            (b.x0 - at_first.x0 - 5.0).abs() < 1e-6,
            "{b:?} {at_first:?}"
        );
        assert!((b.y0 - at_first.y0 + 3.0).abs() < 1e-6);
    }

    /// The bitmap editor of the preferences gets the file as its argument;
    /// a macOS application bundle opens through `open -a` (#593).
    #[test]
    fn the_bitmap_editor_gets_the_file() {
        let p = Path::new("/tmp/bitmap-7.png");
        let (prog, args) = editor_command(" \"/usr/bin/gimp\" ", p);
        assert_eq!(prog, "/usr/bin/gimp");
        assert_eq!(args, vec!["/tmp/bitmap-7.png".to_string()]);
        let (prog, args) = editor_command("/Applications/GIMP.app", p);
        if cfg!(target_os = "macos") {
            assert_eq!(prog, "open");
            assert_eq!(args[0], "-a");
        } else {
            assert_eq!(prog, "/Applications/GIMP.app");
            assert_eq!(args.len(), 1);
        }
    }

    /// Fit to page shrinks objects running off the page to fit inside it,
    /// centred; objects wholly off the page stay out (#534).
    /// A drawing on an A4 page: two selected squares (10 x 10 mm, a 1 mm
    /// outline) 40 mm apart, one not selected, an object on a master
    /// layer and a page background.
    fn selection_drawing() -> (App, [tracedraw_core::ShapeId; 2]) {
        use tracedraw_core::{Color, Fill, Layer, Shape, Stroke};
        let mut doc = Document::new("sel", Size::new(210.0, 297.0));
        let square = |doc: &mut Document, x: f64, y: f64| {
            let mut s = Shape::new(
                doc.ids_mut().shape(),
                ShapeKind::Rect {
                    rect: Rect::new(x, y, x + 10.0, y + 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            s.fill = Fill::Solid(Color::rgb8(200, 30, 30));
            s.stroke = Some(Stroke {
                width: 1.0,
                ..Stroke::default()
            });
            s
        };
        let a = square(&mut doc, 50.0, 100.0);
        let b = square(&mut doc, 90.0, 120.0);
        let other = square(&mut doc, 150.0, 200.0);
        let ids = [a.id, b.id];
        doc.pages[0].layers[0].shapes.extend([a, b, other]);
        doc.pages[0].background = Some(Fill::Solid(Color::rgb8(0, 0, 255)));
        let master_shape = square(&mut doc, 5.0, 5.0);
        let mut master = Layer::master(doc.ids_mut().layer(), "Master", Default::default());
        master.shapes.push(master_shape);
        doc.master.push(master);
        let mut app = App::headless();
        app.engine = tracedraw_core::Engine::new(doc);
        if let Some(p) = app.doc().pages.first() {
            app.page = p.id;
        }
        app.selection = ids.to_vec();
        (app, ids)
    }

    /// The shapes a sheet prints (what its page clips hold) and their
    /// bounds on the sheet.
    fn printed(sheet: &tracedraw_core::document::Page) -> Vec<Rect> {
        sheet
            .layers
            .iter()
            .flat_map(|l| &l.shapes)
            .filter_map(|c| match &c.kind {
                ShapeKind::ClipFrame { contents, .. } => Some(
                    contents
                        .iter()
                        .map(|s| {
                            let mut s = s.clone();
                            s.transform_by(c.transform);
                            s.bounds()
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Print range Selection prints only the selected objects (no other
    /// object, nothing of the master layers, no page background) on a
    /// page the size of the area they cover: the sheet sized to the page
    /// is that area, a fixed sheet gets it centred at its size, and the
    /// layouts repeat it (#1001).
    #[test]
    fn the_selection_prints_alone_on_a_page_of_its_size() {
        let (app, _) = selection_drawing();
        // The two squares with their outlines: from (49.5, 99.5) to
        // (100.5, 130.5).
        let (w, h) = (51.0, 31.0);
        let st = PrintState {
            range: 3,
            ..PrintState::default()
        };
        let sheets = print_sheets(&app, &st);
        assert_eq!(sheets.pages.len(), 1);
        let sheet = &sheets.pages[0];
        assert!(
            (sheet.size.width - w).abs() < 1e-6 && (sheet.size.height - h).abs() < 1e-6,
            "{:?}",
            sheet.size
        );
        let shapes = printed(sheet);
        assert_eq!(shapes.len(), 2, "only the two selected squares: {shapes:?}");
        // As they are placed among themselves, the area's corner at the
        // sheet's corner.
        let mut x0: Vec<f64> = shapes.iter().map(|r| r.x0).collect();
        x0.sort_by(f64::total_cmp);
        assert!(
            (x0[0] - 0.5).abs() < 1e-6 && (x0[1] - 40.5).abs() < 1e-6,
            "{x0:?}"
        );
        // On an A4 sheet: centred, at its size.
        let st = PrintState {
            range: 3,
            sheet: 2,
            orientation: 1,
            ..PrintState::default()
        };
        let sheets = print_sheets(&app, &st);
        let sheet = &sheets.pages[0];
        assert_eq!(sheet.size, Size::new(210.0, 297.0));
        let area = printed(sheet)
            .into_iter()
            .reduce(|a, b| a.union(b))
            .expect("printed");
        assert!((area.width() - 50.0).abs() < 1e-6, "{area:?}");
        assert!((area.center().x - 105.0).abs() < 1e-6, "{area:?}");
        // Repeated 2 x 3 on the sheet: six copies of the area.
        let st = PrintState {
            range: 3,
            sheet: 2,
            orientation: 1,
            layout_kind: 3,
            cols: 2,
            rows: 3,
            ..PrintState::default()
        };
        let sheets = print_sheets(&app, &st);
        assert_eq!(printed(&sheets.pages[0]).len(), 12);
        let plan = crate::imposition::plan(&[Size::new(w, h)], &print_layout(&app, &st, &sheets));
        assert!(plan[0]
            .cells
            .iter()
            .all(|c| (c.trim.width() - w).abs() < 1e-6));
    }

    /// Print page background off: the page's colour does not print,
    /// though the page keeps it on screen (#1651).
    #[test]
    fn the_page_background_prints_only_when_asked() {
        use tracedraw_core::{Color, Fill};
        let (mut app, _) = selection_drawing();
        app.selection.clear();
        let blue = format!("{:?}", Fill::Solid(Color::rgb8(0, 0, 255)));
        let sheets = print_sheets(&app, &PrintState::default());
        assert!(format!("{sheets:?}").contains(&blue));
        let st = PrintState {
            page_background: false,
            ..PrintState::default()
        };
        let sheets = print_sheets(&app, &st);
        assert!(!format!("{sheets:?}").contains(&blue));
        assert!(app.doc().pages[0].background.is_some());
    }

    /// A sheet printed through Android's print service lies on opaque
    /// white paper: a printer plugin that draws the PDF on a transparent
    /// bitmap and drops its alpha printed what nothing covers black
    /// (#2663). A page's own background is kept.
    #[test]
    fn android_prints_go_out_on_white_paper() {
        use tracedraw_core::{Color, Fill};
        let mut app = App::headless();
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(20.0, 20.0, 80.0, 60.0),
            radius: 0.0,
            corners: None,
        })
        .expect("rect");
        let mut doc = print_sheets(&app, &PrintState::default());
        assert_eq!(doc.pages[0].background, None);
        // A second sheet with a blue page keeps it.
        let mut blue = doc.pages[0].clone();
        blue.background = Some(Fill::Solid(Color::rgb8(0, 0, 255)));
        doc.pages.push(blue);
        on_white_paper(&mut doc);
        assert_eq!(doc.pages[0].background, Some(Fill::Solid(Color::WHITE)));
        assert_eq!(
            doc.pages[1].background,
            Some(Fill::Solid(Color::rgb8(0, 0, 255)))
        );
        if let Some(out) = std::env::var_os("TRACEDRAW_DUMP_PRINT_PDF") {
            std::fs::write(out, tracedraw_io::pdf::document_to_pdf(&doc)).expect("dump");
        }
    }

    /// Keep size: the selection never prints smaller than it is, even on
    /// a sheet too small for it; without it, it shrinks to fit (#1001).
    #[test]
    fn keep_size_keeps_the_selection_at_its_size() {
        let (app, _) = selection_drawing();
        let a6ish = |keep_size| PrintState {
            range: 3,
            sheet: crate::ui::dialogs::CUSTOM_SHEET,
            custom_sheet: Size::new(30.0, 30.0),
            orientation: 1,
            keep_size,
            ..PrintState::default()
        };
        let width = |st: &PrintState| {
            printed(&print_sheets(&app, st).pages[0])
                .into_iter()
                .reduce(|a, b| a.union(b))
                .map(|r| r.width())
                .expect("printed")
        };
        assert!((width(&a6ish(true)) - 50.0).abs() < 1e-6);
        assert!(width(&a6ish(false)) < 30.0);
    }

    #[test]
    fn fit_to_page_shrinks_what_runs_off_the_page() {
        let mut app = App::headless();
        let size = app.page_size();
        let rect = |app: &mut App, r: Rect| {
            app.new_shape(ShapeKind::Rect {
                rect: r,
                radius: 0.0,
                corners: None,
            })
            .expect("rect")
        };
        // Twice the page's width, off its right edge; another wholly off.
        let wide = rect(
            &mut app,
            Rect::new(10.0, 10.0, 10.0 + size.width * 2.0, 60.0),
        );
        let away = rect(&mut app, Rect::new(-300.0, 0.0, -250.0, 50.0));
        let mut page = app.doc().pages[0].clone();
        fit_contents_to_page(&mut page, false);
        let bounds = |page: &tracedraw_core::document::Page, id| {
            page.layers
                .iter()
                .flat_map(|l| &l.shapes)
                .find(|s| s.id == id)
                .map(|s| s.bounds())
                .expect("shape")
        };
        let b = bounds(&page, wide);
        assert!(b.x0 >= -1e-6 && b.x1 <= size.width + 1e-6, "{b:?}");
        assert!((b.width() - size.width).abs() < 1e-6, "as wide as the page");
        assert!((b.center().y - size.height / 2.0).abs() < 1e-6, "centred");
        assert_eq!(bounds(&page, away), Rect::new(-300.0, 0.0, -250.0, 50.0));
        // Printing the selection fits it whatever it covers.
        let mut page = app.doc().pages[0].clone();
        fit_contents_to_page(&mut page, true);
        assert!(bounds(&page, away).x0 >= -1e-6);
        // Objects inside the page stay as they are.
        let mut page = app.doc().pages[0].clone();
        page.layers
            .iter_mut()
            .for_each(|l| l.shapes.retain(|s| s.id != wide && s.id != away));
        let before = page.clone();
        fit_contents_to_page(&mut page, false);
        assert_eq!(page, before);
    }

    /// Fit to page on facing pages (#2678): an object across the spine
    /// prints split between the two pages, as without Fit to page.
    /// Fitted page by page, it shrank onto its own page and the facing
    /// page printed empty.
    #[test]
    fn fit_to_page_keeps_an_object_across_the_spine_on_both_pages() {
        fn shapes(s: &tracedraw_core::Shape, out: &mut Vec<tracedraw_core::Shape>) {
            match &s.kind {
                ShapeKind::ClipFrame { contents, .. } => {
                    for c in contents {
                        shapes(c, out);
                    }
                }
                _ => out.push(s.clone()),
            }
        }
        let mut app = App::headless();
        for _ in 0..2 {
            app.add_page();
        }
        app.set_facing_pages(true, tracedraw_core::spreads::PageSide::Right);
        app.page = app.doc().pages[1].id;
        let size = app.doc().pages[1].size;
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(size.width - 20.0, 10.0, size.width + 30.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("rect");
        let st = PrintState {
            fit_to_page: true,
            ..PrintState::default()
        };
        let on_sheet = |doc: &Document, k: usize| {
            let mut out = Vec::new();
            for s in doc.pages[k].layers.iter().flat_map(|l| &l.shapes) {
                shapes(s, &mut out);
            }
            out
        };
        let printed = print_sheets(&app, &st);
        assert_eq!(printed.pages.len(), 3);
        // Page 2 has it as drawn, 50 mm wide; page 3 its copy.
        let own = on_sheet(&printed, 1);
        assert_eq!(own.len(), 1);
        let w = own[0]
            .transform
            .transform_rect_bbox(own[0].bounds())
            .width();
        assert!((own[0].bounds().width() - 50.0).abs() < 1e-6, "{w}");
        assert_eq!(
            on_sheet(&printed, 2).len(),
            1,
            "the facing page has its part"
        );
    }

    /// Fit to page scales the page to the paper and leaves what runs off
    /// a page cut at its edge, where it was drawn (#2678): an object put
    /// partly off the page on purpose, or past the outer edge of a
    /// spread, keeps its place and size instead of being pulled onto the
    /// page and shrunk with the page's other objects.
    #[test]
    fn fit_to_page_leaves_what_runs_off_the_page_where_it_is() {
        // The rectangles printed on page `k`, inside any clip frame.
        fn rects(doc: &Document, k: usize) -> Vec<Rect> {
            fn walk(s: &tracedraw_core::Shape, out: &mut Vec<Rect>) {
                match &s.kind {
                    ShapeKind::ClipFrame { contents, .. } => {
                        contents.iter().for_each(|c| walk(c, out))
                    }
                    ShapeKind::Rect { .. } => out.push(s.bounds()),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            for s in doc.pages[k].layers.iter().flat_map(|l| &l.shapes) {
                walk(s, &mut out);
            }
            out
        }
        let mut app = App::headless();
        let size = app.page_size();
        let rect = |app: &mut App, r: Rect| {
            app.new_shape(ShapeKind::Rect {
                rect: r,
                radius: 0.0,
                corners: None,
            })
            .expect("rect")
        };
        rect(&mut app, Rect::new(20.0, 20.0, 60.0, 60.0));
        let off = Rect::new(size.width - 30.0, 100.0, size.width + 70.0, 150.0);
        rect(&mut app, off);
        let st = PrintState {
            fit_to_page: true,
            ..PrintState::default()
        };
        let printed = print_sheets(&app, &st);
        let on_page = rects(&printed, 0);
        assert!(
            on_page.contains(&off),
            "not pulled onto the page: {on_page:?}"
        );
        assert!(on_page.contains(&Rect::new(20.0, 20.0, 60.0, 60.0)));
        // The page is what fits the paper: Fit to page goes to the layout.
        assert!(print_layout(&app, &st, &printed).fit);
        // Facing pages: past the outer edge of the spread, the same.
        for _ in 0..2 {
            app.add_page();
        }
        app.set_facing_pages(true, tracedraw_core::spreads::PageSide::Right);
        app.page = app.doc().pages[2].id;
        let outer = Rect::new(size.width - 30.0, 10.0, size.width + 40.0, 40.0);
        rect(&mut app, outer);
        let printed = print_sheets(&app, &st);
        let on_page = rects(&printed, 2);
        assert!(
            on_page.contains(&outer),
            "kept past the spread's edge: {on_page:?}"
        );
    }

    /// A blend prints as it is drawn when the print moves or recolours
    /// its objects: with Fit to page (the end object running off the
    /// page, left where it is), with the selection printed as the page,
    /// and as a negative,
    /// the steps lie between the start and the end as printed and their
    /// colours run between the printed colours. The steps were evaluated
    /// from the copy of the end object the blend keeps, which the print
    /// left where (and as) it was on the page (#1541).
    #[test]
    fn a_blend_prints_between_its_objects_as_printed() {
        use tracedraw_core::{live::Effect, Color, Fill};
        let mut app = App::headless();
        let square = |app: &mut App, r: Rect, c: Color| {
            let id = app
                .new_shape(ShapeKind::Rect {
                    rect: r,
                    radius: 0.0,
                    corners: None,
                })
                .expect("square");
            app.run(Command::SetFill {
                shapes: vec![id],
                fill: Fill::Solid(c),
            });
            id
        };
        // On an A4 page: the start inside, the end running off the right.
        let a = square(
            &mut app,
            Rect::new(20.0, 100.0, 40.0, 120.0),
            Color::rgb8(255, 0, 0),
        );
        let b = square(
            &mut app,
            Rect::new(180.0, 100.0, 260.0, 180.0),
            Color::rgb8(0, 0, 255),
        );
        app.select(vec![a, b]);
        app.blend_selection(3, 0.0, 0.0, 0.0);
        app.refresh_blends();
        app.selection = vec![a, b];
        // The start, the end and the steps of the first sheet, on the
        // sheet: (bounds, colour) each.
        let printed_blend = |doc: &Document| {
            let mut start = None;
            let mut end = None;
            let mut steps = Vec::new();
            for c in doc.pages[0].layers.iter().flat_map(|l| &l.shapes) {
                let ShapeKind::ClipFrame { contents, .. } = &c.kind else {
                    continue;
                };
                for s in contents {
                    let on_sheet = |s: &tracedraw_core::Shape| {
                        let mut s = s.clone();
                        s.transform_by(c.transform);
                        (s.bounds(), s.fill.preview_color().expect("a colour"))
                    };
                    if s.effects.iter().any(|e| matches!(e, Effect::Blend { .. })) {
                        let ev = tracedraw_core::live::evaluate(s);
                        start = Some(on_sheet(s));
                        steps.extend(ev.below.iter().chain(&ev.above).map(on_sheet));
                    } else if s.effects.is_empty() {
                        end = Some(on_sheet(s));
                    }
                }
            }
            (start.expect("start"), end.expect("end"), steps)
        };
        let between = |(start, end, steps): &((Rect, Color), (Rect, Color), Vec<(Rect, Color)>)| {
            assert_eq!(steps.len(), 3);
            let (s, e) = (start.0.center(), end.0.center());
            // From the start to the end, the middle step half way.
            for (i, (b, _)) in steps.iter().enumerate() {
                let want = s.lerp(e, (i + 1) as f64 / 4.0);
                let got = b.center();
                assert!((got - want).hypot() < 1e-6, "{i}: {got:?}, not {want:?}");
            }
            let [r0, g0, b0] = start.1.to_rgb8();
            let [r1, g1, b1] = end.1.to_rgb8();
            let [r, g, bl] = steps[1].1.to_rgb8();
            let mid = |x: u8, y: u8| (x as i32 + y as i32) / 2;
            assert!(
                (r as i32 - mid(r0, r1)).abs() <= 1
                    && (g as i32 - mid(g0, g1)).abs() <= 1
                    && (bl as i32 - mid(b0, b1)).abs() <= 1,
                "{:?} between {:?} and {:?}",
                steps[1].1,
                start.1,
                end.1
            );
        };
        // Fit to page scales the page, so the end stays where it is.
        let fit = PrintState {
            fit_to_page: true,
            ..PrintState::default()
        };
        let p = printed_blend(&print_sheets(&app, &fit));
        assert!(p.1 .0.x1 > 210.0, "left where drawn: {:?}", p.1 .0);
        between(&p);
        let selection = PrintState {
            range: 3,
            ..PrintState::default()
        };
        between(&printed_blend(&print_sheets(&app, &selection)));
        let negative = PrintState {
            invert: true,
            ..PrintState::default()
        };
        let p = printed_blend(&print_sheets(&app, &negative));
        assert_eq!(p.0 .1.to_rgb8(), [0, 255, 255]);
        between(&p);
        // Exported alone, the start keeps its steps running to the end's
        // place beside it.
        app.selection = vec![a];
        let (doc, bounds) = export_document(
            &app,
            &ExportState {
                selection_only: true,
                ..ExportState::default()
            },
        );
        let s = doc.pages[0].layers[0].shapes[0].clone();
        let ev = tracedraw_core::live::evaluate(&s);
        let last = ev
            .above
            .last()
            .or(ev.below.first())
            .expect("steps")
            .bounds()
            .center();
        // Moved with the start: the last step three quarters of the way
        // from the start's centre (30, 110) to the end's (220, 140).
        let want = tracedraw_core::geometry::Point::new(172.5 - bounds.x0, 132.5 - bounds.y0);
        assert!((last - want).hypot() < 1e-6, "{last:?}, not {want:?}");
    }

    /// A rectangle with a 2 mm outline exported alone: the export takes
    /// the whole outline, 1 mm past the path on every side, where the SVG's
    /// box was the path's and cut half the outline off at each edge
    /// (#2290). Also inside a group.
    #[test]
    fn a_selection_exported_alone_keeps_its_whole_outline() {
        use tracedraw_core::{Color, Stroke};
        let mut app = App::headless();
        let layer = app.active_layer().expect("layer");
        let id = app.engine.new_shape_id();
        let mut s = tracedraw_core::Shape::new(
            id,
            ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(10.0, 10.0, 40.0, 30.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.stroke = Some(Stroke::new(Color::rgb8(0, 0, 255), 2.0));
        app.run(Command::AddShape { layer, shape: s });
        app.selection = vec![id];
        let st = ExportState {
            selection_only: true,
            ..ExportState::default()
        };
        let (doc, bounds) = export_document(&app, &st);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(
            near(bounds.x0, 9.0)
                && near(bounds.y0, 9.0)
                && near(bounds.x1, 41.0)
                && near(bounds.y1, 31.0),
            "{bounds:?}"
        );
        let svg = tracedraw_io::svg::page_to_svg(&doc, 0);
        assert!(svg.contains("viewBox=\"0 0 32 22\""), "{svg}");

        // Grouped: the group has no outline of its own, its rectangle has.
        app.selection = vec![id];
        app.group_selection();
        assert_eq!(app.selection.len(), 1);
        let (_, grouped) = export_document(&app, &st);
        assert!(
            near(grouped.x0, 9.0) && near(grouped.x1, 41.0),
            "{grouped:?}"
        );
    }

    /// Selected objects exported alone keep their spot inks: a PDF of the
    /// selection prints each on its own separation, not as its process
    /// stand-in.
    #[test]
    fn a_selection_exported_alone_keeps_its_spot_inks() {
        use tracedraw_core::{document::Ink, Command, Fill};
        let mut app = App::headless();
        let ink = Ink {
            id: 9,
            name: "Spot_1_W".into(),
            cmyk: [0.0, 0.0, 0.0, 0.1],
        };
        app.run(Command::SetInks {
            inks: vec![ink.clone()],
        });
        let layer = app.active_layer().expect("layer");
        let id = app.engine.new_shape_id();
        let mut s = tracedraw_core::Shape::new(
            id,
            ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(10.0, 10.0, 40.0, 30.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = Fill::Solid(ink.color(1.0));
        s.stroke = None;
        app.run(Command::AddShape { layer, shape: s });
        app.selection = vec![id];
        let (doc, _) = export_document(
            &app,
            &ExportState {
                selection_only: true,
                ..ExportState::default()
            },
        );
        assert_eq!(doc.inks, vec![ink]);
        let text = String::from_utf8_lossy(&tracedraw_io::pdf::document_to_pdf(&doc)).to_string();
        assert!(text.contains("/Separation /Spot_1_W "), "no separation");
    }

    /// The colour models a printed sheet still holds, wherever it keeps a
    /// colour.
    fn colour_models(pages: &[tracedraw_core::document::Page]) -> Vec<String> {
        fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
            match v {
                serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
                serde_json::Value::Object(m) => {
                    if let Some(serde_json::Value::String(s)) = m.get("model") {
                        out.push(s.clone());
                    }
                    m.values().for_each(|x| walk(x, out));
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk(&serde_json::to_value(pages).unwrap(), &mut out);
        out.sort();
        out.dedup();
        out
    }

    /// A drawing for the print options: one 100 mm page, the app on it.
    fn print_app(build: &dyn Fn(&mut tracedraw_core::Document)) -> crate::app::App {
        let mut doc =
            tracedraw_core::Document::new("Print", tracedraw_core::Size::new(100.0, 100.0));
        build(&mut doc);
        let mut app = crate::app::App::headless();
        app.engine = tracedraw_core::Engine::new(doc);
        if let Some(p) = app.doc().pages.first() {
            app.page = p.id;
        }
        app
    }

    fn square(
        doc: &mut tracedraw_core::Document,
        c: tracedraw_core::Color,
    ) -> tracedraw_core::Shape {
        let mut s = tracedraw_core::Shape::new(
            doc.ids_mut().shape(),
            tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(10.0, 10.0, 30.0, 30.0),
                radius: 0.0,
                corners: None,
            },
        );
        s.fill = tracedraw_core::Fill::Solid(c);
        s.stroke = None;
        s
    }

    fn orange(doc: &mut tracedraw_core::Document) -> tracedraw_core::Color {
        let ink = tracedraw_core::document::Ink {
            id: 7,
            name: "Orange".into(),
            cmyk: [0.0, 0.5, 1.0, 0.0],
        };
        let c = ink.color(1.0);
        doc.inks.push(ink);
        c
    }

    /// Convert spot colours to CMYK reaches every spot ink that prints: on
    /// a master layer, as the page's background, as a drop shadow's colour
    /// and as a contour's last colour. They stayed spot inks.
    #[test]
    fn spot_to_cmyk_reaches_master_layers_backgrounds_and_effects() {
        use tracedraw_core::{Color, Fill, Layer};
        let st = crate::ui::dialogs::PrintState {
            spot_to_cmyk: true,
            ..Default::default()
        };
        let cases: Vec<(&str, Box<dyn Fn(&mut tracedraw_core::Document)>)> = vec![
            (
                "master layer",
                Box::new(|d| {
                    let c = orange(d);
                    let s = square(d, c);
                    let mut m = Layer::master(d.ids_mut().layer(), "Master", Default::default());
                    m.shapes.push(s);
                    d.master.push(m);
                }),
            ),
            (
                "background",
                Box::new(|d| {
                    let c = orange(d);
                    d.pages[0].background = Some(Fill::Solid(c));
                }),
            ),
            (
                "shadow",
                Box::new(|d| {
                    let c = orange(d);
                    let mut s = square(
                        d,
                        Color::Cmyk {
                            c: 1.0,
                            m: 0.0,
                            y: 0.0,
                            k: 0.0,
                        },
                    );
                    s.shadow = Some(tracedraw_core::document::Shadow {
                        color: c,
                        ..Default::default()
                    });
                    d.pages[0].layers[0].shapes.push(s);
                }),
            ),
            (
                "contour",
                Box::new(|d| {
                    let c = orange(d);
                    let mut s = square(
                        d,
                        Color::Cmyk {
                            c: 1.0,
                            m: 0.0,
                            y: 0.0,
                            k: 0.0,
                        },
                    );
                    s.effects.push(contour_to(c));
                    d.pages[0].layers[0].shapes.push(s);
                }),
            ),
        ];
        for (name, build) in cases {
            let app = print_app(&*build);
            let sheets = super::print_sheets(&app, &st);
            let models = colour_models(&sheets.pages);
            assert!(!models.contains(&"spot".to_string()), "{name}: {models:?}");
        }
    }

    fn contour_to(c: tracedraw_core::Color) -> tracedraw_core::live::Effect {
        tracedraw_core::live::Effect::Contour {
            steps: 2,
            offset: 2.0,
            outside: true,
            to_center: false,
            both: false,
            fill_to: c,
            outline_to: None,
            fill_end_to: None,
            color_blend: 0,
            corners: None,
            accel_objects: 0.0,
            accel_colors: 0.0,
        }
    }

    /// Separations: a contour's steps and a drop shadow print as the
    /// coverage of each plate's ink, as the object does. Their colours
    /// were kept, so the steps blended to magenta in colour on every
    /// plate, and the magenta plate counted no ink for them.
    #[test]
    fn separations_split_the_colours_of_effects() {
        use tracedraw_core::Color;
        let magenta = Color::Cmyk {
            c: 0.0,
            m: 1.0,
            y: 0.0,
            k: 0.0,
        };
        let app = print_app(&|d| {
            let mut s = square(
                d,
                Color::Cmyk {
                    c: 1.0,
                    m: 0.0,
                    y: 0.0,
                    k: 0.0,
                },
            );
            s.effects.push(contour_to(magenta));
            let mut t = square(d, Color::WHITE);
            t.transform = tracedraw_core::geometry::Affine::translate((50.0, 50.0));
            t.shadow = Some(tracedraw_core::document::Shadow {
                color: Color::Cmyk {
                    c: 0.0,
                    m: 0.0,
                    y: 1.0,
                    k: 0.0,
                },
                ..Default::default()
            });
            d.pages[0].layers[0].shapes.extend([s, t]);
        });
        let st = crate::ui::dialogs::PrintState {
            separations: true,
            ..Default::default()
        };
        let sheets = super::print_sheets(&app, &st);
        let page = &sheets.pages[0];
        for (plate, _) in super::plates(&sheets) {
            let p = super::plate_page(page, plate);
            assert_eq!(
                colour_models(std::slice::from_ref(&p)),
                vec!["gray"],
                "{plate:?}"
            );
        }
        // The contour's magenta and the shadow's yellow are inks of their
        // plates.
        assert!(super::plate_has_ink(page, super::Plate::Process(1)));
        assert!(super::plate_has_ink(page, super::Plate::Process(2)));
    }

    /// Print all text in black reaches text on a master layer and a
    /// character's own outline.
    #[test]
    fn text_in_black_reaches_master_layers_and_character_outlines() {
        use tracedraw_core::{Color, Fill, Layer, Stroke};
        let mut app = crate::app::App::headless();
        tracedraw_text::install();
        app.start_text(tracedraw_core::geometry::Point::new(10.0, 20.0), None);
        app.text_insert("Ab");
        let t = app.text_edit.as_ref().expect("editing").shape;
        app.finish_text();
        let mut doc = app.doc().clone();
        let mut s = doc.find_shape(t).expect("text").clone();
        s.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        if let tracedraw_core::ShapeKind::Text { spans, .. } = &mut s.kind {
            spans[0].fill = Some(Fill::Solid(Color::rgb8(0, 0, 255)));
            spans[0].outline = Some(Stroke::new(Color::rgb8(0, 255, 0), 0.2));
        }
        doc.pages[0].layers[0].shapes.clear();
        let mut m = Layer::master(doc.ids_mut().layer(), "Master", Default::default());
        m.shapes.push(s);
        doc.master.push(m);
        app.engine = tracedraw_core::Engine::new(doc);
        let st = crate::ui::dialogs::PrintState {
            text_black: true,
            ..Default::default()
        };
        let sheets = super::print_sheets(&app, &st);
        let json = serde_json::to_string(&sheets.pages).unwrap();
        for colour in ["\"r\":1.0", "\"g\":1.0", "\"b\":1.0"] {
            assert!(!json.contains(colour), "{colour} left");
        }
    }
}

#[cfg(test)]
mod scratch_191 {
    use super::*;
    use tracedraw_core::{Color, Fill, Stroke, TextSpan};

    #[test]
    #[ignore]
    fn scratch_black_text_outputs() {
        tracedraw_text::install();
        let dir = std::path::PathBuf::from(
            "/tmp/claude-0/-home-claude/ecd2ad2c-4fd8-5343-ab00-e2a436bfbe5e/scratchpad/p191",
        );
        std::fs::create_dir_all(&dir).unwrap();
        let mut report = String::new();
        let colors: Vec<(&str, Color)> = vec![
            ("k100", Color::cmyk_pct(0.0, 0.0, 0.0, 100.0)),
            ("rich", Color::cmyk_pct(60.0, 40.0, 40.0, 100.0)),
            ("reg", Color::Registration),
            ("gray0", Color::Gray { v: 0.0 }),
            ("rgb0", Color::BLACK),
            (
                "lab0",
                Color::Lab {
                    l: 0.0,
                    a: 0.0,
                    b: 0.0,
                },
            ),
            (
                "spot",
                Color::Spot {
                    ink: 1,
                    tint: 1.0,
                    c: 0.0,
                    m: 0.0,
                    y: 0.0,
                    k: 1.0,
                },
            ),
        ];
        for (name, color) in &colors {
            for mode in ["fill", "outline", "paragraph", "group"] {
                let mut app = App::headless();
                app.run(tracedraw_core::Command::SetInks {
                    inks: vec![tracedraw_core::document::Ink {
                        id: 1,
                        name: "Black".into(),
                        cmyk: [0.0, 0.0, 0.0, 1.0],
                    }],
                });
                let id = app
                    .new_shape(ShapeKind::Text {
                        spans: vec![TextSpan::new("WWWW", "Sans", 72.0)],
                        origin: tracedraw_core::geometry::Point::new(20.0, 100.0),
                        frame: (mode == "paragraph")
                            .then(|| tracedraw_core::geometry::Size::new(150.0, 60.0)),
                        align: Default::default(),
                        para: Default::default(),
                        on_path: None,
                    })
                    .expect("text");
                let outline = mode == "outline";
                app.run(tracedraw_core::Command::SetFill {
                    shapes: vec![id],
                    fill: if outline {
                        Fill::None
                    } else {
                        Fill::Solid(*color)
                    },
                });
                app.run(tracedraw_core::Command::SetStroke {
                    shapes: vec![id],
                    stroke: outline.then(|| Stroke::new(*color, 1.0)),
                });
                if mode == "group" {
                    let r = app
                        .new_shape(ShapeKind::Rect {
                            rect: tracedraw_core::geometry::Rect::new(5.0, 5.0, 10.0, 10.0),
                            radius: 0.0,
                            corners: None,
                        })
                        .unwrap();
                    app.select(vec![id, r]);
                    app.group_selection();
                }
                let tag = format!("{name}_{mode}");
                let variants: Vec<(&str, Box<dyn Fn(&mut PrintState)>)> = vec![
                    ("plain", Box::new(|_| {})),
                    ("opb", Box::new(|s| s.overprint_black = true)),
                    ("tb", Box::new(|s| s.text_black = true)),
                    ("s2c", Box::new(|s| s.spot_to_cmyk = true)),
                    ("bmp", Box::new(|s| s.as_bitmap = true)),
                    ("mirror", Box::new(|s| s.mirror = true)),
                    ("invert", Box::new(|s| s.invert = true)),
                    (
                        "marks",
                        Box::new(|s| {
                            s.crop_marks = true;
                            s.registration_marks = true;
                            s.color_bars = true;
                        }),
                    ),
                ];
                for (v, set) in &variants {
                    let mut st = PrintState::default();
                    set(&mut st);
                    let doc = print_sheets(&app, &st);
                    let pdf = tracedraw_io::pdf::document_to_pdf(&doc);
                    std::fs::write(dir.join(format!("print_{tag}_{v}.pdf")), pdf).unwrap();
                }
                let st = PrintState {
                    separations: true,
                    ..PrintState::default()
                };
                let doc = separations_doc(&print_sheets(&app, &st));
                std::fs::write(
                    dir.join(format!("print_{tag}_seps.pdf")),
                    tracedraw_io::pdf::document_to_pdf(&doc),
                )
                .unwrap();
                for (k, std_) in [
                    tracedraw_io::pdf::PdfStandard::None,
                    tracedraw_io::pdf::PdfStandard::X1a,
                    tracedraw_io::pdf::PdfStandard::X3,
                    tracedraw_io::pdf::PdfStandard::X4,
                ]
                .into_iter()
                .enumerate()
                {
                    let opts = tracedraw_io::pdf::PdfOptions {
                        standard: std_,
                        output_condition: "FOGRA39".into(),
                        flatten_dpi: 150.0,
                        ..Default::default()
                    };
                    let pdf = tracedraw_io::pdf::document_to_pdf_with(app.doc(), &opts);
                    std::fs::write(dir.join(format!("publish_{tag}_x{k}.pdf")), pdf).unwrap();
                }
                // Through `.cdr` and back.
                for ver in [14u16, 17, 20, 24, 27] {
                    let bytes = tracedraw_cdr::write::document_to_cdr_version(app.doc(), ver);
                    let Ok((doc, _)) = tracedraw_cdr::open_bytes(&bytes, "t") else {
                        report.push_str(&format!("{tag} v{ver}: unreadable\n"));
                        continue;
                    };
                    let mut texts = Vec::new();
                    fn walk(s: &tracedraw_core::Shape, out: &mut Vec<String>) {
                        match &s.kind {
                            ShapeKind::Text { spans, .. } => out.push(format!(
                                "fill {:?} stroke {:?} span_fills {:?}",
                                s.fill,
                                s.stroke.as_ref().map(|k| k.color),
                                spans.iter().map(|sp| sp.fill.clone()).collect::<Vec<_>>()
                            )),
                            ShapeKind::Group { children } => {
                                children.iter().for_each(|c| walk(c, out))
                            }
                            _ => {}
                        }
                    }
                    for s in doc
                        .pages
                        .iter()
                        .flat_map(|p| &p.layers)
                        .flat_map(|l| &l.shapes)
                    {
                        walk(s, &mut texts);
                    }
                    report.push_str(&format!("{tag} v{ver}: {texts:?}\n"));
                    let mut back = App::headless();
                    back.open_document(doc, None);
                    let pdf = tracedraw_io::pdf::document_to_pdf(&print_sheets(
                        &back,
                        &PrintState::default(),
                    ));
                    std::fs::write(dir.join(format!("cdr{ver}_{tag}.pdf")), pdf).unwrap();
                }
            }
        }
        std::fs::write(dir.join("report.txt"), report).unwrap();
    }
}

# Export

File > Export (Ctrl+E) and File > Export For > Web or Office open the
Export dialog. File > Export starts with the choices of Options > Global
> File Formats, which can also keep those of the last export; the Web and
Office presets start with their own (see `options.md`).

## Layout

- On the left, a 260 x 300 point preview of what will be written: the
  page, or the selection with Selected only, rendered to fit, on a
  checkerboard when the format keeps transparency and Transparent
  background is on, else on white.
- On the right: Format, then the format's own settings, then Selected
  only and All pages, then the estimated file size.

| Format | Settings |
|---|---|
| PNG, WebP, GIF | resolution (1 to 2400 dpi, presets 72, 96, 150, 300), transparent background; WebP also quality (the files are lossless; below 100 the colours are first rounded to fewer levels, WebP's near-lossless way: 1 low bit from 90, 2 from 75, 3 from 50, 4 below, so a lower quality makes a smaller file; #544, `export::tests::webp_quality_shrinks_the_file`) |
| JPEG | resolution, quality (1 to 100) |
| BMP, TIFF, PSD | resolution |
| PDF | standard (PDF 1.4, PDF/X-1a, PDF/X-3, PDF/X-4), bleed, output condition for PDF/X; the author, subject and keywords come from the drawing's properties (File > Publish to PDF has bookmarks and protection, see `publish-to-pdf.md`) |
| SVG, EPS, AI, DXF, HTML, EMF, WMF, PLT | none |
| PostScript (`.ps`), PRN (`.prn`) | none; every exported page in one file (All pages, else the current page), each page setting its size with `setpagedevice` and ending with `showpage` (Document Structuring Conventions 3.0); a `.prn` print file is the same PostScript, to send to a PostScript printer |

- Bitmap formats show the size in pixels, the page (or selection) size
  over 25.4 times the resolution rounded up, as two fields: typing a
  width or a height sets the resolution that gives it, the other side
  following the page's proportions, so a 32 x 32 pixel icon is typed
  rather than worked out.
- File size: vector formats (SVG, EPS, PostScript, PDF, AI) are written in memory and
  measured; bitmap formats are estimated by encoding the preview the same
  way and scaling by the pixel count of the export (times the pages with
  All pages). It shows as "about 820 KB" or "about 3.4 MB".
- Text in PDF and AI files is text with its fonts embedded as subsets,
  as Publish to PDF writes it by default (see `publish-to-pdf.md`, Text
  and fonts). Vector formats write curves with their own segments:
  quadratic segments (TrueType letters, for example) become the cubic
  that draws the same curve in PDF, AI, EPS, PostScript and EMF, and
  stay quadratic in SVG; only DXF, WMF and PLT, which have no curves of
  that kind, approximate them with short lines.
- The preview and estimate are worked out again only when the drawing,
  the page, the selection or a setting changes.
- OK asks where to save; the file name is the drawing's with the format's
  extension.

## Presets

Above the format: Preset, a list of the presets saved (each shown with
its format, "(none)" first), "+" to save the dialog's choices under a
name (typed in a field that opens below, OK or Enter; the same name
replaces that preset), a pencil to rename the chosen preset (a name
another preset has is refused) and "−" to delete it. Choosing a preset
puts back its format, resolution, quality, transparent background,
selection only, all pages and the PDF choices (standard, bleed, output
condition). Presets are kept in the preferences
(`formats.export_presets`; #547,
`prefs::tests::export_presets_save_apply_rename_and_delete`).

## WebP

Lossless (off at first) and Quality (1 to 100). Off, the file is lossy
WebP at that quality (libwebp, in the desktop builds), far smaller: a
picture of 450 KB lossless comes to about 50 KB at quality 10. On, it is
lossless, and below 100 the colours are first rounded to fewer levels
(near-lossless, #544). The browser build has no lossy encoder and always
writes the lossless way (#592, `export::tests::webp_quality_shrinks_the_file`).

## Checks

- `export::tests::the_export_preview_shows_the_page_and_estimates_the_size`
- `export::tests::sizes_read_in_kilobytes_and_megabytes`
- `ui::dialogs::tests::a_pixel_size_sets_the_resolution_that_gives_it`

## Format list and file properties

- The Format list shows each format as "EXT - Description (*.ext)": JPG,
  PDF, AI and PNG first, a separator, then every format in the order of
  its extension, then installed script filters.
- Under the format's settings: Selected only, All pages, and "Do not
  show filter dialog": when on, File > Export (Ctrl+E) goes straight to
  the file with the last choices (Options > Global > File Formats keeps
  them); Export for Web and for Office still show their dialog.
- PDF and AI files: Title (the drawing's title when empty) and Tags,
  written as the file's title and keywords.
- Checks: `ui::dialogs::tests::the_export_list_puts_the_common_formats_first`.


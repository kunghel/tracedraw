# Bitmap import

File > Import (Ctrl+I) of a PNG, JPEG, BMP, GIF, TIFF, WebP or JPEG XL file
embeds the image as a bitmap object centred on the page and remembers
the file (Image > Update from Link reads it again; Options > Global >
File Formats > Link imported pictures to their files, on by default,
can embed it only, #3089). PNG files of 8 or
16 bits, grey or colour, with or without alpha or with a palette, and
baseline or progressive JPEG files all come in
(`bitmap_file::tests::every_common_png_and_jpeg_imports`, #2167).

A picture whose Exif orientation says it was taken with the camera
turned comes in upright, its width and height (and its resolution
across and down) swapped for a quarter turn
(`bitmap_file::tests::an_exif_orientation_stands_the_picture_upright`,
#2167).

A CMYK JPEG (four components) comes in as a CMYK picture with its inks,
as a CMYK TIFF does, its pixels showing the inks through the colour
management as the drawing's CMYK colours show, so exporting an object
as a CMYK JPEG and importing the file gives the colour the object had.
The channels are read as stored: inverted (255 is no ink) under an
Adobe `APP14` marker with transform 0, as the CMYK export and most
programs write them; Y, Cb, Cr of C, M, Y as they are and an inverted
K under transform 2 (YCCK, Photoshop's usual CMYK JPEG: the writer
turns the inverted values it is given into "RGB" as 255 less each, so
the colour transform taken back gives the inks); as they are without
the marker. Before, the image library turned the file into RGB by
itself, taking every file as inverted and leaving the colour management
out, so the picture came in brighter than the same inks in the drawing
(#2163,
`bitmap_file::tests::a_cmyk_jpeg_imports_with_its_inks_as_the_drawing_shows_them`).
A YCCK file then came in with its cyan, magenta and yellow inverted (a
cyan square as red, white as dark grey) (#3025,
`bitmap_file::tests::cmyk_jpegs_of_other_programs_keep_their_inks`,
squares written by libjpeg through ImageMagick and Pillow).

A picture keeps the colour mode its file stores (Image > Mode and the
status bar show it): a 1-bit file (PNG, BMP, TIFF) comes in Black and
White (1-bit), a grey one (PNG, TIFF, a one-component JPEG) Grayscale,
one with a palette (PNG, BMP of 4 or 8 bits, GIF, TIFF) Indexed (8-bit);
a 1-bit file of two colours other than black and white is Indexed.
Before, every picture came in as RGB. A palette TIFF, which did not
open at all, opens with its colours: it is read as the grey levels that
are its indices, looked up in its colour map (TIFF 6.0, section 5)
(#3025, `bitmap_file::tests::imported_pictures_keep_their_colour_mode`).

## JPEG XL (#3210)

`.jxl` files, the bare codestream or the ISO container, are read by
`jxl-oxide` (pure Rust, MIT OR Apache-2.0) through the image crate's
decoder interface, under the same memory limit as the other pictures
(`files::decode_picture`): 8 and 16 bits and float, grey or colour,
with or without alpha, lossless or lossy; a CMYK file comes in as RGB
pixels. The codestream's own orientation is applied by the decoder and
an Exif orientation in the file is not applied again
(`bitmap_file::tests::jpeg_xl_files_export_and_import_losslessly`).

## Opening a picture (#2600)

A picture file opened (Open With TraceDraw in the file manager, the
command line, a recent file: PNG, JPG, BMP, GIF, WebP, TIFF, JPEG XL) becomes a
new, untitled drawing named after the file, whose page is the picture's
size at the resolution the file records; the picture fills the page.
There is nothing to undo or save yet, and Save asks where. A file that
cannot be read opens nothing and the status bar says why. File > Open
lists drawings only, as before. The Windows installer adds TraceDraw to
the pictures' Open With list without taking their default, as the Linux
desktop file and the macOS bundle do (check
`clipboard::linked_watch_tests::an_opened_picture_becomes_a_new_drawing`).

## Replace Image (#2598)

Image > Replace Image..., the bitmap's right-click menu and the Pick
tool's property bar (Replace Image) put another picture file in the
selected bitmap's place: the same position, turn and size. A picture of
other proportions fits inside the old box, centred, uncut and
undistorted. The bitmap's effects go (they were made for the old
pixels) and its link names the new file. One undo step (check
`clipboard::linked_watch_tests::replace_image_keeps_the_place_and_size`).

## The bitmap editor

Edit Bitmap writes the picture to a file in the temporary folder, links
the object to it and opens it in the bitmap editor set in Tools >
Options > Global > File Locations (a program file: `gimp`, its `.exe` on
Windows, or an application bundle on macOS, opened with `open -a`;
Browse picks it, Clear empties it). Empty, the default, opens it with the
system's program for PNG files (#593, `export::tests::the_bitmap_editor_gets_the_file`).
The browser build has no outside editor.

The file is a PNG, or a CMYK TIFF for a CMYK picture (a `.cdr` CMYK
image, or one in the CMYK colour mode), holding the inks it prints from
as Save Bitmap As writes them, so the editor works on the inks and not
on RGB pixels (#1834); a picture with see-through pixels goes with its
alpha channel beside the inks, so the transparency survives the round
trip (#1925). A CMYK TIFF read back (8 bits per ink, with or
without alpha, any compression) gives the picture those inks and pixels
that show them through the colour management, and the CMYK mode; any
other file gives pixels only, and inks the picture kept are dropped,
since they no longer fit (`bitmap_file::tests::edit_bitmap_hands_over_and_reads_back_the_inks`).

## Linked files follow their changes

A linked bitmap (an imported file, or one sent to the system image
editor by Edit Bitmap) follows its file on the desktop: once a second
the files' modification times are read, and a bitmap whose file changed
since it was last seen is read again on the spot, as Update from Link
does (`App::watch_linked_bitmaps`; one undo step per change, the status
bar says "Bitmap updated from its file"). So a save in the outside
editor shows in the drawing at once. A file seen for the first time is
only remembered; a file that cannot be read is left alone. The browser
has no files to watch
(`clipboard::linked_watch_tests::a_linked_bitmap_is_read_again_when_its_file_changes`).

## Size

The bitmap comes in at the resolution its file records, so a 300 dpi
scan 1200 pixels wide is 101.6 mm wide:

| Format | Where the resolution comes from |
|---|---|
| PNG | the `pHYs` chunk, when its unit is the metre (pixels per metre x 0.0254) |
| JPEG | the JFIF header's density in dots per inch or per centimetre; else the EXIF `XResolution`/`YResolution` with their `ResolutionUnit` |
| TIFF | `XResolution`/`YResolution` with `ResolutionUnit` (inch or centimetre) |
| BMP | the header's pixels per metre |

Width in mm = pixels x 25.4 / horizontal dpi, height likewise with the
vertical dpi. A file that records no resolution, only an aspect ratio,
or a value outside 1 to 100000 dpi comes in at 96 dpi. Pasted images
and generated pictures are placed at 96 dpi.

Print-size pictures come in whole: decoding a picture file may take up
to 3 GB, 8 bytes for each of the 375 megapixels a PDF's picture is read
at at its own size (room for 16-bit samples, or a CMYK file's samples
beside the RGB result; `files::picture_decode_limit`). The image library's own
guard, 512 MiB, turned away a 1 m square at 300 dpi in CMYK or a
2.5 x 1 m banner at 300 dpi, and nothing came in (#2000,
`files::tests::print_size_pictures_decode_past_the_libraries_guard`,
`image_dpi::tests::a_picture_longer_than_a_metre_imports_at_its_size`).
The browser, Android and 32-bit systems keep the library's guard. A
file past the limit is not imported and the status bar says why.

## On screen and in exports

A picture is decoded once and kept (`crates/render/src/image_cache.rs`,
512 MB of decoded pixels in the desktop build, 256 MB in the browser;
the least recently drawn go first). Drawn at less than half its pixels'
size, it is drawn from a reduced copy instead: level `k` halves the
sides `k` times, each pixel the average of the four beneath it, and the
level used is `floor(log2(1 / s))` for `s` screen pixels per picture
pixel (at most 12), so it is sampled at between half and the whole of
its size. The reduced copies share the same memory budget and are made
when first needed. An upright picture (not turned or slanted) covers
whole pixels: its edges are rounded to the nearest pixel boundary, so
pictures laid side by side meet without a pale seam (#853,
`band_tests::pictures_side_by_side_meet_without_a_seam`,
`band_tests::a_picture_drawn_small_shows_the_average_of_its_pixels`).

## Checks

- Each reader gives the stored value and nothing for missing, broken or
  out-of-range data (`image_dpi::tests`).
- Given a PNG of 4 pixels at 300 dpi, when imported, then it is
  4 x 25.4 / 300 mm wide; a PNG without `pHYs` is 4 x 25.4 / 96 mm wide
  (`image_dpi::tests::import_places_a_bitmap_at_its_resolution`).

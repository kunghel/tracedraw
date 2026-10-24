# Bitmap import

File > Import (Ctrl+I) of a PNG, JPEG, BMP, GIF, TIFF or WebP file
embeds the image as a bitmap object centred on the page and remembers
the file (Bitmaps > Update from Link reads it again).

## The bitmap editor

Edit Bitmap writes the picture to a PNG in the temporary folder, links
the object to it and opens it in the bitmap editor set in Tools >
Options > Global > File Locations (a program file: `gimp`, its `.exe` on
Windows, or an application bundle on macOS, opened with `open -a`;
Browse picks it, Clear empties it). Empty, the default, opens it with the
system's program for PNG files (#593, `export::tests::the_bitmap_editor_gets_the_file`).
The browser build has no outside editor.

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

## Checks

- Each reader gives the stored value and nothing for missing, broken or
  out-of-range data (`image_dpi::tests`).
- Given a PNG of 4 pixels at 300 dpi, when imported, then it is
  4 x 25.4 / 300 mm wide; a PNG without `pHYs` is 4 x 25.4 / 96 mm wide
  (`image_dpi::tests::import_places_a_bitmap_at_its_resolution`).

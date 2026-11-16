# PMD import

File > Open and File > Import read desktop publishing `.pmd` files (and
the `.pm6` and `.p65` files of later versions); so does a file dropped
on the window (#2822). The layout of these files is not published, so
only what can be recovered without it comes in. The reader is
`tracedraw_io::pmd_import`, the drawing is made in
`apps/tracedraw/src/pmd_open.rs`.

## What comes in

- The text: runs of readable Windows-1252 characters that look like
  written words, in the order they lie in the file, one paragraph each
  (a carriage return or line feed ends one). Names the file keeps for
  itself are left out: paths, short names repeated, the program's own
  words (Default, Untitled and such), and lists of short names set at
  an equal distance (font and style lists). The text goes into one
  frame on an A4 page, in black.
- The pictures stored inside: JPEG, PNG, GIF, BMP and TIFF files found
  by their signatures, each on a page of its own (on the first page when
  there is no text), at 96 dpi when it fits within 20 mm margins, else
  shrunk to fit, centred, named Picture 1, Picture 2 and so on. Specks
  under 8 pixels are left out. Their bytes are not read as text.
- A file kept as a compound file ([MS-CFB]) is read stream by stream,
  the largest first; any other file is read as it is.

## What does not

Where each frame and picture sits, the type and its styles, colours,
drawn lines and shapes, master pages, linked pictures (kept in other
files) and the page size. A note says so when the file is opened, with
the count of paragraphs and pictures recovered on the status line. A
file with nothing readable does not open, and says so.

Checks: `pmd_import::tests` (text and lists, pictures by signature, a
compound file, broken input) and
`pmd_open::tests::text_and_pictures_come_in_and_nothing_is_an_error`.

# Resample

Image > Resample (also on the bitmap property bar) gives the selected
bitmaps a new pixel size; their place on the page stays, and so does
their size unless a new one is set (#3138).

## Fields

- Pixels: the new width and height of the first selected bitmap.
- Percent: the same as a percentage of its pixel size.
- Resolution: the dpi the new width gives at the size on the page.
- Size on page: the width and height the bitmap gets on the page, in
  the unit chosen under Units (the document's unit at first). A new size
  keeps the resolution, so the pixels follow.
- Units: the unit of Size on page.
- Fit to: This page or a paper (A4, Letter, A5, A3, Legal) makes the
  bitmap as big as fits on it, the paper turned the way the picture is,
  keeping its shape and resolution.
- Maintain aspect ratio (on): the height follows the width, and back.
- Method:
  - Smooth (photos), the default: Lanczos 3.
  - Sharp: Catmull-Rom, a little crisper.
  - Hard pixels (pixel art): nearest pixel; every pixel stays a square of
    one colour.
  - Enlarge with detail: Lanczos 3, then, when the picture grows by more
    than 5 %, an unsharp mask with radius `0.6 x g` (0.6 to 4 pixels) and
    amount `0.35 + 0.25 ln g` (up to 1.2), `g` the enlargement, on the
    lightness only, so colours do not shift.
- The original and new pixel sizes and the memory of the new picture
  (4 bytes a pixel) are shown below.

Sizes are 1 to 20000 pixels. With several bitmaps selected, each is
scaled by the same factors as the first; a new size on the page scales
each about its centre by the same amount. One undo step per bitmap.

## Save as file

The lower part of the dialog writes the first selected bitmap, at the
size set above, as a file, for forms that limit the size of an upload
(#1901). The drawing does not change.

- Format: JPG, PNG or PDF. A PDF is one page the bitmap's size on the
  page, the picture in it as JPEG. Transparent pixels go on white in JPG
  and PDF.
- Quality (JPG and PDF, 1 to 100, 85 at first): lower makes the file
  smaller, with less detail. PNG has no quality: it keeps every pixel.
- File size: what the file would be, worked out again once the settings
  rest (not while a slider is dragged).
- Target size (KB) and Fit: the quality goes down (not under 10) to the
  highest that fits, found by halving; when even 10 is too big, the
  pixels go down by the square root of how far the file is over, keeping
  the aspect, and the quality is searched again. The fields take what
  fits. When even a picture 8 pixels across does not fit, the status line
  says so.
- Save As: the save dialog, then the file.

Checks: `resample::tests::quality_sets_the_file_size_and_pdf_wraps_the_jpeg`,
`resample::tests::a_target_size_lowers_quality_then_pixels`.

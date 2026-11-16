# Small print in traced pictures

After Trace Bitmap (outline tracing, every preset but the centreline
ones) has made its curves, rows of small capitals and figures, such as a
slogan or a date under a logo, are read and redrawn with the letters of a
real font. Traced, letters a few pixels tall come out soft and lumpy;
redrawn, they have the straight sides, round bowls and counters of the
font. The code is `apps/tracedraw/src/trace_text.rs`.

## How a row is found

The closed curves of the trace are cut into their contours. Contours of
near colours that touch are one letter (a soft letter can be traced as
several patches of shades). Letters 4.5 to 48 pixels tall standing side by
side on one line, of near colours, at most three letter heights apart,
make a row when there are at least three of them.

## How a row is read

The row's part of the picture becomes a coverage map: the background is
the median colour round the row's edge (a row over a picture or pattern,
whose edge colours spread too much, is left alone) and the letters'
colour is that of the strokes' cores. The letters are the inked column
runs (a run much wider than the rest, two letters touching, is split at
its weakest column).

Then every face is tried: each capital and figure is drawn at the
row's letter height, supersampled, placed on each letter by the centre of
its area, blurred as the picture is, and compared with the picture over
the letter's columns. The faces closest overall are tried again at
heights from 0.88 to 1.12 times and five blurs; the best face, height
and blur give the letters. Each letter is then placed to an eighth of a
pixel, all on the row's common baseline, and the colour of the letters is
the one that, laid over the background as the letters cover it, gives
the picture best.

A row is redrawn only when the font draws it almost exactly: the
coverage left over is at most 6 % of the letters' own for the row, 16 %
for any letter. Otherwise the trace is kept. Lower-case rows, script
lettering and letters on a busy background stay traced.

## What changes in the trace

Every contour of the trace lying inside the row's box goes (the
letters' patches and shades, the holes the letters left in the
background), and one curve with the font's outlines takes their place, in
the trace's colour nearest the letters' (or the letters' own when no
colour of the trace is close). Letters with crossing or overlapping
pieces are welded first, so they fill without holes.

## The fonts

TraceDraw comes with 53 faces of 26 free families, cut down to the
capitals and figures (`apps/tracedraw/assets/fonts/trace/`, fetched by
`scripts/fetch_trace_fonts.py`, SIL Open Font License, see `FONTS.md`
there). The fonts installed on the computer are compared too, once the
font list has loaded (not in the browser). Families whose licence
reserves their name are not bundled, because a cut-down copy may not
carry it.

## Speed

A row takes a fraction of a second on four cores (the faces are compared
on all of them). Rows read are kept by their pixels, so tracing again
with another setting does not read them again.

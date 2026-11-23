# Bitmap tracing (bitmap to vector)

| Mode | Use | Defaults |
|---|---|---|
| Quick trace | one click | the preset of Options > Global > Bitmap Tracing (Optimized by default; Clipart before) |
| Centreline: technical illustration, line drawing | strokes | |
| Outline: line art, logo, detailed logo, clipart, low/high quality image | filled regions | smoothing 25, detail per preset, colours per preset |
| Outline: optimized | filled regions, the detail chosen for each picture (see Best settings below) | smoothing 25, at most 32 colours, the detail found by the search (70 shown, greyed) |

| Control | Default | Range |
|---|---|---|
| Trace type | the menu item's (Outline, Centerline) | the Image type list follows: Technical illustration and Line drawing for Centerline; Line art, Logo, Detailed logo, Clipart, Low quality image, High quality image, Optimized for Outline |
| Image type | the menu item's | choosing one sets Detail, Smoothing, Corner smoothness, Colors and the colour mode from the preset, with the choices of Options > Global > Bitmap Tracing |
| Detail | per preset | 0..100 (tick marks every 10, the value in the box at the right) |
| Smoothing | 25 | 0..100 |
| Corner smoothness | 0 | 0..100 |
| Remove | Background color (Options > Global > Bitmap Tracing) | Background color, Entire color from image, None |
| Remove: Automatically / Specify | Automatically | Specify: a colour swatch and an eyedropper that takes the colour from a click on the preview |
| Objects: Merge adjacent objects of the same color | on | |
| Objects: Remove object overlap | off | |
| Objects: Group objects by color | off | |
| Image: Delete original image | off (Options > Global > Bitmap Tracing) | |
| Colors tab: Color mode | RGB (Black and white for the line art presets) | RGB, Grayscale, Black and white |
| Colors tab: Number of colors | per preset | 2..256 |
| Colors tab: list | the trace's colours with their curve counts | tick two or more and Merge selected colors; the swatch of a row changes the colour its curves are drawn in |
| Reset, undo, redo | | Reset: the image type's settings; the arrows step through every settings state of the dialog, Reset included |
| Preview | Before and After | the shared preview (`before-after-preview.md`): Before and After (the picture above, the trace below), Before and After side by side, Before, Large Preview (the trace alone in the whole area), Wireframe Overlay (the curves as blue lines over the picture); zoom to fit, zoom out, zoom in, pan, the wheel zooms, a drag pans |
| Overlay | 80 | with Wireframe Overlay only, a slider in the preview bar: how strongly the picture shows under the curves, 0 (the curves alone) to 100 (the picture as it is); a tint on the picture's rectangle, nothing traced or worked out again |
| Options... | | Options > Global > Bitmap Tracing, then back to the dialog |
| Best optimization | | a button of the bottom row: looks for the detail that traces the picture best (see Best settings below), off the interface thread ("Optimizing..." and a spinner while it runs, greyed for a centreline trace), and puts it in the Detail slider as a step the arrows undo |

## Commands

Image > Trace > Automatic, Repeat Last Trace (the selected bitmaps traced
with the last trace's settings, no dialog), Open Last Trace Settings
(the dialog on them), Trace > Centerline and Trace > Outline with their
presets; a bitmap's right-click menu has the same (#452, `ops2::tests::the_last_trace_can_be_repeated_and_reopened`).
The last trace is kept for the session.

### Progress window

Automatic, Repeat Last Trace and the dialog's OK (when its preview has
not traced every selected picture already) trace off the interface
thread (`trace_job.rs`), so the program keeps answering while a trace
runs (an enlarged picture with Best optimization and small print can take
many seconds). A window (`ui/trace_progress.rs`) shows the turning busy
mark, the step the trace is at (Enlarging the picture, Choosing the best
detail, Finding the colors, Drawing the curves, Checking against the
picture, Matching the letters), "Picture 2 of 3" when several bitmaps
are traced, a bar with the percentage and the time taken. The steps
report through the trace's cancel flag (`trace_progress.rs`); the bar
counts only the steps the trace takes, weighted by about how long each
takes. Cancel, Esc and the close button stop the trace and place
nothing. When every picture is traced the window closes and the curves
are placed as one undo step, as before. The browser traces in the frame
(`trace_job::tests`, `trace_progress::tests`).

## The dialog

A window the user sizes (1040 x 700 points at first, never under 780 x
540; a maximize button in the title bar fills the main window, pressing
it again restores the size). Left, the shared Before and After preview
of the first selected bitmap (`before-after-preview.md`): a Preview
list (Before and After, Before and After side by side, Before, Large
Preview, Wireframe Overlay, with the Overlay slider beside the list in
the overlay view), zoom to fit, zoom out and zoom in buttons, a pan
button, and a spinner while a trace is being made; under the preview
the status line `Curves: N, Nodes: N, Colors: N` (the whole picture's
trace; with a dots sign after it while only the quick look is in).
Before and After stacks the picture over its trace, each in its own
pane, with the same zoom and place; the wheel zooms about the pointer
(fit / 10 to 64 points per preview pixel), a drag pans, the images sit
on a checkerboard (one 2 x 2 texture repeated, a single rectangle a
frame). The preview copy is at most 1024 pixels on its longer side.
Right, the
Settings and Colors tabs over the shared settings panel (the field
colour, framed, each slider under its label and stretched to the
panel's width, a rule between groups; `before-after-preview.md`) and
the Reset button with the undo and redo arrows under it; bottom, the
help button, Options... and Best optimization at the left, OK and Cancel
in the bottom right corner (the row's room left over goes before OK, so
they stay in the corner whatever the width). Enter is OK, Esc and the
close button cancel. With the Optimized image type the Detail slider is
greyed with "Chosen for each picture." under it. Controls that do not apply are greyed, not hidden: a
centreline trace greys Corner smoothness, Remove and the Objects boxes;
Group by color is greyed while Merge adjacent is on (every colour is
one curve already, so no groups are made).

### Live preview

There is no Preview button: the trace is made again whenever a setting
changes, off the interface thread (`trace_preview.rs`), and the After
pane, the wireframe and the counts take the result when it comes in.

- Every request traces in two stages: a quick look on a copy at most 240
  pixels across (left out when the picture is no bigger), then the whole
  picture. The quick look shows within tens of milliseconds; the whole
  picture's trace replaces it when ready, and OK places exactly that
  trace (not made again), so OK is immediate once the counts are in.
- A change while a trace runs cancels it and the new settings are
  traced instead; the pending request is always the latest, never a
  queue. The tracer looks at the flag all along: every few rows of the
  noise removal and of the colour matching, in each round of the colour
  reduction, every 4096 pixels of the despeckling, between colours,
  between the patches of a colour and between the rings fitted, between
  the passes of the thinning; so a stale trace ends within moments and
  the preview never stays busy on it. Closing the dialog raises the
  flag of the running trace too, and the results of a cancelled trace
  are never shown.
- While a slider is dragged (the pointer held) a new value waits 80 ms
  for the value to rest before it is traced; a click, a list, a check
  box or a release trace at once.
- Nothing is traced again while the settings that change the trace
  and the picture are the ones on show: Group objects by color, Delete
  original image, the background colour while nothing is removed, the
  number of colours in black and white, the outline settings of a
  centreline trace are not part of the request. A colour edited on the
  Colors tab is not traced again either: the whole picture's trace on
  show takes the new colours and is drawn again off the interface
  thread. A frame never waits for a trace (results arrive through a
  channel and the worker asks for a repaint); the picture is decoded
  once per bitmap, and the Colors tab's list of colours is worked out
  once with each result, not every frame.
- The quick look traces the art alone: the box of the picture's opaque
  pixels, its transparent margins cut off, shrunk to at most 240 pixels
  (360 where it is the preview's last result), and the curves are moved
  back to where the art lies, across and down each at the scale the copy
  really has (a line a pixel wide keeps its length). The quick look
  traces at the settings' own detail: the Optimized preset's search is
  made for the whole picture only (on the desktop its trace follows at
  once; in the browser OK makes it), so a frame never runs the search. A drawing with wide transparent margins
  kept only a fraction of the copy before, and its preview came out in
  blobs.
- In the browser (no threads) the quick look is made in the frame; the
  whole picture is traced on OK when it is larger than 400 000 pixels.
  The quick look is then the request's last result: the preview rests
  on it (nothing is traced again while the settings stand, the spinner
  stops, an idle dialog asks for no frames) and the counts show it
  without the dots sign. A note in the error colour beside the counts
  says the preview is reduced, that OK traces the whole picture and that
  the desktop app shows it exactly. Every result says whether it is the last of
  its request (`Shown::done`), so a result that will be followed by
  another never counts as the end.
- A trace whose thread ended without a result is not started again:
  the preview keeps what it shows until a setting changes.

### Remove

- Background color: the regions of the background colour that reach the
  picture's edge (4-connected flood from the border) are left out; the
  patches of that colour inside the picture (the white of an eye) are
  traced as curves of their own.
- Entire color from image: every pixel of the background colour is left
  out, inside the picture too.
- None: the background is traced like any colour.
- Automatically: the background colour is the commonest palette colour
  along the border that is paper (brightness 180 or more, its channels
  within 48 of each other). A dark or coloured surround is part of the
  design (the dark square behind a gold emblem) and stays unless its
  colour is specified (#2928). With Background color, the background's
  soft shades go with it: from the border on through every pixel within
  56 of the background colour and 10 of its neighbour that lies on no
  edge (a vignette, a faint gradient), and the edge pixels a shade so
  emptied leaves round the art. The edge pixels of a shade beside the
  pixels of it that went go too when flat pixels of the shade stay
  elsewhere (inside the letters): in a picture traced on its own pixels
  they were a pale line round every letter. Specify: the colour of the swatch, or the colour
  under a click on the preview with the eyedropper (taken from the
  picture, so an anti-aliased or compressed colour still stands for the
  palette colour nearest it).

### Objects

- Merge adjacent objects of the same color (on): every region of a
  colour is one combined curve (its holes included). Off: each separate
  patch (8-connected) of a colour is a curve of its own, with its holes.
  In this tracer every region of a colour is found at once, so "adjacent"
  joins every region of the colour, not only the touching ones.
- Remove object overlap (off): a region is drawn whole under the
  regions in its holes (step 3 below), so its curve is its outline
  alone. On: each region is exactly its own pixels, holes and all.
  Either way, where two regions meet both use the same curve, so no
  page shows between them and no curve runs beside another (#1250).
- Group objects by color (off): on, the trace's group holds a group per
  colour (in the order the colours stack), each with that colour's
  curves.

### Colors tab

Color mode (RGB, Grayscale: the picture's greys quantised to the number
of colours, Black and white: two colours by brightness), Number of
colors (2 to 256), `N color(s) in the trace`, then the colours of the
trace on show with their curve counts, each with a check box and a
colour swatch. Merge selected colors joins the ticked colours into the
first of them in the list's order (the pixels of the others take its
label before the regions are found, so one combined region comes out);
a colour of the list must match a palette colour within 12 (RGB
distance), so a palette changed by other settings never joins colours
by accident. The swatch of a row changes the colour its curves are
drawn in (kept as traced colour to shown colour). A change to what is
traced (detail, smoothing, corner smoothness, colours, mode, trace
type or image type) forgets the merges and the edits, which belonged to
the trace before; a step of the undo and redo arrows does not, it
brings back the settings of that step as they were, merges and edits
included.

### Reset, undo, redo

The settings as they stand after each change (a slider's value once the
pointer is released, not every value of a drag) are a step; the arrows
step back and forward through them, and a change after an undo drops
the steps undone; the arrows themselves never drop the steps to redo.
Reset puts the image type's settings back as a step of its own, so it
can be undone.

## Algorithm (outline mode)

1. Noise (#611, off at detail 88 and up: taking it out there takes the
   finest lines of the art too):
   each colour channel of each opaque pixel becomes the median of the
   pixel and its four edge neighbours (opaque ones only), once at detail
   75 and up, twice below. Lone odd pixels and the grain of compressed
   images go, straight edges, square corners and one pixel lines along
   rows or columns stay (a line's end pixels go). Without it the grain of
   flat colours split them into many shades whose patches left holes
   showing the page inside the art.
   Blur a little when detail < 50, then quantise the colours with median
   cut to the number of colours (black and white: by brightness). Below
   the highest detail, in art drawn in flat colours (at least 30 % of the
   opaque pixels lie inside one: their 8 neighbours within 24 of them),
   the palette is cut from those pixels only, so the in-between colours
   of anti-aliased and compressed edges take no places of their own; the
   colours of the palette of every pixel that lie further than 56 from
   all of those and not between the two nearest (a hairline of a colour
   of its own) take up to a quarter of the places, the flat colours the
   rest: never more colours than Number of colors.
   Split colours (#611): in flat art (the 8 commonest colours cover 85 %
   of the pixels) two colours of the palette nearer each other than 40
   (RGB distance) and than the sum of their spreads (the root mean
   square distance of their pixels from them), or that near and
   scattered through each other (they share at least 0.6 pixel edges
   per pixel of the smaller), are one colour split by its noise, and
   join, at the mean of their pixels; two groups join only while their
   means stay within 40 and every colour of one lies within 40 of every
   colour of the other, so a run of shades each near the next (a pink
   cheek, the blends of its blurred edge, the yellow round it) does not
   chain into one mean colour none of them had. The steps of a gradient
   and distinct colours however close (a 246 grey on white) lie further
   apart than their spread and stay; photos are left as they are.
   Near shades: below detail 95, two palette colours nearer than
   `10 + (100 - detail) / 2` (30 at the Clipart detail) join, nearest
   first, when one is solid (at least half its pixels flat) and the
   other not (the fringe of a black), or when their pixels are scattered
   through each other (they share at least 0.6 pixel edges per pixel of
   the smaller: a flat colour's noise split in shades); once a group has
   grown, another joins it only while the means of their pixels stay
   that near, so a run of shades does not chain, while two lone colours
   that near join whatever their pixels' means. Two solid colours,
   however close (a 246 grey logo on white), and the bands of a gradient
   stay. A group keeps its solid member's colour; rounds repeat until
   nothing joins.
   Then each palette colour moves onto the commonest exact colour of its
   pixels when that is within 16, so a black traces as #000000 and the
   art keeps its own colours. Black and white is left out of both: its
   colours stay #000000 and #FFFFFF. The colours no pixel has any more
   then leave the palette, so a background colour specified or picked,
   and the colours merged on the Colors tab, are matched among the
   trace's own colours.
2. Edges (#611, #1250, off at detail 95 and up, where the specks stay
   too): a pixel is flat when its 8 neighbours share its colour, and a
   colour is a colour of its own when at least half its pixels are flat
   or it has a core somewhere (a flat pixel whose 8 neighbours are flat
   too: a block of 5 by 5; a small feature such as the cheek of a face
   a dozen pixels across has few flat pixels against its edge, but a
   core); the others only line the edges (the fringes of a blurred or
   compressed edge, a video frame, a scaled screenshot). A colour with
   fewer than one pixel in 8 flat is a fringe colour (a band a pixel or
   two wide however long; the bands of a photo's gradient, no colours
   of their own either, are flat inside), and its pixels are fringe
   pixels where they lie in a band between two colours of their own:
   the pixels of the colours that are none, taken together whatever
   their colours, in a 4-connected region touching two colours of their
   own, the second along a tenth of the region's edge at least (the
   glint in an eye, a few shades of white none of them a colour of its
   own, touches the dark of the eye alone and is no fringe). A pixel
   that is not flat, or is of a colour that is none of its own, with no
   flat pixel of its own colour (of its own) within reach, looks at the
   flat pixels of colours of their own within reach (4 px; 8 px for a
   fringe pixel, since the fringes of a compressed edge come in several
   shades, bands side by side, and from the inner band the colour across
   the outer one lies further): of every pair of them it takes the pair whose RGB
   segment its colour is nearest, when it lies between them (within
   `max(12, 0.08 * distance)` of the segment; not the two colours
   nearest it, as a grey of a black edge on white lies nearer a pale blue
   beside them than black), and goes to the one it is nearer along that
   segment, so the edge runs where the art's anti-aliasing puts it;
   else it takes the nearest colour when within 48. A fringe pixel
   whose own colour lies off every segment (the blends of a compressed
   edge drift in colour, which is stored coarser than brightness, and
   overshoot the colours either side) is placed by its palette colour
   instead, between a pair with twice the slack, or a compressed blend
   of the pair (its brightness, and each of its two colour differences
   against the brightness, between theirs within the slack: along an
   edge of black and yellow the pixels run from black to a pale,
   greyish yellow, which lies off the segment yet between the two in
   brightness and colour taken apart), or joins the colour of its own
   within 64 of its palette colour, going to the side its own colour is
   nearer. Lines of their own colour (text, a hairline on a fill) keep
   it: they lie far from every colour around them and off every
   segment.
   Fringe bands (#1250): a 4-connected patch at most 5 px thick (twice
   its area over its edge count), of a colour between the two colours
   it touches most (at least 64 apart, the second on a tenth of its
   edges or more; between with twice the slack above, a compressed
   blend of the two as above, or within 64 of one of the two, as the
   overshooting band of a compressed edge is),
   both solid where they touch it (at least half their
   pixels there flat or beside a flat pixel of their colour), is the
   soft edge of the two: each of its pixels takes the one its own colour
   is nearer along the segment joining them. The pieces of a thin letter
   in two shades stay, the other shade being no solid area.
   Specks: first the colours with fewer pixels in all than the speck
   size below join each other when within 80 (a glint blurred into a
   white and two greys, a pixel or two of each, is one speck of nine
   pixels, not nine specks of one pixel passing their pixels around);
   then every 4-connected patch of one colour smaller than
   `round(((100 - detail) / 10)^2)` pixels (4 at 80, 16 at 60, 42 at 35;
   2 at least below detail 100, so the Detailed logo preset at 90 drops
   single stray pixels; none at detail 100) takes the colour of a neighbour, so stray dots and
   the fringes of soft edges make no curves of their own: the neighbour
   whose colour is nearest its own when one is within 40; else, when its
   colour lies between two neighbours at least 64 apart (the anti-aliased
   part of a thin dark stroke on a light fill), the one it is nearer;
   else, when its colour is at least 64 from every neighbour's, it has
   at least a quarter of the speck size (2 pixels at least; 4 pixels are
   enough when it stands at least 160 from every neighbour, the glint in
   a dark eye, which noise never does once the picture is quietened) and
   spans two pixels both ways, it is a dot of the art (a halftone dot,
   the end of a hatch line) and stays; else the one most of its
   neighbours have.
   Patches joined into others are looked at again (up to 4 passes), so a
   letter split into several dark shades stays a letter. Transparent
   pixels are left alone.
   Bumps and notches (#1250, off at detail 95 and up): twice over, a
   pixel at least 5 of whose 8 neighbours have one other colour takes
   it, when its own neighbours are the three on one side of it (a bump
   of a pixel on a straight edge, or a notch in one). Corners, the
   pixels of a line one pixel wide (their own on two sides) and its end
   stay.
3. The borders between the regions, along the pixel edges, are found
   once over the whole picture: each runs from one junction (a pixel
   corner where three regions meet, or two meet diagonally; the
   picture's edge and the background taken out count as a region) to
   the next, or round a loop (an island inside one region). Each border
   is fitted once (step 4), and every region's rings are put together
   from the borders around it, forwards on one side and backwards on the
   other, turning right where a region touches itself at a corner. Where
   two regions meet, their outlines are the same curve: no gap, no
   overlap, and no second curve a pixel or two beside the first (#1250;
   before, each region was fitted on its own and reached 2 px under its
   lighter neighbours, so the wireframe showed two curves along every
   border and the nodes were doubled). The colours stack bottom first:
   a colour found in another's holes (with no page showing in the hole)
   above it, the head under its eyes and cheeks; among the colours free
   to go next the darkest; colours in each other's holes (black eyes in
   a yellow head inside a black outline) darkest first, unless one of
   them holds at least four times as many pixels of the other as the
   other holds of it: that one is the one around. A few blots of the
   ground left between two colours of the art (a compressed picture, the
   blend at a thin gap) no longer put the ground over the outline it
   holds, where the two would meet cut to the same edge with the page
   showing between them as a light line (#3727). A hole smaller than
   half a pixel of the art is no hole. Unless Remove
   object overlap is on, every hole of a region (4-connected) whose
   regions stacked below it cover at most one part in twenty of it is
   filled (those blots go under it, hidden): the region is drawn whole
   under them, so its curve is its outline alone and the edges inside
   are drawn once, by the regions on top (a black outline drawing
   becomes one silhouette under its colours, a head one ring under its
   eyes). Regions side by side meet along their shared curve, as in the
   trace the program that wrote the #1250 sample made of the same
   picture (each region one ring, the borders coinciding).
4. Each border is fitted on the finer grid (see Fine edges below), at
   every smoothing, 0 included (#2925: Detail 100 with Smoothing 0 put a
   node on every pixel step, 14 479 nodes for a logo):
   - one point in the middle of every step of the finer grid, smoothed
     lightly (0.45 px; 0.9 px in a picture with hard edges) so the grid's
     steps go;
   - corners and the ends of thin strokes found on the straight sides
     of the border (see Corners below); a corner is put where the lines
     of its two sides cross, so a square keeps its corners to a tenth
     of a pixel;
   - the points between corners smoothed with a spread of
     `0.3 + smoothing / 100 * 1.5` px (1.4 px at least in a picture with
     hard edges), the rounded part beside each corner left out and the
     two ends fixed. Each smoothing pass is followed by one that pushes
     the points back out (1.06 times what the pass after would pull
     in), so a high Smoothing takes the ripples out without drawing
     round shapes in;
   - a border between two junctions keeps its ends on them, where the
     borders of the other regions meeting there end too, and is fitted
     like a stretch between corners;
   - each stretch between corners is one straight line when no point
     lies further than 0.2 px from it, else as few cubic Beziers as keep
     every point within `0.35 + ((100 - detail) / 100)^1.5 * 0.6` px
     (0.2 before: a tenth to a quarter more nodes for the same likeness)
     (a ring without corners in quarters meeting smoothly, grown about
     its middle by up to a quarter to the area of its border, so a dot
     of three pixels is not drawn smaller): least
     squares on chord-length parameters with Newton steps, split at the
     worst point (P. J. Schneider, "An Algorithm for Automatically
     Fitting Digitized Curves", Graphics Gems, 1990).
5. Stack regions as step 3 orders them; leave out the background (see Remove
   above) when requested. The background found (no colour specified) is
   the commonest colour on the picture's edge; when no colour touches
   the edge (art on a transparent background) there is none and nothing
   is removed (#1250: the commonest of none was the last colour of the
   palette, and Remove entire color took it out of the art, such as the
   yellow of a bee's head, leaving its fringe as an outline). The same
   when the edge is mostly transparent: a colour is the background only
   when more of the edge is that colour than is transparent (#1930: the
   stems of a floral pattern that run off the edge were its only colour
   there, so Remove background took their black for the background and
   erased every shape touching the edge).

## Fine edges

The colour of a pixel on the edge of two colours says how much of it
each covers, so the edge is traced inside the pixels (`trace_fine.rs`;
#2925, #2926, #2927, #2929).

- Edge pixels: from each pixel, steps to the brightest and to the
  darkest neighbour (by the red, green or blue channel alone where that
  channel changes 1.5 times more than the brightness in the 3x3 block:
  an edge between two colours equally bright, purple on blue, is found
  too) while each gains 5 levels, 6 steps at
  most, find the two ends of the edge it lies on (48 apart at least)
  and its share between them. After the cleaning of step 1 each edge
  pixel takes the label of the nearer end: the label the cleaning left
  that end when its colour is within 48 of it; else (the core of a
  stroke too thin to reach its colour, which the noise filter took out)
  the nearest colour still in the picture that the end lies on the way
  to from the colour at the other end, at least 0.3 of the way, else the
  nearest. So no third colour that looks like the blend rings a shape
  (the grey halo round black letters), and a grey hairline in a black
  and white picture is black.
- A step of a thin slanted line touches the next by a corner only: a
  small patch that stands out from its neighbours (64) and reaches the
  smallest kept area with the patches it touches by corners is no
  speck.
- In flat art, a colour with no flat pixel (every stroke of it under
  three pixels wide) lying between the colour around it and a colour
  with flat areas, at least 0.3 of the way, is that colour: grey small
  print is black.
- The labels are made on a grid 4 times finer (fewer times for large
  pictures: 6 million fine pixels at most): the stand-in colours of the
  pixels (the blend of the two labels' colours by the pixel's share on
  an edge) are interpolated (Catmull-Rom) and each fine pixel takes the
  nearest colour among the labels of the four pixels around it. Borders
  are walked and fitted on this grid and the curves scaled back, so an
  edge covering 30 % of its pixels is traced 0.3 px in, and the trace
  lies on the picture to a fraction of a pixel.
- A picture with hard edges (at least 70 % of its borders between full
  colours, no blend) has nothing inside its pixels to read: its steps
  are smoothed more instead.
- An end of an edge is taken to be full colour when it is within 56 of
  its label's colour; a fainter one (the core of small print, which
  never reaches its colour) counts as three quarters of the way, so thin
  letters are not traced thinner than they are. An end pixel with fewer
  than two neighbours within 24 of its colour may take the colour it
  lies on the way to even when its own label is near: the gap inside a
  small letter becomes paper.
- A patch of two pixels or more with one colour all round it and at
  least 120 from it is a gap of the art (the bowls of a small B) and is
  no speck.
- Sharpening before the edges are placed, by itself: the softness of a
  picture is the number of edge pixels that are a real mix (0.1 to 0.9
  of the way) for each place two labels touch. A crisp anti-aliased
  picture has about 0.7, one enlarged or blurred 1.0 to 1.3. From 0.8
  on, growing to full at 1.25, the stand-in colours are sharpened
  (unsharp mask, blur of 0.75 px, up to 2.2 times the difference), so
  the bowls and gaps of small print open before the finer grid is made;
  corners are also taken to be rounded further by the blur (below). A
  crisp picture is left as it is.

### Corners

Corners are found on the polygon of each lightly smoothed border
(Douglas-Peucker, 0.15 px).

- Each side has a line fitted through its points (least squares). A
  line within 0.15 px and 8 degrees of level or upright (12 degrees for
  a side under 4 px; a side under 0.9 px is too short to say) is level
  or upright exactly, so the stems and bars of small print come out
  straight.
- Ends of thin strokes: a half turn (132 to 206 degrees) over at most 5
  short sides (1.5 px each), between two longer sides running opposite
  ways no more than 2.6 px apart, is two corners on the line across the
  tip. A short slanted side beside the tip takes the direction of the
  level or longer side across the stroke (the blur slants the short
  underside of a T's arm). Two sides of 2.2 px or more closing on each
  other by more than 20 degrees are a wedge: one corner where they meet
  when that is within 2.2 px of the tip (the notches of an N), else a
  blunt end square to the level or upright side (the top of an N's
  stem).
- Other corners: a run of polygon points each turning 25 degrees or
  more the same way, no more than 0.9 px apart (1.5 px in a soft
  picture). The corner's two sides are, on each side of the run, the
  first of the next three sides that is level or upright (the first is
  often the slope the blur leaves), else the first of 0.9 px or more; a
  side under 2 px gives way to one of 4 px or more after it that runs
  another way. A side goes on through the sides after it that lie on
  its line (level ones within 0.25 px, slanted ones turning under 8
  degrees), and the line is the longest one's.
  Their lines must turn by `35 + corner smoothness * 1.3` degrees at
  least, 170 at most. A run turning more than 110 degrees is two corners
  (a notch) when one side inside it stands out 1.4 times from the next
  longest, else none.
- No corner where the curve is the art's own: the rounding between the
  two sides (the chord from where the border leaves the line of the one
  by 0.15 px to where it comes onto the line of the other) may be no
  wider than a corner of radius 0.7 px rounds (1.2 px in a soft picture;
  1.4 times that between two level or upright sides of 2 px or more
  that do not bend on; 1.6 px at least between two sides of 4 px or
  more), and a slanted side that turns on at its far
  end like an arc's (15 to 46 degrees, turn times length up to 1.3) is
  part of a bowl.
- A bowl has no corner: four polygon points or more in a row turning
  the same way by 15 to 60 degrees each, 150 degrees or more in all,
  no more than 1.5 px apart, the longest side between them at most 1.4
  times the middle one (the round bottom inside a small U).
- A round ring has none either: eight polygon points or more, every
  one turning the same way by at most 60 degrees, the longest side at
  most 2.5 times the side a quarter of the way up from the shortest
  (the inside of an O; a square with rounded corners has four long
  sides and keeps its corners).
- A ring no longer than 10 px round is smooth when only one corner was
  found on it, and in a picture with soft edges whatever was found.
- Between two level or upright sides the wider rounding (1.4 times) is
  also allowed when one of them is a straight side of 4 px or more that
  does not bend on, whatever the other does (the corner where the stem
  of a B meets its top).

## Enlarged pictures

`Settings::pixel_scale` says how many pixels of the picture make one
pixel of the art: 1 for a picture traced as it is, 4 for one enlarged
four times before the trace. Above 1:

- every area the trace measures in pixels is in pixels of the art: the
  smallest patch kept, the smallest dot, the glint and the gap of a
  letter are `pixel_scale` squared times as many pixels;
- soft-edge bands (fringes) may be `pixel_scale` times as thick;
- the picture is put on a grid twice as fine at most, where its size
  allows (`scale_for`, 2 at most): its edges are sharp already, and
  half a pixel places a straight edge without the one-pixel steps a
  grid of its own pixels leaves in it;
- the corner finder and the smoothing work in pixels of the art times
  0.875 (an enlarging model rounds corners a little less than blur
  does), and the curves may stray half as far as in a picture traced
  as it is (its edges are placed more exactly);
- the gap of a small letter (one colour all round, 120 or more from
  its own) stays from half a pixel of the art: the enlarging draws the
  gap of an A five pixels high smaller than it was;
- the darker rim enlarging leaves along the edge of a colour with a
  lighter one is that colour's: an edge pixel whose colour lies beyond
  the colour it is the edge of, seen from the colour across the edge,
  keeps it, where it would be a line of the nearest darker colour of
  the picture;
- the place two sides meet may lie 0.4 times as far from the rounded
  corner as in a picture traced as it is (the tip of a small A is where
  the enlarging drew it, not well above the letter);
- the round end of a thin stroke is squared off only when its two
  corners would lie within a quarter of the end's width of the border
  (a square end rounded to half a circle leaves them a fifth away): a
  round end whose corners would lie further is a bend (of an S) and
  stays round;
- the curves come out in pixels of the picture given; whoever enlarged
  it scales them back.

### Faithful fit and the check

The borders of an enlarged picture are fitted by `trace_fit`, not by
the fitter of a picture traced as it is: the enlarging has drawn every
edge sharp, so the curves copy the picture and do not guess at what a
blur hid.

- Where the edge runs between the pixels is read from the picture:
  across each point of a border (1.5 px either side) the colour goes
  from one region's to the other's, and the point is put where it is
  half way. A border moves only when its two colours are 40 or more
  apart and four fifths of its points show that step; one between two
  shades too alike to tell, or seen only here and there, stays as the
  labels drew it.
- Corners are found by how much the border turns over 1.5 px against
  3 px either side: a corner turns as much over the short reach, an
  arc about twice as much over the long one. A corner is put where its
  two straight sides cross when that lies within 1.1 px of the border;
  a rounded one is no corner and stays round.
- The round end of a thin stroke is made square. A stroke no wider
  than 2 px of the picture before it was enlarged has no shape of its
  end in that picture: the rounding is the enlarging's. Where the
  border turns back on itself round such a stroke (2.6 radians or
  more, its two sides straight within 0.3 px), the end is put level
  or upright when it is nearly so and else square across the stroke,
  a tenth of the stroke's width back from the round tip. Only a turn
  round the shape is squared: of the two regions of a border the one
  covering less of the picture is the shape, so the inside of a U and
  the ends of a counter stay round.
- The straight runs of a stretch are lines: 14 px or more within
  0.3 px of one line, the border turning 0.25 radians or more over
  the 2 to 8 px past each end (6 px within 0.2 px between two turns
  of 0.6 radians over 1 to 4.5 px). A wide arc is as straight over a
  short way but turns no faster past the run than along it, and is
  no run: the side of a stem is a line and a circle stays curves.
- Each stretch between two corners is the fewest lines and cubics
  that stay within the tolerance (0.85 of that of a picture traced as
  it is: about a third of a pixel at full detail, more as the detail
  falls) of its points, found over every way to cut it at
  points a pixel apart. Of two ways with as many pieces the one with
  the shorter curves is taken, so a straight side is a line to its
  end. A line nearly level or upright (within 0.09 radians, its
  ends moving 0.8 px at most) is made exactly so, and the end of a
  stroke between two straight sides is put level, upright or square
  across it when its corners lie within 1.5 px of the bend.
- Every stretch is then measured against its own border, both ways
  (each point of the border to the curve, each point of the curve to
  the border). One that strays is fitted again at 0.6 and at 0.35 of
  the tolerance, and at last drawn as the border's own points in
  straight pieces. A fit that deforms a small letter is not kept.

Round shapes are fitted as what they are:

- a closed border with no corner that lies within 1.3 tolerances of a
  circle, or of an ellipse whose axes are level and upright, is that
  shape exactly: four curves with their nodes at its top, bottom and
  sides. An ellipse whose two radii differ by less than 3% is a circle;
- a stretch between two corners, or between two straight runs, that
  lies within 1.3 tolerances of an arc of a circle turning 0.9 rad or
  more is that arc, in curves of at most 1.95 rad each, in place of the
  straight runs and free curves it would have been. An arc of a level
  ellipse is taken the same way when it turns 2 rad or more and lies
  within half that distance (an ellipse bends to fit what is not one,
  the round end of a U with its two sides), no flatter than 0.3;
- an arc keeps the ends of its stretch (bent to them at its two ends
  only) and meets a straight run along it; one that would meet it more
  than 0.2 rad off is not taken;
- a round between two straight runs that lies within 1.3 tolerances of
  a circle touching both lines is the arc of that circle, from where it
  touches the one to where it touches the other (the inside of a U is
  two sides and a half circle); each run keeps at least 30% of its
  length;
- a shape that is none of these (an ellipse turned on its side, an S)
  is the fewest lines and cubics, as before.

### Small letters by their strokes

A letter a few pixels tall in the picture before it was enlarged has
strokes about two pixels wide there. The enlarging gives it an outline,
but its stems lean, its bowls bulge and its strokes thicken and thin.
Such a letter was drawn with a pen of one width, and that says more
about it than its outline does (`trace_strokes`):

- a letter is a shape (in one colour, or in shades of it within 48 of
  each other, which become the shade most of it has) no larger than 20
  px of the picture before it was enlarged, whose strokes are 0.8 to
  2.7 of those pixels wide and of one width (80% of its middle line
  within 30% of the middle width);
- the letter is thinned to its middle line. The short branches the
  thinning grows into the corners of a square end are dropped, two
  strokes going straight through a meeting (within 0.5 rad) are one
  stroke, straight across it, and a branch ends straight on the
  stroke it meets (the bar of an H runs straight from stem to stem);
- each stroke's middle line is fitted like a border, to 0.2 px of the
  picture before it was enlarged: straight lines, arcs, corners. A
  short piece that cuts the corner between two lines is the corner
  where they cross;
- a middle line with curves has its straight stretches taken first: at
  least 1.5 widths long (12 points of the grid at least, 70% of it the
  stretch's own and not what the line goes on past a free end), no
  point further than 0.6 points from its line, its two halves within
  0.1 rad of each other. Each is a line, and what lies between two of
  them is the arc that joins them when it is one. Two lines going
  opposite ways with a round between them are joined by the half
  circle that touches both (a U is two stems and a half circle);
- a line of the middle line within 0.07 rad of level or upright is
  level or upright;
- the letter is drawn again with those lines at the width its strokes
  have (a stroke at least two widths long that differs by more than
  12% has its own, 30% at most: the bar of a T thinner than its stem).
  A free end is cut square across the stroke, where the picture's
  stroke ends, and is reached along the bend the stroke has there (0.08
  to 0.8 rad over one width: the ends of a C go on round its ring); a
  corner of the middle line is a pointed corner;
- letters side by side are a word: of one colour (or shades within 48),
  their heights, tops and bottoms within 15% of the taller one's height
  of each other, no further apart than 3 heights. A word of three
  letters or more has one width of stroke, the middle one of its
  letters, taken by every letter whose own is within 25% of it (and
  then by all its strokes). A level line of the middle line, or an
  upright free end, within 0.75 px of the picture before it was
  enlarged of the word's top or bottom (the middle ones of its
  letters) lies on it, so the letters are of one height. A letter that
  no longer agrees with the picture this way is drawn again by itself;
- the letter drawn again is compared with the picture's: where more
  than 1.5% of it lies further than 0.6 px of the picture before it
  was enlarged (0.3 of the stroke's width at most) from the letter of
  the picture, or it has another number of holes, the letter is left
  as the picture has it;
- the borders of a letter drawn again are fitted as they are, and are
  not placed by the picture's colours again.

So an F, an H or a T of small print is a few exact lines with square
ends, and an O a ring of two ellipses. A shape that is no stroke (a
dot, a leaf, a letter with thick and thin strokes) is not touched.

After the curves are made the whole trace is checked against the
picture, up to twice:

- the trace is drawn at the picture's size and compared with it as
  `wrong_share` does (a colour 60 or more from every colour drawn
  within a pixel);
- each patch of 0.75 px of the art or more that differs (the gap of a
  small letter the cleaning closed) takes the one colour of the
  palette the picture is nearest there, when that is much nearer
  (0.6 of the distance) than the colours it had and is no shade of
  what is drawn there (within 60 of it: as a patch of its own it would
  be a seam inside the shape);
- the curves are made again and the change is checked the same way: a
  patch more than half of which still differs goes back to what it
  was. With no patch kept the check ends.

A small piece of a shade inside a small letter (16 px of the art at
most) joins the larger piece beside it when the colours the two have
in the picture, all in all, are within 48: the letter is one shape
whose colour runs from one shade to the other, drawn as a blend, with
no seam across it. A piece whose own colour is further (a pale leaf on
a darker one) stays a shape.

At any scale, the pale inside of a small letter that opens onto the
paper is paper: a patch of at most 40 px of the art, most of its edge
against a colour 120 or more from its own, that also touches a colour
within 48 of its own takes that colour (the inside of a small G or C,
left a shade darker than the paper by blur or by enlarging, was a
region of its own closing the letter).
- The cleaning of edges, fringes and specks runs at every detail, 100
  included.

## Gradients

In flat art, the bands the colour reduction cuts a blend into are one
region again (`trace_blend.rs`, #2929). A patch is part of a blend when
its colour changes by at least 0.25 levels a pixel (a plane fitted to
its pixels, edge pixels left out); a patch under 12 pixels or one pixel
wide joins a blend beside it. A single such patch with no blend beside
it is a blend by itself when a linear blend leaves at most 0.35 of its
colour variance. Two patches join when the colours two
pixels either side of their border are within 18 at the middle border
place. Flat colours never join, however soft the edge between them. The
region takes one colour (the mean) and, when its pixels lie within 18
(root mean square) of a blend along the direction its colour changes
most and the two ends are at least 24 apart, a linear blend from the
colour at one end to the colour at the other: drawn as a gradient in
the preview and placed as a linear fountain fill. Recolouring a colour
on the Colors tab makes its region flat.

## Presets

Logo: detail 75, smoothing 20, 16 colours (50, 25, 8 before: letters
melted and slogans were lost). Detailed logo: detail 90, smoothing 15,
32 colours (75, 20, 24 before).

## Best settings (Optimized, Best optimization)

The detail that draws a picture best: each detail of 50, 60, 70, 80,
90 and 95, and the detail the settings have, traces samples of the picture at its own size, its art alone (the
box of its opaque pixels) when no more than 400 pixels across, else the
four squares of 200 pixels in it with the most edges (neighbouring
pixels more than 60 apart), side by side a transparent pixel apart,
with nothing removed (the samples hold the art only). Each trace is
drawn back and compared with the samples: a pixel is drawn wrong when
its colour in one is further than 60 from every pixel of the other
within a pixel (an edge a pixel off is not wrong; a lost dot or line, a
region in another colour, a gap are). The detail with the lowest score
wins: the share drawn wrong (percent) plus 0.3 per node per pixel of
the samples' sides, so a finer detail is chosen while it draws the
picture better for the nodes it adds, and no further. The detail the
settings have stays unless another scores better by more than 0.05, so
the search never leaves a trace worse than the one it started from and
a second search changes nothing (#2930). On the
desktop the details are traced at once, a thread each; in the browser
one after the other, and only 60, 70 and 80. A search whose picture or
settings changed before it ended (another image type, a detail set by
hand, Reset, a step undone) is not put in. Enter on the focused Best
optimization button starts the search and leaves the dialog open.

Centreline mode: Zhang-Suen thinning, then the skeleton is traced into
polylines, smoothed (Catmull-Rom) and output as curves with a stroke
width equal to the mean local thickness.

## Checks

- `pick_drag_tests::the_trace_preview_follows_a_slider_without_a_button`:
  a click on the Detail slider changes the value and the trace on show
  is made again, with no Preview button in the dialog.
- `pick_drag_tests::changing_the_trace_type_swaps_the_image_types`:
  Centerline in the Trace type list chooses Technical illustration and
  the Image type list offers Line drawing, not Logo.
- `pick_drag_tests::the_eyedropper_specifies_the_background_colour`:
  Specify, the eyedropper and a click on the black part of the preview
  make black the background, so the white is traced instead.
- `pick_drag_tests::merge_adjacent_and_group_by_color_change_the_result`:
  two black squares are one curve, two with Merge adjacent off, and OK
  with Group by color places a group holding one group of the two; with
  Merge adjacent on again, Group by color is greyed (a click does
  nothing).
- `pick_drag_tests::the_trace_preview_list_offers_large_preview_and_the_wireframe_overlay`:
  Large Preview shows the trace alone in the whole area; Wireframe
  Overlay shows the Overlay slider (80 at first), clicks on it set the
  value, the slider goes with the other views, and the idle dialog in
  the overlay view asks for no frame and traces nothing again.
- `pick_drag_tests::reset_and_the_arrows_undo_the_settings`.
- `pick_drag_tests::the_arrows_keep_merged_colours_and_the_steps_to_redo`:
  two colours merged, then a new detail forgets the merge; undo brings
  it back with the step to redo kept, redo goes forward again.
- `trace_dialog::tests::only_settings_that_change_the_trace_change_its_key`.
- `pick_drag_tests::frame_times_of_the_trace_dialog` (ignored): frame
  times of a slider drag with a 2000 x 2000 picture in the dialog, how
  long the whole picture's trace takes to show, and the idle dialog's
  frames once it is in (how many ask for another frame).
- `pick_drag_tests::frame_times_of_the_trace_dialog_resting_on_the_quick_look`
  (ignored): the same where the whole picture waits for OK (the
  browser's way), idle on the quick look and dragging Detail.
- `pick_drag_tests::the_trace_dialog_rests_once_the_trace_is_in`: with
  the trace in, on the desktop and where the whole picture waits for
  OK, idle frames ask for no frame, start no trace and draw no spinner;
  a change traces once more and the dialog rests again.
- `trace_preview::tests`: a request gives the quick look then the whole
  trace, a cancelled one nothing, and the live preview traces each
  distinct request once, debounced while the pointer is held; new
  colours alone are not traced again
  (`new_colours_alone_are_not_traced_again`); closing the dialog ends
  the running trace soon (`clearing_stops_the_running_trace`); with the
  whole picture deferred the quick look is the last result
  (`with_the_whole_picture_deferred_the_quick_look_is_the_last_result`)
  and the preview rests on it (`a_deferred_request_rests_on_its_quick_look`);
  a trace that died is not started again
  (`a_trace_that_died_is_not_started_again`).
- `pick_drag_tests::trace_ok_is_at_the_right_and_best_optimization_sets_the_detail`:
  OK and Cancel lie under the settings column, right of Best
  optimization; a click on it finds a detail of the search's list, off
  the interface thread, as a step the arrows undo.
- `pick_drag_tests::the_optimized_image_type_greys_the_detail`,
  `pick_drag_tests::a_stale_best_optimization_is_dropped`.
- `trace::tests::black_and_white_stays_pure`,
  `trace::tests::a_specified_background_is_removed_after_shades_join`,
  `trace::tests::the_palette_keeps_to_the_number_of_colours`.
- `trace_preview::tests::the_quick_look_traces_the_art_without_its_margins`.
- `trace::tests::halftone_dots_stay_and_the_regions_below_are_whole`: a
  halftone heart (3 pixel yellow-green dots on teal in a black ring, on
  a transparent background, with noise), Clipart preset: at least 9 in
  10 dots drawn, at most 4 regions, the black ring one ring, under 3 %
  of the pixels drawn wrong.
- `trace::tests::shades_join_their_colour_which_keeps_its_value`: a
  black square with a dark brown fringe on noisy white is #000000 and
  one white.
- `trace::tests::hairlines_of_their_own_colour_keep_it`.
- `trace::tests::the_optimized_preset_draws_the_picture_as_it_is_in_few_nodes`:
  an anti-aliased three colour logo: at most 4 curves, under 200 nodes,
  under 1.5 % drawn wrong; a cancelled search gives nothing.
- `trace::tests::wrong_share_forgives_edges_a_pixel_off_and_counts_lost_dots`,
  `trace::tests::holes_of_regions_drawn_above_are_filled`.
- `trace::tests::removing_the_background_or_the_entire_colour_or_nothing`,
  `trace::tests::merge_adjacent_off_makes_a_curve_per_patch`,
  `trace::tests::remove_overlap_keeps_regions_to_their_pixels`,
  `trace::tests::merged_and_edited_colours_and_grey_mode`,
  `trace::tests::a_cancelled_trace_gives_nothing`,
  `trace::tests::every_stage_stops_at_the_flag`,
  `trace::tests::a_trace_cancelled_while_it_runs_ends_soon`,
  `trace::tests::scaled_renders_and_wireframes_and_stats`.

- `trace::tests::traces_are_smooth_curves_with_sharp_corners_and_no_specks`:
  a 30 px circle is 2 to 12 curves, no straight steps, within the
  fitting tolerance of the circle and 2 % of its area; a square with
  specks around it is one square with corners on its pixel corners; at
  detail 100 the specks stay.
- `trace::tests::despeckle_fills_small_patches_with_their_surroundings`.
- `trace::tests::denoise_removes_lone_pixels_and_keeps_edges`.
- `trace::tests::noisy_flat_art_traces_without_holes_or_specks`: the
  yellow disc and black band with noise of up to 10 per channel, Clipart
  preset: under 1000 subpaths (over 1400 before) and fewer than 1 in 200
  pixels inside the disc showing the page (about 3800 of 9800 before).
- `trace::tests::neighbouring_regions_leave_no_gaps_and_edges_make_no_slivers`:
  an anti-aliased yellow disc crossed by a black band on white, Clipart
  preset: two or three regions, no point more than 2 px inside the art
  outside every curve or inside a white one, and the band's edges one
  curve shared by both colours (under 1 in 20 points along the curves
  running beside another curve instead of on it).
- `trace::tests::neighbouring_regions_share_one_curve_on_each_border`
  (#1250): an anti-aliased bee (a yellow head with black eyes and orange
  cheeks, blue wings beside it, a body of black and yellow stripes),
  Clipart preset: under 1 in 20 points along the curves lie 0.05 to
  2.5 px beside another curve (the doubled borders), no point 3 px
  inside the art outside every curve, the head one yellow ring under its
  eyes; the same with Merge adjacent off and with smoothing 0.
- `trace::tests::borders_meet_at_junctions_and_islands_are_loops`,
  `trace::tests::colours_inside_holes_stack_above_the_one_around`.
- `trace::tests::compressed_flat_art_traces_as_few_clean_regions`: a
  yellow disc with a black band and a 3 px dark bar on white,
  anti-aliased, with noise of up to 6 and blue bled a pixel sideways,
  Clipart preset: at most 3 regions and 8 subpaths (20 regions before),
  and the bar is still drawn dark.
- `trace::tests::blurred_edges_go_to_the_colours_either_side` (#1250):
  black and yellow meeting along a wavy edge blurred over 4 px, with
  noise, Clipart preset with nothing removed: two regions of one ring
  each and at most 40 nodes.
- `trace::tests::art_on_a_transparent_background_keeps_every_colour`
  (#1250): a yellow disc with a black band on a transparent background
  keeps both colours, the head drawn yellow, whatever Remove says.
- `trace::tests::stems_running_off_a_transparent_edge_are_kept`
  (#1930): a black stem running off the bottom of a transparent picture
  and a dot inside, Line Art at detail, smoothing and corner smoothness
  100 with Remove background found automatically: both are drawn.
- `trace::tests::fringes_go_to_the_two_sides_and_strokes_stay`,
  `trace::tests::smoothing_takes_bumps_and_notches_off_edges_and_keeps_lines`.
- `trace::tests::edge_pixels_go_to_the_flat_colours_beside_them`,
  `trace::tests::close_colours_and_colour_counts_are_kept` (a 246 grey
  logo on white at the Logo preset and with two colours, a grey ramp at
  96 colours), `trace::tests::the_blend_of_two_colours_is_no_shape_at_any_detail`,
  `trace::tests::dilation_grows_a_mask_by_a_square`.

- Given a 200x200 image of a black circle on white, outline mode, 2
  colours, then one closed curve with area within 2% of the circle.
- Given the same with "remove background", then the white region is not
  emitted.
- `trace::tests::anti_aliased_art_is_fitted_at_the_highest_detail`
  (#2925): at Detail 100 and Smoothing 0 an anti-aliased disc is at most
  16 nodes within 0.25 px of its circle, and a square turned 30 degrees
  is four straight lines with a node within 0.3 px of each corner.
- `trace::tests::soft_small_capitals_have_straight_sides_and_sharp_corners`:
  a blurred H and L seven pixels high are straight lines only, level or
  upright, 18 corners.
- `trace::tests::an_edge_between_colours_equally_bright_is_found`,
  `trace::tests::a_single_shape_with_a_gradient_is_one_blend`,
  `trace::tests::a_high_smoothing_keeps_the_size_of_a_round_shape`,
  `trace::tests::single_pixels_are_specks_below_the_highest_detail`,
  `trace_fine::tests::only_a_soft_picture_is_sharpened`.
- `trace::tests::an_enlarged_picture_is_measured_in_pixels_of_the_art`,
  `trace::tests::the_gap_of_a_letter_open_to_the_paper_is_paper`,
  `trace_fine::tests::a_bowl_between_two_straight_sides_has_no_corner`,
  `trace_fine::tests::a_round_ring_and_a_tiny_one_have_no_corner`,
  `trace::tests::no_pale_line_is_left_round_a_letter_on_a_shaded_background`,
  `trace::tests::the_gap_of_a_small_letter_in_an_enlarged_picture_stays`,
  `trace::tests::the_dark_rim_of_an_enlarged_edge_is_no_line`,
  `trace_fine::tests::the_bend_of_a_small_s_is_not_squared_off`.
- `trace_fit::tests::a_rectangle_is_four_lines_with_sharp_corners`,
  `trace_fit::tests::a_disc_is_a_few_curves_within_the_tolerance`,
  `trace_fit::tests::a_stem_with_a_round_end_keeps_straight_sides`,
  `trace_fit::tests::the_edge_is_placed_by_the_picture_between_its_pixels`,
  `trace_fit::tests::open_borders_keep_their_ends_and_short_ones_are_safe`,
  `trace_fit::tests::a_stem_is_a_straight_run_and_a_wide_arc_is_not`,
  `trace_fit::tests::the_round_end_of_a_thin_stroke_is_made_square`,
  `trace_fit::tests::a_disc_is_a_circle_of_four_curves`,
  `trace_fit::tests::an_ellipse_is_four_curves_and_a_turned_one_is_not_forced`,
  `trace_fit::tests::a_half_disc_is_an_arc_and_a_u_keeps_its_sides`,
  `trace_strokes::tests::a_soft_letter_comes_back_with_straight_strokes_and_square_ends`,
  `trace_strokes::tests::a_shape_that_is_no_stroke_is_left_alone`,
  `trace_strokes::tests::a_letter_in_two_shades_is_drawn_as_one`,
  `trace_strokes::tests::the_middle_line_of_a_t_is_two_strokes`,
  `trace_strokes::tests::the_middle_line_of_a_u_is_two_stems_and_a_half_circle`,
  `trace_strokes::tests::a_free_end_follows_the_bend_of_its_stroke`,
  `trace_strokes::tests::the_letters_of_a_word_share_a_width_and_a_line`,
  `trace_fit::tests::a_round_between_two_sides_is_the_arc_that_touches_both`,
  `trace::tests::a_small_letter_of_an_enlarged_picture_is_traced_as_drawn`,
  `trace::tests::the_check_finds_a_closed_gap_and_leaves_a_shade`,
  `trace::tests::a_small_piece_of_a_shade_joins_the_letter_it_is_of`
  (the faithful fit and the check of enlarged pictures).
- `trace::tests::an_edge_is_traced_where_it_runs_inside_its_pixels`,
  `ops2::tests::a_trace_lies_exactly_over_its_bitmap` (#2926).
- `trace::tests::edge_pixels_make_no_ring_of_a_look_alike_colour`
  (#2929), `trace::tests::a_gradient_is_one_region_with_a_blend`
  (#2929: a red to yellow bar is one curve with a blend along it; two
  flat colours with a soft edge stay two).
- `trace::tests::thin_faint_strokes_keep_their_shape_and_take_the_full_colour`
  (#2927): a ring 7 px across drawn a pixel wide in mid grey is a black
  ring with its hole at the Logo, Detailed logo and Optimized presets.
- `trace::tests::only_paper_is_removed_and_all_of_it` (#2928).
- `trace::tests::the_search_keeps_the_detail_when_none_is_better`
  (#2930).

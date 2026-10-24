# Bitmap tracing (bitmap to vector)

| Mode | Use | Defaults |
|---|---|---|
| Quick trace | one click | the preset of Options > Global > Bitmap Tracing (Clipart by default) |
| Centreline: technical illustration, line drawing | strokes | |
| Outline: line art, logo, detailed logo, clipart, low/high quality image | filled regions | smoothing 25, detail per preset, colours per preset |

| Control | Default | Range |
|---|---|---|
| Detail | per preset | 0..100 |
| Smoothing | 25 | 0..100 |
| Corner smoothness | 0 | 0..100 |
| Colour mode | RGB | RGB, CMYK, Grayscale, Black and white |
| Number of colours | per preset | 1..256 |
| Delete original | off (Options > Global > Bitmap Tracing) | |
| Remove background | on (Options > Global > Bitmap Tracing) | auto or colour |

## Commands

Bitmaps > Quick Trace, Repeat Last Trace (the selected bitmaps traced
with the last trace's settings, no dialog), Open Last Trace Settings
(the dialog on them), Centerline Trace and Outline Trace with their
presets; a bitmap's right-click menu has the same, as in the reference
program (#452, `ops2::tests::the_last_trace_can_be_repeated_and_reopened`).
The last trace is kept for the session.

## Dialog preview

The Trace Bitmap dialog opens with the bitmap before and its trace after,
side by side (200 points each, a checkerboard behind transparency). The
trace is worked out on a copy of the bitmap at most 240 pixels on its
longer side, again whenever a setting changes: closed regions filled with
their colours, centrelines stroked. Preview still counts the curves of the
full trace.

## Algorithm (outline mode)

1. Blur a little when detail < 50, then quantise the colours with median
   cut to the number of colours (black and white: by brightness).
2. Specks (#611): every 4-connected patch of one colour smaller than
   `round(((100 - detail) / 10)^2)` pixels (none at detail 100, 4 at 80,
   16 at 60, 42 at 35) takes the colour most of its neighbours have, so
   stray dots and the fringes of soft edges make no curves of their own.
   Transparent pixels are left alone.
3. For each colour, the boundary rings of its mask along the pixel
   edges.
4. Smoothing 0: each ring as straight lines along the pixel edges
   (Douglas-Peucker, tolerance `0.3 + (100 - detail) / 100 * 2.5` px).
   Otherwise each ring is fitted (#611):
   - one point in the middle of every pixel edge, so stair steps become
     slopes;
   - corners where the ring turns by more than `60 + corner smoothness *
     0.8` degrees over 3 pixels (the sharpest point of each run), moved
     onto the pixel corner standing out, so a square keeps its corners;
   - the points between corners smoothed `round(1 + smoothing * 0.12)`
     times with weights 1, 2, 1;
   - each stretch between corners (a ring without corners in quarters
     meeting smoothly) fitted with as few cubic Beziers as keep every
     point within `0.2 + (100 - detail) / 100` px: least squares on
     chord-length parameters with Newton steps, split at the worst point
     (P. J. Schneider, "An Algorithm for Automatically Fitting Digitized
     Curves", Graphics Gems, 1990). Ends take the direction over 4
     points.
5. Stack regions dark to light; remove the region of the background
   colour (the most common colour on the border) when requested.

Centreline mode: Zhang-Suen thinning, then the skeleton is traced into
polylines, smoothed (Catmull-Rom) and output as curves with a stroke
width equal to the mean local thickness.

## Checks

- `trace::tests::traces_are_smooth_curves_with_sharp_corners_and_no_specks`:
  a 30 px circle is 2 to 12 curves, no straight steps, within the
  fitting tolerance of the circle and 2 % of its area; a square with
  specks around it is one square with corners on its pixel corners; at
  detail 100 the specks stay.
- `trace::tests::despeckle_fills_small_patches_with_their_surroundings`.

- Given a 200x200 image of a black circle on white, outline mode, 2
  colours, then one closed curve with area within 2% of the circle.
- Given the same with "remove background", then the white region is not
  emitted.

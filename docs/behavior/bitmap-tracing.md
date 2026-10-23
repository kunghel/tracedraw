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

1. Quantise colours with median cut to `n` colours, after a 3x3 median
   filter when detail < 50.
2. For each colour, build the binary mask and extract contours with the
   Potrace path decomposition (turn policy: minority) and corner
   detection (alphamax `= 1 - smoothing/100 * 0.66`).
3. Fit each contour with cubic Beziers (Potrace's optimal curve fitting,
   tolerance `0.2 + (100 - detail)/100 * 1.5` px).
4. Stack regions dark to light, largest first; remove the region
   matching the background colour when requested.

Centreline mode: Zhang-Suen thinning, then the skeleton is traced into
polylines, smoothed (Catmull-Rom) and output as curves with a stroke
width equal to the mean local thickness.

## Checks

- Given a 200x200 image of a black circle on white, outline mode, 2
  colours, then one closed curve with area within 2% of the circle.
- Given the same with "remove background", then the white region is not
  emitted.

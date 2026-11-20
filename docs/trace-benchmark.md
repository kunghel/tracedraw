# Trace Bitmap quality benchmark

`apps/tracedraw/src/trace_bench.rs` (#2938) checks Trace Bitmap on
pictures made the way logos reach people, measured against the clean
picture each was made from instead of against its noisy copy. It runs
with the other tests (`cargo test -p tracedraw trace_bench`), so CI fails
when a measure gets clearly worse than its baseline.

## Fixtures

Each one is drawn in the test from shapes, with 4 x 4 samples a pixel,
300 x 300 pixels, so no file with someone else's rights is needed:

| Fixture | Input | Checks |
|---|---|---|
| jpeg-logo-q35 | colour logo, JPEG quality 35 | compression noise, halos |
| thin-black-art | 1.5 to 3 px rings and lines, spaced capitals 14 px tall, JPEG 60 | thin strokes, small text |
| flat-colour-aa | gold star in a green ring, anti-aliased | curves of a flat logo |
| dark-emblem | emblem and capitals on a navy backdrop that is part of the design, JPEG 70 | a dark backdrop kept |
| grey-gradient-backdrop | the colour logo on a light grey gradient, JPEG 60, measured against the logo on white | the backdrop left out |
| smooth-gradients | radial disc and linear bar | gradients |
| drop-shadow | blue rounded square over a soft grey shadow | soft shadow |
| small-capitals-q50 | capitals 10 and 20 px tall, JPEG 50 | small text survival |

Each goes through Logo, Detailed Logo, Clipart and Optimized.

## Measures

Curves, nodes and colours of the trace; wrong share (percent of pixels
drawn wrong against the clean picture, `trace::wrong_share`, an edge a
pixel off not counted); halos (closed shapes under 12 square pixels);
text wrong share inside the capitals' box; and the shift of the ink's
centre (pixels) between the clean picture and the trace.

A run fails when, against its line in `BASELINE`, the wrong share grows
by more than a quarter and 1 point, the text wrong share by a quarter
and 2 points, the nodes by 30 % and 20, the halos by half and 5, or the
ink shifts more than 1.5 pixels. A change that makes the trace better
updates the baseline: the run writes the new lines to `baseline.txt`.

## Report

```
TRACE_BENCH_OUT=/tmp/bench cargo test -p tracedraw trace_quality_benchmark -- --nocapture
```

writes `report.md` (the table above, one row per fixture and preset),
`baseline.txt` and one PNG per run: the input, the clean picture and the
trace side by side.

## Findings of the first baseline (0.50)

- The logo on a grey gradient comes out right (the backdrop is left out)
  but with 375 to 826 nodes where the same logo on white takes about
  100: faint pieces of the gradient along the top stay as shapes.
- The soft drop shadow is lost or banded (1.5 % wrong for every preset).
- Capitals 10 px tall in a JPEG are 7 to 10 % wrong inside their box.

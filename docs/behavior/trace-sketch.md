# Trace: clean up a pencil sketch

The Outline tab of the trace dialog, under the image options: **Clean up
a pencil sketch**, off at first, greyed for a centreline trace
(`Settings::clean_sketch`, `trace_sketch.rs`). On, the picture is cleaned
before the trace and traced black on white (black and white mode, two
colours, white removed, one curve for the black). No machine learning.

All measures are in a unit `u` of the picture's longer side / 300 pixels
(at least 1):

1. Pencil: a pixel is ink where its lightness (over white where see-
   through) is below the paper's (the lightness blurred `8u` each side,
   three box passes) by more than 18 and below 90 % of it, so a sheet
   photographed with uneven light still separates.
2. Closing by `2.5u` (exact Euclidean distances): hatching and doubled
   outlines become one solid shape; holes not touching the edge smaller
   than `(6u)²` pixels fill, a letter's counter (larger) stays.
3. Opening by `1.5u`: lines thinner than about `3u` (construction lines,
   guides) go; then patches smaller than `(5u)²` pixels.
4. The shapes blurred by `0.75u` (at least 1 pixel) and cut at half way,
   for smooth edges.

Tests: `trace_sketch::tests::a_hatched_letter_becomes_solid_and_the_guides_go`,
`a_cleaned_sketch_traces_as_black_shapes`, `distances_are_exact`.

## With AI

Only when AI is turned on in Tools > Options > AI and the service set up
there makes pictures: with Clean up a pencil sketch ticked, a words field
and a Clean up with AI button show under it (hidden otherwise, greyed while
no bitmap is selected or a request is waiting). The button runs Sketch to
Curves (see ai-photo.md): the service redraws the selected sketch from the
words (empty: the lines inked and filled in solid black) and the result is
traced into curves over the sketch. The words are kept for the next time.
Nothing is sent unless the button is pressed.

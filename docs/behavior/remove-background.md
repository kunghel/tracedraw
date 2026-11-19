# Remove Background (on this computer)

Image > Remove Background... (`menu.bitmaps.remove_background_local`,
also on the bitmap property bar and in a bitmap's context menu) opens a dialog that makes the background of
the selected bitmaps transparent without an AI service
(`bg_remove::remove_background`). The AI command stays as Remove
Background with AI (`menu.bitmaps.remove_background`).

## Dialog

A window the user sizes: the shared Before and After preview at the left
(`before-after-preview.md`: the Preview list, zoom and pan, a
checkerboard where transparent), and at the right:

| Setting | Range | At first | Meaning |
|---|---|---|---|
| Background | a list | one row, Automatic | what to remove, one row each: Automatic (everything but the subject) or a colour (see Colour list) |
| Tolerance | 0 to 100 (Delta E), whole numbers | 14 | for a picked colour, how far a colour may be from it and still be background; for Automatic, how much of the soft edge counts as backdrop; the tolerance of every row that has none of its own |
| Edge softness | 0 to 20 px | 1 | feather of the edge, in pixels of the bitmap (previews scale it) |
| Contiguous | on/off | on | a picked colour goes only where connected to the border (through a backdrop that drifts in colour too) or to what Automatic took; Automatic leaves parts inside the subject that only share a flat backdrop's colour (see Algorithm) |
| Remove specks | on/off | on | loose kept spots under 1/2000 of the picture (at least 12 px) go |
| Clean edge colors | on/off | on | half-transparent edge pixels lose the backdrop's colour |

The settings sit in the shared settings panel with the Reset, undo and
redo row under it (`before-after-preview.md`): Reset puts every setting
back, the arrows step through the settings states (a slider's drag is
one step on release, a colour picked, added or removed, a row's
tolerance, a box, Reset). OK changes every selected bitmap in one undo
step (`apply_to_bitmaps`) and keeps the settings, the colour list too, for
the rest of the session (`App::last_bg_remove`): the dialog opens with them
next time (`bitmap_dialogs::open_remove_background`). The points where
colours were picked belong to one bitmap and are not kept. The "?" opens
`bitmaps/remove-background`.

### Colour list

`BgSettings::colors`, a list of `BgPick` (colour or Automatic, own
tolerance or none, the point it was picked at). Each row shows:

- a swatch: the colour, or for Automatic a half light, half dark square
  (Automatic goes by the subject, not by a colour, see Algorithm);
- an eyedropper: the next plain click on a preview pane sets this row's
  colour (and its seed point), then the eyedropper goes off;
- on the first row only, Automatic: the row finds the subject again
  (shown as a label while it does);
- its tolerance field (0 to 100, whole numbers): showing the dialog's
  Tolerance until it is changed, then the row's own;
- an x button: the row goes; the last row cannot go (greyed), and an
  empty list is one Automatic row.

Under the rows, "+" (Add colour) turns the eyedropper on for a new row: the
next plain click on a pane adds the colour under it. With the eyedropper
on (from any row or "+"):

| Click on a pane | Does |
|---|---|
| plain | sets the row the eyedropper was taken for, or adds a row after "+"; the eyedropper goes off |
| Shift+click | adds the colour under the pointer as a new row (skipped when listed already); the eyedropper stays on |
| Alt+click | removes the row whose colour is nearest (CIE76) to the one under the pointer; an Automatic row counts with its nearest learnt colour, and on a tie the picked row goes; the eyedropper stays on |

The colour is taken from the picture before the change (a removed pixel
can be picked too). The pan button turns the eyedropper off.

While the eyedropper is on, a click on the drawing picks too: the dialog
renews the shared picker request (`color_picker::pick_for`, the request
the colour pickers' eyedroppers use), so the modal layer lets the click
through to the canvas, which samples the colour there and hands it back;
Shift and Alt work as on a pane. A colour picked on the drawing has no
seed point (the click may be off the bitmap). With the eyedropper off no
request is left waiting.

### Brushes (#1723, #1915, #3422)

Erase and Restore paint on the preview: one is on at a time (clicked
again, it is off, as is the pan button or an eyedropper). Each is a
button with the brush's round tip in its colour (red with a minus for
Erase, green with a plus for Restore) and its name, framed at rest and
pressed in while on (`bitmap_dialogs::bg_brush_button`). While a brush
is on a drag on a pane paints instead of panning and a click is a dab.
The result follows the brush as it paints: the preview is made from the
settings with the stroke being painted as their last stroke
(`bitmap_dialogs::bg_preview_settings`), a new result per point added
(a point every fifth of the radius), from the subject found already
(see Algorithm, step A6), so painting stays light. The panes that do not
show the result show the stroke as a tint in the brush's colour. Under
the pointer a ring in the brush's colour, edged dark and light so it
shows on any picture, gives the brush's size, with a dot at its centre,
and the pointer is a crosshair. Letting go makes the stroke one step of
the settings (undo and redo as for a slider). Brush size is a
percentage (0.5 to 25, default 3) of the picture's larger side, and the
stroke's points are fractions of the picture, so the preview copy and the
bitmap are painted in the same places. Clear strokes takes them all
away. Strokes belong to the bitmap and are not remembered for the next
time; the other settings are.

### Fill background (#1915)

Off by default. On, it starts white, with a Top and a Bottom colour: the
backdrop fades from one to the other down the picture (the same colour
twice is a plain fill) and the result is opaque, ready to export.

The preview is the shared live preview (`before-after-preview.md`): the
result is cached under a hash of the settings and the copy, made again only
when they change; while a slider is dragged it is worked on a small copy,
and the shown size follows when the pointer is released.

The result shown once the pointer is let go is OK's own: the background is
removed from the bitmap at its own size (decoded once per bitmap,
`bitmap_dialogs::bg_full_bitmap`) and the result is shrunk to the copy with
its colours weighted by opacity (`bitmap_dialogs::bg_preview_result`,
`bg_remove::shrink_result`). Automatic finds the subject from the pixels it
is given, so a subject found on the smaller copy could differ from the one
OK finds (until 0.49 the preview kept grass and a corner of the backdrop
that OK removed).

## Algorithm

Two ways of telling background, used together when the list holds both:
an Automatic row finds the subject (`bg_subject.rs`), a picked colour
takes what is near it (`bg_remove::by_picked_colours`). Until 0.48
Automatic learnt colours from the border and removed by colour too,
which took a shoulder that touched the border for backdrop and left
backdrop of another shade behind (#3422).

### Automatic: the subject

A1. A copy at most 256 px across (box average) is worked on. When fewer
   than a third of its two pixel border is opaque the picture already
   has a transparent background and Automatic leaves it alone.
A2. Sure backdrop on the border. The border's opaque pixels are
   clustered in CIE Lab (farthest-point seeds at least 10 Delta E apart,
   at most eight, k-means, ten rounds). Each cluster's cover is the sum
   of its pixels' weights: 1 on the top rows, 1 falling to 0.5 down the
   sides, 0.35 on the bottom rows (a subject mostly leaves the picture
   at the bottom). The cluster with the most cover is sure backdrop, and
   so is any with at least half as much (a two-tone backdrop). Border
   pixels of a sure cluster, within 10 Delta E of its centre, are fixed
   as backdrop; the other border pixels are left open, so a shoulder,
   an arm or a product that touches the border is not backdrop.
A3. First guess. A fill from the fixed pixels walks to a neighbour that
   differs by at most 3.5 Delta E and lies within 45 of a sure colour.
   Then gaps: a pixel not reached counts as backdrop seen through the
   subject when its colour is within 6 Delta E of the mean backdrop
   within a tenth of the picture around it; on a coloured backdrop
   (chroma c, as min(1, chroma / 25)) a darker pixel counts its
   lightness difference times (1 - 0.75 c) and may be 6 + 4 c away, a
   backdrop in the subject's shadow keeping its hue. A connected part of
   such pixels that touches what the fill reached is dropped again: the
   fill stopped at an edge there, so it is something lying on the
   backdrop (a white petal on a cloud). The rest of the picture is the
   first guess of the subject.
A4. Models and cut, up to five times. Each side gets a mixture of up to
   six Gaussians over (L, a, b, x, y), position scaled so the longer
   side is 40 (k-means from farthest-point seeds, full covariance with
   a floor of 4 on the colour variances and 2 on the position's). A
   pixel's lean is ln p(subject) - ln p(backdrop), held to +-20; pixels
   of a gap (A3) get 20 less. Neighbours (eight) pull together by
   24 exp(-beta d^2) / distance, beta = 1 / (2 mean d^2) over the
   picture, d their Delta E. The labelling of least cost is a minimum
   cut of the pixel grid (`bg_subject::Flow`: augmenting paths from two
   search trees kept between paths, whole numbers, 1/64 of a unit); the
   fixed border pixels and transparent pixels are tied to the backdrop.
   The models are made again from each labelling; it stops when under a
   thousandth of the pixels change.
A5. Parts. Backdrop connected to the border (or transparent) is
   Backdrop. Backdrop enclosed by the subject is a Gap, and goes, when
   the outside backdrop is not flat (over 3 % of it further than 2.5
   Delta E from the sure colours: a photo) and the part holds at least
   1/4000 of the copy (6 px); else it is Inside, which stays with
   Contiguous and goes without: on a flat backdrop (a drawing, a scan)
   a part of the same colour inside the subject may be drawn that way
   (the inside of a letter or a ring, a white shirt on white). On a flat
   backdrop such a part stays whole, to its rim: step A6 leaves it.
A6. The edge, at the picture's size, on a copy at most 512 px across
   (a larger picture takes the coefficients and maps from it, bilinear).
   Within a band of 2 px of the small copy round the cut, the opacity is
   a guided filter of the cut by the picture's colour (radius 1.5 bands,
   epsilon 2e-4): thin parts the small copy lost come back and
   anti-aliased edges are soft. Then, up to four bands from the cut, a
   pixel whose colour C lies between the subject's colour F there (mean
   of the pixels more than four bands inside) and the backdrop's B (mean
   of the backdrop within three bands) is as opaque as its place t
   between them, never more than the filter gave: C within
   0.08 |F - B| + 4 of the line (twice that: no change), plus up to 30
   more right at the cut for t under 0.3 (the light fringe a sharpened
   photo has). Deeper than about a band this holds only where the
   picture is busy (the spread of lightness within 2/3 band at least
   14 to 30: hair), not on smooth skin a sky lights. Opacities are then
   levelled: under low = min(0.45, Tolerance / 100) is backdrop, over
   1 - low / 2 subject.
   The subject of a picture up to 1024 x 1024 px is kept (the last four,
   by the pixels, Tolerance and Contiguous), and the cut of the small
   copy (the last six): brushes, softness, specks and the backdrop work
   on a subject found already.
A7. Clean edge colors, where Automatic took the backdrop: by the opacity
   m before the feather, C = m F + (1 - m) B is solved for F with the
   local B; under m = 0.35 the local F stands in more and more. Near the
   edge, on a coloured backdrop (its colour less its grey longer than
   25), a pixel loses what it has of the backdrop's hue beyond what the
   subject has there, so hair keeps no tint of the old backdrop.

### Picked colours

1. Each picked row gives its colour with its tolerance.
2. Each pixel's background colour is the one it lies deepest inside: the
   least d_k - t_k over the colours k (d_k its CIE76 Delta E, t_k that
   colour's tolerance); its distance d and tolerance t are that colour's
   (transparent pixels: d = 0). So a pixel is background when it is
   within the tolerance of any listed colour, and the steps below work on
   the union. With every tolerance equal this is the nearest colour.
3. Opacity: 0 up to t, a smoothstep ramp over a band of max(4, 0.6 x t)
   past it, 1 beyond.
4. Contiguous: a breadth-first fill from the border pixels, from the
   point each picked row was picked at with the 8 pixels around it (so a
   colour picked inside the subject, a hole, goes), and from what
   Automatic took (so a picked colour lying on the backdrop goes),
   reaches pixels under tolerance + band; it passes freely through
   pixels within the tolerance and at most 2 + ceil(softness) pixels
   into the ramp, so it takes the anti-aliased rim but not the subject
   (t + band, t and the band are each pixel's own, step 2). From a
   background pixel of a picked colour it also walks on to a neighbour
   whose colour differs from it by at most max(3, t / 3) Delta E (t of
   the pixel it walks from), as long as that neighbour is within
   max(3 t, t + band) of its colour: a backdrop that drifts away from
   the picked colour (a texture, a gradient, a soft shadow under the
   subject) goes whole, pixel by small step, while the subject's edge, a
   sharp step, stops the walk (its anti-aliased rim is taken by the ramp
   as before). Pixels reached by such steps are background whatever
   their distance. Unreached pixels stay opaque.
   Off: every pixel gets the ramp's opacity.
5. A pixel's opacity is the lesser of Automatic's and the colours'.

### Both

6. Remove specks: 4-connected kept parts (opacity at least 0.5) smaller
   than 1/2000 of the picture (at least 12 px) go.
7. Brush strokes, in order, after Remove specks and before Edge
   softness: within the radius of the polyline through a stroke's points
   (a lone point is a dab), Erase sets the opacity to 0 and Restore to 1,
   blended over the last pixel of the rim (v = clamp(r - d + 0.5, 0, 1)).
   When nothing is taken (a transparent border) the strokes still apply.
8. Edge softness: two box blurs of the opacity with the radius in pixels.
9. Clean edge colors: where a picked colour took the pixel, for
   0.05 < opacity < 1, the colour is solved from C = a F + (1 - a) B with
   B that colour, clamped; where Automatic did, step A7.
10. The new opacity multiplies the pixel's own alpha.
11. Fill background: each pixel is put over the backdrop's colour on its
    row (linear from Top on the first row to Bottom on the last), alpha
    255.

## Checks

- `bg_remove::tests::the_backdrop_goes_and_the_subject_stays`: a gradient
  backdrop goes at both ends, the subject keeps its colour, a
  background-coloured dot inside it stays, a loose speck goes.
- `bg_remove::tests::everywhere_mode_takes_background_coloured_parts_inside_too`
- `bg_remove::tests::edge_pixels_lose_the_backdrops_colour`
- `bg_remove::tests::learnt_colours_cover_a_two_tone_backdrop`
- `bg_remove::tests::picked_colours_and_tolerance`
- `bg_remove::tests::a_transparent_picture_is_left_alone`
- `bg_remove::tests::feathering_softens_only_the_edge`
- `bg_remove::tests::a_drifting_backdrop_goes_and_the_subject_stays`: a
  soft shadow 36 Delta E from the white backdrop, over a faint texture,
  goes with the backdrop, by Automatic and by a picked white; the
  subject keeps its edge; Everywhere mode goes by the picked colour
  alone.
- `bg_remove::tests::automatic_keeps_a_subject_that_leaves_the_picture`
  (#3422): a drawn portrait on a shaded, noisy blue backdrop. Shoulders
  and shirt on the bottom and side borders stay as they were, the
  backdrop goes at every side and inside the loop of a raised arm, no
  pixel left of the hair's rim is blue, and Fill background fills what
  was removed only.
- `bg_remove::tests::a_flat_part_inside_the_subject_stays_with_contiguous`
- `bg_remove::tests::picked_colours_and_tolerance`: Automatic takes the
  whole backdrop and keeps the subject at tolerance 0, 14 and 45.
- `bg_subject::tests::the_minimum_cut_is_the_cheapest_labelling`: the
  cut against every labelling of a 3 x 3 grid, forty random grids.
- `bg_subject::tests::a_covariance_is_inverted`,
  `bg_subject::tests::odd_pictures_are_safe` (one pixel wide, one
  colour, transparent; the subject is found once per picture and
  settings).
- `bg_remove::tests::the_fill_goes_through_every_listed_colour`:
  Automatic alone stops at a green patch on the backdrop; Automatic and
  green (or two picked colours) take both; the subject stays.
- `bg_remove::tests::a_colour_picked_inside_goes_from_its_seed`: a third
  colour inside the subject goes from its seed point (Contiguous) or
  everywhere (off); feathering keeps the subject.
- `bg_remove::tests::each_colour_has_its_own_tolerance`
- `bg_remove::tests::the_colour_list_adds_and_removes`
- `bg_remove::tests::the_brushes_erase_and_restore_where_painted`,
  `strokes_follow_the_picture_at_any_size`,
  `the_brushes_work_on_a_transparent_picture`,
  `a_backdrop_fills_behind_the_subject`
- `bitmap_dialogs::tests::the_remove_background_preview_shows_what_ok_makes`:
  the preview of a copy is OK's result on the bitmap, shrunk; the quick
  copy of a slider drag is worked on directly.
- `bg_remove::tests::a_shrunk_result_keeps_removed_colours_out`
- `pick_drag_tests::remove_background_brushes_paint_on_the_preview`: Erase
  drag paints a stroke without panning, Restore click is a dab, the brush
  turns off, Clear strokes, Fill background.
- `pick_drag_tests::remove_background_shows_the_stroke_being_painted`:
  the preview's settings hold the stroke being painted and the result
  shows it; the dialog's settings get it on release.
- `pick_drag_tests::remove_background_adds_and_removes_colours`: "+" and a
  click add a row, Shift+click adds and keeps the eyedropper, Alt+click
  removes the nearest, x removes a row, the wheel sets a row's own whole
  tolerance, OK is one undo step and the list comes back next time.
- `pick_drag_tests::the_remove_background_eyedropper_picks_on_the_shared_preview`
- `pick_drag_tests::remove_background_picks_a_colour_on_the_drawing`: a
  click on the drawing adds a row after "+", Shift keeps the eyedropper,
  and no picker request outlives it.
- `pick_drag_tests::frame_times_of_the_remove_background_dialog`
  (ignored, `--release`): idle frames and a Tolerance drag with three
  colours listed on a 2000 x 2000 bitmap.
- `ui::dialogs::tests::every_dialog_draws_and_accepts_enter` draws the
  dialog.

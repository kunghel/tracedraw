# Layer effects

Effects > Layer Effects opens the Effects docker, whose top section,
Layer effects (#1141), stacks looks on the selected objects and sets each
one with its values drawn live on the page: there is no OK or Cancel,
every change is one undo step ("Layer Effects") and goes to every
selected object; the values shown are the first selected object's. Code:
`ui/layer_effects.rs`; the effects in `tracedraw_core::live::Effect`, the
pixels in `tracedraw_render` (`draw_outer_shadows`, `draw_inner_shadow`,
`draw_blurred`).

The list, one row each:

| Entry | Effect | Values |
|---|---|---|
| Bevel / Emboss | `Bevel` (Soft edge or Emboss) | type, depth, level, light angle, intensity, light and shadow colors |
| Outline | `Contour`, one step outside | width, color |
| 3D | `Extrude`, back parallel | distance and direction of the depth, light angle and intensity |
| Inner shadow | `InnerShadow` | distance, angle, blur, opacity, spread, color, blend mode |
| Outer shadow | `OuterShadow`: a drop shadow of the object's shape beneath it and all its effects draw; any number beside the object's own drop shadow | as Inner shadow |
| Inner glow | `Glow` inside: an inner shadow that does not move | radius, opacity, color, blend mode |
| Outer glow | `Glow` outside: a drop shadow that does not move, solid up to the edge and fading beyond it (feather direction Outside) | as Inner glow |
| Color overlay | `ColorOverlay`: the object's shape over it in one color | color, opacity |
| Gradient overlay | `GradientOverlay`: the shape over it with a linear gradient | from and to colors, angle, opacity |
| Gaussian blur | `GaussianBlur`: the object drawn on a layer of its own and blurred (three box passes) | radius |
| Blend If | `BlendIf`: the object drawn on a layer of its own, each pixel kept as much as its own lightness and the lightness of what lies beneath it fall in two ranges | This object and Beneath ranges, Show all |
| Displace | `Displace`: the outline pushed by the lightness of the photo beneath, and the photo's shadows and highlights taken onto the object | Horizontal, Vertical, Shading, Strength, Use photo beneath |
| Halftone | `Halftone`: the object drawn with all its other effects on a layer of its own, then turned into dots (`tracedraw_render::halftone`) | dot, color mode, color, dot spacing, dot scale, softness, angle, center X and Y, cut by the object's edge |

- The check box switches the entry's effect on. Ticked the first time it
  adds the effect with the values shown; cleared, the effect is kept
  switched off at its place in the stack (as the stack's own switch
  below) and comes back with its values when ticked again.
- Clicking the name shows that entry's values under the list. Changing a
  value of an entry that is not on switches it on with that value.
- The cross takes the effect off the objects.
- Defaults: bevel as the Bevel docker's with level 4; outline 1 mm black;
  3D 2 mm down and right; inner shadow 1 mm at 315°, 50 %, blur 1 mm,
  Multiply; outer shadow 2.1 mm at 315°, 50 %, blur 1.5 mm, Multiply;
  glows pale yellow, 75 %, Screen, 1.5 mm inside and 2 mm outside;
  color overlay red at 50 %; gradient overlay black to white at 90°,
  50 %; blur 1 mm; Blend If shows all of the object itself and hides it
  over dark pixels beneath, fading in up to a quarter of the way to
  white (0 / 63 on the 0 to 255 scale); halftone round RGB dots 1 mm
  apart at 45°, scale 100 %, softness 10 %, centred, cut by the edge.

Inner shadow spread (0 % by default, older files read as 0 %) chokes
the feathered edge: the blurred coverage is divided by 1 minus the spread
(at most 99 %) and capped at full, so the shadow stays solid over that
share of the blur and fades over the rest. Drawn on the halftone's own
layer, an inner shadow keeps each pixel's coverage and darkens edge pixels
as fully as the inside, so no light rim turns into dots.

Halftone: a grid turned by the angle, with a dot centre at Center X and Y
across the object's bounds (0 % left and bottom), takes in each cell the
mean color of the drawn pixels. The dot's area is the tone times the scale
squared times the cell (exact until dots touch; a line is a band as wide as
the tone). RGB: three dots in the cell, red, green and blue, each as large
as its channel, added as light (white where all three meet), for light
dots on a dark ground. CMYK: cyan, magenta, yellow and black from the
color (black the lack of the brightest channel), on screens at 15°, 75°,
0° and 45° past the angle, each an ink dot multiplying white paper that
follows the object's edge. One color: dots of the chosen color as large as
the darkness (1 minus luma). Cut by the edge multiplies the dots by the
object's own alpha; off, the tone is weighed by the cell's alpha and the
dots stay whole (they reach half a cell past the edge, so bands and the
layer grow by a cell). Softness widens the dot edge up to a cell (at
least one pixel). The effect applies last, to everything the object and
its effects draw, and the object's other effects stay live under it.

Blend If. Each range is a strip from black to white with four markers:
where the dark end starts to show and where it shows fully, where the
light end starts to fade and where it is gone. A pixel of lightness `l`
shows by `min(rise, fall)`: `rise` is 0 below the first marker, 1 from
the second up and `(l - m0) / (m1 - m0)` between; `fall` mirrors it at
the light end (`live::blend_if_weight`, the markers put in order first).
The object's pixel is kept by the product of its own range's weight and
the weight of the target's pixel beneath (over white paper where nothing
is drawn yet); lightness is `0.299 R + 0.587 G + 0.114 B`
(`live::blend_if_lightness`). Dragging a marker moves its end; the two
markers of an end in one place move together, and Alt+drag splits them,
taking the one on the side the pointer goes, so the end fades in over
the gap. The values show as 0 to 255 (`a / b` for a split end). Show all
sets both ranges back to the whole strip. A light text laid on a photo
with the dark end of Beneath split takes the photo's grain: it shows
over the light grains and lets the dark ones through.

Displace (#3595). Ticked, each selected object reads the topmost picture
drawn beneath it on its page (an earlier object of its layer, or one on
a layer under it, inside groups too) that its box overlaps: the
picture's lightness (`0.299 R + 0.587 G + 0.114 B`, over white where it
is transparent) is sampled on a grid over the object's own box grown by
the larger of the two amounts, about half a millimetre a cell on the
page and at most 160 cells along the longer side, each cell the average
of up to four by four samples (`surface_map::sample`). Cells off the
picture take its average lightness `mean`, the grey that pushes nothing.
A point `p` of the outline (curves flattened finely, as the other warps
do) goes to `p + T (h d, v d)`, `d = clamp(2 (L(p) - mean), -1, 1)`,
`L(p)` the map's lightness there (bilinear between the cells' middles),
`h` and `v` the Horizontal and Vertical amounts in mm and `T` the page
to object turn: light parts go up and right on the page, dark parts down
and left, so a print follows the folds of a cloth, the grain of a paper
or the bricks of a wall (`SurfaceMap::displace`). A text stays text.

Shading takes the photo onto the object as the renderer draws it: on a
layer of its own, each pixel's colour `c` (premultiplied, alpha `A`)
becomes `c' = c m + (A - c m) s` with `m = 1 - k (1 - l / mean)` where
the target beneath (over white paper) has lightness `l < mean` with
Shadows or Both, else 1, and `s = k (l - mean) / (1 - mean)` where
`l > mean` with Highlights or Both, else 0, `k` the strength
(`live::surface_shade`). Defaults: 2 mm both ways, Shadows at 60 %.

The map stays with the object it was read for: values set from another
object's effect keep each object's own map. Use photo beneath reads it
again for every selected object, and a move with the Pick tool does so
for the moved objects, in the move's undo step
(`App::displace_after_move`). No picture beneath: "No photo beneath",
and the outline stays as it is (the shading still reads what lies
beneath). Convert to Curves keeps the bent outline.

Fill opacity, under the list, fades the object itself (`FillOpacity`,
which multiplies the object's opacity after the other effects took it as
it was): overlays, shadows and glows keep their own. At 100 % the effect
goes. An outer glow is solid under the object, so with the fill faded it
shows through.

The renderer draws outer shadows and glows, inner glows, the blur,
Displace's shading and Blend If (which the vector exports leave out, drawing the object whole);
the vector exports (PDF, SVG) draw the overlays, as objects, and leave
the pixel effects out, as they do the inner shadow. A page's bands are
drawn with room for the widest of these to meet (`reach_walk`). Presets
keep all of them (`style_presets::portable`).

Tests: `live::tests::layer_effect_overlays_and_fill_opacity`,
`live::tests::blend_if_weight_rises_and_falls_over_split_ends`,
`tests::blend_if_shows_the_object_where_the_lightness_falls_in_range`
(render),
`surface_map::tests::the_map_reads_the_photo_beneath_the_object`,
`surface_map::tests::displace_bends_the_outline_with_the_photo`,
`surface_map::tests::the_shading_takes_the_shadows_and_highlights`,
`tests::displace_shading_takes_the_shadows_and_highlights_beneath`
(render),
`ui::layer_effects::tests::displace_takes_the_photo_beneath_and_samples_it_again_after_a_move`,
`tests::layer_effects_shadow_glow_and_blur_the_object` (render),
`tests::halftone_turns_the_object_and_its_effects_into_dots` (render),
`tests::inner_shadow_spread_darkens_the_feathered_edge` (render),
`live::tests::an_inner_shadow_saved_without_spread_reads_back`,
`halftone::tests::*` (dot areas, RGB, CMYK, cut and whole dots, screen
angle and centre),
`ui::layer_effects::tests::entries_switch_set_and_remove_their_effects`.

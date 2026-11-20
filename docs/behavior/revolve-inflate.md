# 3D Revolve and Inflate

Effects > 3D Revolve and Inflate opens a panel that turns the first selected
object (not a picture) into a shaded 3D look made of vector shapes. Apply
adds the result as one group on top, deletes the object unless Keep
original is on, selects the group, all in one undo step ("3D"). The shaded
colour is the object's solid fill, else its outline colour, else a mid
grey.

## Settings

| Control | Default | Range | Meaning |
|---|---|---|---|
| Mode | Revolve | Revolve, Inflate | Below |
| Position | | 16 views | Revolve: Front, Back, Left, Right, Top, Bottom, six off-axis and four isometric views; each sets Tilt, Turn and Roll (Custom when they match none) |
| Facets | 36 | 3 to 256 | Revolve: facets round the axis |
| Tilt | 20° | -90 to 90° | Revolve: the view looks down on the object by this much |
| Turn | 0° | 0 to 360° | Revolve: the object turned round its axis |
| Roll | 0° | -180 to 180° | Revolve: the view turned round the line of sight (after tilt) |
| Offset | 0 mm | 0 to 10000 mm | Revolve: the axis this far beyond the profile's edge (a ring) |
| Cap | on | | Revolve: ends of the profile off the axis closed by a disc (a pie piece when the sweep is open); a cap facing away is left out |
| Surface | Plastic | Plastic, Diffuse, No shading, Wireframe | Plastic: lit with the highlight; Diffuse: no highlight; No shading: each piece its own colour; Wireframe: facets as outlines 0.1 mm wide in the base colour, no fill |
| Sweep | 360° | 10 to 360° | Revolve: how far round the profile is spun, centred on the front |
| Axis on the right | off | | Revolve: the axis at the profile's right edge, not its left |
| Depth | 60 % | 10 to 200 % | Inflate: the dome's height in percent of the largest distance from the outline inward |
| Light direction | 135° | 0 to 360° | Where the light comes from on the page, counter-clockwise from the right |
| Light height | 45° | 0 to 90° | The light's elevation over the page |
| Shine | 40 | 0 to 100 | Highlight strength (Blinn highlight, power 40 for a hard light) |
| Light colour | white | | Tints the lit side and the highlight (the ambient light stays white) |
| Intensity | 100 % | 0 to 200 % | The light's strength |
| Softness | 0 % | 0 to 100 % | How far the light wraps past the edge it lights up to, and how broad the highlight is |
| Ambient light | on, 28 % | 0 to 100 % | An even light from everywhere; off, the side away from the light is black |
| Only the art | off | | Revolve with art: draw the wrapped art without the surface |
| Art | none | | Use selection picks the selected objects as art |
| Art across | 50 % | 0 to 100 % | Revolve: the art's centre round the way (50 in front); Inflate: across the shape's bounds |
| Art up | 50 % | 0 to 100 % | Revolve: along the profile's length; Inflate: up the shape's bounds |
| Art size | 30 % | 1 to 100 % | The art's width: of the way round (Revolve) or of the shape's width (Inflate) |
| Symbol | | | A document symbol drawn out (groups opened, nested symbols followed) as the art |
| Fit | | | Art across 50, up 50, size 100 and Stretch on |
| Stretch | off | | The art's height over the whole profile (Revolve) or its box over the shape's (Inflate) instead of keeping its proportions |
| Shade art | on | | The art lit like the surface; off, its own colours |
| Keep original | off | | Keep the object under the result |

Shading: with ambient `a` (0 when off), softness `s` and intensity `k`,
the diffuse term `d = max(0, (n·l + s) / (1 + s))` and brightness
`a + (1 - a) d k`; the part above `a` is tinted by the light colour. The
highlight is `shine k (1 - s/2) max(0, n·h)^(40 - 34 s)`, tinted by the light
colour, with `l` toward the light and `h` halfway between it and the viewer.
The defaults give `0.28 + 0.72 max(0, n·l)` and power 40.

## Editable result

The group keeps, under the data key `tracedraw.shape3d`, the settings
(art included) and the object it was made from. With such a group selected
the panel loads its settings; Apply (shown as Update) builds it again from
the kept object in the same layer, replacing the group in one undo step.
Release puts the kept object back (fresh ids) in place of the group.
Groups from Halftone sphere keep their half circle the same way.

## Halftone sphere

The Halftone sphere row makes a ball without a selection: a halftone of
round dots on a 45° screen (Dots across a disc, 6 to 80, default 22; dots
large in the middle, small at the rim), wrapped as art onto a half circle
80 mm across revolved all the way round in the middle of the window (art in
front, stretched over the front half from pole to pole), lit by the panel's light, tilt and turn.
Only the dots are drawn, as black filled curves shaded where they lie; the
result is one group in one undo step.

## Revolve

The first subpath of the object's outline (flattened) is the profile; the
axis is the vertical line through its left (or right) edge. Each profile
segment and each of the Facets steps round the axis make a four-sided facet
(a triangle where it touches the axis), drawn back to front as a filled
curve with a hairline outline of its own colour. Facets seen from inside
(an open cup) are lit on the side seen. Art is flattened, laid on the
surface (its width as a share of the way round at the profile's mean
radius, its height keeping its proportions along the profile; all the way
round it wraps past the back) and drawn on top in its own colour, shaded
where it lies; the parts facing away are left out.

## Inflate

A height field over a grid of 120 cells across the object's longer side:
inside the outline (even-odd), the distance to the outline `d`, the height
`top sqrt(1 - (1 - d/dmax)^2)` with `top` = Depth × `dmax`. Each cell is
shaded by its slope; cells of the same shade (64 steps a channel) in a row
merge into one band, a little larger than its cells so no seam shows. The
bands go in a clip frame shaped like the object. Art is placed on the
shape's bounds, swells away from its centre with the dome (up to 20 %) and
is shaded by the slope under its middle.

## Checks

- `shape3d::tests::revolve_makes_facets_lit_from_the_light`
- `shape3d::tests::revolve_art_shows_only_on_the_near_side`
- `shape3d::tests::inflate_is_highest_in_the_middle`
- `shape3d::tests::applying_revolve_and_inflate_is_one_step_each`
- `shape3d::tests::the_light_colour_intensity_softness_and_ambient_change_the_shade`
- `shape3d::tests::halftone_sphere_is_dots_only_big_in_the_middle_one_step`
- `shape3d::tests::hiding_the_surface_keeps_only_the_art`
- `shape3d::tests::positions_roll_offset_and_cap_shape_the_body`
- `shape3d::tests::a_3d_object_is_updated_in_place_and_released`
- `shape3d::tests::symbols_and_groups_become_art_and_fit_stretches_it`

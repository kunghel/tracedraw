# Frame Generator

Effects > Frame Generator opens the Frame Generator panel, which draws a
decorative frame as vector curves. With objects selected, the frame goes
round their bounds: its outer edge lies Margin + Band width outside them.
With nothing selected, its outer edge lies Margin inside the page. Apply
adds the frame as one group (named "Frame Generator") on top, in one undo
step ("Frame"), and selects it. The thin lines are stroked with Line width
and round caps and joins; ornaments (leaves, dots, beads, diamonds) are
filled; everything takes the chosen colour.

## Settings

| Control | Default | Range | Meaning |
|---|---|---|---|
| Style | Classic scrolls | Ruled lines, Classic scrolls, Art deco, Beads, Greek key, Scallops, Interlaced waves, Guilloche | Below |
| Corners | Square | Square, Round, Notched, Cut | How the band's outlines turn the corners (turn size 3/4 of the band width; lines further in follow it concentrically). Not shown for Art deco, Greek key and Interlaced waves, which have their own corners |
| Margin | 5 mm | -500 to 500 mm | Gap from the selection's bounds, or inset from the page edge |
| Band width | 8 mm | 0.5 to 200 mm | Width of the decorated band (at most half the frame's smaller side) |
| Line width | 0.5 mm | 0.05 to 10 mm | Width of the thin lines; outer lines are 2 or 3 times wider |
| Repeat | 8 mm | 1 to 200 mm | Length of one bead, key, scallop or wave; each side (or the whole way round, for waves) holds the nearest whole number |
| Lines | 12 | 2 to 60 | Guilloche: how many waved lines weave along the band |
| Ornaments on the sides | off | | A diamond flanked by two dots in the middle of each side |
| Color | black | | Colour of every line and ornament |

## Styles

| Style | Pieces |
|---|---|
| Ruled lines | A thick outer line (3 × Line width), a thin line 45 % in and a thin inner line |
| Classic scrolls | A double outer line, an inner line; in each corner a leaf on the diagonal and a scroll curling along each side, ending in a dot |
| Art deco | Three lines with corners stepped twice; in each corner of the inner area three quarter-circle arcs and three rays |
| Beads | Outer and inner lines and filled beads along the middle of the band, one in each corner |
| Greek key | Outer and inner lines (square corners), a baseline and a hooked spiral per repeat on each side, a square rosette with a diamond in each corner |
| Scallops | A scalloped outline (arcs bulging outward) and two inner lines |
| Interlaced waves | Outer and inner lines with round corners and two waves half a wave apart winding round the middle of the band |
| Guilloche | Outer and inner lines with round corners and Lines fine strands (0.35 x Line width) round the middle of the band: strand k is offset 0.36 x band x (0.55 + 0.45 cos(m u + phase / 2)) x sin(n u + phase), u the share of the way round, n waves of about Repeat, m = n / 4 rounded, phase 2 pi k / Lines |

A frame that does not fit (its outer edge smaller than twice the band
width) is not drawn; the status bar says so.

## Checks

- `frame_gen::tests::every_style_stays_in_its_band`
- `frame_gen::tests::corners_take_away_the_right_area`
- `frame_gen::tests::beads_sit_in_every_corner_and_scallops_bulge_out`
- `frame_gen::tests::a_frame_goes_round_the_selection_in_one_step`

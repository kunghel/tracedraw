# Area Fill

Fills any enclosed area, even one formed by overlapping objects, by
creating a new object from the area under the click. The objects that
enclose it stay as they are.

## Property bar

| Control | Choices | Default |
|---|---|---|
| Fill | Specify (the colour beside it), Use default (the default fill for new objects; a colour when that is none, so the area shows), None | Use default; the colour is yellow (C0 M0 Y100 K0) |
| Outline | Specify (the width and colour beside it), Use default (the default outline for new objects), None | Use default; 0.2 mm black |
| Up to outline edges | On: each outline counts as wide as it is drawn (#3321) | Off |
| Live | On: a click inside a group paints a live fill of the group (below) | Off |

The lists show their choices in that order: Specify first, then Use
default, then None (`fill_tool_bars::SMART_ORDER`; the values kept in
the preferences stay 0 for Use default, 1 for Specify, 2 for None, so
older preferences keep their choice;
`pick_drag_tests::smart_fill_lists_offer_specify_then_default_then_none`).
The colour buttons open the colour palette's swatches and, under Other,
a swatch that opens the colour picker beside the button for any other
colour (see `contour.md`, #1262). The choices are kept in the preferences
(`tools.smart_fill_mode`, `smart_fill_color`, `smart_outline_mode`,
`smart_outline_width`, `smart_outline_color`, `smart_outline_edges`,
`smart_fill_live`).

With Up to outline edges on, every object with an outline wider than 0
also adds the band its outline draws (as Convert Outline to Object makes
it, arrowheads included) as an opaque object right above it: the band's
edges are walls and the middle line under it is cut away (step 1's
rule for opaque fills). So the area stops at the inner edge of the
outlines, and lines whose middles leave a gap but whose widths overlap
close it. A click inside a band makes nothing, as a click on a thin
outline does. Objects with live effects count as before, by their middle
lines
(`pick_drag_tests::area_fill_can_stop_at_the_edges_of_wide_outlines`).

## Algorithm

1. The outlines: every outline of every visible object on the page
   (stroked or not, open curves too, a curve crossing itself enclosing
   the areas between its crossings), groups opened up; an object with
   live effects counts as it is drawn (its warped outline and the
   objects its effects add). The area may lie anywhere, on the page or
   off it on the desktop, in a drawing of any size (#499).
2. The end of an open outline that stops within 0.01 mm of another
   outline (or of its own start) is joined to it by a short line, so a
   drawing whose lines nearly meet, as files carry from rounding,
   encloses its cells all the same (#899). A wider gap stays open. The
   line runs through the end, from a millionth of the outlines' extent
   behind it to at least as far past the outline it meets (half the gap
   when that is more): the faces are cut on an integer grid of about a
   thousand millionth of the extent per unit (twice that at the
   coarsest), and an end within a unit of an outline, as the pieces an
   opaque fill cuts away come back
   (step 1), rounds to either side of it, so a line that only ran on
   from the end could miss the outline and the area leaked (#1281, the
   side of a circle extruded backwards).
3. The outlines, flattened to 0.02 mm, cut a box around them all (their
   bounds and 2 mm more) into faces (`shaping::enclosed_area`). The face
   under the click is the area; a face that reaches the box's margin is
   open (nothing encloses it), and a click there or outside the box
   makes nothing and the status bar says so; a click exactly on an
   outline makes nothing.
4. The outlines' own curves are put back into the face's sides, as the
   Shaping commands do: the new object follows the outlines exactly,
   its sides the same Bezier segments, with nodes only where outlines
   cross (#734; it was traced from pixels before, which gave jagged
   sides with many nodes). Outlines inside the area make holes in it.
   The object is put on top of the active layer, selected, with the
   fill and outline the bar says; one undo step.

## Inside a symmetry (#2998)

A click that falls in a symmetry object (the one being edited, or the
topmost whose original and copies cover the click) fills every copy:
the area is taken back to the original's side by the mirror or turn
that made the copy clicked (`live::symmetry_back_to`: the slice
between two mirror lines holding the original's middle) and joins the
symmetry as one of its objects, so its mirrors fill the same area in
each copy. One undo step; out of editing the symmetry is selected
afterwards, in editing the new object inside it.

## Checks

- `ops2::symmetry_tests::area_fill_in_a_symmetry_fills_every_copy` and
  `live::tests::a_point_in_a_symmetry_copy_goes_back_to_the_original`

- `ui::fill_tool_bars::tests::smart_fill_takes_the_bars_fill_and_outline`
  (a 40 mm square's inside becomes a 40 mm object; the bar's fill and
  outline, its own colour and outline, none).
- `ui::fill_tool_bars::tests::smart_fill_works_off_the_page_in_a_large_drawing`
- `shaping::tests::enclosed_area_keeps_the_outlines_curves`
- `shaping::tests::enclosed_area_closes_hairline_gaps` (lines 0.0002 mm
  short of a frame divide it; 0.2 mm short they do not; an open ring
  closed by a hairline is an area of its own)
- `shaping::tests::enclosed_area_closes_gaps_below_the_grid` (a line
  whose ends stop a few hundred millionths of a millimetre short of the
  frame, or past it, divides it)
- `extrude::tests::area_fill_reaches_the_side_of_a_back_cylinder` and
  `extrude_tool::tests::area_fill_takes_the_side_of_a_back_cylinder`
  (the curved side of a circle extruded with a Back type fills as one
  area from a click anywhere in it)
- `pick_drag_tests::area_fill_keeps_the_curves_it_fills_between`
- Given a circle 60 mm across, when the Area Fill tool clicks inside it,
  then the new object has four curves, every point within 0.02 mm of the
  circle; given the circle overlapping a rectangle, a click in the
  overlap makes exactly the overlap, its round side the circle's curve.

## Live

With Live on, a click goes to a group first: the selected group, or else
the topmost unlocked group on the page whose bounds hold the click. The
outlines of the group's objects (inside nested groups too; their fills
hide nothing) are the walls, and the area they close around the click is
painted as the live effect Live Paint on the group
(`tracedraw_core::live_paint`). The effect keeps each fill as a point in
the group's own space with its colour, and draws, under the group's
objects, the area the lines close around each point as they are at the
time. Moving, reshaping, adding or deleting a line inside the group moves
the edges of the fills it bounds; a point the lines no longer close round
draws nothing until they close again. The effect keeps at most 500
fills.

The colour is the bar's Fill: Specify its colour, Use default the default
fill when it is a flat colour (else the Specify colour). A click in an
area that already has a fill gives it the new colour; with the Fill set
to None it takes that fill away, and the effect goes when its last fill
does. Each click is one undo step and selects the group. When no group
closes an area round the click, the click makes an ordinary area as with
Live off. The Outline choices and Up to outline edges do not apply to
live fills.

- `tracedraw_core::live_paint::tests::live_paint_fills_follow_the_lines`
- Given a square of four lines crossed by one line and one across, a
  click in a cell fills exactly that cell; when the line moves the fill's
  edge goes with it; a second click recolours the fill, one with no
  colour removes it, and a click outside the square changes nothing.
- `pick_drag_tests::live_smart_fill_paints_a_cell_that_follows_its_lines`
- Given a group of a square and a line across it, with Live on a click in
  the left cell adds no object, gives the group one painted area ending
  at the line, which widens when the line moves 8 mm right; one undo
  takes the fill away.

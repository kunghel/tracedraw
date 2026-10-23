# Area Fill

Fills any enclosed area, even one formed by overlapping objects, by
creating a new object from the area under the click. The objects that
enclose it stay as they are.

## Property bar

| Control | Choices | Default |
|---|---|---|
| Fill | Use default (the default fill for new objects; a colour when that is none, so the area shows), Specify (the colour beside it), None | Use default; the colour is yellow (C0 M0 Y100 K0) |
| Outline | Use default (the default outline for new objects), Specify (the width and colour beside it), None | Use default; 0.2 mm black |

The colour buttons open the colour palette's swatches and a picker for
any other colour. The choices are kept in the preferences
(`tools.smart_fill_mode`, `smart_fill_color`, `smart_outline_mode`,
`smart_outline_width`, `smart_outline_color`).

## Algorithm

1. The area around the outlines (their bounds and 2 mm more, on the page
   or off it on the desktop; #499) is drawn at 4 pixels per mm, fewer
   for a large drawing (16 million pixels at most), as walls: every
   outline of every visible object on the page (stroked or not, open
   curves too, a curve crossing itself enclosing the areas between its
   crossings), groups opened up; an object with live effects counts as
   it is drawn (its warped outline and the objects its effects add).
2. From the pixel under the click the region is flooded up to the walls.
   A click on a wall makes nothing; a click outside the outlines' area,
   or a flood that reaches its margin (nothing encloses it), makes
   nothing and the status bar says so
   (`fill_tool_bars::tests::smart_fill_works_off_the_page_in_a_large_drawing`).
3. The region, with the wall pixels it touches so the new object meets
   the outlines, is traced to rings, each simplified within a pixel; the
   rings become one closed curve on top of the active layer, selected,
   with the fill and outline the bar says; one undo step.

## Checks

- `ui::fill_tool_bars::tests::smart_fill_takes_the_bars_fill_and_outline`
  (a 40 mm square's inside becomes a 40 mm object; the bar's fill and
  outline, its own colour and outline, none).

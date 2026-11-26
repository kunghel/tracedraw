# Contour

Concentric copies of an object's outline, inside, outside or to the
centre, as a live effect (`Effect::Contour`): the object keeps its own
outline and the steps are drawn from it, so the contour can be changed or
cleared at any time. The tool is `contour_tool.rs`; its bar and the
Contour panel share the controls in `ui/contour_bar.rs`.

| Control | Default | Range |
|---|---|---|
| To centre, Inside, Outside, Both | Outside | |
| Steps | 3 | 1 to 999 (to centre: as many as fit); a slider on a logarithmic scale with its number box |
| Offset | 2 mm | 0.001 to 1000 mm, between steps |
| Corners | Mitred | Mitred, Round, Bevelled |
| Colour direction | Direct | Direct, Clockwise, Counterclockwise (through the hues) |
| Last step's fill and outline colours | white, black | |
| Last fill colour (the end of a fountain fill, #1265) | white | |
| Object acceleration, colour acceleration | 0 % | -100 to 100 % |

The colour buttons of the bar (fill, outline and last fill colour)
open the colour palette's swatches
and, under Other, the colour's swatch: a click on it closes the list
and opens the colour picker beside the button (viewer, Model list,
eyedropper, sliders), which stays open while it is used and closes on a
click elsewhere or Esc. The picker used to sit inside the list, so the
first click closed both and the Model list took the list away with it
(#1262, `ui::fill_tool_bars::color_pick`, shared by the colour buttons
of the Extrude and Smart Fill bars;
`ui::fill_tool_bars::tests::other_opens_a_picker_that_stays` and
`the_first_click_on_other_opens_a_picker_that_works`).

Ctrl+F9 opens the Contour panel.

A contour in a `.cdr` file saved by another program (version 9 seen,
#1031) opens live: the object gets this effect with the file's offset,
step count, side and end colours, and the steps the file stores go
(`docs/cdr-format.md`, the Contour row).

## Formulas

- With `n` steps, step `i` (1 to `n`) is the object's outline offset by
  `n * offset * accelerate(i / n, objects)`: outward for Outside, inward
  for Inside. To centre: inward by `i * offset` until nothing is left.
- Distances are measured on the page: the outline is offset with the
  object's scale, stretch and rotation applied and the steps are taken
  back into the object's own coordinates, so an 80 mm inside step of a
  scaled polygon lies 80 mm from its edges (#754).
- The object's area is read with its fill rule: text, and objects set
  to the nonzero rule, with the nonzero rule, so the overlapping strokes
  of a letter (as many variable fonts draw them) make no holes, which
  the even-odd rule gave contours of their own (#2156).
- `accelerate(t, a) = t ^ (4 ^ a)`: 0 spaces the steps evenly, positive
  values pack them toward the object, negative ones toward the last
  step.
- Corners: the offset outline's corners are mitred (sharp; corners under
  about 9 degrees are cut off), round, or bevelled. Contours from older
  files, which have no corner setting, are round. A round corner's curve
  leaves the straight side before it along that side and arrives at the
  next side along it, so it runs on from the sides it joins and stays at
  the contour's distance (#1542,
  `shaping::tests::a_round_offset_corner_runs_on_from_its_sides`). The
  curves of an offset are fitted within 0.05 mm of it and within a
  tenth of the offset, whichever is less, so the round corners of a
  contour a few hundredths of a millimetre wide are still arcs and not
  straight cuts (#2176,
  `shaping::tests::the_round_corners_of_a_narrow_offset_stay_round`).
- Colours: step `i` of the `n` drawn takes `accelerate(i / n, colours)`
  of the way from the object's fill (and outline) colour to the last
  step's colours, mixed directly or through the hues clockwise or
  counterclockwise. An object without fill gives unfilled steps.
- A fountain-filled object's steps keep its fountain (kind, angle,
  stops' places) and change its colours (#1265): each stop at place `s`
  (0 at the start, 1 at the end) goes the step's share of the way to
  the colour `s` of the way from the fill colour to the last fill
  colour, so on the last step the fountain runs from the fill colour to
  the last fill colour. Contours from older files have no last fill
  colour and take the fill colour for both ends. Other fills (solid,
  pattern, texture, mesh) give solid steps from their first colour, as
  before.
- Outside steps are drawn behind the object, farthest first; inside
  steps in front of it, outermost first.
- Both (#3085): the steps go inside and outside at once, as many on
  each side, with the same offset, corners, colours and accelerations;
  each side is drawn exactly as the one-sided contour draws it (the
  outside steps behind, the inside ones in front). Saved as `both: true`
  on the effect, with `outside` true; a contour without it (older files,
  and every one-sided contour, which does not write it) is one-sided, so
  older versions read a both-sided contour as an outside one
  (`live::tests::a_contour_can_go_inside_and_outside_at_once`,
  `contour_tool::tests::the_contour_can_go_both_ways`).

## The tool

- A drag starting on an object (inside it counts, filled or not): away
  from the object's middle gives an outside contour, toward it an
  inside one (To centre and Both stay as they are); the distance from the start
  is `steps * offset` (the steps kept, the offset following). The
  contour is drawn live while dragging; one undo step.
- A click selects the object under the pointer (or nothing).
- A picture takes a contour (#3245): with see-through pixels the steps go
  round its opaque pixels (read at most 512 cells along its longer side,
  a cell opaque when any pixel in it is at least half opaque, steps
  straightened within 0.75 of a cell, specks under 4 cells left out),
  otherwise round its box; they are filled from white toward the last
  step's colour (`live::tests::a_picture_takes_its_contour_round_what_shows`,
  `contour_tool::tests::a_picture_takes_a_contour`).
- A group takes a contour (#3245): the steps go round the area its
  objects cover as one (their union, so overlapping objects make one
  outline), in the fill and outline of the first object in it that has a
  fill (else an outline) (`live::tests::a_group_takes_a_contour_round_all_its_objects`,
  `contour_tool::tests::a_group_takes_a_contour`).

## Handles

With the tool and one object selected that has a contour, the canvas
shows its handles (#1265, `ContourHandles`, `canvas::draw_contour_handles`,
plain segments and two small rectangles, the offset square 11 screen
pixels wide, or the node size of Options > Nodes and Handles and two
pixels more when that is larger, #2731):

- A line from the middle of the object's right edge (page bounds, `a`),
  to the right for Outside and Both, to the left for Inside and To
  centre.
- The offset handle (a square) at its end: `a + d * steps * offset`
  (To centre: `a + d * offset`), `d` being 1 or -1. Dragging it sets the
  distance `t` along the line from `a` (the pointer's height is
  ignored): `offset = t / steps` (To centre `offset = t`), at least
  0.01 mm, the steps kept.
- The steps handle (a bar across the line) at `a + d * offset`, the
  first step's distance without acceleration (none To centre). Dragging
  it to `t` gives `steps = round(steps * offset / t)` (1 to 999) and the
  offset that keeps the last step where it was.
- A press within 12 screen pixels (or the pick distance of Options >
  Nodes and Handles, when larger, #2731) takes a handle, the steps handle first
  where they meet; the pointer shows a sideways arrow over them. A drag
  from anywhere else draws a new contour as before.
- The contour is drawn live at every move; the first move of a drag
  starts an undo step "Contour" and the others change it, so a drag is
  one step.

## Property bar and panel

- The controls show the selected object's contour and edit the selected
  contours live (a run of changes is one undo step). Apply (shown when
  a selected object has no contour) adds the contour, as Enter does in
  a number box of the bar or panel (d#2945,
  `contour_bar::tests::enter_in_a_box_of_the_panel_applies_the_contour`);
  Clear Contour
  removes it; Break Apart turns the steps into objects, grouped beside
  the object when there are several (`App::break_effect_groups`). Apply
  is offered for a selected group too; it was not, so a group seemed
  unable to take a contour from the bar (d#3667). Copy Contour From...
  waits for a click on another object and gives the selected objects
  its contour, as Object > Copy Effect > Contour From does; a right
  click cancels (d#3667,
  `contour_bar::tests::a_group_is_offered_apply_and_a_contour_can_be_copied`).
- Object > Break Contour Group Apart (Ctrl+K, and the right-click
  menu), which takes the place of Break Apart while a selected object
  has a contour, does the same (#424;
  `contour_tool::tests::break_apart_breaks_a_contour_group`). A blend
  gives Break Blend Group Apart and an extrusion Break Extrude Group
  Apart; a drop shadow's Break Drop Shadow Apart comes first.
- The values are kept in the preferences (`tools.contour`).

## Checks

- `live::tests::contour_outside_three_steps`
- `live::tests::contour_offset_is_measured_on_the_page`
- `live::tests::contour_steps_keep_a_fountain_toward_both_end_colours`
- `shaping::tests::offset_corners_are_mitred_round_or_bevelled`
- `shaping::tests::offset_reads_overlapping_strokes_with_the_fill_rule_given`
- `live::tests::effects_read_text_with_the_nonzero_rule`
- `effect_outline_tests::inside_contour_of_letters_keeps_its_distance` and
  `outside_contour_of_letters_keeps_its_distance` (text crate): on the
  letters of a static and of a variable face, every point of a round
  contour lies at the offset from the letters' outline, of a mitred one
  no nearer.
- `contour_tool::tests::a_drag_draws_a_live_contour`
- `contour_tool::tests::the_bar_edits_the_selected_contour`
- `contour_tool::tests::handles_drag_the_offset_and_the_steps`: a 20 mm
  square with 3 outside steps of 2 mm has its line from (20, 10) to
  (26, 10) and the steps handle at (22, 10); the offset handle dragged to
  x = 32 gives 3 steps of 4 mm in one undo step; the steps handle then
  dragged to x = 22 gives 6 steps of 2 mm.
- `ui::contour_bar::tests::the_bar_and_the_panel_draw`
- Given a 20 mm square, outside, 3 steps of 2 mm, mitred, then the
  outermost step is a 32 mm square and its fill is the end colour.
- Given a 10 mm square offset by 2 mm, then the area is 196 mm2 mitred,
  `180 + 4 pi` round and 188 bevelled.
- Given an octagon scaled 1.7 times with its sides level, inside, one
  step of 20 mm, then the step lies 20 mm inside each side on the page;
  given a 20 mm square stretched 3 times across, one 4 mm inside step,
  then it lies 4 mm inside on all four sides.

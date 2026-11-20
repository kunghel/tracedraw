# Transformations panel

Five pages, chosen with the picture buttons at the top of the panel:
Position, Rotate, Scale and Mirror, Size and Skew. Alt+F7, Alt+F8,
Alt+F9 and Alt+F10 open the panel on Position, Rotate, Scale and Mirror
and Size (pressed again on the page shown, they close the docker
column); Object > Transformations opens any of the five.

Each page is laid out the same way: a line under the
picture buttons, the page's name with a colon (Rotate shows a turning
arrow before its angle instead), the fields as spin boxes (up and down
arrows at their right, a click steps by a whole millimetre, pixel,
degree or percent, or a tenth of an inch) with the unit after them,
some room, the check box, the 3 x 3 reference point under it (for a
relative position also the direction picture, a small grid with four
arrows, beside it), a line, Copies in a spin box and Apply across the
panel. Distances are in the ruler unit
(through the drawing scale), angles in degrees (counter-clockwise
positive), scales in percent.

| Page | Fields (defaults) | What Apply does, `b` the selection's bounds, `a` its reference point |
|---|---|---|
| Position | X, Y (0, 0); Relative position (on) | relative: moves by (X, Y); otherwise moves `a` to (X, Y) counted from the ruler origin |
| Rotate | Angle (0); Center X, Y (0, 0); Relative center (on) | turns by Angle about `c + (X, Y)`, or about (X, Y) from the ruler origin when Relative center is off; `c` is the centre of rotation moved on the canvas (Pick tool, rotate mode) when there is one, else `a` |
| Scale and Mirror | X, Y (100 %); Mirror horizontally, Mirror vertically (off); Proportional (on) | scales by X/100 and Y/100 about `a`, each negated when its mirror button is down; Proportional copies a typed X into Y and Y into X |
| Size | W, H (the selection's size); Proportional (on) | scales about `a` by W / width(b) and H / height(b) (a side of zero size stays); Proportional keeps H / W at the selection's ratio |
| Skew | X, Y (0); Use anchor point (off) | `x' = x + tan(X) (y - p.y)`, `y' = y + tan(Y) (x - p.x)` about `p`: `a` with Use anchor point, else the centre of `b`; the reference point is pale without it |

- The reference point defaults to the centre. Corners and edge middles
  pick the point of `b`; the top row is the top of the selection.
- On the Position page with Relative position on, the 3 x 3 boxes pick a
  direction, not a point (the centre shows as a ring): a side or a
  corner sets X to `-width(b)`, 0 or `width(b)` and Y to `height(b)`, 0
  or `-height(b)` that way, so Apply moves the selection, or puts each
  copy, right next to the last one; the centre sets (0, 0). The offset
  follows the selection's size (a new or another selected object, after
  Apply) until X or Y is typed into or Relative position is turned off
  and on again.
- The absolute fields follow the selection: Position with Relative
  position off shows where `a` is, Rotate with Relative center off shows
  `c` (the centre moved on the canvas, else `a`), and Size shows the selection's width and height, each time the
  selection or its bounds change (also after Apply). Turning a Relative
  box on sets its fields to 0.
- Copies (0 to 500, default 0): with 0, Apply transforms the selection
  (paragraph text frames take a scale as a new frame, as everywhere).
  With n copies the selection stays as it is and n copies are added to
  the active layer, copy k transformed k times; the last copies become
  the selection, so another Apply goes on from there. The copies are one
  undo step named after the page. Copies of linked text frames copied
  together are linked to each other, as Step and Repeat links them.
- Rotate keeps its centre: after Apply (with or without copies) the
  centre of rotation of the new selection is the point it turned about,
  so Apply again goes on round the same point (copies make a wheel
  round a centre moved off the object). Picking a reference point on
  the Rotate page lets a centre moved on the canvas go, and the turn is
  about that point again.
- With nothing selected the fields, the boxes, Copies and Apply are pale;
  the page buttons stay usable.

## Rotate by Two Lines (#2870)

Object > Transformations > Rotate by Two Lines (with something
selected, `rotate_lines.rs`) waits for four clicks, each snapped as a
drawing click: the two ends of a base line on the selection, then the
two ends of a target line anywhere. The clicked lines show on the
canvas, the last to the pointer, and the status line says which line
comes next. After the fourth click the selection turns about the base
line's first end by the angle from the base line to the target line
(first end to second, the shorter way round), one "Rotate" undo step.
A line whose ends are the same point turns nothing; a right click
cancels
(`rotate_lines::tests::the_base_line_takes_the_target_lines_angle`).

## Checks

- `ui::transform_docker::tests::position_moves_by_an_offset_or_to_a_point`
- `ui::transform_docker::tests::rotate_scale_size_and_skew_keep_the_reference_point_still`
- `ui::transform_docker::tests::copies_repeat_the_transformation_in_one_step`
- `ui::transform_docker::tests::rotated_copies_turn_about_the_centre_moved_on_the_canvas`
- `ui::transform_docker::tests::absolute_fields_follow_the_selection`
- `ui::transform_docker::tests::relative_position_boxes_pick_a_direction_the_size_of_the_selection`
- `ui::transform_docker::tests::the_docker_draws_with_nothing_selected`
- `ui::transform_docker::tests::the_shortcuts_open_a_page_and_close_it_again`
- `ui::transform_docker::tests::the_docker_draws_every_page`
- `textflow::tests::transformation_copies_of_a_chain_are_linked`

## Isometric faces

The Skew page also shows six buttons under Apply, Left, Right, Top,
Left ↑, Right ↑ and Top ↻, and Reset. Each sets every selected object
onto a face of an isometric cube about the middle of its own box,
keeping its width and height (and a mirror) and dropping whatever turn
or slant it had, so a second face replaces the first instead of adding
to it; Reset sets it flat again. It is the object's transform alone: a
text stays text and edits as before. One undo step ("Isometric Face",
"Reset Face"). With `c = cos 30°` and `s = 1/2`, a face takes the
object's x and y directions (page y up) to:

| Face | x | y |
|---|---|---|
| Left | (c, -s) | (0, 1) |
| Right | (c, s) | (0, 1) |
| Top | (c, s) | (-c, s) |
| Left ↑ | (0, 1) | (-c, s) |
| Right ↑ | (0, 1) | (-c, -s) |
| Top ↻ | (c, -s) | (c, s) |

The new transform is `translate(m') * F * scale(sx, sy) * translate(-m)`,
`m` the middle of the object's box, `m'` where it was on the page, `sx`
and `sy` the lengths of the old transform's columns. Three texts set on
Top, Left and Right and moved edge to edge make a cube.
Test: `ui::transform_docker::tests::isometric_faces_set_the_slant_and_reset_takes_it_away`.

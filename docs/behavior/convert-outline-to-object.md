# Convert Outline to Object

Object > Convert Outline to Object (Ctrl+Shift+Q) replaces the outline
of each selected object by a new closed object filled with the outline's
colour, the shape the outline draws; the original keeps its fill and
loses its outline. One undo step, and the new objects are selected.

- A hairline counts as 0.25 mm wide.
- A dashed outline keeps its dashes: each dash becomes a closed shape of
  its own and the gaps stay empty, the pattern in widths as drawn
  (#490).
- A variable or calligraphic outline, or one inside or outside the path,
  becomes its band.
- A closed path, an ellipse included, is stroked all round, with no
  ends at its start.
- The result is clean: the stroke is computed on short lines, then
  fitted back to lines and cubic curves within 0.01 mm, with corners
  kept as corners and as few nodes as that allows. An ellipse's outline
  becomes two rings of a few curves each, not hundreds of short lines
  (#622). Each smooth run between corners is fitted by least squares,
  refined by Newton steps and split in the middle until it fits, so the
  nodes fall evenly; a smooth closed ring is split at its leftmost and
  rightmost points.

## Checks

- Given a line with a dashed outline, the result is one closed shape
  per dash with the gaps empty
  (`tools2::tests::a_dashed_outline_becomes_one_shape_per_dash`).
- Given an ellipse with a 4 mm outline, the result is a 4 mm ring of 8
  to 16 curves and no lines, its outer size 64 x 44 mm
  (`tools2::tests::an_ellipse_outline_becomes_a_few_curves`).
- Given a straight line with butt ends, the result is a rectangle of
  four lines (`tools2::tests::a_straight_outline_becomes_a_rectangle`).
- The fitting: a flattened circle becomes four curves within 0.02 mm,
  a rectangle four lines, collinear points one line, an open arc one
  curve keeping its ends, and degenerate input does not panic
  (`tracedraw_core::fit::tests::a_flattened_circle_becomes_four_curves`,
  `tracedraw_core::fit::tests::a_rectangle_keeps_four_corners_and_lines`,
  `tracedraw_core::fit::tests::collinear_points_become_one_line`,
  `tracedraw_core::fit::tests::open_runs_keep_their_ends`,
  `tracedraw_core::fit::tests::nothing_breaks_on_degenerate_input`).

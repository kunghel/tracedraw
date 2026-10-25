# Variable Outline tool

The Variable Outline tool (its own button at the bottom of the toolbox,
after the Fill flyout) makes an outline thicker or thinner along its
length with width nodes.

## Using it

- Select an object with a path (a curve, a rectangle, an ellipse, text
  converted to curves). A press away from the selected object's outline
  selects the object under the pointer.
- Drag across the outline: a width node is added where the drag starts
  (or the node under the pointer is picked) and the outline reaches to
  the pointer on both sides. With Shift (or Alt) held, only the side the
  pointer is on changes (#374); it counts while the drag goes on, so
  pressing or letting go of it mid-drag switches between one side and
  both (#492).
- A drag that starts on a node's centre slides the node along the path
  when it first moves along the path (kept between its neighbours), and
  changes its reach when it first moves across it; a drag from one of a
  node's reach handles (the dots at its ends) changes its reach. One undo
  step per drag (#479,
  `width_nodes_slide_along_the_path_and_their_handles_change_the_reach`).
- Picking several nodes: Shift+click picks the nodes from the picked one
  to the one clicked; Ctrl+click adds a node or takes it out; a plain
  click picks one alone. A drag that starts on a picked node changes
  every picked node the same way, and the property bar's widths set
  them all (`shift_and_ctrl_pick_nodes_and_shift_drags_one_side`).
- Each drag is one undo step.
- Delete (or Backspace) removes the picked nodes, one step. An object with no
  outline gets a hairline one when its first node is added.

## Property bar

- Left width and Right width: how far the outline reaches on each side
  of the path at the picked node, in document units. The left is to the
  left of the path's direction.
- Delete Node removes the picked node; Clear Variable Outline removes
  every node of the selected objects, giving back the plain outline.

## Defaults and formulas

- A node sits at `t` (0 to 1) of the path's length over all its
  subpaths. A new node takes the reach the outline already has there.
- The ends of the path keep half the outline width on each side unless
  a node sits on them. Between neighbouring nodes `a` and `b` the reach
  eases with `s = u^2 (3 - 2u)`, `u = (t - t_a) / (t_b - t_a)`.
- The band is built from the path flattened to 0.05 mm and cut into
  pieces no longer than 1/256 of its length (at least 0.05 mm), with 48
  pieces between neighbouring nodes whose reach differs, however short
  that span (so the ease is a smooth curve, not a few straight pieces;
  #492, `shaping::tests::variable_band_eases_smoothly_between_close_nodes`),
  split at
  the nodes; corners get round joins, ends are butt. The pieces of one
  segment share its direction and every piece has its ends on the path
  as corners, so the pieces and joins meet without cracks
  (#627, `shaping::tests::a_variable_band_along_a_curve_has_no_cracks`).
- With "Scale with object", the reach scales with the object like the
  width.
- On screen the page is drawn in strips on several threads, each
  leaving out the objects that cannot reach it. A band counts as
  reaching as far as its widest node, however thin the outline's own
  width, so no strip leaves part of it out (#626,
  `tracedraw_render::band_tests::a_variable_outline_is_drawn_whole_across_the_strips`).
  Inside a group or a clip frame the same holds: a group is left out of
  a strip only when every object in it is (groups inside groups too),
  and a clip frame when its frame is, since the group's own bounds count
  no outline and used to cut a variable or wide outline inside it flat
  at the strip's edge
  (`tracedraw_render::band_tests::outlines_inside_groups_are_drawn_whole_across_the_strips`,
  `tracedraw_render::culling_tests::objects_out_of_sight_are_skipped`).
- The outline's colour, or its gradient, pattern or texture fill, paints
  the band. Convert Outline to Object (Ctrl+Shift+Q) gives the band as an
  object: built from the path flattened to 0.005 mm and fitted to a few
  curves, its butt ends straight lines (#627,
  `tools2::tests::a_variable_outline_becomes_a_band_of_few_curves`). SVG and PDF exports write the band as a filled path; `.cdr`
  files are written with the plain width.

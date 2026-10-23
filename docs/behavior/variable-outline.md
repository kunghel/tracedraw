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
  the pointer on both sides. With Shift (or Alt) held at the start of
  the drag, only the side the pointer is on changes (#374).
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
  pieces no longer than 1/256 of its length (at least 0.05 mm), split at
  the nodes; corners get round joins, ends are butt.
- With "Scale with object", the reach scales with the object like the
  width.
- The outline's colour, or its gradient, pattern or texture fill, paints
  the band. Convert Outline to Object (Ctrl+Shift+Q) gives the band as an
  object. SVG and PDF exports write the band as a filled path; `.cdr`
  files are written with the plain width.

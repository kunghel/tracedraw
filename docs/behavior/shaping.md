# Shaping

Object > Shaping, the property bar's shaping buttons (with two or more
objects selected) and the Shaping panel.

## Operations

The target gives the result its fill, outline and opacity; the other
selected objects are the sources. With objects picked one by one
(click, Shift+click) the target is the one selected last; with a
selection made by a marquee or a freehand region, the target is the
bottom-most object of it (as long as the selection is not changed by a
click afterwards). Checked by
`ops::curves_and_target_tests::shaping_targets_the_last_clicked_object_but_the_bottom_one_of_a_marquee`.

| Operation | Needs | Result | What stays (menu, property bar) |
|---|---|---|---|
| Weld | 2 | one object covering all of them | nothing |
| Trim | 2 | the target without what the sources cover | the sources |
| Intersect | 2 | the area all of them share | the sources and the target |
| Simplify | 1 | each object without what objects above it cover | the objects, changed in place |
| Front Minus Back | 2 | the front object without the ones behind (its attributes) | nothing |
| Back Minus Front | 2 | the back object without the ones in front (its attributes) | nothing |
| Boundary | 1 | an outline around the selection (the target's attributes) | every object |

The result goes on top of the active layer and becomes the selection; an
empty result changes nothing and the status bar says so. Each operation
is one undo step.

When the object whose attributes the result takes is a group: Trim, Front
Minus Back and Back Minus Front cut each object of the group on its own,
each keeping its fill, outline and opacity, the pieces a group (pieces
cut away entirely are left out); Weld and Intersect take the attributes
of the group's first object with a fill. The result took the group's own
attributes, none, and showed only in Wireframe (#539,
`back_minus_front_with_a_group_keeps_each_objects_colours`).

## Curves

- The operations run on the outlines flattened to 0.02 mm (`i_overlay`),
  and the curves of the objects are then put back into the result
  (`shaping::recover_curves`): every vertex of the polygon result that
  lies on an object's outline (within 0.03 mm, the crossing of two
  flattened edges lies up to 0.02 mm off the curves) is placed on it, and
  a stretch of vertices running along one subpath becomes that stretch
  of the subpath itself, with its segments and handles. The result keeps
  the Bezier segments of the objects, with new nodes only where their
  outlines cross; two circles welded give a handful of curve segments,
  not hundreds of tiny lines.
- An edge that does not follow an object's outline (a chord across an
  arc, as where a cut runs straight across a curve) stays a straight
  line: a stretch is taken from the source only when the edges lie
  within 0.04 mm of it, so the result is never worse than the polygon.
  Nodes of the objects on the result stay nodes.
- Checked by `shaping::tests::welding_circles_keeps_their_arcs`,
  `shaping::tests::lines_and_arcs_both_come_back` and
  `shaping::tests::a_chord_stays_a_line`.

## Panel

- Seven picture buttons choose the operation; under them its name, a
  large picture and what it does, and that the last object selected is
  the target.
- Weld, Trim and Intersect each keep their own choice of what to leave:
  "Leave original source object" and "Leave original target object"
  (Weld: neither, Trim: the sources, Intersect: both, as the menus do).
  The choices and the operation are kept in the preferences
  (`panels.shaping_op`, `panels.shaping_leave`).
- The button under them, named after the operation, applies it; it is
  pale until enough objects are selected, and a line says how many.

## Checks

- `ui::shaping_docker::tests::what_the_operations_leave_of_the_originals`
- `ui::shaping_docker::tests::the_docker_draws_every_operation`
- `ui::propbar::tests::shaping_and_grouping_show_for_several_objects_groups_and_parts`

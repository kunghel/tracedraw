# Convert Outline to Object

Object > Convert Outline to Object (Ctrl+Shift+Q) replaces the outline
of each selected object by a new closed object filled with the outline's
colour, the shape the outline draws, placed right above its object in
that object's layer. An object whose fill draws something keeps its fill
and loses its outline; one without a fill, or an open line or curve
(whose fill is not drawn unless the document fills open curves), is
replaced by the new object, so no invisible object stays behind (#999).
One undo step, and the new objects are selected.

- A hairline counts as 0.25 mm wide.
- A dashed outline keeps its dashes: each dash becomes a closed shape of
  its own and the gaps stay empty, the pattern in widths as drawn
  (#490).
- A variable or calligraphic outline, or one inside or outside the path,
  becomes its band, fitted to lines and cubic curves within 0.01 mm. A
  Variable Length band is built from the path flattened to 0.005 mm for
  it: the eased width becomes a few curves on each side and the butt ends
  lines (#627).
- A constant-width outline is not flattened: each segment of the path is
  offset by half the width to either side, a line as a line and a curve
  as the fewest cubic curves that stay within 0.01 mm of its true offset
  (a curve's offset is fitted where it is not a cubic itself), so the
  sides keep about as many nodes as the path has and smooth nodes stay
  smooth. An ellipse's or a circle's outline is two rings of four curves
  each; a smooth curve of three segments with butt ends becomes six
  curves and two lines (#622). Before, the stroke was computed on short
  lines and fitted back to curves, which gave several times the path's
  nodes on curves, with near-straight kinks between them.
- Miter and bevel corners and square ends are drawn as the outline
  draws them (miter limit included). Round ends and round corners are
  arcs of the pen's circle around the path's end or corner, drawn with
  nodes on the side lines and at each end's tip: each round end is two
  curves meeting exactly at its tip, a round corner one curve up to a
  right angle, two beyond; straight sides stay single lines (#623).
- The sides, ends and corners are joined into one shape (nonzero union,
  the curves put back after the union): where the outline overlaps itself
  (the inner side of a corner, the subpaths of a combined curve that
  cross) the result is filled as the outline was drawn, not a hole under
  the even-odd fill of curves (#997). Each closed subpath ends with its
  own line or curve back to its start.
- A closed path, an ellipse included, and an open one that ends where it
  starts, is stroked all round, with no ends at its start.

## Checks

- Given three rectangles, the middle one with an outline and no fill,
  the result is three objects with the outline object in the middle
  and the original gone; Undo brings the original back in one step
  (`tools2::tests::an_unfilled_object_is_replaced_by_its_outline`). An
  open curve is replaced the same way
  (`tools2::tests::an_open_curve_is_replaced_by_its_outline`); a filled
  middle rectangle keeps its fill with the outline object right above it
  (`tools2::tests::a_filled_object_keeps_its_fill_under_its_outline`).
- Given a combined curve of a curved stem and a straight bar crossing
  it with a 4 mm round outline, the result is one closed shape filled
  at the crossing and along both strokes
  (`tools2::tests::crossing_subpaths_become_one_filled_shape`).
- Given a line with a dashed outline, the result is one closed shape
  per dash with the gaps empty
  (`tools2::tests::a_dashed_outline_becomes_one_shape_per_dash`).
- Given an ellipse with a 4 mm outline, the result is a 4 mm ring of 8
  to 16 curves and no lines, its outer size 64 x 44 mm
  (`tools2::tests::an_ellipse_outline_becomes_a_few_curves`).
- Given a smooth open curve of three segments (four nodes) with a 2 mm
  butt-ended outline, the result has at most 10 nodes, two lines (the
  ends) and the area of the outline drawn within 0.2 %; with round ends
  at most 12 nodes; a circle of four curves with a 2 mm outline becomes
  two rings of four curves, 2 mm apart
  (`tools2::tests::a_curved_outline_keeps_about_as_many_nodes_as_its_path`).
  The same curve converted the old way had 32 nodes.
- With the Shape tool on the outline of a smooth curve converted with
  butt ends, the corner widgets show at its four butt corners only
  (`tools2::tests::corner_widgets_show_only_where_they_round_something`,
  see `shape-tool.md`, Corner widgets).
- Given a straight line with butt ends, the result is a rectangle of
  four lines (`tools2::tests::a_straight_outline_becomes_a_rectangle`).
- Given a curved open path with a thick butt-ended outline (an arc 87 mm
  across with a 5.8 mm outline, as in #622), the result is four cusp
  nodes at the butt corners, one curve along each long side and
  straight ends
  (`tools2::tests::a_thick_butt_ended_arc_becomes_four_nodes_and_two_curves`).
- Given a line with a corner, round ends and round corners, the result
  is four straight sides, two curves per end and one for the corner,
  with nodes at the tips and where the ends meet the sides
  (`tools2::tests::round_ends_and_corners_become_few_curves`); on a
  curved path the ends also meet the sides at nodes and the whole
  outline stays within 16 segments
  (`tools2::tests::round_ends_of_a_curve_meet_its_sides_at_nodes`).
- Given a 0.25 mm line with a corner, round ends and a round corner,
  and a straight hairline with round ends, the results reach as far as
  the strokes and have their areas within 0.5 %
  (`tools2::tests::a_thin_round_outline_keeps_its_shape`).
- Given a line with a Variable Length node of 10 mm in the middle of a
  2 mm outline, the result is two curves per side and two straight
  ends; along an arch, two straight ends and at most 16 curves, with the
  area of the band drawn within 0.2 %
  (`tools2::tests::a_variable_outline_becomes_a_band_of_few_curves`).
- The fitting: a given node with no vertex within the tolerance goes to
  the nearest vertex within its reach
  (`tracedraw_core::fit::tests::a_node_between_two_vertices_takes_the_nearest`);
  of two nodes within reach, the nearer one.
- Given 10 mm and 17 mm lines with round ends, each tip has a node on
  it (within 0.001 mm), each end two curves
  (`tools2::tests::wide_round_ends_have_a_node_at_each_tip`).
- The fitting: given nodes where a stadium's ends meet its sides, it
  becomes two lines and four curves with nodes at the tips
  (`tracedraw_core::fit::tests::given_nodes_split_a_stadium_where_its_ends_meet_its_sides`);
  a flattened circle becomes four curves within 0.02 mm,
  a rectangle four lines, collinear points one line, an open arc one
  curve keeping its ends, and degenerate input does not panic
  (`tracedraw_core::fit::tests::a_flattened_circle_becomes_four_curves`,
  `tracedraw_core::fit::tests::a_rectangle_keeps_four_corners_and_lines`,
  `tracedraw_core::fit::tests::collinear_points_become_one_line`,
  `tracedraw_core::fit::tests::open_runs_keep_their_ends`,
  `tracedraw_core::fit::tests::nothing_breaks_on_degenerate_input`).

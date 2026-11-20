# Draw in Perspective

View > Draw in Perspective shows a perspective grid over the page. While
it is on, every object a drawing tool makes (rectangles, ellipses,
polygons, curves, brush strokes, text frames...) is laid onto the active
plane of the grid. Objects made with the Pick tool (pasted, duplicated,
moved) are left as they are.

## The bar

At the top left of the drawing window:

- Finish: leaves the mode (the grid goes; the objects keep their
  perspective).
- 1-Point, 2-Point, 3-Point, 4 Corners: the kind of grid (a new grid of
  that kind, sized to the page; 4 Corners asks for its corners, and
  clicked again sets them anew).
- The planes: Ground, Left, Right (and Top with 1-Point). The active
  plane's grid lines are strong, the others faint.
- Grid: show the grid lines.
- Depth (5 to 98 %): how far towards the vanishing points the grid
  reaches (not with 4 Corners).
- Put on Plane: lays the selected objects (drawn before, pasted or
  imported) onto the active plane, one undo step each.

## 4 Corners

A plane set over a photo: after choosing 4 Corners the bar says to click
the plane's four corners on the page, and the next four clicks are its
corners (near left, near right, far right, far left, as the floor, a
wall or a road shows them); the grid shows once the fourth is in. Then:

- The corners drag (squares); the middle of a side drags the side, both
  its corners.
- Ctrl+drag on the middle of a side pulls up a plane standing on that
  side (Front, Right, Back, Left after the first to fourth side), which
  becomes the active plane; its top's middle (a circle) sets its height.
  Let go with no height (under half a millimetre), the plane goes.
- A plane seen from beneath is turned over, so what goes onto it reads
  the right way round.

The standing planes are square to the base plane in the photo's
perspective. The base plane's two pairs of sides meet at its vanishing
points `V1` and `V2` (homogeneous, at infinity for parallel sides).
With both on the page's plane and the page's middle `P` between them
(`(V1 - P).(V2 - P) < 0`, a camera pointed at the middle), the upright
lines head for `V3`, the point making `P` the meeting point of the
heights of the triangle `V1 V2 V3`: `(V3 - P).(V1 - V2) = 0` and
`(V3 - V1).(V2 - P) = 0`. Otherwise the upright lines are parallel,
square to the horizon (upright on the page when the horizon is level).
A standing plane on side `a b`, `h` mm high at the side's middle `m`:
its top passes through `m + h u` (`u` the unit direction from `m`
towards `V3`) and heads for the side's vanishing point; its top corners
are where that line crosses the upright lines through `a` and `b`
(`Session::vertical_vanishing`, `wall_quad`).

## Objects moved with the Pick tool

While drawing in perspective, a move with the Pick tool (a drag, also
with a copy) ends by:

- An object already on the active plane (its Perspective corners a
  rectangle of the plane) sliding along it: its rectangle on the plane
  keeps its size, and its middle goes where the move took the middle on
  the page.
- Any other object whose box's middle lands inside the active plane's
  grid going onto it there, as large as it was on the page at that
  spot: its rectangle on the plane is centred where its middle landed,
  its page width and height divided by how long the plane's unit sides
  are on the page there (the derivative of the plane's map), so an
  object dropped near the horizon comes out small and one dropped near
  the viewer large (`dropped_corners`).

Both join the move's undo step, and the object stays what it was: a
text keeps editing as text, a rectangle stays a rectangle
(`App::perspective_after_move`). Objects let go outside the grid, onto a
clip frame or another page are left as the move left them.

## Defaults

- Horizon at 55 % of the page height.
- 1-Point: the vanishing point in the middle of the horizon; the near
  face from 15 % of the width, 70 % of the width wide, 8 % up from the
  bottom and 75 % of the height tall; depth 70 %.
- 2-Point: vanishing points at -35 % and 135 % of the width on the
  horizon; the near corner in the middle, 8 % up; depth 45 %.
- 3-Point: as 2-Point, with the vertical vanishing point at 220 % of the
  height above the corner; the walls' vertical edges lean to it.

## Handles

Drag on the page: the vanishing points (circles; with 2- and 3-Point the
two side points share the horizon), the near bottom corner and the near
top corner (squares; with 1-Point the top corner also sets the width).

## How objects are laid down

Each plane is the projective image of a unit square (`homography`, the
same as the Perspective effect's). The drawn object's box is taken onto
the plane: each side of the box becomes the mean of its two corners'
plane coordinates, giving a rectangle on the plane, whose corners on the
page are the object's Perspective effect (Object > Add Perspective),
so the result is exact and the Shape tool edits its four corners. A box
drawn past a plane's vanishing line is pulled towards the grid until it
fits, and one that lies wholly beyond it stays flat. Drawing the object
and laying it down are one undo step.

## How the Perspective effect maps an outline

The object's box goes onto the four corners by the projective map that
takes its corners there (`live::perspective_in`). The nodes and control
points of the outline are mapped, so every segment stays one segment:
straight sides stay straight and a curve bends as its handles do. Two
neighbouring corners dragged onto one point (a triangle) bring the
handles along that side to the point too, so the outline comes to a tip
there: an ellipse becomes a drop. Corners that meet, or that were dragged
a little past each other (within a fiftieth of the frame), are set a
hair apart in the order of the side across from them; the box could not
be mapped onto three points, and the object stayed flat or, with the
corners crossed, came out as a sliver running off past the frame (#879,
`live::tests::a_perspective_with_two_corners_together_makes_a_tip`). A
frame folded over itself on purpose puts part of the outline past the
horizon: that outline is flattened and mapped point by point instead
(`a_folded_perspective_stays_finite`).

## The Perspective effect on a group

Object > Add Perspective on a group warps every object in it with the
group's four corners, before the objects' own effects (a contour rings
the warped object). Ungroup (Ctrl+U, also Ungroup All) keeps the
perspective: each object takes a Perspective effect at the front of its
stack, the group's projection as seen in the object's space, so the
objects stay drawn as they were and the Shape tool edits each one's
corners. An object that had a perspective first in its stack keeps one:
the two are folded into it. Only the group's perspective is kept; its
other effects go with the group, as before (#674).

## Checks

- Given a 4 Corners plane over the picture of a square floor taken by a
  camera turned 30 degrees and tilted 20 degrees down, the upright
  vanishing point found from its corners is the camera's, and a plane
  pulled up on a side has its upright sides heading there
  (`four_corners_find_the_upright_direction_of_the_photo`).
- Given a rectangle moved into a 4 Corners plane with the Pick tool, it
  lies on the plane, one undo step with the move; moved again, it slides
  along the plane with the same size on it
  (`objects_moved_onto_the_plane_lie_on_it_and_slide_along_it`).

- Given a group of two rectangles, one scaled, the group turned and
  given a perspective, and the other rectangle with no effect, an
  outside contour, a push and pull distortion or a perspective of its
  own, when it is ungrouped, then each rectangle has one Perspective
  effect, first in its stack, and is drawn where it was in the group,
  its contour or distortion included
  (`command::tests::ungrouping_keeps_the_perspective_on_each_object`,
  core).

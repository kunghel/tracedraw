# Shape tools

Rectangle, 3-point rectangle, Ellipse, 3-point ellipse, Polygon, Star,
Complex star, Spiral, Graph paper, Common shapes and Action Lines. All of them
draw by dragging a box; the object takes the toolbox defaults (no fill,
a 0.2 mm black outline) unless the Properties panel set other defaults.

## Live preview

Pressing to draw lets go of whatever was selected at once. While the
button is held the object itself follows the pointer, built
with the current property bar settings (corner radius, arc angles,
points and sharpness, revolutions, rows and columns, the chosen common
shape, action lines): the same geometry the release creates. It is drawn
in the colour and width of the default outline, at least one pixel wide
(the selection colour when new objects get no outline), and over it a
black hairline on a white line twice as wide, so its edge shows on any
outline colour and any drawing under it (#670; the 3-point tools'
preview and their base line too, while it is dragged and while it waits
for the third click). The zoom and text frame drags, and a
box tool's rubber band, are a hairline (blue, black for a text frame) on
a white line twice as wide, so they show over a black fill too (#672).
Crop keeps a plain rectangle. After the release the new
object is selected and shows the eight handles and the centre marker
whatever tool is active.

With the Rectangle, Ellipse, Polygon, Star (and Complex star), Graph
paper, Action Lines, 3-point rectangle, 3-point ellipse, Spiral and
Common shapes tools (and the Text tool) still chosen, the pointer over one of those handles
shows the resize arrows and a drag from it resizes the selection as the
Pick tool's handles do (Shift from the centre, Ctrl in steps, Alt
freely, a right click for a copy), the tool staying chosen. A click on
a handle leaves the selection as it is; a press anywhere else draws a
new object as before
(`pick_drag_tests::a_drawing_tool_resizes_by_the_handles`). The Text
tool does the same with the handles of a text just typed (or a frame
just drawn), the text staying open for typing (#3258,
`pick_drag_tests::a_text_just_typed_resizes_by_its_handles_with_the_text_tool`).

## Drag modifiers (every box tool)

- Ctrl constrains the box to a square (circle, regular polygon).
- Shift grows the box from the start point as its centre; it is read
  while dragging, so it can be pressed or released mid-drag.
- Snapping applies to the start and current points (see `snapping.md`).
- A drag shorter than 0.05 mm in either direction creates nothing.
- 3-point rectangle: Ctrl while dragging the base keeps it to steps of
  the constrain angle from its start (#934,
  `pick_drag_tests::ctrl_keeps_the_three_point_base_to_the_constrain_angle`).
  3-point ellipse: Ctrl while dragging the first line draws a circle
  round the press point, the line its radius (turned in steps of the
  constrain angle, as long as the drag), and the circle is made when the
  button is let go, with no third click; the preview shows the circle
  with the radius (#2550,
  `pick_drag_tests::ctrl_draws_a_three_point_circle_round_the_press_point`).
  The rectangle takes the dragged base as one of its sides and the third
  click as its height. The ellipse takes the base as its centre line (an
  axis, from end to end) and grows from it to the third click on both
  sides alike, so the click sets half its other axis (#1511,
  `pick_drag_tests::a_three_point_ellipse_grows_from_its_centre_line_to_both_sides`).

## Rectangle (F6)

- New rectangles take the corners set on the property bar (or in Tools >
  Options > Toolbox > Rectangle tool) while nothing is selected; default
  square, round, relative scaling off (the corners keep their size).
- Corner style: Round (a quarter circle), Scalloped (a quarter circle
  centred on the corner, cutting inwards) or Chamfered (a straight cut).
  The size is the radius, or for a chamfer how far the cut starts from
  the corner.
- Four sizes: top left, top right, bottom left, bottom right (the bar
  shows them as two columns, left corners then right ones). With Edit
  Corners Together (the lock, on by default) a size typed in one field
  goes to all four. A corner is not limited to half the shorter side:
  the two corners on a side together may be as long as the side, so on
  a 100 x 80 mm rectangle with a 10 mm top left corner and square
  bottom corners the top right corner can be 80 mm, a quarter circle
  down the whole right side. When the two corners on some side are
  longer than it, every corner is drawn smaller by the same factor (the
  side's length over their sum, for the tightest side): equal corners
  stop at half the shorter side. The sizes typed are kept (the drawing
  grows back to them when the rectangle grows) and the fields show the
  sizes drawn (#1757, `Corners::fit_factor`,
  `document::tests::corners_shrink_only_when_a_side_is_too_short_for_its_two`,
  `ui::propbar::tests::a_corner_field_takes_what_the_neighbours_leave`).
- Each field is the corner seen at that position on the page, not the
  rectangle's own corner of that name: a rectangle turned by 180
  degrees, flipped, or stretched across its own side with a handle
  (as a flap is made from a panel) shows and edits in its top left
  field the corner that lies at the top left (`Corners::seen_through`:
  the rectangle's x axis counts as horizontal when it leans that way
  more than it rises, else as vertical, and the y axis as the other
  one, each with its direction; a rectangle turned by less than 45
  degrees keeps its corners where they were). The Properties docker's
  Corners section and the mixed marks read the corners the same way
  (#1670, `document::tests::corners_are_read_where_they_are_seen`,
  `ui::propbar::tests::corner_fields_edit_the_corner_seen_at_their_position`).
- Relative Corner Scaling (off by default for new rectangles): off, the
  corners keep their size on the page when the rectangle is resized, and
  stay circular when it is stretched in one direction only. On, they
  scale with the rectangle; a stretch in one direction stretches them
  too, so round corners become elliptical.
  Turning it on or off keeps the corners as they look. The size is
  measured against the rectangle's own scaling: scaling a group evenly
  grows its rectangles' corners (they still keep that size afterwards),
  stretching a group stretches them, and ungrouping keeps them as they
  look.
  Drawings saved before this default changed keep what they store: a
  rectangle without the setting in the file scales its corners as it
  did. Rounded rectangles opened from `.cdr` files keep their corner
  size, at the size the corners measure on the page (a corner larger
  than half the shorter side stays so); one the file stretches gets
  round corners of its shorter scale, the stretch going into its size
  unless its outline scales with the object (a skewed one keeps
  corners that follow it)
  (`document::tests::keep_corner_size_leaves_the_outline_and_takes_the_stretch`)
  (`parse::tests::rounded_rectangles_keep_their_corner_size` in the
  .cdr reader). Preferences saved by older versions, which could not store the
  choice, start with it off once.
  Checked by `pick_drag_tests::rounded_corners_keep_their_size_when_relative_scaling_is_off`,
  `pick_drag_tests::new_rounded_rectangles_keep_circular_corners_when_stretched`
  and `settings::tests::rectangle_corners_keep_their_size_by_default_and_after_upgrading`.
- Saving: corners that keep their size are written as one plain radius
  where the format has one. `.cdr` versions with rectangle objects
  (before 15) get a stretched rectangle with one round radius as a
  rectangle of its page size and that radius (the stretch moves out of
  its matrix), unless the stretch also widens an outline that scales
  with the object or the axes are skewed; then, and in later versions,
  the outline goes out as a curve. SVG and PDF write every rectangle as
  the path it draws, so the corners there are exactly as shown
  (`write::tests::corners_one_radius_cannot_hold_are_written_as_curves_and_stretched_fixed_ones_as_rectangles`,
  `tests::corners_that_keep_their_size_survive_native_svg_and_pdf`).
  A single radius is written at the size drawn (at most half the
  shorter side), and a corner past half the shorter side goes out as
  the curve drawn (`write::tests::corners_past_half_the_shorter_side_are_written_as_drawn`,
  `tests::a_corner_taking_a_whole_side_survives_native_svg_and_pdf`).
- The sizes on the bar are page sizes in the ruler unit; with relative
  scaling on a stretched rectangle shows the smaller of its two scales.
- Double-clicking the tool button draws a page frame: a rectangle the
  size of the page, selected. With Shift held, the rectangle frames the
  selection as drawn (its effects included); with nothing selected, the
  page.

## Ellipse (F7)

- Property bar: ellipse, pie or arc, with start and end angles (degrees,
  counter-clockwise from +x) and a direction toggle. Default: ellipse.
  Pie or Arc first picked starts at 0 to 270 degrees, on the property bar
  as in the Options dialog. The bar started from 90 to 90 degrees, a
  whole turn: a "pie" that looked like an ellipse with a line from its
  centre to the top, kept for every new ellipse after. A saved tool
  setting whose angles meet is read back as a plain ellipse
  (`app::tests::a_whole_turn_pie_saved_by_the_old_bar_draws_an_ellipse`).

## Polygon (Y) and Star

- Points: default 5, range 3..500 (property bar).
- The first vertex is at the top centre. The dragged box is the
  polygon's own bounding box: the model keeps the ellipse the outer
  vertices lie on, so the ellipse is computed from the unit polygon's
  vertex bounds `(x0, x1, y0, y1)` as `rx = w / (x1 - x0)`,
  `ry = h / (y1 - y0)`, centre `(left - x0 rx, bottom - y0 ry)`.
- With Ctrl the drag is held to a square and the polygon (or star) is
  regular: its vertices lie on the circle of that square, every side
  and angle equal, instead of being stretched to fill the square
  (#1929, `pick_drag_tests::ctrl_draws_a_regular_polygon`).
- Star sharpness: default 0.5 (0 = polygon, 1 = thinnest spikes). The
  inner points sit at `1 - sharpness` of the vertices' radius. A
  negative sharpness, down to -0.99, makes an inverted star (#1331): the
  inner points lie past the centre, on the far side, at
  `-(1 - |sharpness|)` of the radius, so -0.99 is next to the centre
  and -0.01 near the rim opposite; the outline crosses itself. The
  property bar, the Coordinates docker and Options show it from -99 to
  99 percent; 0 typed keeps the star's side at 1 or -1. `.tdraw` files
  keep the negative value (older files read as before); SVG, PDF, EPS
  and `.cdr` get the outline as drawn (`.cdr` writes every polygon as a
  curve). A `.cdr` polygon record is read with its inner point's
  distance only, so it is never an inverted star: no file holding one
  was available to learn how the format stores it.
- The Star tool's property bar starts with the Star and Complex Star
  buttons, which pick what it draws. A complex star (default 9 points,
  sharpness 2) joins each vertex to the one `sharpness + 1` further on,
  so its sides cross; points 5..500, sharpness 1..`(points - 1) / 2 - 1`
  (at least 1). When that step shares a factor with the points the star
  is several closed subpaths (6 points: two triangles). Filled even-odd,
  its middle stays empty. Its vertices are a polygon's, so the dragged
  box fits them the same way; the Shape tool shows the vertices only and
  dragging one grows or shrinks the star about its centre. `.cdr` files
  get it as a curve.
- Polygons are symmetric: moving one node with the Shape tool moves its
  mirrored counterparts (see `shape-tool.md`).

## Spiral (A)

- Revolutions: default 4 (1..100). Symmetric (constant pitch) or
  logarithmic (radius grows as e^(4t), normalised to the box) spirals.
- Sampled at 48 points per revolution and smoothed; an open curve.

## Graph paper (D)

- Rows x columns: default 4 x 3. The result is a group of rows x columns
  rectangles that fill the box exactly.
- Radial (Cells or Radial on the bar, also on the tool's options page,
  kept in the preferences): Rings (5) ellipses about the box's middle,
  ring `i` of `n` (outermost first) `(n - i) / n` of the box across,
  then Spokes (8, 0 to 360) open lines from the middle to the outer
  ring at `90° + 360° k / spokes`; one group, filled and outlined as the
  cells are (the spokes unfilled), one undo step "Graph Paper". The
  preview shows the rings and spokes (`tools2::radial_grid_path`,
  `tools2::tests::graph_paper_draws_a_radial_grid`).

## Common shapes

71 shapes in five sets (`common_shapes.rs`), picked on the property bar:
the button shows the shape and its name; a click opens the sets as tabs
(the current shape's set first) over a grid of outlines, nine to a row,
each named in its tooltip. Picking one closes the grid. The first tab,
All, shows every set at once, each under its name, scrolling past 70 %
of the window's height (#1650,
`toolbar::tests::the_common_shapes_picker_has_an_all_tab`).

- Basic shapes (18): parallelogram, trapezoid, diamond, triangle, right
  triangle, hexagon, octagon, plus, heart, lightning, crescent, ring,
  cylinder, cube, frame, teardrop, smiley face, no symbol.
- Arrows (12): right, left, up, down, left and right, up and down,
  four-way, chevron, pentagon arrow, notched, striped, bent.
- Flowchart (27): process, alternate process, decision, data, predefined
  process, internal storage, document, multiple documents, terminator,
  preparation, manual input, manual operation, connector, off-page
  connector, card, punched tape, summing junction, or, collate, sort,
  extract, merge, stored data, delay, magnetic disk, direct access
  storage, display.
- Banners (8): ribbon, folded ribbon, wave, double wave, scroll,
  explosion, burst, pennant.
- Callouts (6): rectangle, rounded, oval and cloud callouts, line and
  bent line callouts.

Each shape is drawn into the box dragged as one curve object with the
default fill and outline. Parts meant to look separate (a cylinder's top
and body, a cube's faces, a ribbon's tails, the pages behind a multiple
document) are separate subpaths edge to edge, never overlapping, and
holes (ring, frame, the smiley's eyes and mouth, the no symbol's
openings) run against the outline, so they stay open under any fill
rule. Inner details (the bars of a predefined process, the cross of a
summing junction, a callout's leader) are straight two-point lines that
are stroked, not filled. Rounded corners (alternate process, the
terminator's ends, a rounded callout) keep their size in long boxes.

### Adjusting a shape (#4045)

A common shape stays what it is after it is drawn: the object keeps the
shape, the box it was drawn into and its adjustments
(`Shape::common_shape`, saved in `.tdraw` files). While it is selected,
the Common Shapes tool (right after drawing) and the Shape tool show its
adjustment handles, diamonds filled in the node colour with a white edge,
instead of the curve's nodes. Dragging a handle draws the shape again
from its box with the new setting, live while dragging, and the whole
drag is one undo step ("Edit Shape"). Each handle moves in a straight
line with the values it sets, so it stays exactly under the pointer
until a value reaches the end of its range. A handle that sets two
values (an arrow's, a callout's tail) moves freely in both directions.
Snapping applies to the dragged handle. Fills and gradients stay in
place, as for any new outline.

| Shapes | Handle | Sets (default; range) |
| --- | --- | --- |
| Parallelogram, data | top left corner | slant of the top edge (0.25, data 0.2; 0 to 0.95 of the width) |
| Trapezoid, hexagon, preparation | top left corner | how far the ends come in (0.25; 0 to 0.5) |
| Manual operation | bottom left corner | same (0.2; 0 to 0.5) |
| Triangle, extract, merge | apex | where the apex stands along its edge (0.5; 0 to 1) |
| Octagon | top left corner | size of the cut corners (0.293; 0 to 0.5) |
| Plus | top left of the upper arm | arm thickness (0.3; 0.02 to 0.98) |
| Crescent | inner curve's middle | width of the inner curve (0.6; 0 to 0.95 of the outer) |
| Ring, no symbol | inner edge, right | inner radius (0.3 and 0.36; up to 0.48 of the box); the no symbol's bar keeps its width |
| Cylinder, magnetic disk, direct access storage | front edge of the end | height of the end ellipse (0.12; 0.01 to 0.5) |
| Cube | back corner of the top face | depth of the top and side faces (0.25; 0.01 to 0.9) |
| Frame | inner edge, left | border thickness, the same all round (0.15 of the shorter side; up to 0.49) |
| Smiley face | bottom of the mouth | smile, 1 a smile, 0 straight, -1 a frown |
| Right, left, up, down, striped arrows | where head meets shaft | head length (0.4; 0 to 1) and shaft thickness (0.4; 0.02 to 1) together |
| Left and right, up and down arrows | same | head length (0.3; up to 0.5) and shaft thickness |
| Four-way arrow | right head's base; top shaft's corner | head length and head width; shaft thickness |
| Chevron, pentagon arrow | the notch; the point's base | depth of the point (0.3) |
| Notched arrow | as an arrow, plus the notch | head, shaft and notch depth (0.15) |
| Bent arrow | top of the tail; head's corner | shaft thickness; head length and width |
| Alternate process | end of the top left corner | corner radius (0.15 of the shorter side; up to 0.5) |
| Predefined process | left bar | bars' inset (0.1) |
| Internal storage | where the lines cross | both lines' places |
| Document, multiple documents | start of the wavy edge | wave height (0.12; 0 to 0.4) |
| Manual input, off-page connector | left edge's top | height of the slope's low end, of the point's sides |
| Card | cut corner | size of the cut (0.2) |
| Punched tape, wave, double wave | start of the bottom edge | wave height |
| Stored data, display | top left | depth of the curved side, inset of the point |
| Ribbon | left notch | notch depth (0.15) |
| Burst | an inner point | depth of the points (0.82 of the outer radius) |
| Pennant | bottom of the flag | height of the flag (0.5) |
| Callouts (all six) | tail's tip | where the tail points, anywhere around the box (the rectangle callouts' tail root follows along the bottom edge, the oval callout opens toward the tip, the cloud callout's puffs line up toward it) |

The other shapes (diamond, decision, right triangle, heart, lightning,
teardrop, process, terminator, connector, summing junction, or, collate,
sort, delay, folded ribbon, scroll, explosion) have no handles; they show
neither handles nor nodes while they are common shapes. Resizing,
moving, turning or skewing the object keeps it a common shape. Any other
change to its outline (moving a node elsewhere, an envelope or warp
made part of it, a knife cut, welding) and Convert To Curves leave an
ordinary curve, which shows its nodes again; so does an object whose
outline is no longer the one its settings draw, and a shape from a newer
version this one does not know
(`common_shapes::tests::every_handle_follows_a_drag`,
`common_shapes::tests::a_curve_is_live_while_it_is_its_shape`,
`pick_drag_tests::a_common_shape_is_adjusted_by_its_handles`).

### My shapes

The sixth tab of the grid, My shapes (`my_shapes.rs`, #481, #782), holds
the user's own shapes, drawn like the built-in ones:

- Add Selection as Common Shape (the button above the tab's grid, Object >
  Create and the right-click menu) keeps the outline of the selected
  objects as one shape at the end of the set and picks it: every object's
  outline on the page (groups, text and symbol instances by their
  outlines; bitmaps are left out) together as one curve, scaled into the
  unit square, with its proportions (width over height) kept beside it.
  It is named after the one object (its name or its kind) or "My shape
  n". Nothing with an area (nothing selected, a level or upright line)
  adds nothing and says so on the status bar. A small dialog then asks
  for a name with that one shown and selected (#2360): typing replaces
  it, OK or Enter keeps it, Skip or Escape keeps the name shown. Rename
  on a shape's right-click menu asks again; a blank name keeps the old.
- A click on a shape picks it; the property bar shows it in its own
  proportions with its name. Picking a built-in shape leaves My shapes.
  The picker stays open after a pick, to try another shape; a click
  outside it or on its button closes it (#2361).
- A drag draws the picked shape into the box like a built-in one, one
  curve with the default fill and outline (filled when the outline it
  came from had a closed part, otherwise an open curve). Ctrl keeps the
  shape's proportions instead of making a square: the larger side of the
  drag sets the size, the start corner stays. Shift draws from the
  centre.
- Remove (beside the button, and the right-click menu of a shape) takes
  the picked shape out; the built-in sets cannot be changed. The tool
  then draws the built-in shape picked before.
- The set is kept in the preferences folder as `my_shapes.json` (in the
  browser in the page's local storage), written at once after every
  change, so it is there after a restart: `{"kind": "tracedraw-my-shapes",
  "version": 1, "shapes": [{"name", "path", "aspect", "closed"}]}`, each
  outline an SVG path in the unit square. A missing, damaged or foreign
  file gives an empty set; outlines that do not read are left out. At
  most 200 shapes of at most 5000 path elements each.

## Action Lines

The bar (#1617), all kept while the program runs:

- Lines: default 12 (2..500), parallel (default) or radial.
- Length (0..100 %, 70): how much shorter than the longest a line may
  come out; 0 makes them all as long as the box (parallel) or reaching
  its edge (radial).
- Middle (radial, 0..90 %, 30): the empty middle as a share of the
  radius; lines start there or further out.
- Width (mm, 0): 0 draws thin open lines with the default outline, as
  before; more draws each line as a filled stroke, in the default
  outline's colour, no outline. Then:
  - Width var. (0..100 %, 50): how much thinner a stroke may come out.
  - Ends: Square, Pointed (default: a point at the start, the middle for
    radial lines, full width at the far end) or Both pointed (widest in
    the middle).
- Spacing (0..100 %, 0): how far a line may stray from even spacing, as
  a share of a step.
- Angle (degrees, 0): parallel lines turn about the box's middle; radial
  lines' first direction.
- Oval (radial): lines reach the box's ellipse instead of the circle in
  it.
- From (parallel): Right (lines end at the right edge, as before), Left
  or Centre; the thin end points away from that side.
- New Random: another seed, so other random lengths, widths and spacing.

Lengths, widths and spacing are pseudo-random from the seed, so the
same settings and box always give the same object, and the live preview
is that object. One curve object.

Checks: `action_lines::tests` (same settings same lines, a seed others;
even lengths and the three sides; the empty middle and the oval; the
three ends as filled strokes, a square stroke twice the area of a
pointed one; the angle and stray spacing).

## Speech Balloon

- Drag a box: the balloon's body fills it and its tail points down and
  to the left, its tip half the box's height below the box, a quarter of
  the way across. Body and tail are welded into one closed curve
  (`balloons.rs`), filled with the default fill (white when that is
  none) and the default outline (hairline black when none).
- Styles: Speech (ellipse), Thought (a cloud of 11 bumps with three
  bubbles toward the tip, 16 %, 11 % and 7 % of the smaller side, at 30 %,
  62 % and 92 % of the way from the cloud's rim, instead of a tail),
  Whisper (Speech with a 4-2 dashed outline), Shout (a 16-point burst,
  points 0.83 to 1.0 and notches 0.72 to 0.8 of the half box) and Caption
  (a box rounded by 20 % of its smaller side).
- Tail width: 5 to 100 % of the body's smaller side (30), measured across
  the body's middle where the tail starts. Tail bend: -100 to 100 (0); the
  sides bow sideways by `bend/100 * 0.35 *` the tail's length. A tip
  inside the body gives no tail.
- The balloon is kept in the object's data (`balloon`: style, body box,
  tip, width, bend, in the object's own space). With the tool, the
  selected balloons show a diamond at the tail's tip; a drag from it
  (within 8 pixels) points the tail elsewhere, one undo step. The bar
  shows the selected balloon's style, width and bend, and a change goes
  to the selected balloons (one undo step) and to the next ones.
- Add Text on the bar (discussion #3522), enabled with a balloon
  selected: a paragraph text whose frame is the middle of the body (68 %
  of the body box for Speech and Whisper, 84 % for Caption, 58 % for
  Thought, 50 % for Shout), centred whatever the bar's alignment for new
  texts, the caret in it to type.

## Checks

- `balloons::tests::every_style_makes_one_closed_curve_reaching_the_tip`
- `balloons::tests::a_balloon_is_drawn_its_tip_dragged_and_its_style_changed`
- `balloons::tests::add_text_types_inside_the_balloon`

- Given a 3 mm black default outline, while a rectangle or an ellipse is
  dragged out, the outline is painted at its width and the edge line is
  painted after it, on top (#621,
  `pick_drag_tests::the_edge_of_the_shape_being_drawn_shows_over_a_wide_outline`).
- `pick_drag_tests::overlays_are_drawn_on_a_white_line`
- `common_shapes::tests::every_shape_fills_its_box`: every shape stays in
  its box and fills at least 80 % of it both ways, and is named.
- `common_shapes::tests::rounded_corners_keep_their_size_in_long_boxes`:
  a terminator in a 100 x 20 mm box has ends of radius 10 mm; ring and
  frame areas are the outer less the hole.
- `my_shapes::tests::the_selection_becomes_a_shape_that_is_kept_and_read_back`:
  a 40 x 20 mm triangle becomes a shape of aspect 2 that is written to
  `my_shapes.json`, read back the same and removed.
- `my_shapes::tests::a_shape_of_my_own_is_drawn_by_dragging_and_ctrl_keeps_its_proportions`:
  a drag from (100,100) to (160,130) draws the triangle in that box (900
  mm2); with Ctrl a drag to (10,30) gives a 60 x 30 mm box.
- `my_shapes::tests::groups_and_several_objects_make_one_shape`,
  `my_shapes::tests::damaged_and_foreign_files_give_an_empty_set`.

- Given a drag from (10,10) to (30,20) with Ctrl, then the rectangle is
  20 x 20 mm.
- Given the same drag with Shift, then the rectangle spans (-10,0) to
  (30,20) (centre at the start point).
- Given a 5-point star with sharpness 0.5 in a 40 mm square, then its
  bounds are 40 x 40 mm and it has 10 nodes.
- Given a 5-point star with sharpness -0.7 in a 20 mm circle, then its
  vertices are a sharpness 0.7 star's and each inner point is that
  star's mirrored through the centre
  (`geometry::tests::negative_sharpness_inverts_the_star`; the property
  bar takes -40: `ui::propbar::tests::the_sharpness_box_takes_a_negative_value`;
  files: `tracedraw_io::tests::inverted_stars_round_trip_in_native_files_and_export_their_outline`,
  `write::tests::inverted_stars_are_written_as_the_curves_they_draw`).
- `geometry::tests::complex_stars_cross_their_sides` (pentagram winding
  2 in the middle, two triangles for 6 points, sharpness limits) and
  `ui::propbar::tests::the_star_tool_draws_complex_stars_that_fill_the_drag`.
- Given a 40 x 20 mm rectangle with 5 mm corners, then it loses
  (4 - pi) 25 mm2 with round corners, 25 pi with scalloped ones and 50
  with chamfered ones (`document::tests::corner_styles_cut_the_expected_area`).
- Given fixed 4 mm corners on a rectangle scaled 3 x 2, then the corners
  still cut (4 - pi) 16 mm2
  (`document::tests::fixed_corners_keep_their_size_when_scaled`).
- Given a new rectangle with 5 mm corners stretched to twice its width
  with a side handle, then its corners are still 5 mm circles; with
  relative scaling turned on and the height doubled, they grow.
- `ui::propbar::tests::corner_edits_change_the_selection_in_one_step_or_the_defaults`,
  `ui::propbar::tests::the_rectangle_tool_draws_with_the_default_corners`.

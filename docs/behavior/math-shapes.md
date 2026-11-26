# Math Shapes

Panel `DockerTab::MathShapes` (Object > Create > Math Shapes, Window >
Panels). Code: `apps/tracedraw/src/math_expr.rs` (formula parser and
evaluator), `apps/tracedraw/src/math_shapes.rs` (geometry, pure functions
of the settings) and `ui/math_shapes_docker.rs`. Tests:
`math_shapes_tests.rs` and the tests in `math_expr.rs`.

## Formulas

- Operators `+ - * / ^` (right associative), unary minus, implicit
  multiplication (`2x`, `3sin(x)`, `(x+1)(x-1)`, `ab`), `|x|`, `x²`, `x³`,
  `√x`, `5!`, comparisons `< > <= >= == !=` (1 or 0).
- Decimal point or comma (`2,5`); inside a call's brackets a comma separates
  arguments.
- Variables x, y, t (also `theta`, `θ`); parameters a, b, c, d, k; constants
  `pi`/`π`, `e`, `tau`, `phi`/`φ`.
- Functions: sin cos tan cot sec csc asin acos atan sinh cosh tanh asinh
  acosh atanh sqrt cbrt abs exp ln log (base 10, or `log(b, x)`) log2 floor
  ceil round sign frac fact, `min`/`max` with any number of values, mod pow
  hypot `root(x, n)` (odd roots of negatives are negative) atan2, and
  `if(cond, then, else)`.
- Letters run together are split into known names, longest first (`sinx`
  is `sin(x)`, `ab^2` is `a*b^2`).
- `lhs = rhs` is read as `lhs - (rhs)`; a leading `y =`, `f(x) =` or
  `r =` is dropped.
- At most 2,000 characters and 64 nested brackets. A formula that does not
  read gives an error (empty, unknown name, something missing or misplaced
  at a position, brackets, wrong number of values, too long) shown in red
  under its field; other curves still draw.
- With Angles in degrees on, trig functions take and give degrees and the
  θ range is in degrees.

## Graph

- Model space: mm, y up. The plane is x range × y range scaled by Unit
  (mm per unit, x and y; Same scale keeps them equal); Size edits the units.
- Curves (at most 12): function y = f(x) (optional domain), parametric
  x(t), y(t), polar r(θ), equation in x and y (marching squares on a grid,
  cells across a pole rejected), points (named or not, formulas allowed,
  joined, guides to the axes, labels with coordinates).
- Sampling is adaptive: a function is split further where it bends; where
  it jumps past the plane between two samples (an asymptote) or is not
  defined, the curve breaks. Everything is clipped to the plane
  (Liang-Barsky) unless Draw the plane is off (ornaments).
- Marks: zeros, the y intercept and turning points of functions, with
  their coordinates.
- Plane: axes crossing at the origin or at the edge when 0 is out of range,
  arrows, axis names, ticks and numbers per step (x as multiples of π), a
  single 0 at the origin, grid, minor grid with divisions, border, each
  with its own width and colour; numbers and labels in the chosen font.
- At most 200,000 points per curve (sampling stops there) and 1,000 grid
  lines or ticks per axis (the step grows when there would be more).
- Smooth curves: each sampled curve goes through `fit::fit_path` (0.01 mm).

## Figures

- Circle by radius, diameter, circumference or area; triangle by SSS, SAS,
  ASA, right (two legs), isosceles (base and leg), equilateral (measures
  that make no triangle give the impossible message and draw nothing);
  regular polygon by side or circumradius with a star step; ellipse by
  semi-axes; arc, sector or segment by radius, start and sweep.
- Aids drawn in the aid colour: vertex names, lengths, angle arcs with
  degrees (a square at right angles), centre, radius, apothem, diameter,
  ellipse axes and foci, heights, medians, bisectors, incircle and
  circumcircle with centres, diagonals. Lengths in mm, cm or bare numbers
  with the chosen decimals.
- The Measures list gives every computed value (sides, angles, perimeter,
  area, circumradius, apothem, interior angle, focal distance,
  eccentricity, arc length, chord).

## Output

- Create: one group (`ShapeKind::Group`) centred on the window's middle,
  holding one group per part (grid, axes, numbers, each curve, figure, aids,
  labels) of `ShapeKind::Path` curves and `ShapeKind::Text` labels, added
  with one `Command::AddShape` and selected: one undo step.
- The group keeps its settings as JSON under `tracedraw.math_shapes`.
  Selecting it loads them into the panel; Update deletes it and adds the new
  group in the same batch, centred where it was.
- Settings are sanitized (values clamped, non-finite replaced) before
  drawing and when read from a group or preferences.
- 23 presets (empty plane, parabola, lines, sine, tangent, exponential and
  log, bell curve, damped wave, square wave, piecewise, conics, lemniscate,
  rose, Archimedean and golden spirals, cardioid, Lissajous, heart,
  butterfly, superformula, spirograph, astroid, points).

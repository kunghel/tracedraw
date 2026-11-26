# Math Shapes

The [[docker.math_shapes]] panel draws from mathematics: a Cartesian plane
with axes, grid and numbers and the curves of your formulas on it, or a
circle, triangle, polygon, ellipse or arc made from its measures, with the
measures written beside it. What it makes are ordinary curves and text you
can edit, recolour or export. Open it from
[[menu.object > menu.object.create > menu.object.math_shapes]] or from
[[menu.window > menu.window.dockers]].

The two buttons at the top switch between [[math.mode_graph]] and
[[math.mode_figure]]. The preview at the top of the panel shows exactly
what [[math.create]] draws.

## Graphs {#graph}

**1.** Pick a starting point in [[math.preset]], such as a parabola, sine
waves, a rose, a spiral or the conics, or start from the empty plane.

**2.** Each curve has its own card. Choose its [[math.kind]]:

- [[math.kind_function]]: type the formula, such as `2x^2 - 3x + 1`,
  `sin(x)` or `sqrt(x)`. A gap is left where the function is not defined,
  and the curve breaks at asymptotes instead of joining them.
- [[math.kind_parametric]]: a formula for x and one for y, both in `t`.
- [[math.kind_polar]]: the distance from the origin as a formula of the
  angle, such as `3cos(4t)` for a rose.
- [[math.kind_implicit]]: any equation, such as `x^2 + y^2 = 9`.
- [[math.kind_points]]: points such as `A(1, 2); B(-3, 4)`, joined or not.

A formula with a mistake turns red and says what is wrong; the other curves
still draw. [[math.help]] at the bottom of the panel lists the operators,
functions and constants.

**3.** Give each curve a colour, line width, dash, a fill under it or
inside it, and [[math.curve_label]] to write the formula beside it.
[[math.marks]] marks where a function crosses the axes and its highest and
lowest points. [[math.add_curve]] puts more curves on the same plane.

**4.** The letters a, b, c, d and k in a formula are
[[math.parameters]]: drag their sliders and watch the curve change.

**5.** Under [[math.plane]], set the x and y ranges, the size of one unit
in millimetres (or the size of the whole plane), the axes with arrows and
names, the ticks and numbers (numbers as multiples of π for waves), the
grid and a finer [[math.minor]] like graph paper, each with its own colour
and width. Turn [[math.show_plane]] off to draw the curves alone, as
ornaments.

**6.** Click [[math.create]]. The graph comes in as one group in the middle
of the window: grid, axes, numbers and each curve in groups of their own.
[[math.smooth]] makes each curve smooth Bézier curves with few nodes.

## Figures {#figure}

Pick a [[math.figure]] and say what is [[math.given]]:

- A circle from its radius, diameter, circumference or area.
- A triangle from three sides, two sides and the angle between them, two
  angles and the side between them, a right triangle, an isosceles or an
  equilateral one. Measures that make no triangle are said so in red.
- A regular polygon from its side or its circumradius, or a star.
- An ellipse from its semi-axes, an arc, sector or segment.

Under [[math.show]], choose what to draw and write: names of the
vertices, lengths, angles with their arcs, the centre, radius, apothem,
heights, medians, bisectors, the incircle and circumcircle, diagonals and
foci. The list under [[math.facts]] gives every measure worked out: sides,
angles, perimeter, area and more.

## Changing it later {#update}

Select a group made by the panel: its settings come back into the panel.
Change them and click [[math.update]]: the group is drawn again in the
same place, in one undo step.

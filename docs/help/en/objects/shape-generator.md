# Shape Generator

The [[docker.shape_generator]] panel draws patterns, line art, isometric
blocks and cone templates from a few settings, as ordinary curves you can
edit, recolor, cut or export. Open it from
[[menu.object > menu.object.create > menu.object.shape_generator]] or from
[[menu.window > menu.window.dockers]].

Pick a [[shapegen.kind]], change its settings and watch the preview under
them: it shows exactly what [[shapegen.create]] draws. The number under the
preview counts the curves.

## Generators {#generators}

- [[shapegen.kind_dot_grid]]: a grid of circles, squares or diamonds whose
  size changes from the middle to the [[shapegen.falloff]] edge
  ([[shapegen.scale_ratio]]), with a [[shapegen.warp]] (a lens that pushes
  the cells out or pulls them in, or ripples) and an optional
  [[shapegen.elliptical]].
- [[shapegen.kind_truchet]]: square tiles of quarter arcs (with one or more
  [[shapegen.bands]]), filled quarter discs or diagonals, each turned at
  random. The edges of every tile meet their neighbours, so the result
  repeats with no seam.
- [[shapegen.kind_tessellation]]: triangles, hexagons, scales or chevrons
  with a [[shapegen.gap]] and a [[shapegen.rotation]].
- [[shapegen.kind_line_waves]]: lines bent by a smooth noise inside a bump:
  [[shapegen.wave_width]], [[shapegen.amplitude]], [[shapegen.noise_x]],
  [[shapegen.noise_y]], [[shapegen.spread]], [[shapegen.sharpness]],
  [[shapegen.fade]] and [[shapegen.softness]].
- [[shapegen.kind_rings]]: closed rings between [[shapegen.min_radius]] and
  [[shapegen.max_radius]], each wobbled by a noise that goes once round.
- [[shapegen.kind_field_lines]]: evenly spaced lines that flow round a few
  [[shapegen.poles]], with a [[shapegen.flow]] across the area and an
  optional [[shapegen.swirl]].
- [[shapegen.kind_iso_blocks]]: stacks of cubes on an isometric grid, their
  heights flat, random, a pyramid or a wave, with three shaded faces.
- [[shapegen.kind_cone]]: the flat template of a cone, a tapered cylinder or
  a cylinder, see [Cone templates](#cone).
- [[shapegen.kind_voronoi]]: cells round scattered sites, see
  [Voronoi cells](#voronoi).
- [[shapegen.kind_stipple]] and [[shapegen.kind_halftone]]: dots and marks
  sized by a picture's tones, see [Pictures as dots](#tones).
- [[shapegen.kind_spirograph]]: a wheel rolling inside or outside a ring
  ([[shapegen.spiro_fixed]], [[shapegen.spiro_rolling]],
  [[shapegen.spiro_pen]]), Lissajous figures, roses and mandalas.
- [[shapegen.kind_guilloche]]: rings bent by two waves, each turned by the
  [[shapegen.guil_twist]] so the lines weave, as on banknotes.
- [[shapegen.kind_fractal]]: a branching tree, the Koch snowflake, the
  Sierpinski arrowhead, the dragon curve, a plant or your own L-system,
  see [L-systems](#lsystems).
- [[shapegen.kind_tiling]]: Penrose rhombs, which never repeat, and star
  patterns on squares or hexagons shaped by the [[shapegen.tile_angle]].

Generators that make random choices have a [[shapegen.seed]]: the same seed
always gives the same result, and [[shapegen.new_seed]] picks another one.

## Creating and changing {#create}

- [[shapegen.create]] draws the objects as one group in the middle of the
  window. One undo removes it.
- The group keeps its settings. Select it and click [[shapegen.update]] to
  make it again with the panel's settings in the same place, or
  [[shapegen.load]] to bring its settings back into the panel.
- [[shapegen.combine]] makes one curve per color instead of a group of
  separate curves, lighter for large patterns.
- [[shapegen.make_pattern]] creates the objects and turns them into a
  vector pattern fill.

The settings are kept between sessions. One generation makes at most
40,000 curves; past that it is cut and the panel says so.

## Cone templates {#cone}

Type the object's [[shapegen.cone_top]], [[shapegen.cone_bottom]] and
[[shapegen.cone_height]]. The template is a ring segment (a sector when one
diameter is 0, a rectangle when both are the same) at real size, with a
[[shapegen.cone_tab]] on one side and an optional [[shapegen.cone_bleed]]
outline. [[shapegen.cone_pieces]] cuts it into several pieces, each with its
own tab. The panel shows the slant height, the radii and the angle.

To put a design on the object, draw it flat as a strip and select it, then
click [[shapegen.wrap]]: the template is drawn and the strip is bent onto
it, its width going round the object [[shapegen.cone_repeat]] times and its
top along the top edge, so it rolls up straight on the real object.

## Voronoi cells {#voronoi}

Each cell holds the part of the area nearer to its site than to any other.
Place the sites with [[shapegen.points]] and [[shapegen.spacing]] (grid
sites move by the [[shapegen.jitter]]), then even the cells out with
[[shapegen.relax]] steps or the [[shapegen.relax_once]] button. A
[[shapegen.gradient]] makes the cells smaller toward the middle or along a
direction. For cutting, [[shapegen.inset]] leaves a gap between cells and
[[shapegen.corner_radius]] rounds them. [[shapegen.show_cells]],
[[shapegen.show_sites]] and [[shapegen.show_delaunay]] choose what is drawn.

Edit the sites right in the preview: drag one to move it and watch its
neighbours follow, double-click to add one, right-click one to delete it.
Sites placed by hand are kept with the settings; [[shapegen.reset_points]]
goes back to generated ones.

To fill a shape, select it and click [[shapegen.use_selection]]: the cells
fill its outline and its holes stay empty, and Create puts the result over
it.

## Pictures as dots {#tones}

Select a picture or any objects and click [[shapegen.use_selection]]: the
selection is drawn to a picture whose darkness sizes the marks, and marks
stay inside its outline. Without a selection a round ramp stands in.

- [[shapegen.kind_stipple]] spreads [[shapegen.dots]] by darkness and
  spaces them evenly over [[shapegen.st_iter]] steps; with
  [[shapegen.st_by_tone]] dark places get bigger dots.
- [[shapegen.kind_halftone]] puts a [[shapegen.ht_shape]] in every
  [[shapegen.ht_cell]] of a screen turned by [[shapegen.ht_angle]], sized
  between [[shapegen.ht_min]] and [[shapegen.ht_max]]; Lines draws bands
  that thicken where it is dark. [[shapegen.ht_cmyk]] makes four screens,
  one per ink, each in its own group.

[[shapegen.invert]] puts the marks in the light places instead.

## L-systems {#lsystems}

An L-system starts from the [[shapegen.frac_axiom]] and replaces every
symbol by its rule, once per level. The result is drawn by a turtle: F, G,
A and B draw a step, f moves without drawing, + and - turn by the angle,
[ and ] keep and come back to a place (for branches), | turns round. Write
the [[shapegen.frac_rules]] as `F=F+F--F+F`, one per line or separated by
semicolons. [[shapegen.frac_edit]] copies a built-in fractal into a
custom one to change it. Very deep levels stop growing at 400,000
symbols.

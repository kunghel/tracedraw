# Shape Generator

Panel `DockerTab::ShapeGenerator` (Object > Create > Shape Generator, Window >
Panels). Code: `apps/tracedraw/src/shape_gen.rs` (geometry, pure functions of
the settings) and `ui/shape_gen_docker.rs`. Issues #3412 and #3413.

## Common rules

- Model space: mm, y up. Each generator draws into the Width x Height box
  with its bottom left at the origin (iso blocks and cone templates take
  their own size); Create centres the result on the window's middle.
- Output: one group (`ShapeKind::Group`) of `ShapeKind::Path` curves,
  added with one `Command::AddShape` (one undo step) and selected. The
  group is named after the generator and carries its settings as JSON in
  its object data under `tracedraw.shape_generator`; Update deletes it and
  adds the new group in the same batch, centred where it was.
- Paint: lines take the default outline (or Line width), solids the default
  outline's colour (black when none), cube faces greys E8, 9C and 4E with a
  hairline black outline.
- Combine merges curves of the same paint into one path (never for iso
  blocks, whose faces must stay in back-to-front order).
- At most `MAX_ITEMS` (40,000) curves; past it the result is cut and the
  status line and panel say so.
- Randomness: SplitMix64 seeded by Seed; noise is a 3D gradient noise with
  a quintic fade, two octaves (`fbm`). Same settings, same curves.
- Settings are sanitized (every value clamped, non-finite values replaced
  by defaults) before drawing and when read from preferences or a group.

## Generators

- Dot grid: pitch = longer side / Cells. Distance d from the middle,
  normalised by Falloff x half diagonal (elliptical: by the box's half
  sizes); size = pitch x Cell size x lerp(1, Edge size, smoothstep(d)).
  Lens warp inside the falloff: d' = d^(1 - 0.75 k), k = Strength / 100.
  Ripple: r' = r + k x 0.8 pitch x sin(4 x 2 pi r / falloff).
- Truchet: tiles of side Tile, a random flip each; arcs round two opposite
  corners. Band radii t/2 - 0.2t + 0.4t k/(n-1): symmetric about t/2, so
  every edge meets its neighbour whichever way it is turned, and below
  t / sqrt 2 so opposite corners' arcs never cross.
- Tessellation: tiles laid over the box's circumscribed square, turned by
  Rotation about the middle, kept when their middle is inside; Gap shrinks
  each tile towards its middle by half the gap (inradius ratio).
- Line waves: line i at y = t h; bump e(x) = exp(-(|x - w/2| / (Wave width
  w / 2))^Sharpness); y += (t - 1/2) Spread e + Amplitude e (1 - Fade |2t -
  1|) fbm(u Noise X, i Noise Y). Samples 240 to 40 by Softness; curves
  through the samples as Catmull-Rom cubics.
- Rings: radius lerp(min, max, k/(n-1)) + Amplitude fbm(cos a f, sin a f,
  k Drift), f = Detail / 2; closed Catmull-Rom curves.
- Field lines: Poles placed in the middle 70 % with charges of either sign;
  field = flow + sum q (p - c) / |p - c|^2, turned by Swirl. Streamlines
  traced both ways with a midpoint step of spacing / 4 from a shuffled
  jittered grid of seeds, stopping at the box, near a pole or nearer than
  spacing / 2 to a line already drawn (occupancy grid).
- Iso blocks: screen (x, y) = ((i - j) a cos 30, -(i + j) a sin 30 + z a);
  drawn by diagonal i + j, then upward; three faces per cube.
- Cone template: slant s = sqrt(h^2 + ((D - d) / 2)^2); radii from the apex
  r = diameter x s / |D - d|; angle = pi |D - d| / s. Equal diameters give a
  rectangle pi D by h. Pieces split the angle; the tab is a trapezoid on the
  right side, its ends chamfered by the tab width; Bleed is a second
  outline grown by the bleed. Wrap maps the selection's box: across to the
  angle (repeated Repeats times round), down to the radius from the top
  edge's to the bottom edge's; lines are cut short first so they bend.

## Cells and tones (`shape_gen_cells.rs`)

- Picked selection (Use Selection): its box sets Width and Height and the
  place Create and Update use (`source_center`); its closed curves are
  welded into the outline (`boundary`, box coordinates, holes by even-odd);
  for stippling and halftone the selection is drawn alone to a PNG of
  about 240 px on the longer side, kept as base64 (`tone`). Transparent
  pixels (alpha under half) and places outside the outline get no mark.
  Without a picture the tone is a radial ramp, dark in the middle.
- Voronoi sites: Random n = area / spacing^2; Uniform one per spacing
  square; Hex and Square lattices moved by jitter x spacing. A gradient
  keeps each site with probability d = max(0.03, 1 - strength t), t the
  distance from the middle over the half diagonal (radial) or the place
  along the direction (linear), reversed for small cells at the edge.
  Sites outside the outline are dropped; at most 20,000.
- Relaxation: with no gradient and no outline, exact Lloyd (each site to
  its cell's area centroid); else weighted Lloyd on a raster of 24 samples
  per site (20,000 to 250,000), weights d^2 inside the outline (sites settle
  at a density following the square root of the weight), 0 outside.
  Panel points (`vor_points`, 0 to 1 across the box) replace the generated
  sites and are never relaxed automatically; Relax applies one step and
  keeps the result as panel points.
- Cells: the box clipped by the perpendicular bisector of each neighbour in
  order of distance, neighbours found ring by ring in a bucket grid, until
  the ring's distance is at least twice the farthest corner. Inset clips by
  every edge moved in by half the inset (so neighbours are an inset apart);
  corners are filleted with radius r (less on short edges) as one cubic;
  then the cell is intersected with the outline (inset by the inset).
  Delaunay edges join sites whose cells share a bisector edge.
- Stippling: first dots by rejection with darkness as the chance, then
  weighted Lloyd on the picture's pixels (or a 200 x 200 raster) for the
  steps; dot diameter lerp(smallest, largest, darkness) or the largest.
- Halftone: lattice of the cell size turned by the angle about the box's
  middle; darkness from 1 - luma over white; ink amounts CMY from the
  colour with K = 1 - max(R, G, B). Mark size cell x lerp(min, max,
  sqrt(ink)) so the area follows the ink; Lines sample every third of a
  cell along each row and join the band's top and bottom edges, thickness
  cell x lerp(min, max, ink). CMYK screens at 15, 75, 0 and 45 degrees,
  filled with 100 % of each ink, grouped per ink.

## Curves and tilings (`shape_gen_curves.rs`)

- Spirograph, fractals: drawn at any size, then scaled to fit the box,
  centred, keeping proportions.
- Wheel inside a ring (hypotrochoid): x = (R - r) cos t + d cos((R - r) t /
  r), y = (R - r) sin t - d sin((R - r) t / r); outside (epitrochoid) with
  R + r and a minus. t runs 2 pi r' / gcd(R', r') where R' and r' are the
  radii in tenths of a millimetre (at most 400 turns); samples (|R -+ r| / r
  + 1) x 48 per turn, joined by Catmull-Rom cubics.
- Lissajous: (sin(a t + phase), sin(b t)), t to 2 pi. Rose: r = cos(n / d
  angle) with n / d reduced, angle to pi d when n d is odd, else 2 pi d.
- Mandala: rings of N petals between r_i and r_(i+1), every other ring
  turned half a sector; the seed picks each ring's petal (teardrop, lobe
  or leaf) and width; a circle at each ring's base.
- Guilloche: ring k radius base_k + A1 sin(w1 a + k twist) + A2 sin(w2 a -
  2 k twist), base from the inner to the outer radius.
- Tree: segment, then two branches at -+ angle (times 1 -+ jitter) and
  length x ratio (times 1 -+ jitter / 2), down the levels.
- L-systems: presets Koch F--F--F, F=F+F--F+F, 60 degrees; Sierpinski A,
  A=B-A-B, B=A+B+A, 60; dragon FX, X=X+YF+, Y=-FX-Y, 90; plant X,
  X=F+[[X]-X]-F[-FX]+X, F=FF, 25. Rules parse as one symbol = text,
  separated by ; , or new lines; others are skipped. Growth stops at
  400,000 symbols (the last level that fits is drawn). Each branch or
  pen-up starts a new open curve.
- Penrose: a wheel of ten half-rhomb triangles of radius the half diagonal
  x 1.05, subdivided by the golden ratio until the edge is at most the
  tile size (at most 11 levels); halves of the same kind sharing a base
  are joined into rhombs, then cut to the box (Sutherland-Hodgman). Thin
  rhombs light (E8), thick dark (4E) when filled.
- Star patterns: square cells of the tile size (or pointy-top hexagons of
  that edge); from each edge's middle a ray toward each corner at the
  contact angle to the edge, stopped where it meets the neighbouring
  edge's ray; segments turned by Rotation about the box's middle and
  clipped to the box.

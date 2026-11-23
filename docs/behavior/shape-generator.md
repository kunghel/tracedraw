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

## Fills inside a shape (`shape_gen_fill.rs`)

- Outline: the picked outline flattened (tolerance the longer side / 4000,
  at least 0.002 mm) into rings read by the even-odd rule, or the box.
- Hatch: lines on multiples of the spacing through the box's middle (so
  they stay put while the angle turns); each line meets the rings'
  edges (half-open in y, so a corner on the line counts once) and the
  sorted crossings pair into the segments kept. Crossed adds the angle +
  crossing angle. By tone: L layers at angle + k 180 / L degrees, layer k
  kept along each segment (sampled every third of the spacing) where the
  darkness is over k / L (at least 0.02).
- Offset rings: the outline (or box) and `offset(outline, -k spacing)` for
  k = 1 up to Most rings, until the offset is empty.
- Tone lines: darkness on the source raster (outside the outline 0),
  marching squares at levels k / (N + 1), saddles split by the cell's
  mean; segments joined through shared grid edges into chains, closed
  when they meet, drawn as Catmull-Rom curves.
- Maze: nx x ny cells of the cell size centred in the box, a cell in use
  when its middle is inside the outline; a depth-first walk from the
  first unvisited cell (seeded choice among unvisited neighbours) for
  each region. Walls are the unit edges without a passage, merged into
  straight runs; the entrance is the bottom edge of the first cell in
  row order, the exit the top edge of the last. Passages: the walk's
  lines, a new line at each branch, straight runs kept as one segment.
- Space-filling curves from L-systems with 90 degree turns: Hilbert A,
  A=+BF-AFA-FB+, B=-AF+BFB+FA- (order n: n rewrites, 4^n corners); Moore
  AFA+F+AFA with A=-BF+AFA+FB-, B=+AF-BFB-FA+ (n - 1 rewrites, closed);
  Peano X, X=XFYFX+F+YFXFY-F-XFYFX, Y=YFXFY-F-XFYFX+F+YFXFY (9^n corners).
  Corners map to the box's grid cell middles (x and y scaled apart);
  with an outline, runs of corners inside it become open curves. Round
  corners: straight to each segment's middle, a quadratic round each
  corner.

## Slice grid (`shape_gen_slice.rs`)

- Cells over the selection's box (the preview: the Width x Height box):
  1 x rows (Rows), columns x 1 (Columns) or columns x rows (Grid), each
  shrunk by half the gap on every side (at most 45 % of the cell).
- Move of strip k of n, t = (k + 0.5) / n: Random a seeded value in -1 to
  1, Wave sin(2 pi waves t), Ramp 2t - 1, Alternate +1 for even k, -1
  for odd. Rows move by their row (Random draws x and y apart), Columns
  by their column; in a Grid x follows the row and y the column. Moves
  are the values times Move across and Move up or down.
- Slice Selection: per cell a ClipFrame whose frame is the moved cell
  rectangle (no fill, no outline) and whose contents are copies of the
  selected top-level objects with new ids, each moved by the cell's
  move; all in one group named after the generator, added to the active
  layer in one undo step with the originals deleted (kept with Keep
  original).

## Differential growth (`shape_gen_growth.rs`)

- Start: a ring of points one spacing apart (radius size % of half the
  box's smaller side, at least two spacings), a horizontal open line
  (size % of the width), or n rings at seeded random places; each is
  halved until all its points are allowed, up to eight times. The start
  is the box's middle, or the inside cell nearest to it.
- Allowed: inside the box less half a spacing, and inside the outline
  with half a spacing to spare left, right, up and down (a grid of
  inside flags, 400 cells along the box's longer side).
- A step: each point moves by a quarter of each neighbour edge's stretch
  past the spacing, plus smoothing times the way to its neighbours'
  middle, plus half a spacing times the sum of pushes (unit vector times
  (r - d) / r) from points within the repulsion radius (spacing times
  Repulsion; its own neighbours left out), at most half a spacing; a move
  to a place not allowed is not made. Then edges longer than 1.4
  spacings split at their middle, and Growth rate random edges split, up
  to the point limit.
- It settles when nothing split and no point moved a hundredth of a
  spacing, or 50 steps after the point limit; at most 5000 steps.
- Seeded and deterministic: Create and Update run the saved steps (or
  reuse the live growth when it is exactly there).
- Live: the panel advances it up to 12 ms a frame; running, Steps
  follows it; paused, it catches up with Steps (going back restarts it);
  any other change restarts it. Space over the panel starts or pauses,
  Shift+Space or Stop makes the curves (Create, one undo step).

## Deform and split (`shape_gen_warp.rs`)

- After any generator but the cone template, slice grid and growth.
- Pins: anchor as a share of Width and Height, pull as a share of the
  longer side (at most 64). Every curve is flattened (tolerance side /
  4000, pieces at most side / 300 long) and each point moves by the sum
  of the pulls times exp(-d^2 / 2 sigma^2), sigma = Reach % of the longer
  side; the curves become straight-line paths, closed ones stay closed.
  The drawing's frame stays its unbent box.
- Split by shape: the picked outline (the selection's closed curves and
  text welded, holes kept) is stored centred on the origin and placed on
  the drawing's middle; outline-only curves are cut where they cross it
  (`shaping::cut_open`, nonzero inside): Inside in another colour keeps
  both sides, the inside parts with the second colour and width (0 keeps
  the line width); Inside only and Outside only keep one side as plain
  lines. Every piece is open; filled marks are left whole. Create puts
  the drawing's middle on the picked outline's middle.
- Preview: drag from empty space adds a pin and sets its pull, drag a
  pin's end or start edits it, right click deletes it (Voronoi keeps its
  site editing instead).

## Backgrounds (`shape_gen_bg.rs`)

- Palette: 1 to 6 colours; `Paint::Tone(t)` fills with the colour a share
  t / 255 along it, mixed linearly between neighbouring colours. The i-th
  of n shapes gets t = i / (n - 1) (the first colour for one shape).
- Layered waves: band 0 (the back) is the whole box when there are two
  or more; band i of n has its level at h (n - i) / n and an edge sampled at 97 points: the level plus
  amplitude % of h times a weighted sum of `Detail` sines (frequency near
  k x Waves across, random phase, weights 1/k times 0.5 to 1.5, summed
  to 1), clamped to the box; Catmull-Rom curve through them, closed down
  to the bottom (or up to the top) corners.
- Blobs: points round a circle at evenly spaced angles nudged by up to
  0.2 x randomness of a step, each at r (1 - randomness x u); closed
  Catmull-Rom curve. One blob: centred, r = size % of the shorter side
  / 2. Several: centres at random in the box, r scaled by 0.45 to 1.
- Soft gradient: the box as a closed rectangle filled with a smooth
  `Mesh` of size x size cells; every node a random palette colour, inner
  nodes moved by up to 0.45 x wander % of a cell (edge nodes stay on the
  edge). The mesh moves with the drawing onto the page. Deform and split
  do not apply to it.

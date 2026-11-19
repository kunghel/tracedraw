# Symbol Scatter

Effects > Symbol Scatter opens the Symbol Scatter panel, which fills the
selected objects (shapes, curves, text, groups; pictures are skipped) with
copies of a symbol. Apply makes the copies as one group on top of the
selection, in one undo step ("Symbol Scatter"), and selects it.

## Settings

| Control | Default | Range | Meaning |
|---|---|---|---|
| Symbols | Star | Star, Heart, Circle, Sparkle, Flower, Triangle, Diamond, Drop, Mixed, Picked objects | The copies' shape; Mixed picks among all built-in symbols, Picked objects among the objects Use selection took |
| Use selection | | | The selected objects (with an outline) become the symbols and Picked objects is chosen |
| Spacing | 5 mm | 0.1 to 1000 mm | The least distance between two copies' centres |
| Size | 50 % to 90 % | 1 to 400 % each | The copy's longer side, in percent of the spacing, picked between the two |
| Random turn | 180° | 0 to 180° | Each copy turns at random up to this either way |
| Stay inside | on | | The whole copy inside the outline; off, only its centre |
| Color | Their own | Their own, Object filled, Random palette colors, One color | Below |
| Seed | 1 | 0 to 999999 | The same seed gives the same copies; Shuffle adds 1 |
| Keep objects | on | | Off deletes the filled objects |

## Placement

The outline is every selected object's page outline together, read by the
even-odd rule, so holes (letter counters, rings) stay empty. Points are
thrown at random over its bounds from the seed; a point is kept when it is
inside and no kept point lies closer than the spacing (a grid of cells
`spacing / sqrt(2)` wide holds at most one point, so a check looks at the
5 x 5 cells around it). About 30 tries per `spacing²` of the bounds are made.
With Stay inside, a copy whose outline (flattened, up to 64 points) leaves
the shape is shrunk to 60 %, twice at most, and left out if it still does
not fit.

One Apply makes at most 5000 copies; an outline whose bounds would hold more
than 20000 is refused at once. Both say "Too many symbols (n): raise the
spacing". Nothing placed says "No symbol fits".

## Copies

Built-in symbols are curves with the colour of the first filled object that
has a plain colour (black without one). Picked objects are copied whole
(groups too, with fresh ids), scaled so their longer side is the copy's size,
turned about their centre, with their gradients carried along. Object
filled, Random palette colors and One color paint every plain fill and
outline of the copy (inside groups too); unfilled parts stay unfilled.

Checked by `symbol_scatter::tests`.

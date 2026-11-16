# Color Styles docker (#778)

Window > Dockers > Color Styles keeps the drawing's colour styles
(`Document::color_styles`: a name, a colour and an optional harmony
name). Styles of one harmony are listed together and can be changed
together. The work is in `apps/tracedraw/src/color_styles.rs`, the panel in
`ui/color_styles_docker.rs`.

## Objects follow their style

A style is linked to the objects by its colour: whenever a style's colour
changes (edited, swapped, merged, turned with others, converted, a new
harmony rule), every use of the old colour in the drawing (fills of every
kind, gradient stops, mesh nodes, pattern colours, outlines, character
fills, inside groups, clip frames, blends and symbols) takes the new one.
The styles and the drawing change in one undo step named "Color Styles"
(`Command::SetColorStyles` then `Command::ReplaceColors`, which maps all
its colour pairs at once, so a swap needs no third colour).

## The panel, top to bottom

| Control | What it does |
|---|---|
| New from selection | A style for each fill colour of the selection that has none (as before). |
| New color style | A style of the base colour (as before). |
| New from document | A style for every colour the drawing's objects use that has none: fills, gradient stops, outlines. |
| ... | Delete unused (styles whose colour no object uses), Sort by name, Sort by hue (greys first by brightness, then round the hue circle; a harmony stays together, sorted inside), Convert to CMYK / RGB (spot and registration colours stay), Add to document palette. The last three work on the picked styles, or all when none is picked. |
| List / Swatches | Rows (pick box, colour, name, Fill, Outline, x) or 22 px swatches. Loose styles first, then each harmony under its name. |
| All / None | Pick every style, or none. |

Picking: the box of a row; a swatch click picks it alone, Ctrl+click adds
or removes it, Shift+click adds the run from the last picked; a double
click on a swatch fills the selection. A harmony's name picks its styles
(Ctrl adds them).

With styles picked: Fill and Outline (the first picked), Swap (exactly two:
each takes the other's colour), Merge (two or more: the first picked stays,
the others are deleted and their colours map to its colour), Make harmony
(a new harmony "Harmony N"; its styles move next to each other where the
first was), Leave harmony, Select objects (objects of the active page, on
visible unlocked layers, using any picked colour at any depth), Delete.

Change together: a hue wheel with a dot per picked style (angle = hue,
distance = saturation); dragging round it turns every picked colour by the
angle the pointer turned. Hue (-180 to 180 degrees), Saturation and
Brightness (-100 to 100, added to each colour's HSB values, clamped) move
them all. A CMYK or grey style stays in its model. A drag of the wheel or
of a slider is one undo step (the engine's step group).

Harmony rule (shown when exactly one whole harmony is picked): the
harmony is worked out again from its first colour by the rule (Analogous,
Complementary, Split complementary, Triadic, Tetradic, Monochromatic, as
`palette::Harmony::colors`): the base first, then the rule's other
colours; the harmony's existing styles take them in order, keeping their
names (their objects follow), new ones are named "<harmony> N", and styles
past the rule's count leave the harmony as loose styles.

New gradient: a harmony "Gradient N" of Colors (2 to 20, default 5)
shades of the first picked style (or the base colour), lightest first:
for t from -1 to 1, lighter shades lower the saturation and raise the
brightness towards white by -t x Spread, darker ones lower the brightness
by t x Spread x 0.8; Spread 0 to 100% (default 60%).

The Harmony editor at the bottom (rule buttons and Base color) is as
before; a second harmony of the same rule is named "<rule> 1", "<rule> 2".

## Tests

- `color_styles::tests` (swap and merge pairs, grouping order and
  adjusting, a new rule keeping the base and names, gradients and sorting,
  the drawing following a swap in one undo step, New from document,
  Select objects, Delete unused)
- `tracedraw_core::color_convert::tests::replace_colors_swaps_pairs_at_once`

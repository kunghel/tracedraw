# Transparency

| Control | Default | Range |
|---|---|---|
| Type | Uniform | None, Uniform, Gradient (linear, radial, conical, square, shaped), Pattern, Texture |
| Transparency | 50% | 0..100 |
| Merge mode | Normal | Normal, Add, Subtract, Difference, Multiply, Divide, If lighter, If darker, Texturize, Hue, Saturation, Colour, Invert, And, Or, Xor, Red, Green, Blue |
| Apply to | Fill and outline | fill, outline, both |
| Freeze | off | |

Gradient transparency uses the same geometry as gradient fills with
grayscale stops where white = opaque and black = transparent. The
object is rendered to a scratch layer, multiplied by the transparency
mask, then composited with the merge mode. A shaped gradient follows
the object's outline (`tests::shaped_transparency_follows_the_object`).

The Transparency tool shows a gradient mask's nodes as Interactive Fill
shows a fill's (`fill_tool.rs`, #448): start, end and skew handle, the
nodes between and the midpoint slider of each blend; a drag across an
object sets the line (a gradient mask there already keeps its nodes), a
double click on the line adds a node and on a node removes it. Edits are
one undo step "Transparency" per drag; the fill is left alone
(`fill_tool::tests::the_transparency_mask_has_nodes_and_a_midpoint`).

Uniform transparency with Normal is the object's opacity. With any
other merge mode it is kept as a mask of one grey (its lightness the
opacity, `mixed::uniform_mask`), drawn the same way, so Multiply and the
rest work on uniform transparency too, as in the reference program; the
Transparency part reads it back as Uniform with its amount and merge
mode (`mockup_tests::mockup_steps`).

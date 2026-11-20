# Stretch zones of a symbol

A symbol can keep its corners when an instance is stretched: a frame, a
button or a label then keeps its rounded corners, notches and ornaments at
their size, however wide or tall the instance is made.

## Setting the zones {#set}

Select an instance and choose
[[menu.object > menu.object.symbol > menu.object.symbol_stretch_zones]].
Turn on [[stretch_zones.on]] and set how far each line is set in from the
symbol's sides: [[stretch_zones.left]], [[stretch_zones.right]],
[[stretch_zones.top]] and [[stretch_zones.bottom]]. The sketch shows the
symbol's box with the four lines. Click OK.

The four lines part the symbol into nine zones. When an instance is
stretched:

- the four corner zones keep their size;
- the zones along the sides stretch along their side only;
- the middle zone takes up the rest, both ways.

Every instance of the symbol follows, on screen, in print and in exported
files. Curves are cut where they cross a line, so a shape that runs across
a line bends with the stretch there. Pictures inside the symbol, and the
width of outlines, stretch as a whole. An instance shrunk smaller than its
corners stretches as a whole too.

## Turning the zones off {#off}

Open the dialog again and turn off [[stretch_zones.on]]: instances stretch
as a whole again. The zones stay with the symbol when it is edited.

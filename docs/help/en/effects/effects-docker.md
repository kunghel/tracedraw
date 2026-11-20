# The Effects panel

An object can carry several live effects at once, applied one after the
other. The [[docker.effects]] panel lists the effects of the selected object in
the order they apply, from the top down. Open it with
[[menu.window > menu.window.dockers > menu.window.dockers_effects]].

![The Effects panel](../../img/effects-effects-docker.png)

## Managing effects {#stack}

Each row of the list has:

- a check box that turns the effect on or off. An effect turned off keeps its
  place and settings, but is not drawn, exported or printed;
- the effect's name, dimmed while it is off;
- up and down arrows that move the effect earlier or later, which changes the
  result (for example a contour of an envelope, or an envelope of a contour);
- a cross that removes the effect.

Every change is one undo step. Each effect is still edited with its own tool
or panel.

Above the list, the layer effects are switched on with their boxes and set on
the right, live on the page. **Transform** draws copies of the object,
each one the one before it scaled (**Scale H** and
**V**, in percent), moved, turned by **Angle** about
the object's centre and mirrored with **Reflect X** or
**Reflect Y**; **Copies** sets how many (0 transforms the
object itself). **Turn about H** and **V** move the point the copies turn
about away from the centre, so a turn of 30° and 11 copies set the object
in a ring round that point. **Random** gives each step a random share of those
values. Outlines keep their width unless **Scale outlines** is on. A
thick round-capped ring with an 85% scale, a 15° turn and a dozen copies
makes a spiral of rings. The object stays as it is, so its shape, color and
the effect stay editable; **Effects > Flatten Effects** makes the
copies objects.

**Grid** repeats the object in **Columns** and **Rows**,
to the right and down, the object itself being the top left cell. **Gap H**
and **V** set the space between cells (less than nothing overlaps them),
and **Shift rows** moves every other row to the right for brick and
honeycomb layouts. With the Pick tool, the round handles on the second column
and the second row drag the gaps (near nothing, the cells touch), and the
square past the last corner drags out more or fewer cells. Draw one tile,
color its faces with the Shape Builder, then give it a grid: the tile stays
editable and the whole pattern follows it.

Under the list, the built-in looks give the selected objects a finished
look with one click: chrome, liquid metal, gold, liquid glass (which blurs
what lies beneath it) and clear glass. Each picture shows the selected object
wearing the look; one undo removes it. Your own saved presets come after them.

## Other effect commands {#commands}

- [[menu.effects > menu.effects.flatten]] turns the effects of the selected
  objects into plain objects.
- [[menu.effects > menu.effects.merge_shapes]] groups the selected objects
  and gives the group a live [[effect.offset_path]] that joins them into one
  outline, rounded where they meet and painted like the bottom object: move
  an object of the group and the shapes flow together or apart. Set
  **Smooth** in the layer effects at the top of the panel; on any object,
  **Offset** grows the outline (or shrinks it, below zero) live.
- [[menu.effects.picker]] shows a list of effects to apply to the selection.
- [[menu.object > menu.object.clear_effect]] removes the effect of the
  selection.
- [[menu.object > menu.object.copy_effect]] copies an effect from another
  object, and [[menu.object.clone_effect]] makes a clone that follows it; see
  [Copying and cloning effects](copy-and-clone.md).

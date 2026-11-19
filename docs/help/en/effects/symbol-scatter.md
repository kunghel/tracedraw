# Symbol Scatter

Symbol Scatter fills a shape, a curve or a line of text with many copies of a
symbol: stars in a heart, flowers in a letter, your logo across a badge. The
copies never bunch up, and each one gets its own size and turn.

## Scattering symbols {#scatter}

Open the [[docker.symbol_scatter]] panel with
[[menu.effects > menu.effects.symbol_scatter]].

1. Choose the symbols in the [[docker.scatter_symbols]] list: a built-in
   symbol (star, heart, circle, sparkle, flower, triangle, diamond, drop),
   all of them mixed, or your own objects. For your own, select one or more
   objects and click [[docker.scatter_use_selection]]; each copy is one of
   them, picked at random.
2. Select the objects to fill. Text works as it is, and holes (the inside of
   an O, a ring) stay empty.
3. Set the options:
   - [[docker.scatter_spacing]]: the least distance between two copies (5 mm
     at first). Smaller spacing makes more copies.
   - [[docker.scatter_size]]: the smallest and largest copy in percent of
     the spacing (50 % to 90 % at first). Over 100 % the copies overlap.
   - [[docker.scatter_rotation]]: how far each copy may turn either way (180
     degrees turns them freely; 0 keeps them upright).
   - [[docker.scatter_inside]]: every copy stays wholly inside the outline,
     shrunk or left out when it does not fit. Off, only its center has to be
     inside.
   - [[docker.scatter_color]]: the copies keep their own colors (built-in
     symbols take the filled object's color), take the filled object's color,
     random colors of the document palette, or one color you choose.
   - [[docker.scatter_seed]]: the same seed gives the same arrangement;
     [[docker.scatter_shuffle]] takes the next one.
   - [[docker.scatter_keep]]: off removes the filled objects.
4. Click [[docker.apply]].

The copies come as one group on top, selected, and one undo removes them. One
Apply makes at most 5000 copies; raise the spacing for large objects.

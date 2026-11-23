# Aligning and distributing objects

Aligning lines objects up by their edges or centers; distributing spaces them
evenly. Use the [[docker.align_distribute]] panel, the commands of
[[menu.object > menu.object.align_distribute]], or the keys below.

![The Align and Distribute panel](../../img/objects-align-distribute.png)

## Aligning {#align}

1. Select the objects. Select the one the others should line up with last.
2. In the [[docker.align]] part of the panel, choose [[docker.align_to]]:
   - [[docker.align_active]] (at first): the object selected last; with one
     object selected, the page;
   - [[docker.align_page_edge]]: the edges of the page;
   - [[docker.align_page_center]]: the center of the page;
   - [[docker.align_grid]]: each object to the nearest line of the document
     grid.
3. Click one of the six buttons: left edges, centers on a vertical line,
   right edges, top edges, centers on a horizontal line, or bottom edges.

[[docker.align_use_outline]]: with this on, the outer edges of the outlines
meet, instead of the paths themselves.

The keys always align to the object selected last:

| Key | Command |
|---|---|
| <kbd>L</kbd> | [[menu.object.align_left]] |
| <kbd>R</kbd> | [[menu.object.align_right]] |
| <kbd>T</kbd> | [[menu.object.align_top]] |
| <kbd>B</kbd> | [[menu.object.align_bottom]] |
| <kbd>E</kbd> | [[menu.object.align_centers_h]] |
| <kbd>C</kbd> | [[menu.object.align_centers_v]] |
| <kbd>P</kbd> | [[menu.object.center_to_page]] |

## Distributing {#distribute}

1. Select three or more objects (two or more to distribute across the page).
2. In the [[docker.distribute]] part, choose [[docker.distribute_to]]:
   - [[docker.distribute_selection]] (at first): the first and last objects
     stay where they are, and the others move between them;
   - [[docker.distribute_page]]: the objects spread across the whole page,
     the first touching one side and the last the other;
   - [[docker.distribute_spacing]]: the first object stays and the others
     follow it at the distance in [[docker.distribute_gap]] (between the
     objects for the gaps, else between their edges or centers).
3. Click one of the eight buttons: left edges, centers, gaps or right edges
   across the page, or top edges, centers, gaps or bottom edges down it.
   Spreading the gaps gives equal space between the objects whatever their
   sizes.

With [[docker.distribute_spacing]], once a button has spread the
selection you can change the spacing and watch the objects follow: turn
the mouse wheel over the box, or drag the slider under it. Let go of the
slider, or move the pointer away from the box, to keep the result as one
undo step; press <kbd>Esc</kbd> to put the spacing and the objects back.

With too few objects the buttons are pale and the panel says how many are
needed. Each align or distribute is one undo step.

To place copies at even distances in one step, see
[Repeat copies](step-and-repeat.md).

## Arranging in a grid {#grid}

[[menu.object > menu.object.align_distribute > menu.object.arrange_in_grid]]
puts the selected objects in rows and columns, in the order you selected
them, starting at the top left corner of the selection. Choose how many
objects go in each row and the gaps between the columns and the rows.
Every cell is as wide as the widest object and as tall as the tallest, and
each object sits at the centre of its cell, so objects of different shapes
and sizes line up. It is one undo step.

The window has more choices. Order fills the cells in the order you
selected the objects or as they lie now, top row first and left to right.
Cells makes every cell the same size or fits each column to its widest
object and each row to its tallest. Place in cell is a square of nine
places: click a corner, the middle of a side or the centre to say where
each object sits in its cell. With Preview on, the drawing shows where
every object will go, as outlined boxes, before you click OK; Fill With
Copies shows its copies the same way and counts them as you change the
gaps.

## Fit objects to a path {#fit-to-path}

The [[docker.fit_to_path]] panel places objects along a line, a curve or the
outline of a shape, with copies, spacing, placement and rotation of your
choice. Select the objects, then the path last (Shift+click), and choose
[[menu.object > menu.object.fit_objects_to_path]]. See
[Fitting objects to a path](fit-objects-to-path.md) for every setting.

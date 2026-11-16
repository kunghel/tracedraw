# Color harmonies and changing styles together

The [[docker.color_styles]] panel can do more than keep a list of colors.
Styles can be picked and changed together, grouped into harmonies, swapped,
merged and shaded into gradients; whatever happens to a style, the objects
that use its color follow, in one undo step. See
[Color styles](color-styles.md) for making styles and applying them.

## Showing the styles {#view}

[[docker.cs_view_list]] shows each style on a row with its color, its name
and its buttons; [[docker.cs_view_swatches]] shows them as color swatches.
Loose styles come first, then each harmony under its name.

## Picking styles {#pick}

- In the list, tick the box at the start of a row.
- In the swatches, click a swatch to pick it, Ctrl+click to add one or take
  it away, Shift+click to pick the run from the last one picked. A double
  click fills the selected objects.
- Click a harmony's name to pick all its styles.
- [[docker.cs_pick_all]] and [[docker.cs_pick_none]] pick every style or
  none.

With styles picked, the panel offers:

- [[docker.cs_fill_picked]] and [[docker.cs_outline_picked]]: the first
  style picked on the selected objects;
- [[docker.cs_swap]] (two styles picked): each style takes the other's
  color, and so do their objects;
- [[docker.cs_merge]]: the styles become the first one picked; the others
  are deleted and their objects take its color;
- [[docker.cs_group]] and [[docker.cs_ungroup]]: put the styles in a new
  harmony, or take them out of theirs;
- [[docker.cs_select_objects]]: select the objects of the page that use
  them;
- [[docker.cs_delete_picked]]: delete them (objects keep their colors).

## Changing styles together {#adjust}

Under [[docker.cs_adjust]] each style picked is a dot on the
[[docker.cs_wheel]]: drag round the wheel to turn them all, keeping how far
apart they are. The [[docker.cs_hue]], [[docker.cs_saturation]] and
[[docker.cs_brightness]] sliders move all of them at once. A drag is one
undo step.

When a whole harmony is picked, [[docker.cs_rule]] works it out again from
its first color by another rule ([[harmony.complementary]],
[[harmony.triadic]] and the others).

## Gradients {#gradient}

[[docker.cs_new_gradient]] makes a harmony of shades of the first style
picked (or of the [[docker.base_color]] when none is), from lighter to
darker: [[docker.cs_steps]] sets how many, [[docker.cs_spread]] how far
apart the ends are.

## More {#more}

The **...** button holds [[docker.cs_delete_unused]],
[[docker.cs_sort_name]], [[docker.cs_sort_hue]], [[docker.cs_to_cmyk]],
[[docker.cs_to_rgb]] and [[docker.cs_to_palette]]; the last three work on
the styles picked, or on all of them. [[docker.cs_from_document]] makes a
style of every color the drawing uses.

# How a group puts its members together

A group normally draws its members one after another onto what lies beneath
it, and its transparency goes on each member, so members that overlap show
through each other. Two options in [[menu.object > menu.object.group]] change
that for the selected groups.

## Drawing a group as one {#as-one}

With [[menu.object.group_as_one]] on, the group is drawn on its own first and
then laid down as one picture:

- its transparency goes on the whole group, so where members overlap, the
  lower ones do not show through the upper ones;
- merge modes of its members mix only with the group's own members, not
  with what lies beneath the group.

## Members hiding those beneath {#hide-beneath}

With [[menu.object.group_knockout]] on, each member hides the members beneath
it where it lies, even where it is see-through: a see-through member shows
what lies beneath the group, not the other members. Use it for see-through
shapes that overlap without darkening where they meet.

Both options can be on together, and both go with the group when it is
copied and saved. Ungrouping drops them.

## In exported files {#export}

In PDF, a group with [[menu.object.group_knockout]], or a see-through group
drawn as one, goes in as a picture with its transparency. In SVG, a group
drawn as one keeps its transparency on the group, and a group with
[[menu.object.group_knockout]] goes in as a picture.

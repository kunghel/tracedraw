# Drop shadows

A drop shadow is a soft shadow beneath an object, as if it floated above the
page. It is a copy of the object's silhouette in the shadow color, offset and
blurred.

## To add a drop shadow {#create}

1. Choose the [[tool.drop_shadow]] tool from the Effects flyout.
2. Drag from an object to where the shadow should fall.

Clip frames and groups take shadows too: a frame casts the shadow of what it
shows (its fill and the contents within it), and a group the shadow of each of
its objects. A click on a shadow with the [[tool.pick]] tool selects its
object; with the [[tool.drop_shadow]] tool it shows the shadow's handles, and
a drag on the shadow moves it. Shadows show in exported and printed PDF
files, with their color, opacity and feathering.

A double-click on a shadow with the [[tool.pick]] tool chooses the
[[tool.drop_shadow]] tool for its object, with the shadow's handles on the page
and its settings on the property bar. Right-click an object with a shadow and
choose [[context.edit_shadow]], or use the same button in the shadow's section
of the Properties panel, to do the same. An object that has a shadow of its own
is always picked by a click on it, also where another object's shadow falls
over it.

## Settings {#settings}

On the property bar:

- the **offset** of the shadow (2 mm right and 2 mm down at first);
- its **opacity** (50% at first);
- its **feathering**, how soft its edge is (15% at first);
- its **color** (black at first).

A change there applies at once to the selected objects, also to an object
that has no shadow yet: choose Flat, or an edge, to give it that kind of
shadow directly. Shadow handles can be grabbed from a little outside the
squares, in every kind of shadow; where they lie close together, a press takes
the nearest one. Drag the white start handle to an edge of the object to stand
the shadow on that edge, or into the middle for a flat shadow.

[[menu.object > menu.object.clear_effect]] removes the shadow. To copy the
shadow of another object, use
[[menu.object > menu.object.copy_effect > menu.object.copy_shadow_from]].

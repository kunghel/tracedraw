# Linking copies into a symbol

When a drawing already holds many copies of one object, they can become
instances of one symbol, so that a change made once shows on all of them.

## Linking the copies {#link}

Select one of the copies and choose
[[menu.object > menu.object.symbol > menu.object.symbol_from_copies]]. That
object and every copy of it lying on a layer (the same object moved, turned,
scaled or mirrored, with the same fill, outline and effects) become instances
of one new symbol, each where its object was and in its place in the stacking
order, in one step to undo. The status bar says how many were linked.

Copies inside groups, and copies with another colour, outline or effect, stay
as they are.

## Changing them all {#edit}

Edit the symbol as any other (see [Symbols](symbols.md)): every instance
shows the change at once.

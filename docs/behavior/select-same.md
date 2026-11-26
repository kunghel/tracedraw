# Select Same

Open **Window > Panels > Select Same** or **Edit > Select Same**.
Select one or more reference objects on the
canvas, check the properties to compare, then press **Select Same**.
The panel reports how many matching objects were selected. It keeps
its options when the selection changes.

The default properties are **Fill** and **Object type**. All other
properties start off. The default scope is the current page, including
its visible, unlocked master layers. **Current layer only** restricts
the search to the active layer. The default size tolerance is **0.5 mm**.
There is no default keyboard shortcut for opening the panel. The normal
selection and Undo/Redo shortcuts apply.

The panel keeps the reference selection above a compact **Match properties**
section. Related properties share a row when there is room and stack in
narrow panels or languages with longer labels. Enabling **Dimensions**
reveals its comparison and tolerance directly below that property. The
scope and **Select Same** button stay below the matching properties.

## Matching properties

All checked properties must match the same reference object. With
several references, matching any one reference is sufficient. A candidate
cannot take its fill from one reference and its type from another.

| Property | Comparison |
| --- | --- |
| Fill | The complete fill value, including gradient or pattern settings; no fill matches no fill. |
| Outline color | The outline color independently of its width; no outline matches only no outline. |
| Outline width | Widths differ by at most 0.0005 mm; no outline matches only no outline. |
| Object type | The same object category; artistic text and paragraph text are distinct types. |
| Dimensions | The width and height of the object's axis-aligned selection bounds in page coordinates, including parent transforms, symbol definitions and live geometry, and excluding outline thickness. |
| Font family | A nonempty text span has the same font family after trimming whitespace and converting to lowercase. |
| Font size | A nonempty text span has a size within 0.01 pt of a reference span. |

Font comparisons apply only to text. When Font family and Font size
are both checked, the same pair of spans must satisfy both properties.

## Dimensions

Let the reference dimensions be `(rw, rh)`, the candidate dimensions
`(cw, ch)`, and the nonnegative tolerance in millimetres `t`.

- **Same dimensions:** `abs(cw - rw) <= t` and
  `abs(ch - rh) <= t`.
- **Larger dimensions:** `cw >= rw - t` and `ch >= rh - t`, with
  `cw > rw + t` or `ch > rh + t`.
- **Smaller dimensions:** `cw <= rw + t` and `ch <= rh + t`, with
  `cw < rw - t` or `ch < rh - t`.

A wider but substantially shorter object is therefore neither larger
nor smaller. The comparison uses the displayed width and height;
turning an object can change these dimensions. The tolerance affects
dimensions only, not colors, outline width or text size.

Dimensions use the object's own visual bounds, as its selection box does.
Live warps and geometry effects are included. Intermediate steps of a
blend linked to a separate end object are excluded from the start object's
dimensions. Symbols are measured from their definitions, including nested
instances and their transforms. A missing or cyclic symbol definition has
no measurable dimensions and cannot match a dimensions query.

## Groups, containers and scope

The search is limited to selectable objects on the current page. Hidden
or locked layers and object branches are skipped. While editing clip
contents, candidates remain within those contents.

A group or clip frame holding a locked object cannot be a reference or
match, including when the lock is on the frame itself or a nested hidden
object. Its unlocked contents can still match independently.

By default the search visits group and clip-frame contents. **Exclude groups
and clip frames** skips those objects and their entire contents, including
nested groups and clip frames. This applies to both candidates and reference
objects, independently of the Object type property. A selected group,
container or object inside one does not supply reference attributes while
this option is checked.

With exclusion off, a reference group is expanded to its contents when
Object type is off. Candidates are likewise the contents rather than the
containers in that case. Otherwise a selected group remains a group reference.

When a matching container is selected, its descendants are not selected
alongside it. This prevents later editing commands from acting twice
on the same objects.

## Selection and history

The reference is always the current selection when the button is
pressed. With no selection or no checked property, the button is
disabled and the panel explains what is needed. A valid search with
no matches clears the selection.

A changed selection is one Undo/Redo step while the drawing is
unchanged. Undo restores the reference selection; Redo restores the
matches. Searching does not edit objects, mark the document modified,
or create a document-history step. Comparisons run on request, rather
than scanning the drawing every frame.

## Checks

- `select_same::tests` in `tracedraw-core`: combined properties,
  transformed dimensions, text spans, container traversal and query history.
- `select_same::tests` in the app: selection scope and selection Undo/Redo.
- `ui::select_same_docker::tests`: panel controls and empty-selection feedback.

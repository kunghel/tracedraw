# Context menu customization

Options > Customization > Context Menus controls the visibility and order of
operations in 32 independently configured right-click menu locations. By
default, all operations are visible in their original order. Operations
added by a later version are visible by default; in a customized sibling
list they follow the saved items in their relative default order.

Choose a menu location, then expand a parent menu to see its indented child
operations. The tree follows the menu's hierarchy, including nested submenus.
Clear a checkbox to hide an operation in that location. A hidden parent hides
its entire submenu without changing its children's individual choices.

Drag a row's handle to place it before or after another row at the same level.
Move Up and Move Down offer the same operation without dragging and can be
used from the keyboard. Moving a parent carries its whole branch with it;
children stay in their original parent menu. The next time the right-click
menu opens, it uses the chosen order for the operations available there.

Search the translated operation labels or stable operation keys. Search
ignores case and requires every word entered. Matching children keep their
parent path visible, so the results still show where an operation belongs.
Matching a parent also shows its children. Search expands the matching
branches temporarily; clearing it restores the previous expansion choices.
Unavailable operations stay in the editor so preferences can be set before
opening a suitable document or selection. Visibility does not make an
unavailable command available.

Canvas objects, nodes, text, locked objects, guidelines, empty pages and text
tab rulers have independent settings. Other locations cover page tabs,
rulers, toolbars, panel tabs and titles, Objects panel rows, palettes, fonts,
the save dialog, the shelf, saved shapes, recent files and right-button drops.
The Objects panel's object menu has preferences independent of the canvas.

Dynamic target lists use a stable operation identifier. For example, hiding
Move to Layer hides the operation and its current target layers together.
Reordering that operation moves the target list as a group; the targets keep
their existing order. Renaming a layer, palette, file or collection does not
alter preferences.

Document palette swatches and the Palette Manager's document entry have
separate locations because their menu structures differ. Within Object
Styles, document-defined style targets keep their existing slot between the
editable static actions; sorting static actions does not relocate that list.

Restore This Menu clears visibility and order overrides for the selected
location, including operation keys from a newer version. Restore All Menus
clears all context-menu overrides. OK saves the choices with application
settings; Cancel, Escape or the close button restores both visibility and
order from when Options opened. These preferences do not change a drawing
or its native file.

The page uses the existing keyboard-accessible controls. On narrow desktop
windows its tabs wrap, and the menu location, search, restore controls, OK
and Cancel remain inside the dialog.

Main menus, command search, keyboard shortcuts, toolbar commands and direct
right-button gestures keep their existing behavior. This feature adds no
default shortcut. Empty separator groups are omitted when their operations
are hidden.

Preferences store hidden operation keys and explicit sibling-order
overrides by menu location and parent path. Unknown keys survive settings
round trips. No per-frame settings synchronization or repaint loop is
required.

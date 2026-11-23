# Full-screen preview

F9, View > Full-screen Preview and the Full-screen Preview toolbar button
expand the current canvas view to fill the display. The desktop window
enters full-screen mode, hiding its title bar and the editor's bars and
panels.

## Area shown

The preview keeps the canvas's current centre and zoom. The additional
screen area reveals the surrounding drawing at the same scale, filling
the display without fitting the old canvas rectangle inside a border.
Distant objects do not change the zoom or make the visible objects
smaller. Objects beside the page remain visible when they fall inside
the expanded view. The page border follows Options > Display >
Full-screen preview > Show page border (on by default).

The centre and zoom are captured once, before the window changes size.
The centre is the canvas's screen centre converted through the current
view: `x = (screen_x - origin_x) / zoom` and
`y = (origin_y - screen_y) / zoom`. That document point is placed at the
full-screen preview's centre with the same zoom. A later window resize
reveals more or less of the surrounding drawing without changing its
scale or centre.

View > Preview Selected Only fits the selection's bounds into the
screen, keeping its aspect ratio, and enters the same full-screen mode.
This retains its existing selection-area behaviour.

## Return to editing

The exit hint appears for the first 1.5 seconds of each preview, then
hides automatically even when the pointer stays still. Resizing the
window or moving the pointer does not show it again.

Press Esc or F9, or click the preview, to return. The window returns to
its previous full-screen state; a window that was already full-screen
stays full-screen. The canvas's pan, zoom and selection are preserved.
Editing shortcuts do not change the drawing while the preview is open.

## Checks

The tests in `apps/tracedraw/src/ui/preview.rs` and
`apps/tracedraw/src/ui/preview_tests.rs` cover the visible area, objects
outside the page, full-screen entry and exit, viewport changes and
restoration of the editing view.

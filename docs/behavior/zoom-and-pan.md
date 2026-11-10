# Zoom and Pan

## Zoom tool (Z, F2)

- Click zooms in by 2 at the point; Shift+click or right-click zooms
  out by 2 (with this tool the right button never opens the context
  menu). Dragging a marquee zooms to fit that rectangle.
- Zooming to a rectangle (marquee, selection, all objects, page) centres
  that rectangle in the window, wherever it lies on the page, with a
  40 px margin: `zoom = min((W - 80) / w, (H - 80) / h)`.
- Double-clicking the tool button zooms to fit all objects (F4).
- F2 picks the tool for one zoom: after a click, a right click or a
  marquee the tool before it comes back (#1039,
  `pick_drag_tests::f2_zooms_once_and_gives_the_tool_back`); Z or the
  toolbox button keeps it until another tool is chosen. Shift+F2 zooms
  to the selection; F4 zooms to fit
  all objects; Shift+F4 zooms to the page.
- These keys work while a text is being edited too, and the editing
  goes on: after F2's one zoom the text is edited on where it was
  (#1699, `pick_drag_tests::zoom_keys_work_while_editing_text`).
- `+`/`=` and `-` zoom by 1.25 about the canvas centre (except that `+`
  duplicates the selection in place with a Pick tool, and with the Shape
  tool and nodes chosen `+` and `-` add and delete nodes); the zoom combo
  on the standard toolbar steps through 10, 25, 50, 75, 100, 150, 200,
  300, 400, 800 and 1600 % (beyond the last step the factor doubles).

## Mouse wheel

- Default (Options > Tools > Default action for mouse wheel: Zoom): the
  wheel zooms by the zoom step per notch about the pointer, in a glide
  over a few frames, not in one jump; Ctrl+wheel scrolls vertically;
  Alt+wheel scrolls horizontally (#766,
  `pick_drag_tests::a_wheel_notch_zooms_in_a_glide`).
- Zoom step (Options > Tools > Zoom/Pan > Zoom per wheel notch, #895):
  1 to 100 %, 15 % at first (a factor of 1.15 a notch). A notch is a line of the
  wheel on the desktop, three lines or 100 pixels in the browser
  (`smooth_wheel_notch`); a touchpad's small pixel steps (under 8
  points) zoom their share of a notch at once, unsmoothed.
- The glide follows the wheel's speed (#1648, `view_motion.rs`): the
  zoom still to come is kept as a logarithm and each frame takes
  `1 - 0.1^(dt / T)` of it, `T` the time to reach 90 %. A notch after a
  pause (speed up to 8 notches a second) glides with `T = 0.10 s`;
  faster turns, the notches of an event over the time since the last
  one (at most 0.25 s), shorten it in proportion,
  `T = 0.10 * 8 / speed`, down to 0.03 s. What is left under 0.0001 goes
  at once, so a glide always ends; it goes on about its point when the
  pointer leaves the canvas, and a repaint is asked only while it moves
  (`view_motion::tests::faster_wheel_turns_glide_shorter`,
  `view_motion::tests::a_glide_adds_up_to_the_steps_and_ends`,
  `pick_drag_tests::a_fast_wheel_turn_glides_shorter`).
- Smooth zoom (Options > Tools > Zoom/Pan, on at first): off, every notch
  zooms its whole step at once
  (`pick_drag_tests::smooth_zoom_off_zooms_the_fixed_step_at_once`).
- With the Scroll setting: the wheel scrolls vertically; Shift+wheel
  scrolls horizontally; Ctrl+wheel zooms.
- A trackpad pinch zooms by the gesture's factor with either setting.
- On a touch screen two fingers pan the view and a pinch zooms, with any
  tool (the first finger's drag is dropped).

## Pan (H)

- Drag moves the view; the middle button pans with any tool. Pan never
  changes the document.
- The middle button also pans while a shape (rectangle, ellipse,
  polygon, star, and the other drag-to-draw shapes) is being drawn with
  the left button held: the shape's far corner stays under the pointer
  as the page moves, the drag goes on after the middle button is let
  go, and releasing the left button makes the shape. Changing the tool
  drops an unfinished shape drag (#1923).
- Pan inertia (Options > Tools > Zoom/Pan, off at first, #831): a pan
  drag (the Pan tool, the middle button, two fingers) let go while
  moving keeps the view sliding the same way and easing out. The drag's
  speed is smoothed over its last frames (`0.4 * before + 0.6 * now`);
  let go at 150 points a second or faster, with the pointer moved in the
  last 0.06 s, the view slides on at that speed (at most 8000 points a
  second), which falls by `exp(-dt / 0.3 s)` a frame; under 15 points a
  second it stops, so a flick slides about 0.3 s worth of its speed. A
  press, a wheel turn or a new pan drag stops it. A few float
  operations a frame, and a repaint is asked only while it slides
  (`view_motion::tests::a_flick_slides_on_and_eases_out`,
  `view_motion::tests::no_slide_when_off_held_still_slow_or_stopped`,
  `pick_drag_tests::a_flicked_pan_slides_on_with_pan_inertia`).

## Scrolling at the edge during a drag

- While the left button drags objects (moving, sizing), draws (a shape
  dragged out, Freehand, a connector, a text frame), selects (a
  selection box or freehand selection, of objects or of nodes) or drags
  nodes and handles with the Shape tool, a pointer within 24 interface
  points of the drawing window's edge, or past it, scrolls the view
  toward that edge: up to 900 points a second, from nothing at 24
  points in to the full speed at the edge, along each axis on its own
  (a corner scrolls both ways). It goes on while the pointer rests
  there (each frame asks for the next), and the drag follows the pointer
  in the scrolled view, so a box grows and a moved object travels over
  what comes in (#2051, `interaction::edge_scroll_step`,
  `pick_drag_tests::a_drag_at_the_window_edge_scrolls_the_view`). A drag
  that has left the window keeps following the pointer.
- The cost a frame is a few comparisons; nothing scrolls without a drag
  or in a drawing window smaller than four bands.

## Navigator and scrollbars

- The scrollbars cover the page plus a margin of one page in every
  direction; their layout and buttons are in `drawing-window.md`.
- The navigator button sits in the corner between the two scrollbars.
  Pressing and holding it opens a pop-up with a thumbnail of the current
  page (longest side 220 px, aspect kept) and a blue rectangle for the
  visible area. While the button is held, moving the pointer over the
  thumbnail centres the view on that point; releasing closes the pop-up.
  The thumbnail is rendered once per opening at
  `dpi = 220 / page width in mm * 25.4`.
- View > Navigator has the key N (live test of 0.41; free in the shortcut
  table, `options::tests::no_two_default_commands_share_a_shortcut`).
  The key held over the drawing does what the held button does: the
  pop-up shows while it is down and the view follows the pointer over
  the thumbnail; it does nothing while a text field or text editing has
  the keys or a dialog is open. The key can be changed or removed in
  Options > Customization > Commands like any menu command's; a key of
  the user's replaces N. The menu row itself opens the pop-up until a
  click: passing over the thumbnail moves nothing, a press on it centres
  the view there (and follows the pointer while held) and its release
  closes the pop-up; a press anywhere else or Esc closes it. Check:
  `ui::window_bars::tests::view_navigator_has_the_key_n`.

## Checks

- Given 100 % zoom, when the zoom-in step runs, then the zoom is 150 %.
- Given a click with the Zoom tool at the page centre, then the page
  centre stays under the pointer and the zoom doubles.
- Given an object far from the page origin, when zooming to fit it, then
  its centre is the window centre (`view::tests::fit_centres_rectangles_anywhere_on_the_page`).

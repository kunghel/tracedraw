# Objects panel

The Objects tab of the docker shows the drawing as a tree: every page
with its layers, topmost first (or only the current page), then the
Master Page with the master layers, the guides and the document grid
(#1477). A bar at the bottom holds New Layer, New Master Layer and Show
All Pages at the left, the thumbnail size in the middle and Delete at
the right.

| Row | Contents |
|---|---|
| Page, Master Page | open or close arrow, a sheet (cream for the Master Page), name (in bold for the current page and the Master Page); a click opens or closes it |
| Layer (light grey), one step in | collapse arrow, eye (show or hide), printer (print and export or not), padlock (lock or unlock), layer icon (its top sheet blue for a page's layers, amber for master layers), name |
| Active layer | as a layer, on `#D4E6F7` with a 3 px accent bar at the left, name in bold |
| Master layer for odd or even pages | as a layer, followed by "Odd pages" or "Even pages" in small grey; dimmed on the pages it does not show on |
| Master layer in front of the page's layers | as a layer, followed by "In front" in small grey (after "Odd pages" or "Even pages" if it has one); it stays listed under the Master Page (#1132) |
| Guides | open or close arrow (when it has guidelines), eye (show or hide the guidelines), a pale printer (guidelines never print), padlock (locks or unlocks all of the page's guidelines; shut when they all are) |
| Guideline, two steps in (Guides row open) | eye (hides this guideline alone: it is not drawn, picked or snapped to, and stays hidden in the file), padlock (locks it), its kind and position ("Vertical 105.000 mm"); dimmed while hidden; a click selects it on the page, its set becoming the one edited (#1775). A guideline selected in the edited set (dragged out of a ruler, picked on the page or added in Guidelines Setup) opens its Guides row and scrolls to its row, highlighted, so a new guideline shows here; a row closed by hand stays closed until another guideline is selected (#2882). A selected guideline goes with Delete; a right-click on its row opens Delete, Cut, Copy, Paste, Lock or Unlock and Hide or Show, acting on the selected guidelines when the row is one of them, else on its own; a locked one is not deleted or cut (#3012, `a_guideline_row_deletes_and_has_a_menu`) |
| Document Grid | eye (show or hide the grid), a pale printer and a pale shut padlock (the grid never prints and cannot be edited) |
| Object, two steps in | thumbnail: the object itself drawn small (its shape, fill, outline and effects) on a white tile, then its name, or what it is when it has none ("Rectangle", "Curve with 8 nodes", "Paragraph Text: ...") |
| Group | as an object, with an arrow that opens it to its objects, one step further in (16 px) |

- Layer and node rows are 22 px high; an object row is the thumbnail
  size plus 6 px, 22 px at least. The selected objects show in `#CCE8FF`
  with a `#99D1FF` frame; the row under the pointer in `#E5F3FF`. Hidden
  objects show their name and their thumbnail dimmed.
- The active layer is where new objects go (drawn, pasted, duplicated,
  imported). Click a layer to make it active (the selection is cleared).
  Click an object to select it and make its layer active; Shift+click
  selects every object row from the row clicked before to this one,
  Ctrl+Shift+click adds that range, and Ctrl+click (Cmd on a Mac) adds
  one object or takes it out, as in a file list; locked objects stay out
  (#2483,
  `ui::objects_docker::tests::shift_click_selects_a_range_of_rows_and_ctrl_click_one_more`).
- With several objects selected, the eye of one of them hides them all
  when it is visible and shows them all when it is hidden, whatever each
  was; they stay selected, and one undo step brings each back as it
  was. The eye of an object outside the selection hides or shows only
  it (#2571,
  `ui::objects_docker::tests::the_eye_of_a_selected_object_hides_and_shows_the_whole_selection`).
- When the selection changes (picked on the page, Select All, an undo),
  the list scrolls just enough to show the first selected object's row,
  if its layer and groups are open; a selection that stays the same
  leaves the scroll where it is (#1044).
- A layer that is hidden or locked, or a master layer that does not show
  on the current page, cannot be active: new objects then go to the
  page's topmost visible, unlocked layer, as they do before any layer is
  picked. Each open drawing remembers its own pick.
- Objects on master layers show on every page the layer is for (all,
  odd or even pages) under the page's own layers, or over them when the
  layer is in front (its menu's "In front of the page's layers", or To
  Front of Page on one of its objects; To Back of Page puts it behind
  again, one undo step; #1132), and can be clicked,
  marquee-selected, snapped to and selected with Select All or Tab from
  any of those pages while the layer is visible and unlocked.
- Every new drawing has a master layer "Desktop" (#1161, #1775): objects
  moved onto it (Move to Layer, or drag in the docker), beside the pages
  or on them, show and are picked from every page, as the reference
  program's desktop layer does. New objects still go to the page's own
  layer. It is always behind the pages (the in-front choice for all
  pages leaves it), is not counted when master layers are numbered
  (`Layer::desktop`), and in `.cdr` files goes on the master page's
  desktop layer
  (`ui::objects_docker::tests::a_new_drawing_has_a_desktop_shared_by_every_page`).
- The Desktop layer's menu has "Objects put beside the page go here"
  (off by default, kept after TraceDraw is closed; #3100). On, an object
  moved with the Pick tool or pasted so that it lies wholly beside the
  page goes onto the Desktop layer, in the move's or paste's own undo
  step, so pictures pasted together can wait beside the pages and be
  taken from any of them; an object of the Desktop moved onto the page
  goes back to the page's active layer. Objects dropped into a PowerClip,
  pasted while its contents are edited, or moved onto another page of
  the Multipage View go where they did before
  (`pick_drag_tests::objects_put_beside_the_page_go_on_the_desktop_when_it_takes_them`).
- PowerClips (#3053): a PowerClip's row has an arrow, as a group's,
  that opens it to its contents, one step further in, front first. A
  click on a content's row takes the contents out for editing (Edit
  Contents, as Object > PowerClip > Edit Contents does) with that object
  selected, so it can be moved, edited or put in another order among the
  contents in the list; Finish Editing puts them back. A row dragged and
  let go in the middle third of another object's row puts the objects it
  carries inside that object as a PowerClip, as Place Inside Frame does
  (the object becomes the frame; a PowerClip takes them into its
  contents), one undo step, the PowerClip selected and opened in the
  list; the row is framed while the drop would do so. The upper and
  lower parts of the row still put them in front or behind, a group's
  middle still takes them into the group, and a locked object, a group
  or an object inside a group takes none
  (`ui::objects_docker::drag_tests::a_row_dropped_in_the_middle_of_another_powerclips_it`).
- Double-click a layer to rename it; double-click an object to name it
  (Rename Object; an empty name goes back to the description).
- Double-click the page node, or pick Rename in its context menu, to
  rename the page (the Rename Page dialog, as in Page > Rename Page);
  the context menu of another page also has Go to Page.
- A click on a layer or an object of another page makes that page the
  current one first (Layout's page change), then picks the layer or
  selects the object.
- A layer's context menu: Rename; New Layer (on its page) or New Master
  Layer (on the same pages); Select the Layer's Objects (the ones shown
  and not locked); ticks for Show/hide, Print and export the layer and
  Lock layer; Move Up, Move Down; a page's layer has Move to Master Page
  (all pages): the layer becomes a master layer with its name, look and
  objects (a page keeps its last layer, emptied); a master layer has
  Show On (All pages, Odd pages, Even pages), In front of the page's
  layers (a tick) and Move to the Current Page: it becomes the page's
  top layer with its objects, which then show on that page only; Move
  to Page, a list of the other pages (every page for a master layer):
  the layer becomes the top layer of that page with its name, look and
  objects, and that page is shown (a page keeps its last layer,
  emptied); Properties: name, Show/hide, Print and export, Lock and, for
  a master layer, Show On and In front, changed together on OK; then
  Delete Layer. Each is one undo step (#1477).
- An object's context menu is the drawing's own object menu for it,
  whatever tool is on (#2601): the object is selected first unless it is
  part of the selection (then the menu is for the whole selection), and a
  locked object gets the drawing's locked-object menu. Above the drawing's
  commands come the panel's own: Rename; Move to Layer, a list of every
  page's layers and the master layers under their page's name: the
  object (with the rest of the selection when it is selected) goes to
  the top of that layer, keeping its place on the page, in one undo
  step, and stays selected; a layer of another page makes that page the
  current one. So an object of a master layer is moved onto one page's
  layer, or a page's object onto the master page (#1477)
  (`objects_docker::drag_tests::a_right_click_on_an_object_row_opens_the_drawings_object_menu`,
  `objects_docker::drag_tests::an_object_rows_menu_is_the_full_object_menu_with_rename_and_move`).
  On a touch screen a finger held still on the row and lifted there
  opens this menu; up to 0.47 it opened under the finger while it was
  still held (#2948).
- A layer's object row dragged along the list (live test of 0.41): while
  the button is held a line marks where it goes; let go on the upper
  half of another object's row it goes in front of that object, on the
  lower half behind it, in that object's layer (of any page or of the
  master page, keeping its place on the page); let go on a layer's row
  it goes on the top of that layer. A selected row carries the rest of
  the selection's layer objects along, keeping their order. One undo
  step; the objects stay selected and their layer becomes the active
  one, on its page. Locked objects stay, a locked layer takes nothing,
  and a row let go on itself or away from the rows changes nothing.
  Groups (#2672): rows inside a group drag too. A row let go on the
  middle half of a group's row goes into that group, on top of its
  objects (the row is framed while it would); on the group row's top or
  bottom quarter it goes in front of or behind the group. Let go on an
  object inside a group, it goes in front of or behind that object in
  the group, so a group's objects change places without ungrouping; a
  group's object let go on an object of a layer comes out of its group.
  Each object keeps its place on the page whatever group it goes into
  (`Command::PlaceBeside`, `Command::PlaceInGroup`); a group left empty
  goes, and a group never goes into itself. On a touch screen a finger
  held on a row (0.8 s) and then moved drags it the same way, and a
  finger lifted ends the drag where it last was; up to 0.47 the hold
  ended the drag, so a row could not be dragged by finger or stylus
  (#2948, `ui::objects_docker::drag_tests::a_finger_held_on_a_row_drags_it_or_opens_its_menu`).
  Checks:
  `ui::objects_docker::drag_tests::a_row_dragged_along_the_list_changes_the_stacking_order`,
  `ui::objects_docker::drag_tests::a_row_dragged_onto_another_layer_moves_the_object_there`,
  `ui::objects_docker::drag_tests::rows_go_into_groups_and_change_places_inside_them`,
  `command::tests::objects_move_into_out_of_and_inside_groups` (core).
- The bottom bar:
  - New Layer adds "Layer n" on top of the page's layers (n is the
    number of layers plus one, or the next number not in use) and makes
    it active.
  - New Master Layer adds "Master Layer n" for all pages and makes it
    active; its context menu adds one for odd or for even pages, and
    under "In front of the page's layers" has a tick for each kind (All
    pages, Odd pages, Even pages): ticking it puts the drawing's master
    layers of that kind in front of the page's own layers (one undo
    step), unticking puts them behind, and new master layers of that
    kind are made in front or behind accordingly. The ticks are kept in
    the preferences (`master_in_front`, off at first) and are also in
    Options > Page, beside New master (all, odd, even), where each
    master layer also has an In front tick (#1132, #1477).
  - Show All Pages (two sheets, shown pressed when on, the default)
    lists every page; off, only the current one
    (`objects_all_pages` in the preferences).
  - Delete removes the selected objects or, with nothing selected, the
    active layer (not the page's last one). It is pale when there is
    nothing it can delete.
- Which layers are closed, which groups are open and which page nodes
  and the Master Page node are closed lasts for the session.

## Thumbnails

- Each object row starts with a thumbnail: the object drawn alone by the
  same renderer as the page, fitted into a square tile with a small
  margin and kept in its proportions (a long thin object gets a strip,
  at least an eighth of the tile thick). A group shows all of its
  objects, a symbol instance its symbol, a hidden object what it would
  draw. An object that draws nothing (an empty group) shows the empty
  tile.
- The size goes from 16 to 96 points in steps of 4, 20 at first, and is
  kept in the preferences (`object_thumb_size`). It changes with the
  slider in the bottom bar (between a small and a large tile, shown when
  the docker is wide enough), the wheel over that slider, or
  Ctrl+wheel (Cmd+wheel on a Mac) anywhere over the list; each notch is
  one step, in the browser too (#1406).
- Thumbnails are drawn at the screen's pixel density and cached per
  drawing and object. A cached thumbnail is checked only when the
  drawing changed (its revision moved on), and drawn again only when the
  object itself changed (compared with the copy it was drawn from), a
  symbol it shows changed, or the size changed. Rows scrolled out of
  sight draw nothing and ask for no thumbnail.
- New thumbnails are drawn for up to 6 ms per frame (at least one); the
  rest wait for the next frames, showing their old picture or, the first
  time, the swatch of their fill (a red diagonal for none) framed in
  their outline colour.
- The cache keeps at most 24 MB of pixels and 4,000 thumbnails; past
  that the thumbnails shown longest ago are dropped (never one shown in
  the current frame) and drawn again if their rows come back into view.
  With 3,000 objects the docker keeps only the thumbnails of the rows on
  screen and of those scrolled past.
- Measured with `objects_docker_frame_times` (3,000 objects, a 600 px
  docker, debug build): a frame with nothing changed takes 2.1 ms and a
  scrolling frame 3.0 ms, against 55 ms and 56 ms before thumbnails,
  when every row was drawn whether in sight or not; after scrolling the
  whole list the cache held 191 thumbnails in 191 KB of pixels.

## Checks

- `ui::objects_docker::tests::objects_show_their_names_or_what_they_are`
- `ui::objects_docker::tests::the_docker_draws_layers_groups_and_selection`
- `ui::objects_docker::tests::new_layers_get_free_names_and_become_active`
- `ui::objects_docker::tests::master_layer_objects_are_picked_where_their_layer_shows`
- `ui::objects_docker::tests::delete_takes_the_selection_else_the_active_layer`
- `ui::objects_docker::tests::clicking_an_object_in_the_docker_makes_its_layer_active`
- `ui::objects_docker::tests::the_list_scrolls_to_an_object_selected_on_the_page`
- `ui::objects_docker::tests::rows_show_thumbnails_drawn_once_per_change`
- `ui::objects_docker::tests::ctrl_wheel_and_the_bar_change_the_thumbnail_size`
- `ui::objects_docker::tests::a_browser_wheel_notch_steps_the_thumbnail_size_once`
- `ui::objects_docker::tests::a_long_list_draws_only_the_rows_in_sight` (and the
  ignored benchmark `objects_docker_frame_times`)
- `object_thumbs::tests::a_thumbnail_shows_the_object_in_its_colours`
- `object_thumbs::tests::pictures_are_drawn_again_only_when_the_object_changes`
- `object_thumbs::tests::the_least_recently_shown_pictures_go_first`
- `object_thumbs::tests::sizes_are_clamped`
- `command::tests::master_layers_reorder_and_delete_down_to_none` (core)

## Object Data

The Object Data docker lists the fields of the first selected object
(name and value), edits, adds and removes them. With several objects
selected, Copy to Selection (#3262) gives the first object's fields to
the others as one undo step ("Copy Object Data"): a field they have of
the same name takes the value, their other fields stay, and the fields
TraceDraw keeps for itself (names with a dot, `connector`, `hinting`)
are not copied
(`ui::dockers2::object_data_copy_tests::copy_to_selection_gives_the_fields_in_one_step`).

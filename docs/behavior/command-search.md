# Search Commands

Help > Search Commands (Ctrl+Shift+P) opens a small box at the top of
the window, over the rest of it, to find a menu command by typing part
of its name and run it, without looking through the menus.

- The list holds every command of the menu bar (the walk Options >
  Customization > Commands uses, `ui::menus::commands`), each with its
  name in the interface language, the menus it sits in and its
  shortcut (the user's own where one was given). It is gathered once
  when the box opens, not every frame.
- Typing narrows the list (`ui::command_search::ranked`): first the
  names that begin with what is typed, then the names that contain it,
  then the names that hold each typed word, then the commands whose
  name, menus or shortcut together hold each word ("object top"
  finds Object > Align and Distribute > Align Top), and last the
  names that hold the typed letters in order ("cnvcrv" finds Convert to
  Curves). Case does not count. An empty box lists every command in
  menu order.
- Up and Down move the highlight (it starts on the first row), Enter
  runs the highlighted command, a click runs the one clicked. The
  command runs as its menu row would (`ui::menus::run_command`); one
  that cannot run now (greyed in its menu) does nothing and the status
  bar says so.
- Esc or a click outside the box closes it without running anything.
- The key is free of every other command, panel and tool key, of the
  keys macOS keeps and of the keys browsers keep from a page
  (`ui::options::tests::no_two_default_commands_share_a_shortcut`,
  `no_default_shortcut_is_a_macos_system_key`,
  `ui::menus::tests::browser_alternatives_are_free`); it can be changed
  in Options > Customization > Commands like any other.

Tests: `ui::command_search::tests`.

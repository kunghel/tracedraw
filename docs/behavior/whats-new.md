# What's new

- Help > What's New opens the Welcome Screen's What's New page (three
  news items, `welcome.news_*` in the language files).
- Help > Mark New Features (a check item, off at start, not kept
  between sessions) marks what is new in this version in the menus: a
  small accent dot at the end of each new row, left of the arrow of a
  submenu leading to one, and over the title of a menu leading to one.
  The rows are listed in `NEW_IN_THIS_VERSION` (`ui/menus.rs`); each
  version replaces the list with its own.
- In this version: Window > New Window, Window > Dockers > Effects,
  Object > Clip to Frame > Opacity Mask and Invert Mask, Help > Updates.

## Checks

- `ui::menus::tests::menu_commands_are_listed_and_run_by_key` (every
  marked row is in the menus; marks show only while the item is on)

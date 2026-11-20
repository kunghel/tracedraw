# Plugins (#1802)

A plugin is a JavaScript script, written against the object model of the
Scripts panel (`behavior/scripting.md`), that adds commands to
Tools > Plugins (`plugins.rs`, `scripting::plugin_info`,
`scripting::run_plugin`). It defines one global object:

```js
const TraceDrawPlugin = {
  name: 'Boxes',                 // required
  description: 'Packaging boxes.', // optional
  version: '1.0',                // optional
  commands: [                    // or one run() of its own
    { name: 'Square box', run() { ActiveLayer.CreateRectangle(20, 20, 50, 50); } },
  ],
};
```

- **Installing**: Tools > Plugins > Install Plugin (a `.js` file) or the
  Scripts panel's **Install as Plugin**. The script is checked first: it
  runs against an empty drawing with the limits of AI Drawing scripts and
  nothing it does is kept, then `TraceDrawPlugin` must have a name and at
  least one command with a `run()` function (a command without a name is
  called "Command n"; a plugin with a `run()` and no `commands` is one
  command named after the plugin). It is kept in the preferences
  (`plugins`) with its source; a plugin of the same name is replaced and
  keeps whether it was on.
- **The menu**: Tools > Plugins lists each plugin that is on, a submenu of
  its commands (a lone command as an item), then Plugin Manager, New
  Plugin and Install Plugin.
- **Keyboard shortcuts**: each command is a menu command with a key of
  its own (`plugins::command_key`, `menu.plugin_command.<plugin>|<command>`),
  so Options > Customization lists it under Tools by its name and gives it
  a shortcut like any other; the menu row shows it (#3519).
- **Running a command**: the plugin's script runs as the user's own
  script, then its command's `run()`; the changes are one undo step named
  "Plugin". What it prints goes to the Scripts panel's output. An error
  keeps what ran before it (as a script does) and goes to the status bar.
- **Plugin Manager**: each plugin with its version, description and
  commands; On (off keeps it installed and takes its commands out of the
  menu), Edit (its source into the Scripts panel, JavaScript chosen) and
  Remove; Install Plugin and New Plugin.
- **New Plugin**: an example plugin of two commands in the Scripts panel
  (`plugins::TEMPLATE`).

Tests: `plugins::tests`.

## Window buttons (#2574)

Options > General, "Minimize, Maximize and Close buttons on the menu bar"
(off by default; desktop only, `settings.window_buttons`) puts the three
buttons at the right end of the menu bar, 40 points each
(`window_buttons.rs`), for a window without the desktop's title bar.
Minimize and Maximize/Restore send the window commands; Close does what
File > Exit does, asking about drawings not saved. The empty part of the
menu bar between the last title and the buttons acts as a title bar: a
drag moves the window, a double click maximizes or restores it.

Tests: `window_buttons::tests`.

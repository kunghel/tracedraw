# Plugins

A plugin adds commands of its own to [[menu.tools > menu.tools.plugins]]:
a packaging box, a cutting layout, a shape you draw often, or a feature
you miss. It is a script, written in JavaScript like the scripts of the
[[docker.scripts]] panel, that you install once and then run from the
menu. Plugins can be shared as `.js` files.

## To install a plugin {#install}

1. Choose [[menu.tools > menu.tools.plugins > menu.tools.plugin_install]].
2. Pick the plugin's `.js` file. Its commands appear under
   [[menu.tools > menu.tools.plugins]], each plugin in a submenu of its
   own (a plugin of one command shows that command alone).

Installing a plugin with the name of one already installed replaces it.

A plugin's commands can have keyboard shortcuts: find them by name in
[[menu.tools > menu.tools.options > menu.tools.options_customization]] and give
them keys like any other command.

## To write a plugin {#write}

1. Choose [[menu.tools > menu.tools.plugins > menu.tools.plugin_new]]. The
   [[docker.scripts]] panel opens with an example plugin of two commands.
2. Change its name, description and commands. Each command has a `name`
   and a `run()` function, which works with the drawing as any script
   does (see [Scripts and automation](scripts.md)).
3. Click [[docker.install_plugin]].

A plugin defines one object, `TraceDrawPlugin`:

```js
const TraceDrawPlugin = {
  name: 'Boxes',
  description: 'Packaging boxes.',
  version: '1.0',
  commands: [
    { name: 'Square box', run() { ActiveLayer.CreateRectangle(20, 20, 50, 50); } },
  ],
};
```

Each command's changes are one step to undo. What it prints goes to the
[[docker.scripts]] panel's output. When a command stops with an error,
the status bar says why and what it did before the error is kept.

## The Plugin Manager {#manager}

[[menu.tools > menu.tools.plugins > menu.tools.plugin_manager]] lists the
installed plugins with their descriptions and commands.

- **[[dialog.plugins_on]]** Turned off, a plugin stays installed but its
  commands leave the menu.
- **[[dialog.plugins_edit]]** Opens the plugin in the [[docker.scripts]]
  panel, to change it and install it again.
- **[[dialog.plugins_remove]]** Uninstalls it.

Plugins are kept with your preferences.

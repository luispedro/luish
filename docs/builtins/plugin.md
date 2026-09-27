# `plugin`

```text
plugin load name|path...
plugin list-loaded
plugin list-available
plugin unload name...
```

Load and unload plugins.

`plugin load` loads each plugin: `name` is the file `name.rhai`, or else
the directory `name`, in `$XDG_CONFIG_HOME/luish/plugins` (by default
`~/.config/luish/plugins`), and an argument that contains a `/` is a path.
A directory plugin runs its `extension.rhai` and then its `rc.lsh`. Loading a
plugin again reloads it. The exit status is 1 if a plugin can't be loaded.
`plugin list-loaded` prints the names of the loaded plugins, and
`plugin list-available` the names of the plugins in the plugin directory,
which `plugin load` can load by name. `plugin unload` removes plugins and
their hooks. With `--no-plugins`, `plugin load` does
nothing.

`plugin` is a built-in only in interactive shells (and their subshells).
Anywhere, `__luish_internal plugin` does the same. See the plugins page of
the documentation for how to write plugins.

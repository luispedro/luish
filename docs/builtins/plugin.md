# `plugin`

```text
plugin load name|source/name|path...
plugin list-loaded
plugin list-available
plugin unload name...
plugin sync
plugin update [source...]
```

Load, unload and fetch plugins.

`plugin load` loads each plugin: `name` is the source called `name` in
`config.toml`'s `plugins.available` (or `std`), else the first of the Rhai
file `name.rhai`, the shell file `name.lsh` and the directory `name` in
`$XDG_CONFIG_HOME/luish/plugins` (by default `~/.config/luish/plugins`);
`source/name` is the plugin `name` of the collection `source`; and any other
argument that contains a `/` is a path. A directory plugin runs its
`init.lsh`, then its `extension.rhai`, then (in interactive shells) its
`rc.lsh` and `post-rc.lsh`; during the startup files of `rc.d`,
`post-rc.lsh` waits for their end. The dependencies that a plugin's
`plugin.toml` lists are loaded first, unless they are already loaded.
Loading a plugin again reloads it. The exit status is 1 if a plugin can't
be loaded. `plugin list-loaded` prints the names of the loaded plugins, and
`plugin list-available` the names of the plugins that `plugin load` can
load by name: those in the plugin directory, and those of the sources in
`config.toml` that are installed. `plugin unload` removes plugins and their
hooks.

`plugin sync` fetches, with git, the sources of the plugins in
`config.toml`'s `plugins.enabled` (and of their dependencies) and of its
`plugins.available`, and records the commit of each in `plugins.lock`, next
to `config.toml`. A source that is already in `plugins.lock` stays at its
commit. `plugin update` fetches the newest commit of each git source, or of
the sources named, and updates `plugins.lock`. Both print the sources whose
commits changed. Interactive shells load the plugins in `plugins.enabled` at
startup, from the commits in `plugins.lock`, without running git. With
`--no-plugins`, `plugin load`, `plugin sync` and `plugin update` do nothing.

`plugin` is a built-in only in interactive shells (and their subshells).
Anywhere, `__luish_internal plugin` does the same. See the plugins page of
the documentation for how to write plugins and list them in `config.toml`.

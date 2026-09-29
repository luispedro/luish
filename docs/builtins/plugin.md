# `plugin`

```text
plugin load name|source/name|path...
plugin list-loaded
plugin list-available
plugin unload name...
plugin add [-y] plugin [name]
plugin sync [-q]
plugin update [-q] [source...]
plugin check
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
load by name and that aren't loaded: those in the plugin directory, and
those of the sources in `config.toml` that are installed. `plugin unload`
removes plugins and their hooks; each is named as `plugin list-loaded`
shows it, or as `plugin load` was given it (such as `std/NAME`).

`plugin add` adds a plugin to `config.toml` and installs it. `plugin` is a
GitHub repository (`owner/repo`, `gh:owner/repo`, or a URL such as
`https://github.com/owner/repo`, where `/tree/branch/dir` gives a branch
and a directory), another git URL (`https://...`, `git@host:path`), a
local path (one that exists, or starts with `/`, `./`, `../` or `~`, or a
`file:///path` URL, which is a git source if `path` is a git repository),
or a plugin of a source that `config.toml` names (`source/name`, such as
`std/name`) or of the plugin directory (`name`). A git source is fetched
first, into a temporary directory, to see what it holds. It is added to
`plugins.enabled`, as `name = { gh = "owner/repo" }` for a source (a
collection of more than one plugin goes to `plugins.available`, to load
its plugins with `plugin load source/name`), keeping the file's comments.
The name is the repository's, file's or directory's, unless `name` is
given. It prints the line it adds and asks before changing the file
(reading the answer from standard input), unless `-y` (or `--yes`) is
given; then it runs `plugin sync` and loads the plugin. The exit status is
1 if it was declined, if the name is already used, or if the source can't
be fetched (then `config.toml` isn't changed).

`plugin sync` fetches, with git, the sources of the plugins in
`config.toml`'s `plugins.enabled` (and of their dependencies) and of its
`plugins.available`, and records the commit of each in `plugins.lock`, next
to `config.toml`. A source that is already in `plugins.lock` stays at its
commit. `plugin update` fetches the newest commit of each git source, or of
the sources named, and updates `plugins.lock`. Both print the sources they
fetch, those whose commits changed, and the number of git sources locked
and the plugins enabled; with `-q` (or `--quiet`) they print only errors.
`plugin check` asks each git source (with `git ls-remote`) for its newest
commit and prints those newer than the locked one, and the sources that
aren't installed, without fetching or changing anything; its exit status is
0 unless a source couldn't be checked. Interactive shells load the plugins
in `plugins.enabled` at startup, from the commits in `plugins.lock`, without
running git. With `--no-plugins`, `plugin load`, `plugin add`, `plugin
sync`, `plugin update` and `plugin check` do nothing.

`plugin` is a built-in only in interactive shells (and their subshells).
Anywhere, `__luish_internal plugin` does the same. See the plugins page of
the documentation for how to write plugins and list them in `config.toml`.

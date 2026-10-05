# `plugin`

```text
plugin load name|source/path|path [option=value...]...
plugin load -c code name
plugin list-loaded
plugin list-available [-a]
plugin unload name...
plugin run file|-c code [arg...]
plugin add [-y] plugin [name]
plugin sync [-q]
plugin update [-q] [source...]
plugin check [-q]
```

Load, unload and fetch plugins.

`plugin load` loads each plugin: `name` is the source called `name` in
`config.toml`'s `plugins.available` (or `std`), else the first of the Rhai
file `name.rhai`, the shell file `name.lsh` and the directory `name` in
`$XDG_CONFIG_HOME/luish/plugins` (by default `~/.config/luish/plugins`);
`source/path` is the plugin at `path` in the collection `source` (`name`,
or `sub/name` in its sub-collection `sub`); and any other argument that
contains a `/` is a path. A plugin of a collection is loaded under the name
`source/path`, others under their file's or directory's name. A directory
plugin runs its `init.lsh`, then its `extension.rhai`, then (in interactive
shells) its
`rc.lsh` and `post-rc.lsh`; during the startup files of `rc.d`,
`post-rc.lsh` waits for their end. The dependencies that a plugin's
`plugin.toml` lists are loaded first, unless they are already loaded. A
plugin whose `plugin.toml` has `luish-version = "X.Y"` isn't loaded by an
older luish, nor are the plugins that depend on it.
The `option=value` arguments after a plugin are its options, which its
`plugin.toml` declares (see the plugins page of the documentation); a
plugin that is already loaded, as a dependency or otherwise, must be given
the same options, or unloaded first.
Loading a plugin again reloads it. The exit status is 1 if a plugin can't
be loaded. At the prompt of an interactive shell with a terminal for
output, `plugin load` and `plugin unload` say what they did (`Loaded NAME`,
`Reloaded NAME`, `Unloaded NAME`, marking dependencies); they are silent in
startup files, functions and command substitutions.

`plugin load -c` loads the Rhai code `code` as the plugin `name` (which
can't contain a `/`), with the current directory as its directory. It
replaces a plugin of that name loaded with `-c`, but not one loaded from
a file or directory, which must be unloaded first (the exit status is
then 1); loading a plugin of that name from a file replaces it, with a
warning.

`plugin list-loaded` prints the names of the loaded plugins (on a terminal,
`No plugins loaded` if there are none), and
`plugin list-available` the names of the plugins that `plugin load` can
load by name and that aren't loaded: those in the plugin directory, and
those of the sources in `config.toml` that are installed, except the
libraries (plugins for other plugins to use, whose `plugin.toml` says
`library = true`), which `-a` (or `--all`) adds. `plugin unload`
removes plugins and their hooks; each is named as `plugin list-loaded`
shows it, or as `plugin load` was given it (such as `std/NAME`).

`plugin run` runs the Rhai file `file`, or the Rhai code `code`, once,
without loading a plugin: it can't register hooks, completers or
commands (that is an error: use `plugin load -c`), and it doesn't unload
or reload any plugin. The Rhai variable `argv` is an array of `file` (or
`-c`) and the `arg`s. The exit status is the value of the last statement
(or of `return`), if it is an integer (modulo 256) or a boolean (true is
0, false 1), and otherwise 0; it is 1 if the code can't be read or
fails. `import` is relative to `file`, or with `-c`, to the current
directory, and the modules are read again for each run. `--no-plugins`
doesn't stop it.

`plugin add` adds a plugin to `config.toml` and installs it. `plugin` is a
GitHub repository (`owner/repo`, `gh:owner/repo`, or a URL such as
`https://github.com/owner/repo`, where `/tree/branch/dir` gives a branch
and a directory), another git URL (`https://...`, `git@host:path`), a
local path (one that exists, or starts with `/`, `./`, `../` or `~`, or a
`file:///path` URL, which is a git source if `path` is a git repository),
or a plugin of a source that `config.toml` names (`source/path`, such as
`std/name`) or of the plugin directory (`name`). A git source is fetched
first, into a temporary directory, to see what it holds: a source that is
neither a plugin nor has any (a `.rhai` or `.lsh` file, or a directory with
one of a plugin's files) is not added. It is added to
`plugins.enabled`, as `name = { gh = "owner/repo" }` for a source (a
collection of more than one plugin goes to `plugins.available`, to load
its plugins with `plugin load source/path`), keeping the file's comments.
The name is the repository's, file's or directory's, unless `name` is
given. It prints the line it adds and asks before changing the file
(reading the answer from standard input), unless `-y` (or `--yes`) is
given; then it says what it added, runs `plugin sync` and loads the plugin.
The exit status is 1 if it was declined, if the name is already used, or
if the source can't be fetched (then `config.toml` isn't changed).

`plugin sync` fetches, with git, the sources of the plugins in
`config.toml`'s `plugins.enabled` (and of their dependencies) and of its
`plugins.available`, and records the commit of each in `plugins.lock`, next
to `config.toml`. A source that is already in `plugins.lock` stays at its
commit. `plugin update` fetches the newest commit of each git source, or of
the sources named, and updates `plugins.lock`. Both print the sources they
fetch, those whose commits changed (`Updating SOURCE OLD..NEW`, with a link
to the changes for a GitHub source), with `plugin update` also those that
didn't, each with the enabled plugins that come from it, and the number of
git sources locked and the plugins enabled; with `-q` (or `--quiet`) they
print only errors. `plugin check` asks each git source (with `git
ls-remote`) for its newest commit and says, for each, whether it is up to
date, pinned to a commit (`rev`, which has nothing newer) or has a newer
commit than the locked one, and which sources aren't installed, without
fetching or changing anything; with `-q` it prints only the sources that can
be updated and those that aren't installed. Its exit status is 0 unless a
source couldn't be checked. Interactive shells load the plugins in
`plugins.enabled` at startup, from the commits in `plugins.lock`, without
running git. With `--no-plugins`, `plugin load`, `plugin add`, `plugin
sync`, `plugin update` and `plugin check` do nothing.

`plugin` is a built-in only in interactive shells (and their subshells).
Anywhere, `__luish_internal plugin` does the same.

On a terminal (unless `$NO_COLOR` is set), what these commands print is
coloured with the styles `plugin.name`, `plugin.ok`, `plugin.update`,
`plugin.warn`, `plugin.error` and `plugin.dim` of the colour scheme in use
(`style`).

See the plugins page of the documentation for how to write plugins and list
them in `config.toml`, and the extensions page for their Rhai code.

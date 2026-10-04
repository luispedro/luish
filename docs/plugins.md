# Plugins

A plugin is a package of configuration and code that luish loads: options, aliases and key bindings (in
`plugin.toml`), shell files that run in the shell as with `.`, and optionally an extension. You install plugins, list
them in `config.toml`, and load them with `plugin load`; this page is about that, and about the files that make up a
plugin.

An **extension** is the part of a plugin written in [Rhai](https://rhai.rs), a small embedded language, which runs
inside the shell: hooks (on directory changes, before each prompt), Tab completion, and commands. It is described in
[](extensions.md). Most plugins don't need one: a plugin written only in shell and `plugin.toml` doesn't start Rhai
at all.

## Installing plugins with `config.toml`

The simplest way to load plugins in every interactive shell is to list them in
`config.toml`. The `[plugins.enabled]` table lists plugins that will be loaded
when luish starts, while `plugins.available` lists plugins that can be loaded
by name.

```toml
[plugins.available]
smarty-prompt = { gh = "luispedro/smarty-prompt", branch = "main" }
work = { path = "~/src/work-plugins" }

[plugins.enabled]
std.bash-completion = "*"      # the plugin bash-completion of the collection std
"std/completion" = "*"         # the plugin completion of std, written differently
smarty-prompt = "*"            # a source that is one plugin
work.proxy = "*"               # the plugin proxy of ~/src/work-plugins
greet = "*"                    # ~/.config/luish/plugins/greet.rhai, greet.lsh or greet/
z = { gh = "bob/luish-z" }     # a source of its own
```

| Key | Meaning |
|---|---|
| `gh = "OWNER/REPO"` | A repository on GitHub, the same as `git = "https://github.com/OWNER/REPO.git"` |
| `git = "URL"` | A git repository: any URL that `git fetch` accepts, including `file://` and SSH ones |
| `path = "DIR"` | A local file or directory, used where it is. A leading `~` is the home directory, and a relative path is relative to the file that has it |
| `branch`, `tag`, `rev` | At most one, for `gh` and `git`: which commit to use. `rev` is a full commit hash. By default, the repository's default branch (its `HEAD`) |
| `subdir = "DIR"` | Where in the repository (or `path`) the plugin or collection is |
| `plugin = "NAME"` | In `plugins.enabled`: which plugin of a collection (`SUB/NAME` in a sub-collection). By default the one with the entry's name, or the only one |

An entry can also be a table, to give the plugin options (see [Plugin options](#plugin-options-plugin-options)),
with `version = "*"` (which can be left out) and `options`, also beside `gh`, `git` or `path`. An empty table is the
same as `"*"`:

```toml
[plugins.enabled]
std.completion = { }
my-plugin = { version = "*", options = { greeting = "hi", level = 2, verbose = true } }
work.proxy = { options = { host = "proxy.example.com" } }
z = { gh = "bob/luish-z", options = { max = 500 } }
```

### Names

A source is either one plugin or a collection of plugins: its `.rhai` and `.lsh` files and its directories that are
plugins. A collection's other directories are sub-collections, which hold more plugins (directories that hold none,
such as a repository's `docs`, are ignored). A plugin of a collection is written `SOURCE/PATH`, where `PATH` is its
name, or `SUB/NAME` in a sub-collection, and it is loaded under that name:

```toml
[plugins.available]
extra = { gh = "luispedro/luish-extra" }

[plugins.enabled]
extra.complete.all = "*"           # the plugin all of extra's sub-collection complete
"extra/complete/bio" = "*"         # the same with / (quoted, as TOML's bare keys can't have it)
```

`plugin list-loaded` shows `extra/complete/all`, `plugin unload extra/complete/all` unloads it, and other plugins
import its modules as `@extra/complete/all/MODULE`. std's plugins are `std/completion`, `std/bash-completion` and `std/notify`.
Other plugins are named after the entry (a source that is one plugin, or `NAME = { gh = ... }`), or after their file
or directory (those of the plugin directory, or loaded by path).

### Adding plugins: `plugin add`

`plugin add` adds a plugin to `config.toml` for you, then runs `plugin sync`
and loads it. It takes a GitHub repository or URL, another git URL, a local
path (or `file://` URL), or a plugin of a source that `config.toml` names. It
fetches a git source first, to check it, and asks before changing the file
(`-y` doesn't ask):

```console
$ plugin add https://github.com/bob/luish-z
Fetching bob/luish-z
Adding to [plugins.enabled] in /home/me/.config/luish/config.toml:
    luish-z = { gh = "bob/luish-z" }
and running plugin sync. Continue? [y/N] y
Fetching bob/luish-z
Locking bob/luish-z at 4b1e2a9
1 git source locked, 1 plugin enabled: luish-z
$ plugin add std/bash-completion
```

A second argument names the plugin (`plugin add bob/luish-z z`). The line
goes at the end of the `[plugins.enabled]` table (a collection of several
plugins goes in `[plugins.available]`, and then `plugin add SOURCE/NAME` enables
one of them), and the rest of the file, with its comments, is left as
it is.

### Fetching plugins: `plugin sync` and `plugins.lock`

Plugins need to be fetch explicitly, by running `plugin sync`.

```console
$ plugin sync
Fetching std
Fetching smarty-prompt
Locking std at 15e39bb (std/completion)
Locking smarty-prompt at 8a1c0de (smarty-prompt)
2 git sources locked, 2 plugins enabled: std/completion, smarty-prompt
```

The first time `plugin sync` runs, it will create a `plugins.lock` file, which
records the commit of each git source that was fetched. You can update plugins
later with `plugin update`, which fetches the newest commit of the source
and updates `plugins.lock`. Specify a plugin name to update only that one:

```console
$ plugin update smarty-prompt
Fetching smarty-prompt
Up to date std at 15e39bb (std/completion)
Updating smarty-prompt 8a1c0de..3f00c2d (smarty-prompt)
   https://github.com/luispedro/smarty-prompt/compare/8a1c0de...3f00c2d
2 git sources locked (1 updated), 2 plugins enabled: std/completion, smarty-prompt
```

`plugin check` asks each git source (with `git ls-remote`) for its newest
commit and reports, for each, whether it is up to date or has newer commits
(and which of your plugins come from it), but it does not change anything.

```console
$ plugin check
Up to date: std at 15e39bb (std/completion)
Update available: smarty-prompt 8a1c0de..3f00c2d (smarty-prompt)
   https://github.com/luispedro/smarty-prompt/compare/8a1c0de...3f00c2d
1 of 2 git sources can be updated (run plugin update)
```

A source pinned with `rev` never has anything newer. The exit status is 0
unless a source couldn't be checked. All three commands take `-q` (or
`--quiet`): `plugin sync` and `plugin update` then print only errors, and
`plugin check` only the sources that can be updated or aren't installed.

On a terminal, the output is coloured with the colour scheme in use: the
styles `plugin.name` (plugin and source names), `plugin.ok`, `plugin.update`,
`plugin.warn`, `plugin.error` and `plugin.dim` (see [](colour-schemes.md)).
`$NO_COLOR` turns the colours off.

luish runs `git` to fetch, so git's own settings apply (credentials, SSH keys, proxies).

## Standard plugins

Luish includes a standard collection of plugins, called `std`. It is not
enabled by default, but you can enable it in `config.toml`:

```toml
[plugins.enabled]
std.completion = "*"        # common commands, and git
std.bash-completion = "*"
```

The `std` library is tied to the version of luish, so it is not affected by
`plugin update`.

- **`completion`** (a directory) completes about 230 common commands: their
  options, with their descriptions, the values of the options (`ls --sort=`,
  `cp -t DIR`, `tar --format=`, `dd conv=`, ...), their subcommands (`systemctl
  restart`, `cargo build`, `apt install`) and their other arguments. It
  includes:
  - coreutils (`ls`, `cp`, `mv`, `rm`, `mkdir`, `tail`, `date`, `dd`, ...),
    grep, diff, tar (the files in the archive, for `tar -xf ARCHIVE`), make (the
    targets of the makefile, also with `-C DIR` and `-f FILE`), rsync, man (the
    pages, in the section given), ssh, scp and sftp (the hosts of
    `~/.ssh/config`, with the files it includes, and of `/etc/hosts`, also after
    `USER@`), and pkill, pgrep and killall (the running processes);
  - shells: luish itself (`luish -o` offers its options), sh, bash and zsh;
  - luish's own built-ins: their options (`typeset -`, `print -`, `ulimit -`,
    `[ -`, `set -o` ...), and for `style`, the names of styles, the words of
    their values and the colour schemes (`style -c <Tab>`);
  - find, fd, sed, awk, jq, rg (its file types), less, file, patch, tree, and
    compressors and archivers (`gunzip` offers `.gz` files, `unzip ARCHIVE` the
    files in the archive);
  - systemctl and journalctl (the units, also with `--user`), loginctl, ps (its
    fields), top, htop, lsof, strace, mount, umount (the mount points), lsblk,
    dmesg, free and tmux (its sessions);
  - curl, wget, ip (the interfaces), ss, ping, dig, ssh-add, ssh-keygen,
    ssh-copy-id, gpg (the keys of the keyring) and openssl;
  - cargo (its commands, also those of plugins, and the packages, binaries,
    examples, features, profiles and dependencies of `Cargo.toml`), rustup (the
    toolchains, targets and components), go, gcc and clang, cmake, meson,
    ninja (the targets), gdb, python (`-m` modules), sqlite3, vim, nvim, nano
    and emacs;
  - pip and uv (the installed packages, for `pip uninstall`), conda and mamba
    (the environments, and the packages of one), pixi (the tasks, environments
    and dependencies of the workspace), npm, npx, yarn and pnpm (the scripts and
    dependencies of `package.json`);
  - apt, apt-get, apt-cache, apt-mark, dpkg, dnf, yum, rpm, pacman (`pacman -S`,
    `-Q`, `-R` ... each with its own options), zypper, apk, brew, snap and
    flatpak: the installed packages, and those that can be installed once the
    word has a letter (except with dnf, yum and zypper, whose lists are slow);
  - git: its commands, with their descriptions, and aliases, the options of
    each command, and its arguments: branches, tags, the end of a range
    (`main..`), remotes, stashes, worktrees, and the files the command can act
    on (modified and untracked files for `git add`, staged ones for `git
    restore --staged`, the tracked ones after a revision for `git diff`, ...);
  - programs that complete themselves: those built with Cobra (gh, glab,
    docker, podman, kubectl, helm, minikube, kind, hugo, rclone, ...) and nix.
    For another program built with Cobra, `complete-cobra PROG...` (in
    a file of `rc.d`, for instance) adds it; without arguments, it lists them.
    The same for Python programs built with Click (black, flask, uvicorn,
    pip-compile, hatch, mkdocs, celery, llm ...) and `complete-click PROG...`.
    Each Tab runs the program, so only register programs built with Click.

  The options of cargo's subcommands, rustup, uv, pixi and openssl's commands
  are read from their `-h`, so they follow the installed version. Short
  options can be combined: `ls -la` offers the options that can follow. Each
  of its modules is compiled on the first Tab
  for one of its commands (a few milliseconds), so loading it costs little.
  Other plugins can complete their commands with its engine (see
  [Reusing std's completion engine](extensions.md#reusing-stds-completion-engine)).
- **`bash-completion`** uses
  [bash-completion](https://github.com/scop/bash-completion), which completes
  the arguments of about a thousand commands, and for which many programs
  install completion files. This requires bash-completion to be installed and
  runs bash to ask for the completions, and it does not provide descriptions.
- **`notify`** asks the terminal for a desktop notification (OSC 777) when a
  command that took at least `$LUISH_NOTIFY_AFTER` seconds (10 by default)
  ends, with the command and whether it failed. foot, WezTerm, Ghostty,
  rxvt-unicode (with its notify extension) and VTE terminals that carry the
  patch for it (GNOME Terminal in Fedora, for one) show them; some, such as
  foot, only while the window isn't focused. kitty and Ghostty can do this
  by themselves (see [Terminal integration](usage.md#terminal-integration)).

## More completion: luish-extra

[luish-extra](https://github.com/luispedro/luish-extra) is a separate collection of completion plugins (in its
sub-collection `complete`) for the programs that `std.completion` leaves out. It completes about 270 more commands,
in five plugins that can each be enabled on its own:

- **`bio`**: bioinformatics tools, such as samtools, bcftools, bedtools, bwa, bowtie2, minimap2, STAR, BLAST+, diamond,
  fastp, cutadapt, assemblers (spades, megahit, flye), and the tools of metagenomics (kraken2, metaphlan, checkm,
  gtdbtk, ...);
- **`science`**: workflows (snakemake, nextflow, nf-core), writing (pandoc, quarto, latexmk), jupyter, R, mlr,
  duckdb, parallel, ...;
- **`gui`**: desktop programs (firefox, libreoffice, code, mpv, inkscape, gimp, ...) and desktop tools (xrandr,
  gsettings, xdotool, ...);
- **`dev`**: Python tooling (pytest, ruff, poetry, twine, ...) and command-line utilities;
- **`system`**: borg (its repositories and archives) and fusermount.

As with `std.completion`, completion knows each program's options and the kind of value each one takes, and often
reads the files you are working on: `samtools sort -O` offers `BAM`, `CRAM` and `SAM`, `samtools view in.bam` the
reference names in its header, `snakemake` the rules of the `Snakefile`, `pytest tests/test_x.py::` the tests in that
file, `ruff check --select F4` the rule codes `F401`, `F403`, ..., and `libreoffice --convert-to` the formats the
installed LibreOffice writes. Each module is compiled the first time Tab is pressed for one of its commands, so
enabling all of them costs little.

To install it, add the repository as a source (under any name, here `extra`), then enable `all`, which loads the
five plugins:

```console
$ plugin add luispedro/luish-extra extra
$ plugin add extra/complete/all
```

Or, by hand, in `config.toml` (then run `plugin sync`):

```toml
[plugins.available]
extra = { gh = "luispedro/luish-extra" }

[plugins.enabled]
extra.complete.all = "*"        # or only some: extra.complete.bio = "*", ...
```

The plugins depend on `std.completion`, whose engine they use, so luish loads it too.

Adding the whole repository needs luish 0.4.0 or newer, which has sub-collections; with luish 0.3.0, add its
`complete` directory instead (see its README). Unlike `std`, it is not tied to the version of luish, so `plugin update`
updates it. Its [README](https://github.com/luispedro/luish-extra#readme) lists every command it completes.

## Plugin formats

A plugin is one of:

1. a directory of files with special names, in shell and Rhai (see [below](#plugin-directories)), not all of which
   need to be there;
2. a single Rhai file, `NAME.rhai`: a plugin that is only an extension, the same as a directory that holds only that
   file, as `extension.rhai`;
3. a single shell file, `NAME.lsh`, which is the same as a directory that holds only that file, as `init.lsh`.

`plugin load NAME` finds them in `~/.config/luish/plugins` (see `help plugin`).

## Plugin directories

A plugin directory can have multiple files which luish uses the following ways:

| File | Meaning |
|---|---|
| `plugin.toml` | Parsed first, to load dependencies, set options, aliases and key bindings, and define colour schemes. |
| `init.lsh` | Next: sourced in the current shell (as with `.`). |
| `extension.rhai` | Next: the plugin's extension, which is loaded and run (see [](extensions.md)). |
| `rc.lsh` | Next, as with `.`, but only in interactive shells (and their subshells). |
| `post-rc.lsh` | Last, as `rc.lsh`, but after the startup files in `rc.d` (see [below](#after-the-startup-files-post-rc)) |
| `prompt-vars.lsh` | Before each prompt, to set variables for `PS1` (see [below](#variables-for-the-prompt-prompt-varslsh)) |

A directory needs at least one of them. Other directories (such as a repository's `docs` or `src`) are not plugins:
in a collection, they are sub-collections if they hold plugins, and are ignored otherwise.

While `init.lsh`, `extension.rhai`, `rc.lsh`, `post-rc.lsh` and `prompt-vars.lsh` run,
`LUISH_PLUGIN_DIR` is the plugin's directory (an absolute path),
`LUISH_PLUGIN_NAME` its name, and `LUISH_PLUGIN_OPTIONS` an associative array of
its options (see [below](#plugin-options-plugin-options)). Afterwards they get
back the values they had before.

`plugin unload` removes what the extension registered (hooks, completers and commands), but it can't undo what the
shell files did (aliases, functions, variables).

## Dependencies, options, aliases and key bindings: `plugin.toml`

A directory plugin can have a file `plugin.toml`, which can list dependencies:

```toml
# ~/src/work-plugins/proxy/plugin.toml
description = "Sets the proxy variables for the office network."

[dependencies]
netutils = "*"                        # the plugin netutils of the same collection
"/lib/util" = "*"                     # a plugin of the same source, from its top
std.completion = "*"                  # a plugin of a source that luish knows
fzf = { gh = "bob/luish-fzf" }        # a source of its own
```


`plugin.toml` can also set options and define aliases and key bindings, in
`[options]`, `[alias]` and `[bindkey]` tables as in `config.toml` (see
[Settings in config.toml](usage.md#settings-in-configtoml)). A plugin can so
package a set of options that go together; they override those in
`config.toml`, which is read first.

```toml
[options]
autosuggest = true
[options.history]
share = true
ignore_space = true

[alias]
gs = "git status"
[alias.global]
G = "| grep"
[alias.suffix]
pdf = "evince"

[bindkey]
"Ctrl-X Ctrl-G" = "undo"
```

### Plugin options: `[plugin-options]`

A plugin can take options, which it declares in the `[plugin-options]` table of its `plugin.toml`, each with its
type (`str`, `int` or `boolean`) and either a default or `required = true`:

```toml
[plugin-options]
greeting = { type = "str", default = "hello" }
level = { type = "int", default = 0 }
verbose = { type = "boolean", default = false }
host = { type = "str", required = true }
user = { type = "str" }                   # neither: unset unless given
```

(The table `[options]` is for the shell's settings, above.) The options are given in `plugins.enabled` (see
[above](#installing-plugins-with-configtoml)), in the `dependencies` of another plugin, in the same way, or to
`plugin load` after the plugin, as `OPTION=VALUE`:

```toml
[dependencies]
proxy = { options = { host = "proxy.example.com", level = 2 } }
```

```console
$ plugin load work/proxy host=proxy.example.com level=2 verbose=true
```

An option the plugin doesn't declare, a value of the wrong type (in `plugin load`, an integer, or `true` or `false`
for a boolean) and a required option that isn't given are errors, and the plugin isn't loaded. The plugin's shell
files see its options in the associative array `LUISH_PLUGIN_OPTIONS`, with those not given set to their default (a
boolean is `true` or `false`, so it can be run as a command), and its extension through `sh::plugin_options()` (see
[](extensions.md)), which gives a map of strings, integers and booleans:

```sh
# init.lsh
greeting=${LUISH_PLUGIN_OPTIONS[greeting]}
if ${LUISH_PLUGIN_OPTIONS[verbose]}; then echo "proxy: ${LUISH_PLUGIN_OPTIONS[host]}"; fi
```

```rhai
// extension.rhai
let level = sh::plugin_options().level;
```

`LUISH_PLUGIN_OPTIONS` is set only while the plugin's files run (`init.lsh`, `rc.lsh`, `post-rc.lsh` and
`prompt-vars.lsh`), as `LUISH_PLUGIN_DIR` is. A function that the plugin defines and that runs later, at the
prompt, doesn't see it, nor does a command it runs (an associative array can't be exported). A plugin that needs its
options later copies them, in `init.lsh`, into a variable of its own:

```sh
# init.lsh
typeset -A _proxy_opts
for k in "${!LUISH_PLUGIN_OPTIONS[@]}"; do _proxy_opts[$k]=${LUISH_PLUGIN_OPTIONS[$k]}; done
proxy_on() { export http_proxy=http://${_proxy_opts[host]}; }
```

Its extension doesn't need to: `sh::plugin_options()` works at any time, also in hooks and commands.

A plugin is loaded once, so when several plugins depend on it (or `plugins.enabled` lists it too), they must give it
the same options, once the defaults are filled in: giving none is the same as giving every option its default. If
they don't, the plugin that asks for different ones fails to load, with an error that says how they differ. This also
holds for a plugin that is already loaded: to load it with other options, unload it first.

### Themes

A theme is a plugin whose `plugin.toml` defines colour schemes, in `[colorscheme.NAME]` tables as in `config.toml` (see
[Colour schemes](usage.md#colour-schemes)). Loading it only makes them available: the user chooses one with
`colorscheme` in the `[style]` table of `config.toml`, or with `style -c`, which a plugin can't do. A scheme can also
give the terminal's own colours, in a `[colorscheme.NAME.terminal]` table (see [The terminal's
colours](usage.md#the-terminals-colours)); whether they are set is the user's choice (`terminal-colors`), not the
plugin's. A plugin's `[style]` table sets defaults for the styles of its own names (such as a prompt's `git.branch`),
which any scheme, and the user, can override:

```toml
[colorscheme.solarized-dark]
inherits = "default-dark"
keyword = "#268bd2"
string = "#b58900"

[colorscheme.solarized-light]
inherits = "default-light"
keyword = "#268bd2"
string = "#b58900"

[style]
"git.branch" = "magenta"
```

The user then chooses the pair:

```toml
# config.toml
[style]
colorscheme = { dark = "solarized-dark", light = "solarized-light" }
```

See [](colour-schemes.md) for how to make the schemes.

### Libraries

A plugin can be meant for other plugins to use rather than for users to load: a set of Rhai modules that their
extensions import (see [Splitting an extension into modules](extensions.md#splitting-an-extension-into-modules-import)),
or shell functions that their shell files call. It says so in its `plugin.toml`:

```toml
description = "Helpers for my other plugins"
library = true
```

A library loads as any other plugin, usually as a dependency of the plugins that use it, but `plugin list-available`
and Tab after `plugin load` leave it out (`plugin list-available -a` lists it too), and it doesn't count when a
collection's only plugin is chosen: a source with one plugin and some libraries is that plugin. A library that is only
Rhai modules needs no other file than `plugin.toml`.

## After the startup files: `post-rc`

An interactive shell starts in this order:

1. the settings in `config.toml`;
2. the plugins that `config.toml` enables (each plugin's `init.lsh`, `extension.rhai`, the options, aliases, key
   bindings and styles in its `plugin.toml`, and `rc.lsh`);
3. the files in `rc.d`, which can load more plugins;
4. the `post-rc.lsh` of each plugin loaded so far, in the order they were loaded;
5. the `post-rc` hooks of their extensions, in the same order;
6. `rc.d/_uncached.lsh`, then `login.d` (in login shells), `$ENV` and `luishrc`.

So a plugin can read, in its `post-rc.lsh`, the variables that you set in `rc.d` to configure it, even though it was
loaded before `rc.d`:

```sh
# post-rc.lsh
case ${MYPROMPT_STYLE:-plain} in
    plain) PS1='$PWD\$ ' ;;
    # ...
esac
```

An extension does the same with a [`post-rc` hook](extensions.md#after-the-startup-files-post-rc).

A plugin loaded later (in `luishrc`, or at the prompt) runs its `post-rc.lsh` and `post-rc` hooks right after its
`rc.lsh`. As `rc.lsh`, they run only in interactive shells. What `post-rc.lsh` does is cached with `rc.d`, but the
`post-rc` hooks run in every shell, since extensions are loaded again in every shell.

## Customizing the prompt

Plugins can change the prompt (`PS1`; `PS2`, for the continuation lines of a command, is unchanged) in two ways:

- **`prompt-vars`** (start here): a plugin computes values, such as the git branch, and gives them to `PS1` as
  variables, from a shell file, `prompt-vars.lsh`, or from its extension (a
  [`prompt-vars` hook](extensions.md#variables-for-the-prompt-prompt-vars)).
- **`prompt-rewrite`** (advanced): an extension writes the whole prompt itself, and can replace what other plugins
  did (see [Rewriting the prompt](extensions.md#rewriting-the-prompt-prompt-rewrite)).

### Variables for the prompt: `prompt-vars.lsh`

A directory plugin's `prompt-vars.lsh` runs before each prompt. Every variable it sets is available to `PS1`:

```sh
# ~/.config/luish/plugins/branch/prompt-vars.lsh
git_branch=$(git branch --show-current 2>/dev/null)
[ -n "$git_branch" ] && git_branch=" ($git_branch)"
```

```sh
# ~/.config/luish/luishrc
plugin load branch
PS1='$PWD$git_branch\$ '
```

Before each prompt, luish:

1. runs the `prompt-vars` hooks and `prompt-vars.lsh` files, plugin by plugin
   in the order the plugins were loaded.
2. expands `PS1` with those variables (parameter expansion, then `%` expansion under `prompt.percent`);
3. puts every variable they changed back as it was (or unsets it).

`prompt-vars.lsh` runs in the current shell, as with `.`, so it can use the
shell's functions and variables. Prompt variables don't leak into the shell, but
if multiple plugins are enabled, later ones can see the variables of the ones
loaded before them. Note, however, that other side effects of a
`prompt-vars.lsh` file (changing the directory, defining functions or aliases,
changing options) are not undone so be careful with them. Also, avoid slow
commands as these are run before every prompt.

## A personal plugin

A plugin of your own, in a git repository, is a good way to keep the same configuration on every machine; see
[](personal-plugin.md).

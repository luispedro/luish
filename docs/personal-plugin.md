# A personal plugin

The recommended way to keep your configuration is to package it as a plugin of
your own: your options, aliases, key bindings, functions and the other plugins
you use. You can put it in a public repository or a shared directory and then
every machine then needs only one line in `config.toml`, and gets the same
setup.

**Local changes stay local.** You can add machine-specific configuration to to
that machine's `~/.config/luish/rc.d/` and `login.d/`, which run _after_ the
plugin.

## An example

[luispedro/luish-personal-plugin](https://github.com/luispedro/luish-personal-plugin)
is the author's own configuration. It has two files:

```text
luish-personal-plugin/
├── plugin.toml   # dependencies, and options, aliases and key bindings
└── rc.lsh        # what plugin.toml can't hold: variables and functions
```

Its `plugin.toml` enables other plugins, as dependencies, and holds most of the
settings, in the same tables as `config.toml` (see [Settings in
config.toml](usage.md#settings-in-configtoml)):

```toml
description = "Luis Pedro Coelho's personal luish configuration."

[dependencies]
std.bash-completion = "*"
std.completion = "*"

[options]
autosuggest = true          # as zsh-autosuggestions

[options.cd]
auto = true                 # autocd

[options.pushd]
auto = true                 # auto_pushd

[options.prompt]
percent = true              # zsh's % sequences in PS1 (set in rc.lsh)

[options.history]
file = "~/.histfile"        # the same file as zsh, which luish reads and writes in zsh's format
share = true
ignore_space = true
reduce_blanks = true
save_no_dups = true

[alias]
ls = "ls --color=auto"
open = "xdg-open"

[alias.global]
"..." = "../.."
"...." = "../../.."
"....." = "../../../.."

[bindkey]
"^[?" = "insert-last-word"  # Alt-?, as well as the default Alt-.
```

Its `rc.lsh` has the rest, which needs shell. It runs in interactive shells only:

```sh
# An empty entry first, so that the current directory is looked in first, as in zsh.
CDPATH=:$HOME:$HOME/Sync/work:$HOME/work

# Without /, so that Ctrl-W deletes one component of a path.
WORDCHARS='*?_-.[]~=&;!#%^(){}<>'

# user@host : /path/, the exit status of the last command if it failed, and # for root.
PS1='%[user]@%[hostname] : %[fg:blue]%[dir]%[fg_off] %([status]..%[fg:red][%[status]]%[fg_off] )%([root].#.§)'

# field N prints the Nth field of each line.
field() {
    [ -n "$1" ] || { echo 'field N' >&2; return 1; }
    awk "{print \$$1}"
}
```

(The repository's `rc.lsh` also redefines `cd`, so that `cd FILE` goes to the file's directory.)

## Using it on a machine

Each machine enables the plugin in `~/.config/luish/config.toml`:

```toml
[plugins.enabled]
personal = { gh = "luispedro/luish-personal-plugin" }
```

and fetches it (and the `std` plugins it depends on) once, in an interactive luish:

```sh
plugin sync
```

A private repository works too, with a `git` source such as `git =
"git@github.com:me/luish-config.git"`: luish runs `git` to fetch, so your SSH
keys and credentials apply.

## Changing it

After you push a change, run `plugin update personal` on each machine to move
it to the newest commit (`plugin check` tells whether there is one).


```toml
[plugins.enabled]
personal = { path = "~/src/luish-personal-plugin" }
```

## What goes where

- **`plugin.toml`**: dependencies, options, aliases and key bindings. Prefer it
  to shell where it can express the setting.
- **`rc.lsh`**: variables, functions and the prompt, for interactive shells.
- **`init.lsh`**: what scripts need too (but most configuration is only for interactive use).
- **`post-rc.lsh`**: what must run after the machine's own `rc.d` (see
  [After the startup files](plugins.md#after-the-startup-files-post-rc)).

A plugin can also have an extension, code in Rhai, for hooks, prompt variables and completion
(see [](extensions.md)), but this is not needed for most configurations.


# Getting started

This page sets up a first configuration for luish: good options for history and directories, a few aliases and key
bindings, Tab completion for common commands and git from the standard plugins, and a prompt. It assumes that you
have used another shell, such as bash or zsh, and that luish is installed (see [](installation.md)).

## Trying it out

Start luish from the shell you use now; `exit` (or Ctrl-D) goes back to it:

```sh
luish
```

It needs no configuration to be usable. Out of the box, it has zsh's emacs keys for line editing (Ctrl-R to search
the history, Up and Down to go through the commands that start with what you typed), syntax highlighting, Tab
completion of commands, files and variables with a menu, and a history that is saved in
`~/.local/state/luish/history`. The rest of this page adds to that.

## Where the configuration goes

The configuration is in `~/.config/luish/` (or `$XDG_CONFIG_HOME/luish/`):

| File | What goes there |
|---|---|
| `config.toml` | Options, aliases, key bindings and the plugins to load |
| `rc.d/*.lsh` | Shell code for every interactive shell: variables and functions |
| `login.d/*.lsh` | The environment, for login shells: `PATH`, `EDITOR` and other exported variables |
| `luishrc` | Shell code that runs last, in every interactive shell: the prompt |

Only interactive shells read `config.toml`, `rc.d` and `luishrc`, so your configuration never changes how scripts or
`luish -c` behave. luish remembers the effect of `config.toml`, `rc.d` and `login.d`, and a new shell restores it
instead of reading them again, until one of them changes (see [Cached startup
files](usage.md#cached-startup-files)): you can edit them and open a new shell, with nothing to rebuild.

Create the directories:

```sh
mkdir -p ~/.config/luish/rc.d ~/.config/luish/login.d
```

## Options, aliases and plugins: `config.toml`

Put this in `~/.config/luish/config.toml`:

```toml
# Suggest the rest of the line from the history, in grey; Right or End accepts it.
[options.editor]
autosuggest = true

[options.history]
size = 10000            # commands to keep, in memory and in the file
share = true            # commands typed in one terminal can be recalled in the others
ignore_space = true     # a command that starts with a space is not saved

[options.cd]
auto = true             # type a directory's name (or ..) to go there

[options.pushd]
auto = true             # cd keeps the directories you were in: dirs -v lists them, cd +N goes back
ignore_dups = true
silent = true

[options.glob]
star = true             # **/ matches any number of directories: ls **/*.c
bare_qualifiers = true  # zsh's glob qualifiers: ls -d *(/) lists directories

[options.prompt]
percent = true          # zsh's % sequences in PS1 (see the prompt below)

[alias]
ls = "ls --color=auto"
ll = "ls -lh"
la = "ls -lAh"
grep = "grep --color=auto"
g = "git"
gs = "git status"

# Global aliases are expanded anywhere in a command, not only at its start.
[alias.global]
"..." = "../.."         # cd .../src
G = "| grep"            # ps aux G ssh

[bindkey]
Ctrl-Right = "forward-word"
Ctrl-Left = "backward-word"

# Tab completion for about 70 common commands and for git.
[plugins.enabled]
std.completion = "*"
```

Each table under `options` is a group of luish's options, and each key in it turns one on: `share = true` in
`[options.history]` is the option `history.share`. `help setopt` describes them all, and [](usage.md) explains
history, line editing and completion in more detail. Aliases are the same as `alias NAME=VALUE`, so `ll` uses the
`ls` alias and also gets `--color=auto`. The `bindkey` table binds keys to the line editor's widgets, whose names are
zsh's (`help bindkey` lists them).

If you already use zsh, you can keep your history: luish writes zsh's file format, so the two shells can share a
file. Add `file = "~/.zsh_history"` (or wherever your `HISTFILE` is) to `[options.history]`.

## The standard plugins

`std.completion` is the plugin `completion` of `std`, the collection of plugins that comes with luish. It completes
the options of common commands with their descriptions (`ls --<Tab>`), and their arguments: modes for `chmod`,
the targets of the makefile for `make`, the hosts in `~/.ssh/config` for `ssh`, users for `chown`, and so on. It loads
`git-completion` too, which completes git's commands, branches, remotes, and the files that each command can act on
(`git add <Tab>` offers the modified and untracked files).

Plugins are fetched once, with `plugin sync`, not when a shell starts. Run it in luish:

```console
$ plugin sync
Fetching std
Locking std at 6a893bc
1 git source locked, 2 plugins enabled: git-completion, completion
```

It records the commit it used in `~/.config/luish/plugins.lock`. New shells now load the plugins; try
`git checkout <Tab>` in a git repository. After upgrading luish, run `plugin sync` again, to fetch the plugins of the
new version.

If you have [bash-completion](https://github.com/scop/bash-completion) installed, as most distributions do, the
`std.bash-completion` plugin also completes the arguments of the many commands that it knows, and of the programs
that install completion files for bash. It is slower (each Tab takes about 50 ms, as it runs bash), and gives no
descriptions. To use it, add it under `[plugins.enabled]`, and run `plugin sync` again:

```toml
std.bash-completion = "*"
```

[](plugins.md) describes the plugins in `std`, and how to install other plugins or write your own.

## Variables and functions: `rc.d`

`config.toml` has no place for shell variables or functions: they go in the files in `rc.d`, which are shell scripts
(their names must end in `.lsh`, and they run in the order of their names). For example, in
`~/.config/luish/rc.d/functions.lsh`:

```sh
# Ctrl-W and Alt-B stop at a /, so Ctrl-W deletes one directory of a path.
WORDCHARS='*?_-.[]~=&;!#$%^(){}<>'

# Make a directory and go into it.
mkcd() {
    mkdir -p -- "$1" && cd -- "$1"
}
```

Aliases can go in these files too (`alias ll='ls -lh'`), if you prefer them next to your functions. The files in
`rc.d` run after `config.toml` and the plugins, so they can change what these set.

## The environment: `login.d`

Exported variables, such as `PATH` and `EDITOR`, are inherited by every program and shell that you start, so they
belong in the startup files of the login shell. For luish, those are the files in `login.d`, which a login shell runs
*instead of* `/etc/profile` and `~/.profile`. Start by running `/etc/profile`, which sets the system's defaults, in
`~/.config/luish/login.d/env.lsh`:

```sh
. /etc/profile
export PATH="$HOME/.local/bin:$PATH"
export EDITOR=vim PAGER=less
```

If your `~/.profile` already has what you need (and is plain `sh`, as it should be), run it instead of repeating it:
`. ~/.profile`.

This matters only once luish is your login shell. Until then, luish inherits the environment of the shell that
started it.

## The prompt: `luishrc`

The prompt goes in `~/.config/luish/luishrc`, which interactive shells run last, after the login files: on some
systems, such as Debian and Ubuntu, `/etc/profile` sets `PS1`, which would replace a prompt set in `rc.d`.

```sh
# The current directory in blue, the exit status in red if the last command failed, and % (# for root).
PS1='%F{blue}%~%f %(?..%F{red}[%?]%f )%# '
```

This uses zsh's prompt sequences, which the option `prompt.percent` in `config.toml` turned on. luish also has
longer names for them, which are easier to read; this is the same prompt:

```sh
PS1='%[fg:blue]%[dir]%[fg_off] %([status]..%[fg:red][%[status]]%[fg_off] )%[prompt_char] '
```

[Prompts](usage.md#prompts) lists the sequences. To show the git branch as well, a plugin gives the prompt a
variable with it (see [Customizing the prompt](plugins.md#customizing-the-prompt)).

`luishrc` isn't cached, so keep it short: what takes time to run, such as `conda`'s or `nvm`'s initialization,
belongs in `rc.d`.

## Checking the configuration

Open a new shell (or a new terminal) to use the configuration, and check it:

```sh
setopt                    # the options that are on
setopt -p history         # the settings of a group
alias                     # the aliases
bindkey                   # the key bindings
plugin list-loaded        # the plugins
```

A mistake in `config.toml`, such as an option that doesn't exist, is reported with its line when a shell starts, and
the rest of the file still applies. `luish --no-rcs` starts a shell that reads no configuration at all, and
`luish --no-plugins` one that loads no plugins, to find out where a problem comes from.

Once you are happy with it, make luish your login shell (see [](installation.md)):

```sh
echo ~/.local/bin/luish | sudo tee -a /etc/shells
chsh -s ~/.local/bin/luish
```

## Coming from bash or zsh

luish doesn't read `~/.bashrc` or `~/.zshrc`. Aliases, functions and exported variables from them usually work in
luish as they are, and can be copied into `rc.d` (or `login.d`, for the environment). zsh's names for options work
too, so `setopt share_history hist_ignore_space` needs no change. Code that uses bash's or zsh's extensions to the
shell language, such as arrays, `[[ ... ]]`, `shopt` or `zstyle`, doesn't work: luish runs POSIX `sh`, as dash
does. [](compatibility.md) lists the details.

Plugin managers such as oh-my-zsh, and the plugins they load, are replaced by luish's own plugins. The standard
plugins cover completion, luish highlights the command line by itself, and the option `editor.autosuggest` takes the
place of zsh-autosuggestions.

## Next steps

- [](usage.md): the command line, the prompt, line editing, history, Tab completion and the startup files in detail.
- [](plugins.md): other plugins, and writing your own, for example to keep the same configuration on every machine.
- [](globbing.md): `**/` and zsh's glob qualifiers, such as `vi *(.om[0])` to edit the newest file.
- `help` lists the built-in commands, and `help NAME` shows the help for one of them.

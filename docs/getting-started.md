# Getting started

## Trying it out

Start luish from the shell you use now; `exit` (or Ctrl-D) goes back to it:

```sh
luish
```

If you've used a shell before, you should feel at home, luish should feel much like zsh.

## The first run

If you installed luish with the [install script](installation.md#from-a-release) and let it write the configuration,
there is nothing to do here: it wrote the recommended configuration below, with
[luish-extra](plugins.md#more-completion-luish-extra) for more completion and colour schemes.

The first time luish starts in a terminal (when `~/.config/luish/` is missing or empty), it shows a menu (Up and
Down choose, Enter confirms; or press an item's number):

1. **Write the recommended configuration** to `config.toml`: Tab completion from the standard plugins (see below),
   suggestions from the history as you type, zsh's `%` sequences in prompts, and history expansion (`!!`, `!$`,
   `^old^new`). luish then runs `plugin sync` to
   fetch the plugins.
2. **Write an empty configuration**: the recommended one, all commented out to turn on later, so that the menu isn't
   shown again.
3. **Add a personal plugin**, as [`plugin add`](plugins.md) takes it: a GitHub repository (`OWNER/REPO` or its URL),
   another git URL, or a path. `config.toml` then has only that plugin, which can hold your whole configuration
   (see [](personal-plugin.md)), and luish runs `plugin sync` to fetch it.
4. **Just start for now**, writing nothing: the menu is shown again next time (as it is after Ctrl-C or Esc).

The rest of this page describes the files, to write or extend them yourself.

## Where the configuration goes

The configuration is in `~/.config/luish/` (or `$XDG_CONFIG_HOME/luish/` if you set `XDG_CONFIG_HOME`), in these files and directories:

| File | What goes there |
|---|---|
| `config.toml` | Options, aliases, key bindings and the plugins to load |
| `rc.d/*.lsh` | Shell code for every interactive shell: variables and functions |
| `login.d/*.lsh` | The environment, for login shells: `PATH`, `EDITOR` and other exported variables |
| `luishrc` | Shell code that runs last, in every interactive shell: the prompt |


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
# G = "| grep"            # ps aux G ssh

[bindkey]
Ctrl-Right = "forward-word"
Ctrl-Left = "backward-word"

# Tab completion for about 230 common commands, including git.
[plugins.enabled]
std.completion = "*"
```

## The standard plugins

`std.completion` is the standard plugin for completion. It completes about 230 common commands (including `ls`, `grep`, `tar`, `ssh`, `make`, `git`, `systemctl`, `cargo` and `docker`).

You need to fetch the plugins once:

```console
$ plugin sync
Fetching std
Locking std at 6a893bc (std/completion)
1 git source locked, 1 plugin enabled: std/completion
```


## Variables and functions: `rc.d`

`config.toml` has no place for defining shell variables or functions, but we can add scripts to `rc.d`. For example, we can add  function to make a directory and go into it, and change the characters that Ctrl-W and Alt-B stop at when deleting or moving by word:

```sh
# Ctrl-W and Alt-B stop at a /, so Ctrl-W deletes one directory of a path.
WORDCHARS='*?_-.[]~=&;!#$%^(){}<>'

# Make a directory and go into it.
mkcd() {
    mkdir -p -- "$1" && cd -- "$1"
}
```

`rc.d` run after `config.toml` and the plugins, so they can change what these set.

## The environment: `login.d`

Thie runs for login shells:

```sh
export PATH="$HOME/.local/bin:$PATH"
export EDITOR=vim PAGER=less
```

If your `~/.profile` already has what you need, you can run it instead of repeating it:

`. ~/.profile`.

## The prompt: `luishrc`

The prompt goes in `~/.config/luish/luishrc`, which interactive shells run last, after the login files: on some
systems, such as Debian and Ubuntu, `/etc/profile` sets `PS1`, which would replace a prompt set in `rc.d`.

```sh
# The current directory in blue, the exit status in red if the last command failed, and % (# for root).
PS1='%[fg:blue]%[dir]%[fg_off] %([status]..%[fg:red][%[status]]%[fg_off] )%[prompt_char] '
```

This uses zsh-like prompt sequences, which is why we set option `prompt.percent` in `config.toml`. Luish's also supports the zsh single-letter sequences, but we prefer the longer names as they are easier to read.

[Prompts](usage.md#prompts) lists the sequences.


## Checking the configuration

Open a new shell (or a new terminal) to use the configuration, and check it:

```sh
setopt                    # the options that are on
setopt -p history         # the settings of a group
alias                     # the aliases
bindkey                   # the key bindings
plugin list-loaded        # the plugins
```

## Coming from bash or zsh

luish doesn't read `~/.bashrc` or `~/.zshrc`. Aliases, functions and exported
variables from them usually work in luish as they are, and can be copied into
`luishrc`.

For backwards compatibility, luish supports many of bash's and zsh's options,
with the same names. For example, `setopt share_history hist_ignore_space`
needs no change from zsh, even though the luish names are `history.share` and
`history.ignore_space`.

Much of bash's and zsh's extensions to the shell language work too: arrays and
associative arrays, `[[ ... ]]`, `function`, `let`, `typeset`,
`${x/pattern/replacement}`, `${!name}` and zsh's `${(j:,:)a[@]}` flags (see
[](usage.md#shell-language-extensions)).

Code that uses `shopt` (luish suggests the matching `setopt`), `zstyle` or
zsh's other modules doesn't work yet. Arrays are indexed from 0, as in bash and
in zsh's `sh` mode, not from 1 as in native zsh.

If you have [bash-completion](https://github.com/scop/bash-completion)
installed, as most distributions do, the `std.bash-completion` plugin can use
it to autocomplete arguments of the many commands that it knows (luish-native
completion takes precendence,though). To use it, add it
under `[plugins.enabled]`, and run `plugin sync` again:

```toml
std.bash-completion = "*"
```

(`plugin add std/bash-completion` does both.)

Compared to luish-native completion, this is slower (each completion takes
about 50 ms, as it runs bash), and gives no descriptions.

For about 270 more commands with luish-native completion, in bioinformatics
(samtools, bwa, kraken2, ...), science (snakemake, nextflow, pandoc, jupyter,
...), desktop programs and Python tooling (pytest, ruff, ...), add
[luish-extra](plugins.md#more-completion-luish-extra):

```console
$ plugin add luispedro/luish-extra extra
$ plugin add extra/complete/all
```


## Next steps

- [](personal-plugin.md): keep your configuration in a plugin of your own, to use the same one on every machine.
- [](usage.md): the command line, the prompt, line editing, history, Tab completion and the startup files in detail.
- [](plugins.md): other plugins, and writing your own.
- [](extensions.md): code in Rhai for your plugins: hooks, Tab completion and commands.
- [](globbing.md): `**/` and zsh's glob qualifiers, such as `vi *(.om[0])` to edit the newest file.

- `help` lists the built-in commands, and `help NAME` shows the help for one of them.

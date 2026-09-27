# Getting started

## Trying it out

Start luish from the shell you use now; `exit` (or Ctrl-D) goes back to it:

```sh
luish
```

If you've used a shell before, you should feel at home, luish should feel much like zsh.

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

# Tab completion for about 70 common commands and for git.
[plugins.enabled]
std.completion = "*"
```

## The standard plugins

`std.completion` is the standard plugin for completion. It completes about 70 common commands (including `ls`, `grep`, `tar`, `ssh`, `scp`, `make` and `git`).

You need to fetch the plugins once:

```console
$ plugin sync
Fetching std
Locking std at 6a893bc
1 git source locked, 2 plugins enabled: git-completion, completion
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

luish doesn't read `~/.bashrc` or `~/.zshrc`. Aliases, functions and exported variables from them usually work in
luish as they are, and can be copied into `rc.d`.

zsh's names for options work too, so `setopt share_history hist_ignore_space` needs no change.

Code that uses bash's or zsh's extensions to the
shell language, such as arrays, `[[ ... ]]`, `shopt` or `zstyle`, doesn't work yet, though.

If you have [bash-completion](https://github.com/scop/bash-completion)
installed, as most distributions do, the `std.bash-completion` plugin can use
it to autocomplete arguments of the many commands that it knows (luish-native
completion takes precendence,though). To use it, add it
under `[plugins.enabled]`, and run `plugin sync` again:

```toml
std.bash-completion = "*"
```

Compared to luish-native completion, this is slower (each completion takes
about 50 ms, as it runs bash), and gives no descriptions.


## Next steps

- [](usage.md): the command line, the prompt, line editing, history, Tab completion and the startup files in detail.
- [](plugins.md): other plugins, and writing your own, for example to keep the same configuration on every machine.
- [](globbing.md): `**/` and zsh's glob qualifiers, such as `vi *(.om[0])` to edit the newest file.

- `help` lists the built-in commands, and `help NAME` shows the help for one of them.

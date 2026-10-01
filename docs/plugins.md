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
| `plugin = "NAME"` | In `plugins.enabled`: which plugin of a collection. By default the one with the entry's name, or the only one |

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
Locking std at 15e39bb
Locking smarty-prompt at 8a1c0de
2 git sources locked, 2 plugins enabled: completion, smarty-prompt
```

The first time `plugin sync` runs, it will create a `plugins.lock` file, which
records the commit of each git source that was fetched. You can update plugins
later with `plugin update`, which fetches the newest commit of the source
and updates `plugins.lock`. Specify a plugin name to update only that one:

```console
$ plugin update smarty-prompt
Fetching smarty-prompt
Updating smarty-prompt 8a1c0de..3f00c2d
2 git sources locked, 2 plugins enabled: completion, smarty-prompt
```

`plugin check` asks each git source (with `git ls-remote`) for its newest
commit and reports whether there are newer commits, but it does not change
anything.

A source pinned with `rev` never has anything newer. The exit status is 0
unless a source couldn't be checked.

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
| `plugin.toml` | Parsed first, to load dependencies and set options, aliases and key bindings. |
| `init.lsh` | Next: sourced in the current shell (as with `.`). |
| `extension.rhai` | Next: the plugin's extension, which is loaded and run (see [](extensions.md)). |
| `rc.lsh` | Next, as with `.`, but only in interactive shells (and their subshells). |
| `post-rc.lsh` | Last, as `rc.lsh`, but after the startup files in `rc.d` (see [below](#after-the-startup-files-post-rc)) |
| `prompt-vars.lsh` | Before each prompt, to set variables for `PS1` (see [below](#variables-for-the-prompt-prompt-varslsh)) |

A directory needs at least one of them. Other directories (such as a repository's `docs` or `src`) are not plugins,
and a collection's plugins are only its `.rhai` and `.lsh` files and its plugin directories.

While `init.lsh`, `extension.rhai`, `rc.lsh` and `prompt-vars.lsh` run,
`LUISH_PLUGIN_DIR` is the plugin's directory (an absolute path) and
`LUISH_PLUGIN_NAME` its name. Afterwards they get back the values they had
before.

`plugin unload` removes what the extension registered (hooks, completers and commands), but it can't undo what the
shell files did (aliases, functions, variables).

## Dependencies, options, aliases and key bindings: `plugin.toml`

A directory plugin can have a file `plugin.toml`, which can list dependencies:

```toml
# ~/src/work-plugins/proxy/plugin.toml
description = "Sets the proxy variables for the office network."

[dependencies]
netutils = "*"                        # the plugin netutils of the same collection
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
2. the plugins that `config.toml` enables (each plugin's `init.lsh`, `extension.rhai`, the options, aliases and key
   bindings in its `plugin.toml`, and `rc.lsh`);
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

# luish-std-plugins

A collection of plugins for [luish](../README.md), known to luish as `std`. It lives in the luish repository for now,
and will become a repository of its own, as an example of how a plugin collection is laid out.

## The plugins

| Plugin | What it does | Needs |
|---|---|---|
| `completion` | Tab completion for about 230 common commands: their options (with descriptions), the values of options, their subcommands, and their other arguments (directories for `mkdir`, users and groups for `chown`, make's targets, ssh's hosts, man pages, the files in an archive for `tar -xf`, systemd's units, installed and available packages, cargo's targets and features, pixi's and npm's tasks ...). Programs built with Cobra (gh, docker, kubectl ..., and more with `complete-cobra PROG...`) or Click (black, flask ..., and more with `complete-click PROG...`) and nix are asked for their own completions. git: its commands (with descriptions) and aliases, the options of each command, and the arguments each command takes (branches, tags, ranges such as `main..`, remotes, stashes, worktrees, and the files it can act on: modified and untracked files for `git add`, staged ones for `git restore --staged`, tracked ones after a revision for `git diff`, ...) | |
| `notify` | A desktop notification (OSC 777) when a command that took at least `$LUISH_NOTIFY_AFTER` seconds (10 by default) ends, for terminals that don't notify by themselves | foot, WezTerm, Ghostty, VTE with its notification patch, or rxvt-unicode |
| `bash-completion` | Completion from [bash-completion](https://github.com/scop/bash-completion), for the commands that have no completer of their own | bash, bash-completion |

For git, `completion` runs git with the options of the command line that choose the repository (`-C DIR`,
`--git-dir=DIR`, `--work-tree=DIR`), so an alias such as `alias g='git -C ~/src'` completes in `~/src`. git
itself lists the options of its commands (`git CMD --git-completion-helper`, which git's own bash completion uses),
so they follow the installed version of git. Options that start with `--no-` are offered once the word starts with
`--no`.

`completion` covers:

- coreutils (`ls`, `cp`, `mv`, `rm`, `mkdir`, `ln`, `chmod`, `chown`, `head`, `tail`, `sort`, `date`, `dd`, `install`,
  ...), grep, diffutils, tar, make, rsync, man, ssh, scp, sftp, pkill, pgrep and killall (`specs.rhai`);
- shells: luish itself, sh (dash), bash and zsh (`shells.rhai`);
- files and text: find, fd, locate, sed, awk, jq, rg, less, more, file, ldd, patch, tree, gzip, xz, zstd, bzip2, zip
  and unzip (`tools.rhai`);
- the system: systemctl, journalctl, loginctl, ps, top, htop, lsof, strace, mount, umount, lsblk, dmesg, free and tmux
  (`system.rhai`);
- the network: curl, wget, ip, ss, ping, dig, ssh-add, ssh-keygen, ssh-copy-id, gpg and openssl (`net.rhai`);
- development: cargo, rustup, go, gcc, clang, cmake, meson, ninja, gdb, python, sqlite3, vim, nvim, nano and emacs
  (`dev.rhai`);
- the package managers of languages: pip, uv, conda, mamba, pixi, npm, npx, yarn and pnpm (`langs.rhai`);
- the package managers of systems: apt, apt-get, apt-cache, apt-mark, dpkg, dnf, yum, rpm, pacman, zypper, apk, brew,
  snap and flatpak (`packages.rhai`);
- git (`git.rhai`);
- programs that complete themselves (`bridges.rhai`): those built with Cobra (gh, glab, docker, podman, kubectl, helm,
  minikube, kind, hugo, rclone ...), which answer `PROG __complete ARGS...`, and nix (`NIX_GET_COMPLETIONS`). For
  another Cobra program, run `complete-cobra PROG...` (in a file of `rc.d`, say) once the plugin is loaded; without
  arguments, it lists the programs. Python programs built with Click (black, flask, uvicorn, pip-compile, hatch,
  mkdocs, cookiecutter, celery, datasette, llm, pgcli ...) answer `_PROG_COMPLETE=fish_complete`; `complete-click
  PROG...` adds more (dbt and mlflow take seconds to start, so they are not registered). Each Tab runs the program,
  so register only programs built with Click.

Each command is a spec: its options as a table written like `--help` output, what the values of options complete to,
its subcommands, and what its arguments complete to (a kind from `kinds.rhai`, such as `dirs`, `users` or `hosts`, or
one of the module, such as `system:units`). `lib.rhai` completes a command line from a spec; the fields of specs are
described there. Some specs are read from the program's own `-h` when it is fast (cargo's subcommands, rustup, uv,
pixi, openssl's commands), so that they follow the installed version. To add a command, add its spec to a module and
its name in `extension.rhai`. A module is compiled on the first Tab for one of its commands (a few milliseconds), so
loading the plugin costs little (about 0.4 ms).

The packages that can be installed are offered once the word has a letter (apt, pacman, apk, brew, flatpak), since
there are tens of thousands of them, and not at all for dnf, yum and zypper, whose lists take seconds to get; installed
packages are offered for removing them.

`bash-completion` is a default completer (registered for `-default-`): it runs, in bash, the function that
bash-completion has for the command, and gives luish what it returns. The commands that have completers of their
own, such as git with `completion`, keep them. bash-completion is looked for in the usual places; set
`BASH_COMPLETION_SCRIPT` to the path of its `bash_completion` script if it is elsewhere. Each Tab takes about 50 ms,
as bash loads bash-completion again, and bash-completion gives no descriptions.

## Layout

A collection is a directory laid out like `~/.config/luish/plugins/`: each `NAME.rhai` (a plugin that is only an
extension), `NAME.lsh` (a plugin that is only shell) and `NAME/` (a plugin directory, with `init.lsh`,
`extension.rhai`, `rc.lsh` and the files they use) in it is a plugin called `NAME`. Other files, such as this
README, are ignored.

```text
luish-std-plugins/
├── README.md
├── notify.rhai                # a plugin that is only an extension
├── bash-completion/           # a plugin directory
│   ├── extension.rhai
│   └── bridge.bash            # run by extension.rhai, through sh::plugin_dir()
└── completion/
    ├── plugin.toml            # its description
    ├── extension.rhai         # registers the completers, and complete-cobra and complete-click
    ├── lib.rhai               # the engine: completes a command line from a spec
    ├── kinds.rhai             # what arguments complete to (dirs, users, hosts ...)
    ├── specs.rhai             # the specs of one group of commands each, imported on the first Tab
    ├── shells.rhai            #   for one of their commands
    ├── tools.rhai
    ├── system.rhai
    ├── net.rhai
    ├── dev.rhai
    ├── langs.rhai
    ├── packages.rhai
    ├── git.rhai
    └── bridges.rhai           # programs that complete themselves (Cobra, Click, nix)
```

Other plugins can use the engine with `import "@std/completion/lib"` (see "Reusing std's completion engine" in the
extensions page of luish's documentation).

## Using them

This collection is the source `std` of luish's plugin configuration. Enable its plugins in `~/.config/luish/config.toml`:

```toml
[plugins.enabled]
std.completion = "*"
std.bash-completion = "*"
```

and run `plugin sync`, which fetches them from luish's repository and pins the commit in `plugins.lock`. Every
interactive shell then loads them. `std` is the tag of the luish release that runs it (`v0.1.0` for luish 0.1.0), so
the plugins match the shell; after upgrading luish, run `plugin sync` again. To follow the `main` branch instead,
name `std` yourself as `{ gh = "luispedro/luish", subdir = "luish-std-plugins", branch = "main" }`, or, to use a
local checkout, for example while working on these plugins:

```toml
[plugins.available]
std = { path = "~/src/luish/luish-std-plugins" }
```

A plugin can also be loaded by its path, in one shell:

```sh
plugin load ~/src/luish/luish-std-plugins/completion
```

`__luish_internal complete LINE` prints what Tab offers for a command line, which helps when working on a
completer (the tests in `tests/plugins/` use it).

A directory plugin here can list the plugins it needs in a `plugin.toml` (see the plugins page of luish's
documentation); a plain name there is another plugin of this collection.

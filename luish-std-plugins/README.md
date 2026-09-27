# luish-std-plugins

A collection of plugins for [luish](../README.md). It lives in the luish repository for now, and will become a
repository of its own, as an example of how a plugin collection is laid out.

## The plugins

| Plugin | What it does | Needs |
|---|---|---|
| `git-completion` | Tab completion for git: its commands (with descriptions) and aliases, the options of each command, and the arguments each command takes (branches, tags, ranges such as `main..`, remotes, stashes, worktrees, and the files it can act on: modified and untracked files for `git add`, staged ones for `git restore --staged`, ...) | git |
| `bash-completion` | Completion from [bash-completion](https://github.com/scop/bash-completion), for the commands that have no completer of their own | bash, bash-completion |

`git-completion` runs git with the options of the command line that choose the repository (`-C DIR`,
`--git-dir=DIR`, `--work-tree=DIR`), so an alias such as `alias g='git -C ~/src'` completes in `~/src`. git
itself lists the options of its commands (`git CMD --git-completion-helper`, which git's own bash completion uses),
so they follow the installed version of git. Options that start with `--no-` are offered once the word starts with
`--no`.

`bash-completion` is a default completer (registered for `-default-`): it runs, in bash, the function that
bash-completion has for the command, and gives luish what it returns. The commands that have completers of their
own, such as git with `git-completion`, keep them. bash-completion is looked for in the usual places; set
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
├── git-completion.rhai        # a plugin in one Rhai file
└── bash-completion/           # a plugin directory
    ├── extension.rhai
    └── bridge.bash            # run by extension.rhai, through sh::plugin_dir()
```

## Using them

luish doesn't load plugins from collections yet. Until it does, load a plugin by its path:

```sh
plugin load ~/src/luish/luish-std-plugins/git-completion.rhai
plugin load ~/src/luish/luish-std-plugins/bash-completion/
```

or link it into your plugin directory, and load it by name:

```sh
ln -s ~/src/luish/luish-std-plugins/git-completion.rhai ~/.config/luish/plugins/
plugin load git-completion
```

Put the `plugin load` lines in `~/.config/luish/luishrc` (or a file in `~/.config/luish/rc.d/`) to load the plugins
in every interactive shell.

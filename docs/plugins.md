# Plugins

A plugin adds functionality to luish. In it general format, it includes
configuration, shell scripts, and extensions.

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
"std/git-completion" = "*"     # the same, written differently
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

### Fetching plugins: `plugin sync` and `plugins.lock`

Plugins need to be fetch explicitly, by running `plugin sync`.

```console
$ plugin sync
Fetching std
Fetching smarty-prompt
Locking std at 15e39bb
Locking smarty-prompt at 8a1c0de
2 git sources locked, 2 plugins enabled: git-completion, smarty-prompt
```

The first time `plugin sync` runs, it will create a `plugins.lock` file, which
records the commit of each git source that was fetched. You can update plugins
later with `plugin update`, which fetches the newest commit of the source
and updates `plugins.lock`. Specify a plugin name to update only that one:

```console
$ plugin update smarty-prompt
Fetching smarty-prompt
Updating smarty-prompt 8a1c0de..3f00c2d
2 git sources locked, 2 plugins enabled: git-completion, smarty-prompt
```

`plugin check` asks each git source (with `git ls-remote`) for its newest
commit and reports whether there are newer commits, but it does not change
anything.

A source pinned with `rev` never has anything newer. The exit status is 0
unless a source couldn't be checked.

luish runs `git` to fetch, so git's own settings apply (credentials, SSH keys, proxies).

### Dependencies, options, aliases and key bindings: `plugin.toml`

A directory plugin can have a file `plugin.toml`, which can list dependencies:

```toml
# ~/src/work-plugins/proxy/plugin.toml
description = "Sets the proxy variables for the office network."

[dependencies]
netutils = "*"                        # the plugin netutils of the same collection
std.git-completion = "*"              # a plugin of a source that luish knows
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

## Plugin directories

A plugin directory can have multiple files which luish uses the following ways:

| File | Meaning |
|---|---|
| `plugin.toml` | Parsed first, to load dependencies and set options, aliases and key bindings. |
| `init.lsh` | Next: sourced in the current shell (as with `.`). |
| `extension.rhai` | Next: the plugin's extension is loaded and run. |
| `rc.lsh` | Next, as with `.`, but only in interactive shells (and their subshells). |
| `post-rc.lsh` | Last, as `rc.lsh`, but after the startup files in `rc.d` (see [below](#after-the-startup-files-post-rc)) |
| `prompt-vars.lsh` | Before each prompt, to set variables for `PS1` (see [below](#variables-for-the-prompt-prompt-vars)) |

A directory needs at least one of them.

While `init.lsh`, `extension.rhai`, `rc.lsh` and `prompt-vars.lsh` run,
`LUISH_PLUGIN_DIR` is the plugin's directory (an absolute path) and
`LUISH_PLUGIN_NAME` its name. Afterwards they get back the values they had
before.

In Rhai, `sh::plugin_dir()` gives the directory at any time, also in hooks. For
a single-file plugin it is the directory of the file.

`plugin unload` removes what the extension registered (hooks), but it can't
undo what the shell files did (aliases, functions, variables).

## Plugin formats

1. a directory of files with special names, in shell and Rhai (see [above](#plugin-directories)), not all of which
   need to be there;
2. a single Rhai file, `NAME.rhai`, which is the same as a directory that holds only that file, as `extension.rhai`;
3. a single shell file, `NAME.lsh`, which is the same as a directory that holds only that file, as `init.lsh`.

## Suggestion: a personal plugin

A plugin doesn't have to be for others. Packaging your own configuration as a plugin in a git repository is a good
way to keep the same options, aliases, key bindings and functions on every machine: each one needs only a line in
`config.toml` and `plugin sync`, and gets the same setup at the commit that `plugins.lock` records.
[luispedro/luish-personal-plugin](https://github.com/luispedro/luish-personal-plugin) is an example:

```text
luish-personal-plugin/
├── plugin.toml   # dependencies, and options, aliases and key bindings
└── rc.lsh        # what plugin.toml can't hold (variables and functions)
```

Its `plugin.toml` enables other plugins, as dependencies, and holds most of the settings:

```toml
description = "Luis Pedro's personal luish configuration."

[dependencies]
std.bash-completion = "*"
std.git-completion = "*"

[options]
autosuggest = true

[options.cd]
auto = true

[options.prompt]
percent = true

[options.history]
file = "~/.histfile"
share = true
ignore_space = true

[alias]
ls = "ls --color=auto"

[alias.global]
"..." = "../.."
"...." = "../../.."

[bindkey]
"^[?" = "insert-last-word"
```

and its `rc.lsh` has the rest, which needs shell:

```sh
CDPATH=:$HOME:$HOME/work
WORDCHARS='*?_-.[]~=&;!#%^(){}<>'
PS1='%n@%m:%/ %(?. .%B!%b)%(!.#.§)'

field() {
    awk "{print \$$1}"
}
```

Each machine enables it in `~/.config/luish/config.toml`:

```toml
[plugins.enabled]
personal = { gh = "luispedro/luish-personal-plugin" }
```

## Example: running code when the directory changes

```rhai
// ~/.config/luish/plugins/dirs.rhai

// Named functions can't see the extension's variables, but closures can.
let visits = 0;

sh::hook("chpwd", |from, to| {
    visits += 1;
    // Show what is in a project directory when entering it.
    if sh::run("test -f Makefile") == 0 {
        sh::run("ls");
    }
    sh::setvar("LAST_DIR", from);
});
```

`chpwd` hooks are called after each successful `cd`, `pushd` or `popd`, with the old and the new directory. `$?` is the same after
the hooks as before them. If a hook fails, the error is printed and the other hooks still run. A `chpwd` hook that
itself runs `cd` does not trigger `chpwd` again.

## After the startup files: `post-rc`

An interactive shell starts in this order:

1. the settings in `config.toml`;
2. the plugins that `config.toml` enables (each plugin's `init.lsh`, `extension.rhai`, the options, aliases and key
   bindings in its `plugin.toml`, and `rc.lsh`);
3. the files in `rc.d`, which can load more plugins;
4. the `post-rc.lsh` of each plugin loaded so far, in the order they were loaded;
5. the `post-rc` hooks of their extensions, in the same order;
6. `rc.d/_uncached.lsh`, then `login.d` (in login shells), `$ENV` and `luishrc`.

So a plugin can read, in its `post-rc.lsh` or `post-rc` hook, the variables that you set in `rc.d` to configure it,
even though it was loaded before `rc.d`:

```rhai
// extension.rhai
sh::hook("post-rc", || {
    let style = sh::getvar("MYPROMPT_STYLE") ?? "plain";
    // ...
});
```

A plugin loaded later (in `luishrc`, or at the prompt) runs its `post-rc.lsh` and `post-rc` hooks right after its
`rc.lsh`. As `rc.lsh`, they run only in interactive shells. What `post-rc.lsh` does is cached with `rc.d`, but the
`post-rc` hooks run in every shell, since extensions are loaded again in every shell.

## Customizing the prompt

Plugins can change the prompt (`PS1`; `PS2`, for the continuation lines of a command, is unchanged) in two ways:

- **`prompt-vars`** (start here): a plugin computes values, such as the git branch, and gives them to `PS1` as
  variables.
- **`prompt-rewrite`** (advanced): an extension writes the whole prompt itself,
  and can replace what other plugins did. It is easy to get wrong, so use it
  only if you know what you are doing.

### Variables for the prompt: `prompt-vars`

A `prompt-vars` hook returns a map of variables, which `PS1` can then use:

```rhai
// ~/.config/luish/plugins/branch.rhai
sh::hook("prompt-vars", || {
    let i = vcs::info();
    #{
        git_branch: if i == () { "" } else { ` (${i.branch ?? "detached"})` },
        last_failed: sh::last_status() != 0,
    }
});
```

```sh
# ~/.config/luish/luishrc
plugin load branch
PS1='$PWD$git_branch\$ '
```

The same can be written in shell, in a directory plugin's `prompt-vars.lsh`.
Every variable it sets is available to `PS1`:

```sh
# ~/.config/luish/plugins/branch/prompt-vars.lsh
git_branch=$(git branch --show-current 2>/dev/null)
[ -n "$git_branch" ] && git_branch=" ($git_branch)"
```

Before each prompt, luish:

1. runs the `prompt-vars` hooks and `prompt-vars.lsh` files, plugin by plugin
   in the order the plugins were loaded.
2. expands `PS1` with those variables (parameter expansion, then `%` expansion under `prompt.percent`);
3. puts every variable they changed back as it was (or unsets it).

A hook's map gives each variable a string, a number or a boolean (which become
text, such as `42` or `true`), or `()` to unset the variable while the prompt
is built. A hook can also return `()` to set nothing. A bad entry (not a valid
name, a readonly variable, another type of value) is reported and skipped, and
a hook that fails is reported; the other hooks and files still run.

`prompt-vars.lsh` runs in the current shell, as with `.`, so it can use the
shell's functions and variables. Prompt variable don't leak into the shell, but
if multiple plugins are enabled, later ones can see the variables of the ones
loaded before them. Note, however, that other side effects of a
`prompt-vars.lsh` file (changing the directory, defining functions or aliases,
changing options) are not undone so be careful with them. Also, avoid slow
commands as these are run before every prompt.

### Rewriting the prompt: `prompt-rewrite`

```{warning}
`prompt-rewrite` is an advanced feature, and easy to get wrong: the hooks run in a fixed order, each one can replace
what the others did, and none of it goes through `PS1` unless a hook asks for it. For most prompts, set `PS1` and
give it variables with `prompt-vars` instead.
```

A `prompt-rewrite` hook returns the whole prompt, which is used instead of `PS1`:

```rhai
// ~/.config/luish/plugins/prompt.rhai

sh::hook("prompt-rewrite", || {
    let status = sh::last_status();
    let mark = if status == 0 { "" } else { "%F{red}[" + status + "]%f " };
    let branch = vcs::info()?.branch;
    let branch = if branch == () { "" } else { " %F{yellow}(" + branch + ")%f" };
    "%F{blue}%~%f" + branch + " " + mark + "%# "
});
```

The order of operations matters:

1. The `prompt-vars` hooks and files run first (see above), so a `prompt-rewrite` hook sees their variables, with
   `sh::getvar`.
2. The `prompt-rewrite` hooks run, from the one registered **last**, until one returns a string: that is the prompt.
   A hook that returns `()` leaves the prompt to the hooks registered before it, and then to `PS1`, so an extension
   can give the prompt only in some directories, for example. A hook that fails (or returns something other than a
   string or `()`) is reported, and the next one is tried.
3. The prompt a hook returns doesn't go through parameter expansion, so `$x` in it stays as it is. It does go through
   `%` expansion if the `prompt.percent` option is on (`setopt prompt.percent`, see [](usage.md)), as in the example.
4. The variables set by `prompt-vars` are put back. What a `prompt-rewrite`
   hook itself changes, with `sh::setvar` or `sh::run`, stays.

`$?` is the same after the hooks as before them, and each hook sees that of the
last command (but `sh::run` changes it for the rest of the hook, so read
`sh::last_status()` first).

A hook that takes a parameter is given the previous prompt: the one the hooks registered before it give, or else
`PS1` (after parameter expansion, with the `prompt-vars` variables, but before `%` expansion, which is done on the
prompt the hook returns). So an extension can add to the prompt of another one, or to `PS1`, instead of replacing it:

```rhai
// ~/.config/luish/plugins/status.rhai
sh::hook("prompt-rewrite", |prev| {
    let status = sh::last_status();
    if status == 0 { prev } else { `%F{red}[${status}]%f ${prev}` }
});
```


## Standard plugins

Luish includes a standard collection of plugins, called `std`. It is not
enabled by default, but you can enable it in `config.toml`:

```toml
[plugins.enabled]
std.completion = "*"        # common commands, and git (git-completion)
std.bash-completion = "*"
```

The `std` library is tied to the version of luish, so it is not affected by
`plugin update`.

- **`completion`** (a directory) completes the options of about 70 common
  commands, with their descriptions, the values of the options (`ls --sort=`,
  `cp -t DIR`, `tar --format=`, `dd conv=`, ...) and their other arguments. It includes
  coreutils (`ls`, `cp`, `mv`, `rm`, `mkdir`, `tail`, `date`, `dd`, ...), grep,
  diff, cmp, tar (the files in the archive, for `tar -xf ARCHIVE`), make (the
  targets of the makefile, also with `-C DIR` and `-f FILE`), rsync, man (the
  pages, in the section given), ssh, scp and sftp (the hosts of
  `~/.ssh/config`, with the files it includes, and of `/etc/hosts`, also after
  `USER@`), and pkill, pgrep and killall (the running processes). Short options
  can be combined: `ls -la` offers the options that
  can follow. It loads `git-completion` too. Its modules are compiled on the
  first Tab (a few milliseconds), so loading it costs little.
- **`git-completion`** (`git-completion.rhai`) completes git's commands, with
  their descriptions, and aliases. It completes arguments, branches, tags, the
  end of a range (`main..`), remotes, stashes, worktrees, and the files the
  command can act on (modified and untracked files for `git add`, staged ones
  for `git restore --staged`, ...).
- **`bash-completion`** uses
  [bash-completion](https://github.com/scop/bash-completion), which completes
  the arguments of about a thousand commands, and for which many programs
  install completion files. This requires bash-completion to be installed and
  runs bash to ask for the completions, and it does not provide descriptions.

## The `sh` module

Extensions reach the shell through the `sh` module:

| Function | Description |
|---|---|
| `sh::hook(kind, fn)` | Register a hook: `"chpwd"`, `"post-rc"`, `"prompt-vars"` or `"prompt-rewrite"` |
| `sh::completer(command, fn)` | Register a completer for a command's arguments (`-default-` for the others) |
| `sh::getvar(name)` | The variable's value, or `()` if it is unset |
| `sh::setvar(name, value)` | Set a shell variable. Throws an error if it is readonly |
| `sh::export(name)`, `sh::unsetvar(name)` | Export or unset a variable |
| `sh::cwd()` | The current directory (as `$PWD`) |
| `sh::plugin_dir()` | The plugin's directory |
| `sh::last_status()` | `$?` |
| `sh::interactive()` | Whether the shell is interactive |
| `sh::run(script)` | Run shell code in the current shell, as `eval` does, and return its status. If it runs `exit`, the extension stops and the shell exits |
| `sh::capture(script)` | Run shell code in a subshell, as `$(...)` does, and return `#{status, out}`, with trailing newlines removed from `out` |
| `sh::quote(text)` | `text` quoted for the shell (in single quotes). Given an array, its strings quoted and separated by spaces |
| `sh::write(fd, text)` | Write text, unbuffered, to fd 1 or 2 |

Rhai's `print(text)` and `debug(text)` write a line to standard output and standard error.

## The `fs` module

The `fs` module answers common questions about files without starting a process, which matters for code that runs
at every prompt or directory change. Relative paths are relative to the current directory. A question about a file
that doesn't exist gives `false` or `()` (use `??` for a default), not an error.

| Function | Description |
|---|---|
| `fs::exists(path)` | Whether the file exists (following symbolic links, as `test -e`) |
| `fs::is_file(path)`, `fs::is_dir(path)` | Whether it is a regular file or a directory (following symbolic links) |
| `fs::is_link(path)` | Whether it is a symbolic link (even one that points nowhere) |
| `fs::kind(path)` | `"file"`, `"dir"`, `"link"`, `"fifo"`, `"socket"`, `"block"` or `"char"` (not following a symbolic link), or `()` |
| `fs::is_readable(path)`, `fs::is_writable(path)`, `fs::is_executable(path)` | Access checks, as `test -r`, `-w` and `-x` |
| `fs::size(path)` | The size in bytes, or `()` |
| `fs::mtime(path)` | The modification time, in seconds since 1970, or `()` |
| `fs::newer(a, b)`, `fs::older(a, b)` | Whether `a` was modified after (before) `b`. A file that exists is newer than one that doesn't, as in `make`, so `fs::newer(source, cache)` is true when the cache is missing |
| `fs::read_file(path)` | The file's contents, or `()` if it can't be read |
| `fs::list_dir(path)` | The names in a directory, sorted, without `.` and `..`, or `()` if it can't be read |
| `fs::readlink(path)` | Where a symbolic link points, or `()` |
| `fs::find_up(name)`, `fs::find_up(name, dir)` | The path of the nearest `name` in the current directory (or `dir`) or one of its parents, or `()`. For example, `fs::find_up(".git")` |

```rhai
// Rebuild a cache only when its source changed.
if fs::newer("aliases.txt", `${sh::getvar("HOME")}/.cache/aliases`) {
    sh::run("make-alias-cache");
}
```

Integers in Rhai are 64-bit, as in shell arithmetic. Floating-point numbers are available too, for example to time
things with `timestamp()` and `.elapsed`.

## The `vcs` module

The `vcs` module tells an extension about the version-control repository around
a directory, as zsh's `vcs_info` does. Only git is supported for now.

`vcs::info()` (or `vcs::info(dir)`) finds the repository from the current
directory (or `dir`) by reading the repository's files, without running git, so
it is cheap enough to call often. It returns `()` outside a repository, and
otherwise a map:

| Field | Description | `vcs_info` |
|---|---|---|
| `vcs` | `"git"` | `%s` |
| `root` | The top directory of the work tree | `%R` |
| `name` | The last component of `root` | `%r` |
| `subdir` | Where the directory is under `root` (`"."` at the top) | `%S` |
| `git_dir` | The git directory (for a worktree or a submodule, the one its `.git` file names) | |
| `branch` | The current branch (while rebasing, the branch being rebased), or `()` when `HEAD` is detached | `%b` |
| `head` | The commit `HEAD` is at (the full hash), or `()` on a branch without commits | `%i` |
| `action` | The operation in progress, or `()`: `"merge"`, `"rebase-i"`, `"rebase-m"`, `"rebase"`, `"am"`, `"am/rebase"`, `"cherry"`, `"cherry-seq"`, `"revert"`, `"cherry-or-revert"` or `"bisect"` | `%a` |
| `step`, `steps` | For a rebase or `git am`, the current step and the number of steps, or `()` | |
| `stashes` | The number of stash entries | |

`vcs::status()` (or `vcs::status(dir)`) runs `git status`, for what needs the index and the work tree, so it costs
more (as `check-for-changes` does in `vcs_info`). It returns `()` outside a repository or if git fails, and otherwise
a map:

| Field | Description |
|---|---|
| `staged` | The number of files with changes in the index (`%c` in `vcs_info`) |
| `unstaged` | The number of files with changes in the work tree that are not in the index (`%u`) |
| `untracked` | The number of untracked files |
| `conflicts` | The number of files with merge conflicts |
| `clean` | Whether all of the above are 0 |
| `upstream` | The upstream branch (such as `"origin/main"`), or `()` |
| `ahead`, `behind` | The number of commits the branch is ahead of and behind its upstream |

git runs with `--no-optional-locks`, so that a prompt doesn't get in the way of git commands running at the same
time.

### Example: the branch in the prompt

```rhai
// ~/.config/luish/plugins/vcs.rhai
sh::hook("prompt-vars", || {
    let i = vcs::info();
    if i == () {
        return #{branch_info: ""};     // outside a repository
    }
    let branch = i.branch ?? i.head?.sub_string(0, 7) ?? "?";
    let action = if i.action == () { "" } else { `|${i.action}` };
    let dirty = if vcs::status()?.clean ?? true { "" } else { "*" };
    #{branch_info: `(${i.vcs})-[${branch}${action}${dirty}] `}
});
```

```sh
PS1='$branch_info$PWD\$ '
```

`vcs::info` is cheap enough to call before every prompt; `vcs::status` runs git, which can be slow in a large
repository.

## Text and bytes

Shell data (variables, paths) are bytes, while Rhai strings hold UTF-8 text. Text that is valid UTF-8 is passed
unchanged. Each byte that is not part of valid UTF-8 becomes one of the characters U+10FF80 to U+10FFFF, and turns
back into that byte when the string goes back to the shell. So a directory name that isn't valid UTF-8 survives a
round trip through an extension. A string containing a NUL character can't be stored in a shell variable.

## Example: completing a command's arguments

Plugins can register completers for specific commands. A completer is called
when Tab is pressed on an argument of its command (also after `sudo`, `env` and
the like, and through an alias: with `alias g='git -C ~/src'`, `g ` calls the
completer for `git` with the words `git`, `-C` and `~/src` first).

A completer receives the words of the command, unquoted, starting with the
command name, and the index of the word being completed. That word ends at the
cursor, and may be empty; the words after it (if the cursor is not at the end
of the command) come after it in the array.

A completer should return an array of candidates or `()`.

A candidate is a string, or a map with a `value` and optionally a `desc`, shown
next to it in the completion menu, and a `suffix`, added after the value when
it is the only match (a space by default; `""` for none).

luish keeps the candidates that start with the word typed, and quotes what it
adds. So a completer can simply return everything that could come next.

To complete only the end of the word, such as what follows `=` in `--format=`,
a completer returns a map with the candidates and the start of the word they
leave alone, which must be a prefix of the word:

```rhai
if words[i].starts_with("--format=") {
    return #{prefix: "--format=", candidates: ["json", "yaml"]};
}
```

The completer registered for `-default-` (as in zsh's `compdef`) is called for
the commands that have no completer of their own and whose arguments luish
doesn't complete itself.

To see what Tab offers for a command line without typing it, for example while
writing a completer or in a test, use `__luish_internal complete LINE`, which
prints each match on a line of its own (the text that replaces the word, then a
tab and the description), in any shell:

```sh
$ __luish_internal complete 'git sw'
switch 	Switch branches
```

Commands that a completer runs are not jobs: like those of `$(...)`, they can't
be stopped with Ctrl-Z, and Ctrl-C does not reach them (the terminal is in the
line editor's mode). Their output goes to the terminal, over the command
line, so use `sh::capture` or redirect it.

```rhai
// ~/.config/luish/plugins/git.rhai

sh::completer("git", |words, i| {
    if i == 1 {
        return [
            #{value: "add", desc: "Add file contents to the index"},
            #{value: "commit", desc: "Record changes to the repository"},
            #{value: "switch", desc: "Switch branches"},
            #{value: "--git-dir=", suffix: ""},
        ];
    }
    if words[1] == "switch" {
        let r = sh::capture("git branch --format='%(refname:short)' 2>/dev/null");
        return if r.status == 0 { r.out.split("\n") } else { [] };
    }
    ()   // the default: filenames
});
```


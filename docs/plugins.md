# Plugins

luish can be extended with plugins written in [Rhai](https://rhai.rs), a small scripting language designed for
embedding. Plugins are opt-in: nothing is loaded unless you ask for it, and a shell that loads no plugins pays nothing
for them.

Plugin support is new. For now, a plugin can run code whenever the current directory changes (the `chpwd` hook),
give the prompt (the `prompt` hook), provide Tab completion for the arguments of commands, query files (the `fs`
module), and ask about git repositories (the `vcs` module).

## Loading plugins

```sh
plugin load NAME|PATH...   # load plugins (loading one again reloads it)
plugin list                # print the names of the loaded plugins
plugin unload NAME...      # remove plugins and their hooks
```

`plugin` is a built-in only in interactive shells, so that scripts find the same commands as in other shells. In a
script, use `__luish_internal plugin` instead.

`plugin load greet` loads `~/.config/luish/plugins/greet.rhai` (or `$XDG_CONFIG_HOME/luish/plugins/greet.rhai` if
`XDG_CONFIG_HOME` is set), or else the directory `greet/` there (see below). An argument that contains a `/`, such as
`./greet.rhai` or `./greet/`, is a path. A plugin's name is its file name without `.rhai`, or its directory's name.

Plugins are usually loaded from `~/.config/luish/luishrc`, which interactive shells read at startup. They can also
be loaded from the cached startup files in `rc.d/`: the cache records which plugins were loaded and loads them again
(but what a plugin's top level changed in the shell is cached with the rest). Start luish with
`--no-plugins` to make `plugin load` do nothing, for example to check whether a problem comes from a plugin.

`plugin load` runs the plugin's top level once, which registers its hooks and completers. If the plugin has an error, it is
reported with the plugin's file and line, nothing from the plugin stays loaded, and the status is 1.

## Plugins with several files

A plugin can also be a directory, which holds files in Rhai and in shell. luish runs these files in it, if they
exist:

| File | When it runs |
|---|---|
| `plugin.rhai` | When the plugin is loaded, like a single-file plugin |
| `rc.lsh` | When the plugin is loaded, after `plugin.rhai`, in the current shell (as with `.`) |

A directory needs at least one of them (or a `login.lsh`, which luish will run in login shells in a later version).
If `plugin.rhai` fails, `rc.lsh` doesn't run.

Other files in the directory are read only when these files ask for them. In Rhai, `import "util" as u;` loads
`util.rhai` from the plugin's directory, and loading the plugin again reads it again. In shell, use
`$LUISH_PLUGIN_DIR`:

```sh
# ~/.config/luish/plugins/work/rc.lsh
. "$LUISH_PLUGIN_DIR/functions.lsh"
alias gs='git status'
work_dir=$LUISH_PLUGIN_DIR   # for functions that need it later
```

While `plugin.rhai` and `rc.lsh` run, `LUISH_PLUGIN_DIR` is the plugin's directory (an absolute path) and
`LUISH_PLUGIN_NAME` its name. Afterwards they get back the values they had before. In Rhai, `sh::plugin_dir()`
gives the directory at any time, also in hooks. For a single-file plugin it is the directory of the file.

A plugin written only in shell doesn't start Rhai at all. When it is loaded from the cached startup files in
`rc.d`, what its `rc.lsh` did is cached with the rest, so it costs no more than the same lines in `rc.d`: a later
shell loads the plugin's `plugin.rhai` again but doesn't rerun `rc.lsh`. Changing either file makes the next shell
run `rc.d` again.

`plugin unload` removes what `plugin.rhai` registered (hooks), but it can't undo what `rc.lsh` did (aliases,
functions, variables).

## Example: running code when the directory changes

```rhai
// ~/.config/luish/plugins/dirs.rhai

// Named functions can't see the plugin's variables, but closures can.
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

## Example: the prompt

```rhai
// ~/.config/luish/plugins/prompt.rhai

sh::hook("prompt", || {
    // Read $? first: sh::run changes it (the shell's own $? is kept).
    let status = sh::last_status();
    let mark = if status == 0 { "" } else { "%F{red}[" + status + "]%f " };
    sh::run("__prompt_branch=$(git branch --show-current 2>/dev/null)");
    let branch = sh::getvar("__prompt_branch");
    let branch = if branch == () || branch == "" { "" } else { " %F{yellow}(" + branch + ")%f" };
    "%F{blue}%~%f" + branch + " " + mark + "%# "
});
```

A `prompt` hook is called before each prompt, with no arguments (but see below), and returns the prompt, which is used instead of
`PS1` (`PS2`, for the continuation lines of a command, is unchanged). The prompt doesn't go through parameter
expansion, but it does go through `%` expansion if the `promptpercent` option is on (`setopt prompt_percent`, see
[](usage.md)), as in the example.

If several hooks are registered, the one registered last is called first, and the first string returned is the
prompt. A hook that returns `()` leaves the prompt to the hooks before it, and then to `PS1`, so a plugin can give
the prompt only in some directories, for example. A hook that fails is reported, and the next one is tried. `$?` is
the same after the hooks as before them.

A hook that takes a parameter is given the previous prompt: the one the hooks registered before it give, or else
`PS1` (after parameter expansion, but before `%` expansion, which is done on the prompt the hook returns). So a plugin
can add to the prompt of another plugin, or to `PS1`, instead of replacing it:

```rhai
// ~/.config/luish/plugins/status.rhai
sh::hook("prompt", |prev| {
    let status = sh::last_status();
    if status == 0 { prev } else { `%F{red}[${status}]%f ${prev}` }
});
```

Returning `()` from such a hook, or failing, keeps the previous prompt. A hook without a parameter doesn't run the
hooks before it at all, so it costs nothing to have them loaded. The hook must be defined in the plugin's own file
(not in a module it imports), as a closure or a named function (`fn prompt(prev) { ... }`).

## Example: completing a command's arguments

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

A completer is called when Tab is pressed on an argument of its command (also after `sudo`, `env` and the like, and
through an alias: with `alias g='git -C ~/src'`, `g ` calls the completer for `git` with the words `git`, `-C`
and `~/src` first). It gets the words of the command, unquoted, starting with the command name, and the index of the
word being completed. That word ends at the cursor, and may be empty; the words after it (if the cursor is not at the
end of the command) come after it in the array. It returns an array of candidates, or `()` to complete the word as if
there were no completer.

A candidate is a string, or a map with a `value` and optionally a `desc`, shown next to it in the completion menu,
and a `suffix`, added after the value when it is the only match (a space by default; `""` for none). luish keeps
the candidates that start with the word typed, and quotes what it adds. So a completer can simply return everything
that could come next.

To complete only the end of the word, such as what follows `=` in `--format=`, a completer returns a map with the
candidates and the start of the word they leave alone, which must be a prefix of the word:

```rhai
if words[i].starts_with("--format=") {
    return #{prefix: "--format=", candidates: ["json", "yaml"]};
}
```

The completer registered for `-default-` (as in zsh's `compdef`) is called for the commands that have no completer of
their own and whose arguments luish doesn't complete itself (as it does for `cd`, `kill` or `unset`). It is given the
words of the command as any other completer is.

A completer registered for a command replaces any earlier one. If a completer fails, the error is shown below the
command line. A completer that runs for more than 2 seconds is stopped (while it runs a command, the time is only
checked when the command has finished).

Commands that a completer runs are not jobs: like those of `$(...)`, they can't be stopped with Ctrl-Z, and Ctrl-C
does not reach them (the terminal is in the line editor's mode). Their output goes to the terminal, over the command
line, so use `sh::capture` or redirect it.

## Example: programs that complete themselves

Many programs can list the completions of their own arguments. Those built with the Go library
[Cobra](https://cobra.dev), such as `gh`, `docker`, `kubectl` and `helm`, do it when run as
`prog __complete ARGS...`. luish doesn't run programs to ask them (a program that doesn't know the convention might
do something else), but a plugin can, for the programs it names:

```{literalinclude} examples/cobra.rhai
:language: rhai
```

Save it as `~/.config/luish/plugins/cobra.rhai`, change the list of programs at the end, and load it with
`plugin load cobra`.

## Example: using bash's completions

[bash-completion](https://github.com/scop/bash-completion) completes the arguments of about a thousand commands,
and many programs install completion files for it. This plugin is a default completer that runs, in bash, the
function that bash-completion has for the command, and gives luish what it returns. It is a directory with two files:

```{literalinclude} examples/bash-completion/plugin.rhai
:language: rhai
```

```{literalinclude} examples/bash-completion/bridge.bash
:language: bash
```

Copy the directory `docs/examples/bash-completion` to `~/.config/luish/plugins/` and load it with
`plugin load bash-completion`. The commands that have completers of their own (such as those of the Cobra plugin)
keep them. bash-completion is looked for in the usual places; set `BASH_COMPLETION_SCRIPT` to the path of its
`bash_completion` script if it is elsewhere. Each Tab takes about 50 ms, as bash loads bash-completion again, and
bash-completion gives no descriptions.

## The `sh` module

| Function | Description |
|---|---|
| `sh::hook(kind, fn)` | Register a hook: `"chpwd"` or `"prompt"` |
| `sh::completer(command, fn)` | Register a completer for a command's arguments (`-default-` for the others) |
| `sh::getvar(name)` | The variable's value, or `()` if it is unset |
| `sh::setvar(name, value)` | Set a shell variable. Throws an error if it is readonly |
| `sh::export(name)`, `sh::unsetvar(name)` | Export or unset a variable |
| `sh::cwd()` | The current directory (as `$PWD`) |
| `sh::plugin_dir()` | The plugin's directory |
| `sh::last_status()` | `$?` |
| `sh::interactive()` | Whether the shell is interactive |
| `sh::run(script)` | Run shell code in the current shell, as `eval` does, and return its status. If it runs `exit`, the plugin stops and the shell exits |
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

The `vcs` module tells a plugin about the version-control repository around a directory, as zsh's `vcs_info` does.
Only git is supported for now.

`vcs::info()` (or `vcs::info(dir)`) finds the repository from the current directory (or `dir`) by reading the
repository's files, without running git, so it is cheap enough to call often. It returns `()` outside a repository,
and otherwise a map:

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
sh::hook("prompt", || {
    let i = vcs::info();
    if i == () {
        return;     // outside a repository: PS1 (or an earlier prompt hook)
    }
    let branch = i.branch ?? i.head?.sub_string(0, 7) ?? "?";
    let action = if i.action == () { "" } else { `|${i.action}` };
    let dirty = if vcs::status()?.clean ?? true { "" } else { "*" };
    `(${i.vcs})-[${branch}${action}${dirty}] ${i.name}/${i.subdir} $ `
});
```

`vcs::info` is cheap enough to call before every prompt; `vcs::status` runs git, which can be slow in a large
repository.

## Text and bytes

Shell data (variables, paths) are bytes, while Rhai strings hold UTF-8 text. Text that is valid UTF-8 is passed
unchanged. Each byte that is not part of valid UTF-8 becomes one of the characters U+10FF80 to U+10FFFF, and turns
back into that byte when the string goes back to the shell. So a directory name that isn't valid UTF-8 survives a
round trip through a plugin. A string containing a NUL character can't be stored in a shell variable.

## Interrupting plugins

Ctrl-C stops plugin code that is running, just as it stops a command. A plugin stopped this way gives status 130.

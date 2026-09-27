# Plugins

A **plugin** adds to luish: it is a directory of shell and Rhai files, or a single Rhai file. The Rhai part of a
plugin is its **extension**: code in [Rhai](https://rhai.rs), a small scripting language designed for embedding, that
runs inside the shell and registers hooks and completers. A plugin written only in shell, as zsh plugins are, has no
extension, and a single `.rhai` file is a plugin that is just an extension. Plugins are opt-in: nothing is loaded
unless you ask for it, and a shell that loads no plugins pays nothing for them.

Plugin support is new. For now, an extension can run code whenever the current directory changes (the `chpwd` hook),
provide variables for the prompt (the `prompt-vars` hook, as a plugin's `prompt-vars.lsh` file can) or rewrite it
entirely (the `prompt-rewrite` hook), provide Tab completion for the arguments of commands, query files (the `fs`
module), and ask about git repositories (the `vcs` module).

## Loading plugins

```sh
plugin load NAME|PATH...   # load plugins (loading one again reloads it)
plugin list-loaded         # print the names of the loaded plugins
plugin list-available      # print the names of the plugins in the plugin directory
plugin unload NAME...      # remove plugins and their hooks
```

`plugin` is a built-in only in interactive shells, so that scripts find the same commands as in other shells. In a
script, use `__luish_internal plugin` instead.

`plugin load greet` loads `~/.config/luish/plugins/greet.rhai` (or `$XDG_CONFIG_HOME/luish/plugins/greet.rhai` if
`XDG_CONFIG_HOME` is set), or else the directory `greet/` there (see below). An argument that contains a `/`, such as
`./greet.rhai` or `./greet/`, is a path. A plugin's name is its file name without `.rhai`, or its directory's name.

Plugins are usually loaded from `~/.config/luish/luishrc`, which interactive shells read at startup. They can also
be loaded from the cached startup files in `rc.d/`: the cache records which plugins were loaded and loads them again
(but what the plugin changed in the shell is cached with the rest). Start luish with
`--no-plugins` to make `plugin load` do nothing, for example to check whether a problem comes from a plugin.

`plugin load` runs the top level of the plugin's extension once, which registers its hooks and completers. If the
extension has an error, it is reported with its file and line, nothing from the plugin stays loaded, and the status
is 1.

## Plugins with several files

A plugin can also be a directory, which holds files in Rhai and in shell. luish runs these files in it, if they
exist:

| File | When it runs |
|---|---|
| `extension.rhai` | When the plugin is loaded. It is the plugin's extension, like the file of a single-file plugin |
| `rc.lsh` | When the plugin is loaded, after `extension.rhai`, in the current shell (as with `.`) |
| `prompt-vars.lsh` | Before each prompt, to set variables for `PS1` (see [below](#variables-for-the-prompt-prompt-vars)) |

A directory needs at least one of them (or a `login.lsh`, which luish will run in login shells in a later version).
If `extension.rhai` fails, `rc.lsh` doesn't run.

Other files in the directory are read only when these files ask for them. In Rhai, `import "util" as u;` loads
`util.rhai` from the plugin's directory, and loading the plugin again reads it again. In shell, use
`$LUISH_PLUGIN_DIR`:

```sh
# ~/.config/luish/plugins/work/rc.lsh
. "$LUISH_PLUGIN_DIR/functions.lsh"
alias gs='git status'
work_dir=$LUISH_PLUGIN_DIR   # for functions that need it later
```

While `extension.rhai`, `rc.lsh` and `prompt-vars.lsh` run, `LUISH_PLUGIN_DIR` is the plugin's directory (an absolute path) and
`LUISH_PLUGIN_NAME` its name. Afterwards they get back the values they had before. In Rhai, `sh::plugin_dir()`
gives the directory at any time, also in hooks. For a single-file plugin it is the directory of the file.

A plugin written only in shell doesn't start Rhai at all. When it is loaded from the cached startup files in
`rc.d`, what its `rc.lsh` did is cached with the rest, so it costs no more than the same lines in `rc.d`: a later
shell loads the plugin's `extension.rhai` again but doesn't rerun `rc.lsh`. Changing either file makes the next shell
run `rc.d` again.

`plugin unload` removes what `extension.rhai` registered (hooks), but it can't undo what `rc.lsh` did (aliases,
functions, variables).

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

## Customizing the prompt

Plugins can change the prompt (`PS1`; `PS2`, for the continuation lines of a command, is unchanged) in two ways:

- **`prompt-vars`** (start here): a plugin computes values, such as the git branch, and gives them to `PS1` as
  variables. You keep writing the prompt in `PS1`, and the variables exist only while the prompt is built.
- **`prompt-rewrite`** (advanced): an extension writes the whole prompt itself, and `PS1` is ignored or passed to it.

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

The same can be written in shell, in a directory plugin's `prompt-vars.lsh`. Every variable it sets is available to
`PS1`:

```sh
# ~/.config/luish/plugins/branch/prompt-vars.lsh
git_branch=$(git branch --show-current 2>/dev/null)
[ -n "$git_branch" ] && git_branch=" ($git_branch)"
```

Before each prompt, luish:

1. runs the `prompt-vars` hooks and `prompt-vars.lsh` files, plugin by plugin in the order the plugins were loaded
   (a plugin's hooks before its file), each setting its variables;
2. expands `PS1` with those variables (parameter expansion, then `%` expansion under `prompt.percent`);
3. puts every variable they changed back as it was (or unsets it).

So the variables don't leak into the shell: after the prompt, `echo $git_branch` prints what it printed before, and
the commands you run don't see them. A plugin loaded later sees the variables of the ones loaded before it, and can
change them. Each hook and file sees `$?` of the last command (read it first thing in `prompt-vars.lsh`, before
another command changes it), and `$?` is the same after the prompt as before.

A hook's map gives each variable a string, a number or a boolean (which become text, such as `42` or `true`), or
`()` to unset the variable while the prompt is built. A hook can also return `()` to set nothing. A bad entry (not a
valid name, a readonly variable, another type of value) is reported and skipped, and a hook that fails is reported;
the other hooks and files still run.

`prompt-vars.lsh` runs in the current shell, as with `.`, so it can use the shell's functions and variables. Only
variables are put back afterwards: don't `cd`, define functions or aliases, or change options in it. Its commands run
before every prompt, so keep them fast; prefer the `vcs` and `fs` modules in a `prompt-vars` hook to running `git` in
shell, when you can.

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
4. The variables set by `prompt-vars` are put back. What a `prompt-rewrite` hook itself changes, with `sh::setvar` or
   `sh::run`, stays.

`$?` is the same after the hooks as before them, and each hook sees that of the last command (but `sh::run` changes
it for the rest of the hook, so read `sh::last_status()` first).

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

Returning `()` from such a hook, or failing, keeps the previous prompt. A hook without a parameter doesn't run the
hooks before it at all, so it costs nothing to have them loaded, but it also discards them: loading a plugin with a
`prompt-rewrite` hook that takes no parameter hides the prompt of every plugin loaded before it, and `PS1`. The hook
must be defined in the extension's own file (not in a module it imports), as a closure or a named function
(`fn prompt(prev) { ... }`).

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
do something else), but an extension can, for the programs it names:

```{literalinclude} examples/cobra.rhai
:language: rhai
```

Save it as `~/.config/luish/plugins/cobra.rhai`, change the list of programs at the end, and load it with
`plugin load cobra`.

## Example: using bash's completions

[bash-completion](https://github.com/scop/bash-completion) completes the arguments of about a thousand commands,
and many programs install completion files for it. This plugin's extension is a default completer that runs, in
bash, the function that bash-completion has for the command, and gives luish what it returns. The plugin is a
directory with two files:

```{literalinclude} examples/bash-completion/extension.rhai
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

Extensions reach the shell through the `sh` module:

| Function | Description |
|---|---|
| `sh::hook(kind, fn)` | Register a hook: `"chpwd"`, `"prompt-vars"` or `"prompt-rewrite"` |
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

The `vcs` module tells an extension about the version-control repository around a directory, as zsh's `vcs_info` does.
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
sh::hook("prompt-vars", || {
    let i = vcs::info();
    if i == () {
        return #{vcs_info: ""};     // outside a repository
    }
    let branch = i.branch ?? i.head?.sub_string(0, 7) ?? "?";
    let action = if i.action == () { "" } else { `|${i.action}` };
    let dirty = if vcs::status()?.clean ?? true { "" } else { "*" };
    #{vcs_info: `(${i.vcs})-[${branch}${action}${dirty}] `}
});
```

```sh
PS1='$vcs_info$PWD\$ '
```

`vcs::info` is cheap enough to call before every prompt; `vcs::status` runs git, which can be slow in a large
repository.

## Text and bytes

Shell data (variables, paths) are bytes, while Rhai strings hold UTF-8 text. Text that is valid UTF-8 is passed
unchanged. Each byte that is not part of valid UTF-8 becomes one of the characters U+10FF80 to U+10FFFF, and turns
back into that byte when the string goes back to the shell. So a directory name that isn't valid UTF-8 survives a
round trip through an extension. A string containing a NUL character can't be stored in a shell variable.

## Interrupting extensions

Ctrl-C stops extension code that is running, just as it stops a command. An extension stopped this way gives status
130.

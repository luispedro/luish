# Extensions

An extension is code written in [Rhai](https://rhai.rs) that runs inside the
shell. It can register hooks (e.g., run when the directory changes, or before
each prompt), Tab completion for commands, and commands of its own. As it runs
in the shell's own process, it can reading files and other data without
starting a process.

## Writing an extension

Here is a simple extension that adds a builtin called `hello`:

```rhai
// ~/.config/luish/plugins/hello.rhai
sh::builtin("hello", |argv| {
    let who = if argv.len() > 1 { argv[1] } else { "world" };
    print(`Hello, ${who}!`);
});
```

```console
$ plugin load hello
$ hello there
Hello, there!
$ hello
Hello, world!
```

The top level of the extension runs once, when its plugin is loaded (in each shell that loads it: extensions are not
cached with the startup files). It registers functions with the [`sh` module](#the-sh-module); they stay registered
until the plugin is unloaded (`plugin unload`) or loaded again. Loading the first extension costs about a millisecond,
once (see [](performance.md#commands-in-rhai)).

`sh::plugin_dir()` gives the plugin's directory at any time, also in hooks. For a single-file plugin it is the
directory of the file. `sh::plugin_options()` gives the options the plugin was loaded with (see [Plugin
options](plugins.md#plugin-options-plugin-options)); a plugin loaded with `-c` has none.

## Code without a file: `plugin load -c`

`plugin load -c CODE NAME` loads Rhai code given on the command line as the plugin `NAME`, as if it were the file
`NAME.rhai`:

```console
$ plugin load -c 'sh::hook("chpwd", |from, to| print(`now in ${to}`))' where
$ cd /tmp
now in /tmp
```

Its directory, for `import` and `sh::plugin_dir()`, is the current directory when it is loaded. Loading it again
with `-c` replaces it, and `plugin unload NAME` removes it. It doesn't replace a plugin of the same name loaded from a
file (unload that one first), but a plugin loaded from a file replaces it, with a warning.

## Running Rhai code once: `plugin run`

To run Rhai code without loading a plugin, as a script or to try something out, use `plugin run` with a file or
with `-c` and the code:

```console
$ plugin run -c 'print(sh::which("git"))'
/usr/bin/git
$ cat count.rhai
print(`${argv.len() - 1} arguments`);
argv.len() > 1
$ plugin run ./count.rhai a b; echo $?
2 arguments
0
```

The code has the [`sh`](#the-sh-module), [`fs`](#the-fs-module) and [`vcs`](#the-vcs-module) modules, as an extension
does, and `argv`: the file (or `-c`) and the arguments. Its last value (or `return`'s) is the exit status if it is an
integer or a boolean, as for [a command](#commands-written-in-rhai-shbuiltin). As it isn't part of a plugin, it can't
register hooks, completers or commands (use `plugin load -c` for that), and `sh::plugin_dir()` and
`sh::plugin_options()` are errors; it leaves
the loaded plugins alone, even one with the same name as the file. `import` is relative to the file, or with `-c`, to
the current directory, and its modules are read again on each run.

Rhai's `eval`, which compiles and runs a string as code, is disabled in
extensions. If you need very flexible code, use a shell file instead.

## Hooks

`sh::hook(kind, fn)` registers a function to be called on an event: `chpwd`, `precmd`, `preexec`, `exit`, `post-rc`,
`prompt-vars` or `prompt-rewrite`. These hooks are luish's way of running code on such events: unlike zsh, luish
doesn't call shell functions with special names (`chpwd`, `precmd`, `zshexit` ...). A hook can call a shell function
with `sh::run`.

### Running code when the directory changes: `chpwd`

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

### Before each prompt and each command: `precmd` and `preexec`

```rhai
// ~/.config/luish/plugins/timing.rhai

// Report the commands that took more than 10 seconds.
let started = ();
sh::hook("preexec", |line| { started = timestamp(); });
sh::hook("precmd", |status| {
    if started != () && started.elapsed > 10.0 {
        sh::write(2, "took " + started.elapsed.to_int() + "s (status " + status + ")\n");
    }
    started = ();
});
```

In interactive shells, `precmd` hooks run before each prompt (but not before `PS2`), before the prompt is built (so
before `prompt-vars`), with the exit status of the last command as an integer. `preexec` hooks run after a command
is read and added to the history, before it runs, with its text as typed (before alias expansion, without the final
newline, and with all the lines of a command that spans several). A line with several commands (`a; b`) gives one
`preexec` call, with the whole line.

`$?` is the same after the hooks as before them. A hook that fails is reported, and the other hooks still run. A hook
that runs `exit` (through `sh::run`) exits the shell. Neither hook runs in scripts or in `-c` commands, even with
`-i`.

### When the shell exits: `exit`

```rhai
// Remove this shell's scratch directory, made when the extension was loaded.
let scratch = sh::capture(["mktemp", "-d"]).out;
sh::setvar("SCRATCH", scratch);
sh::hook("exit", |status| { sh::capture(["rm", "-rf", scratch]); });
```

`exit` hooks run when the shell exits, with `exit` or at the end of its input,
after the `EXIT` trap, with the exit status as an integer (also `$?`). They run
in scripts too (check `sh::interactive()` if that isn't wanted), but not when a
subshell exits, nor when the shell is replaced by `exec` or killed by a signal.
A shell with `exit` hooks doesn't replace itself with the last command of a
script or of `-c`, as it otherwise does. A hook that runs `exit N` changes the
status the shell exits with, and the hooks after it don't run.

### After the startup files: `post-rc`

`post-rc` hooks run after the startup files in `rc.d`, after the `post-rc.lsh` files of the plugins (see
[After the startup files](plugins.md#after-the-startup-files-post-rc) for the order). So an extension can read the
variables that you set in `rc.d` to configure it, even though its plugin was loaded before `rc.d`:

```rhai
// extension.rhai
sh::hook("post-rc", || {
    let style = sh::getvar("MYPROMPT_STYLE") ?? "plain";
    // ...
});
```

As `post-rc.lsh`, they run only in interactive shells, but unlike what `post-rc.lsh` does, which is cached with
`rc.d`, they run in every shell. A plugin loaded later (in `luishrc`, or at the prompt) runs its `post-rc` hooks right
after it is loaded.

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

Before each prompt, the `prompt-vars` hooks run with the `prompt-vars.lsh`
files of the plugins, plugin by plugin in the order they were loaded, and `PS1`
is expanded with their variables, which are then put back as they were (see
[Customizing the prompt](plugins.md#customizing-the-prompt)).

A hook's map gives each variable a string, a number or a boolean (which become
text, such as `42` or `true`), or `()` to unset the variable while the prompt
is built. A hook can also return `()` to set nothing. A bad entry (not a valid
name, a readonly variable, another type of value) is reported and skipped, and
a hook that fails is reported; the other hooks and files still run.

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

## Completing a command's arguments

Extensions can register completers for specific commands. A completer is called
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
        let r = sh::capture(["git", "branch", "--format=%(refname:short)"]);
        return if r.status == 0 { r.out.split("\n") } else { [] };
    }
    ()   // the default: filenames
});
```

## Commands written in Rhai: `sh::builtin`

`sh::builtin(name, fn)` adds a command that functions like a new shell builtin.

It is called with the command's words as an array of strings, its name first
(as `argv[0]`, so the arguments are `argv[1]`, `argv[2]`, ...)

```rhai
// ~/.config/luish/plugins/urlencode.rhai

// urlencode TEXT...: each TEXT percent-encoded, one per line.
sh::builtin("urlencode", |argv| {
    if argv.len() < 2 { throw "usage: urlencode TEXT..."; }
    for text in argv.extract(1) {
        let out = "";
        for c in text.chars() {
            if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || "._~-".contains(c) {
                out += c;
            } else {
                let h = c.to_int().to_hex().to_upper();
                out += if h.len() < 2 { `%0${h}` } else { `%${h}` };
            }
        }
        print(out);
    }
});
```

- **Exit status**: what the function returns: `()` (nothing) is 0, `true` 0 and
  `false` 1, and an integer is taken modulo 256, as `return` does. An error
  gives 1: a string thrown with `throw` is printed as the command's error
  message (`urlencode: usage: urlencode TEXT...`), and other errors with the
  extension's file and position.
- **Output** goes to standard output with `print`, or `sh::write`.
- **Input**: `sh::read_line()` reads a line of standard input, without its
  newline, or gives `()` at the end. Like the `read` built-in, it reads a byte
  at a time, so the rest is left for the commands after it. To read a whole
  file, `fs::read_file` is faster.
- **Lookup**: it ranks as a regular built-in, so a shell function with the same
  name comes first, and `builtin NAME` or `command NAME` still run it. `type`
  shows it as a shell builtin from its plugin. The names of luish's own
  built-ins can't be taken. Registering a name again replaces the command, and
  it goes away when its plugin is unloaded.
- **Ctrl-C** stops it with status 130, as it stops any extension code.

Loops, arithmetic and string processing run two to three times as fast in rhai as in
shell code (see [Performance](performance.md#commands-in-rhai)).

## Splitting an extension into modules: `import`

An extension can use Rhai modules, other `.rhai` files of the plugin: `import
"NAME" as m;` reads `NAME.rhai`, runs its top level once, and makes its
functions available as `m::f()`. `NAME` is relative to the directory of the
file that has the `import`, so `extension.rhai` finds its modules in the
plugin's directory, and a module in a
subdirectory (`import "hts/samtools"`) finds its own neighbours there (`import
"common"` in `hts/samtools.rhai` reads `hts/common.rhai`). This holds wherever
the code runs: in a function or closure of the module, even when an extension
calls it. `NAME` can also be an absolute path, without `.rhai`.

A plugin can use the modules of another plugin: `import "@SOURCE/PLUGIN/MODULE"` reads `MODULE.rhai` in the
directory of the plugin `SOURCE/PLUGIN`, such as `import "@std/completion/lib"` for std's completion engine (see
[Reusing std's completion engine](#reusing-stds-completion-engine)). `PLUGIN` is the plugin's path in its collection
(`@extra/complete/bio/specs` is `specs.rhai` in `extra/complete/bio`). That plugin must be loaded, so list it in the
`[dependencies]` of your [`plugin.toml`](plugins.md#dependencies-options-aliases-and-key-bindings-plugintoml) (here
`std.completion = "*"`), which loads it first. It is found by its name (see [](plugins.md#names)), or, for a plugin
whose name has no source (one of the plugin directory, or loaded by path), by `PLUGIN` alone. The module's own imports
are relative to its files, as above. Within your own source, prefer relative imports (`import "../lib/util"`): the
user names the source.

luish keeps each module once it is compiled, until a plugin is loaded again. An `import` inside a function (a
completer, for instance) reads the module only when the function first runs, so a large plugin loads quickly.

## The `sh` module

Extensions reach the shell through the `sh` module:

| Function | Description |
|---|---|
| `sh::hook(kind, fn)` | Register a hook: `"chpwd"`, `"precmd"`, `"preexec"`, `"exit"`, `"post-rc"`, `"prompt-vars"` or `"prompt-rewrite"` |
| `sh::completer(command, fn)` | Register a completer for a command's arguments (`-default-` for the others) |
| `sh::builtin(name, fn)` | Register a command, called with its words (the name first); see [Commands written in Rhai](#commands-written-in-rhai-shbuiltin) |
| `sh::read_line()` | A line of standard input without its newline, or `()` at the end |
| `sh::getvar(name)` | The variable's value (`$name`: an array's first element), or `()` if it is unset |
| `sh::getarray(name)` | The variable's elements (`"${name[@]}"`) as an array of strings: a string is one element, and an associative array gives its values. `()` if it is unset |
| `sh::getmap(name)` | An associative array as a map, or `()` if the variable is unset or isn't one |
| `sh::setvar(name, value)` | Set a shell variable to a string, to an array of strings (as `name=(...)`) or to a map of strings (an associative array). Throws an error if it is readonly |
| `sh::export(name)`, `sh::unsetvar(name)` | Export or unset a variable |
| `sh::cwd()` | The current directory (as `$PWD`) |
| `sh::plugin_dir()` | The plugin's directory |
| `sh::plugin_options()` | The plugin's options, as a map of strings, integers and booleans (see [Plugin options](plugins.md#plugin-options-plugin-options)) |
| `sh::last_status()` | `$?` |
| `sh::interactive()` | Whether the shell is interactive |
| `sh::which(name)` | The file that running `name` executes, found as the shell finds it (in `PATH`, or `name` itself if it has a `/`), but never a function or a built-in; `()` if there is no such executable |
| `sh::commands(prefix)` | The names of the executables in `PATH` that start with `prefix`, sorted: those that the command line completes |
| `sh::run(script)` | Run shell code in the current shell, as `eval` does, and return its status. If it runs `exit`, the extension stops and the shell exits |
| `sh::capture(argv)`, `sh::capture(argv, stderr)` | Run a program, `argv[0]` (found in `PATH`, never a function or a built-in), with the arguments `argv[1..]`, none of which is parsed as shell code, and standard input from /dev/null. Return `#{status, out}`, with trailing newlines removed from `out`. Its standard error is discarded, or with `stderr`: `"discard"`, `"inherit"` (the shell's), `"merge"` (into `out`, as `2>&1`) or `"return"` (as `err`, without trailing newlines). Set variables for it with `env`: `["env", "COLUMNS=400", "prog", "-h"]` |
| `sh::capture_sh(script)` | Run shell code in a subshell, as `$(...)` does, and return `#{status, out}`, with trailing newlines removed from `out`. Build it with `sh::quote` |
| `sh::quote(text)` | `text` quoted for the shell (in single quotes). Given an array, its strings quoted and separated by spaces |
| `sh::write(fd, text)` | Write text, unbuffered, to fd 1 or 2 |
| `sh::matches(pattern, text)` | Whether `text` matches the shell pattern as a whole, as `case $text in $pattern)` matches it: `*`, `?` and bracket expressions (with ranges and classes such as `[:digit:]`), a backslash escaping the next character, and no special `/` or leading `.`. `!sh::matches("*[!A-Za-z0-9_-]*", name)` checks a name's characters. It matches bytes, so `?` matches one byte of a character that isn't ASCII |
| `sh::expand_prompt(text)` | `text` with its `%` sequences expanded, as in prompts and `print -P` (whether or not `prompt.percent` is on): `sh::write(1, "\x1b]0;" + sh::expand_prompt("%n@%m: %~") + "\x07")` sets the terminal's title |

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
| `fs::realpath(path)` | The absolute path of the file, with every symbolic link (in the file's name and in its directories), `.` and `..` resolved, as `realpath`; `()` if it doesn't exist |
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

## Reusing std's completion engine

`std.completion` completes its commands from **specs**, maps that describe a
command's options, their values, its subcommands and its arguments, with an
engine that other plugins can use for their own commands: `lib::complete(spec,
words, i)`, from `import "@std/completion/lib"`, completes the word `words[i]`
of the command `words`, as a completer does.

The spec format, the kinds and `lib::complete` are part of luish's interface:
std is tied to the version of luish, and later versions keep accepting the
specs of earlier ones.

### Specs

A spec has these fields, all optional:

| Field | Meaning |
|---|---|
| `opts` | The options, as a table in the style of `--help` (see below). |
| `values` | What the values of options complete to, by one of the option's names: a kind or an array of candidates. By default, filenames. |
| `args` | The kinds of the arguments that aren't options, in order; the last one repeats. By default, `["files"]`. |
| `args_if` | The `args` to use instead when an option is given, by one of the option's names (such as `grep -e PATTERN`, after which the first argument is a file). |
| `commands` | The subcommands (`cargo build`), as a table like `opts`: one line per subcommand, with its names (`build, b`), two spaces and its description. The first word that isn't an option is the subcommand, and the rest of the line is completed with its spec. |
| `subs` | The specs of the subcommands, by their first name (by default, a spec with no options and files as arguments). |
| `sub_spec` | For the subcommands not in `subs`: `MODULE:NAME`, for `MODULE::sub_spec(NAME, SUBCOMMAND)`, which gives the spec of a subcommand (or `()`) when it is needed (from its `--help`, for instance). |
| `common` | Options that are also valid after the subcommand, in any subcommand (and in theirs), as a table like `opts`. |
| `modes` | Specs by the name of an option that chooses an operation (`pacman -S`): once it is given, the line is completed with its spec, whose options are added to the command's. |
| `single_dash` | True for a command whose long options start with a single `-` and can't be combined (`find -name`, `gcc -Wall`). An option written `-name=ARG` takes its value after `=`, and `-name ARG` as the next word. |
| `strict_eq` | True for a command whose long options take their value after `=` only if the table says so (`--name=ARG`, not `--name ARG`). Otherwise a long option that takes a value is offered as `--name=`. |
| `guess_values` | True to complete the values of the options that have none in `values` by the name of their argument: filenames for a name with `FILE` or `PATH` in it (in any case), directories for one with `DIR`, and nothing otherwise (rather than filenames). |
| `skip` | Prefixes of words that are neither options nor arguments, such as `+` for `cargo +nightly`, with the kinds they complete to (after the prefix). |

The `values` of a command are also those of its subcommands and modes, unless they have their own.

An options table has one line per option: its names (`-a, --all`), then at least two spaces and its description.
`--name=ARG`, `--name ARG`, `--name <ARG>` or `-n ARG` means that the option takes a value (with all its names), and
`--name[=ARG]` that its long name can take one after `=`:

```rhai
let opts = `
    -a, --all             do not ignore entries starting with .
    --color[=WHEN]        colorize the output
    -I, --ignore=PATTERN  do not list entries matching PATTERN
    -w, --width COLS      set the output width
`;
```

### Kinds

A kind says what a value or an argument completes to:

| Kind | Candidates |
|---|---|
| `files` | filenames (luish's own) |
| `dirs` | directories |
| `none` | nothing: a value to type, such as a number |
| `users`, `groups` | the users and groups in `/etc/passwd` and `/etc/group` |
| `owner` | `USER` or `USER:GROUP` (chown) |
| `mode` | file modes (chmod) |
| `signals` | signal names |
| `processes`, `pids` | the names and the IDs of the running processes |
| `hosts` | ssh's hosts (`~/.ssh/config`) and `/etc/hosts`, after an optional `USER@` |
| `remote` | files, or `HOST:` for a path on a host (scp, rsync) |
| `manpages`, `sections` | manual pages (in the section given before, if any) and sections |
| `targets` | make's targets, from the makefile |
| `members` | the files in the archive of `tar -f ARCHIVE` |
| `dd` | dd's operands (`if=FILE`, `bs=BYTES`, `conv=CONVS` ...) and their values |
| `commands` | the commands in `PATH` |
| `MODULE:NAME` | `MODULE::kind(NAME, cur, words)` |

An array is its own candidates.

A plugin's own kinds and subcommand specs are functions of its modules, named `@SOURCE/PLUGIN/MODULE:NAME` (or by
the module's absolute path, without `.rhai`: `sh::plugin_dir() + "/kinds:NAME"`, which doesn't depend on the name
the user gives the source; a
`MODULE:NAME` without `@` is one of std's own modules). `MODULE::kind(NAME, cur, words)` completes the word `cur` (the
value, without an option before it) of the command line `words`, which starts with the command even in a
subcommand, and returns what a completer returns: candidates, a map with a `prefix`, or `()` for filenames.
`MODULE::sub_spec(NAME, SUBCOMMAND)` returns the spec of a subcommand, or `()`. A module can use its own functions
and import its neighbours in these functions, as anywhere else.

### Example

A plugin `bio`, in a collection enabled as `extra` in `config.toml`, completes `samtools`:

```text
bio/
├── plugin.toml
├── extension.rhai
├── kinds.rhai
└── specs/
    └── samtools.rhai
```

```toml
# bio/plugin.toml
description = "Completion for bioinformatics tools"

[dependencies]
std.completion = "*"
```

The extension only registers the completer; the modules are compiled on the first Tab:

```rhai
// bio/extension.rhai
sh::completer("samtools", |words, i| {
    import "@std/completion/lib" as lib;
    import "specs/samtools" as samtools;
    lib::complete(samtools::spec(), words, i)
});
```

```rhai
// bio/specs/samtools.rhai
fn spec() {
    #{
        commands: `
            view   view and convert SAM, BAM and CRAM files
            faidx  index a FASTA file, or extract regions from it
            index  index a BAM or CRAM file
        `,
        sub_spec: "@extra/bio/specs/samtools:sub",
    }
}

fn sub_spec(name, sub) {
    switch sub {
        "view" => #{
            opts: `
                -b, --bam             output BAM
                -h, --with-header     include the header
                -H, --header-only     print only the header
                -o, --output FILE     write to FILE
                -T, --reference FILE  the reference FASTA
            `,
            values: #{"-T": "@extra/bio/kinds:fasta"},
            args: ["@extra/bio/kinds:alignments", "none"],
        },
        "faidx" => #{
            opts: "-o, --output FILE  write to FILE",
            args: ["@extra/bio/kinds:fasta", "@extra/bio/kinds:regions"],
        },
        "index" => #{args: ["@extra/bio/kinds:alignments"]},
        _ => (),
    }
}
```

```rhai
// bio/kinds.rhai
// The files in the directory of `cur` whose names end in one of `suffixes`,
// and the directories.
fn with_suffix(cur, suffixes) {
    let parts = cur.split_rev("/", 2);   // ["name", "dir"] or ["name"]
    let name = parts[0];
    let dir = if parts.len() == 2 { parts[1] + "/" } else { "" };
    let out = [];
    for n in fs::list_dir(if dir == "" { "." } else { dir }) ?? [] {
        if n.starts_with(".") && !name.starts_with(".") {
            continue;
        }
        if fs::is_dir(dir + n) {
            out.push(#{value: dir + n + "/", suffix: ""});
        } else if suffixes.some(|s| n.ends_with(s)) {
            out.push(dir + n);
        }
    }
    out
}

// The names of the sequences of the FASTA file before, from its index.
fn regions(words) {
    for w in words {
        if w.ends_with(".fa") || w.ends_with(".fasta") {
            let fai = fs::read_file(w + ".fai") ?? "";
            return fai.split("\n").filter(|l| l != "").map(|l| l.split("\t")[0]);
        }
    }
    []
}

fn kind(name, cur, words) {
    switch name {
        "fasta" => with_suffix(cur, [".fa", ".fasta", ".fa.gz", ".fasta.gz"]),
        "alignments" => with_suffix(cur, [".sam", ".bam", ".cram"]),
        "regions" => regions(words),
        _ => throw `unknown kind: ${name}`,
    }
}
```

`@extra/bio/...` finds the plugin by its name, `bio`, however the user named its source; `extra` is for the reader.

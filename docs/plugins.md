# Plugins

luish can be extended with plugins written in [Rhai](https://rhai.rs), a small scripting language designed for
embedding. Plugins are opt-in: nothing is loaded unless you ask for it, and a shell that loads no plugins pays nothing
for them.

Plugin support is new. For now, a plugin can run code whenever the current directory changes (the `chpwd` hook).

## Loading plugins

```sh
plugin load NAME|PATH...   # load plugins (loading one again reloads it)
plugin list                # print the names of the loaded plugins
plugin unload NAME...      # remove plugins and their hooks
```

`plugin` is a built-in only in interactive shells, so that scripts find the same commands as in other shells. In a
script, use `__luish_internal plugin` instead.

`plugin load greet` loads `~/.config/luish/plugins/greet.rhai` (or `$XDG_CONFIG_HOME/luish/plugins/greet.rhai` if
`XDG_CONFIG_HOME` is set). An argument that contains a `/`, such as `./greet.rhai`, is a file path. A plugin's name
is its file name without `.rhai`.

Plugins are usually loaded from `~/.config/luish/luishrc`, which interactive shells read at startup. They can also
be loaded from the cached startup files in `rc.d/`: the cache records which plugins were loaded and loads them again
(but what a plugin's top level changed in the shell is cached with the rest). Start luish with
`--no-plugins` to make `plugin load` do nothing, for example to check whether a problem comes from a plugin.

`plugin load` runs the plugin's top level once, which registers its hooks. If the plugin has an error, it is
reported with the plugin's file and line, nothing from the plugin stays loaded, and the status is 1.

## Example: running code when the directory changes

```rust
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

`chpwd` hooks are called after each successful `cd`, with the old and the new directory. `$?` is the same after
the hooks as before them. If a hook fails, the error is printed and the other hooks still run. A `chpwd` hook that
itself runs `cd` does not trigger `chpwd` again.

## The `sh` module

| Function | Description |
|---|---|
| `sh::hook(kind, fn)` | Register a hook. The only kind for now is `"chpwd"` |
| `sh::getvar(name)` | The variable's value, or `()` if it is unset |
| `sh::setvar(name, value)` | Set a shell variable. Throws an error if it is readonly |
| `sh::export(name)`, `sh::unsetvar(name)` | Export or unset a variable |
| `sh::cwd()` | The current directory (as `$PWD`) |
| `sh::last_status()` | `$?` |
| `sh::interactive()` | Whether the shell is interactive |
| `sh::run(script)` | Run shell code in the current shell, as `eval` does, and return its status. If it runs `exit`, the plugin stops and the shell exits |
| `sh::write(fd, text)` | Write text, unbuffered, to fd 1 or 2 |

Rhai's `print(text)` and `debug(text)` write a line to standard output and standard error.

Rhai in luish has integers but no floating-point numbers.

## Text and bytes

Shell data (variables, paths) are bytes, while Rhai strings hold UTF-8 text. Text that is valid UTF-8 is passed
unchanged. Each byte that is not part of valid UTF-8 becomes one of the characters U+10FF80 to U+10FFFF, and turns
back into that byte when the string goes back to the shell. So a directory name that isn't valid UTF-8 survives a
round trip through a plugin. A string containing a NUL character can't be stored in a shell variable.

## Interrupting plugins

Ctrl-C stops plugin code that is running, just as it stops a command. A plugin stopped this way gives status 130.

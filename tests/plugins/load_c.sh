# `plugin load -c CODE NAME` loads Rhai code as the plugin NAME: it can
# register hooks and commands, its directory (for `import` and
# `sh::plugin_dir()`) is the current directory when it is loaded, and
# loading it again replaces it (unless the code is wrong). It doesn't
# replace a plugin loaded from a file, which must be unloaded first; a
# plugin loaded from a file replaces it, with a warning.
mkdir -p d/sub
echo 'fn hi() { "module" }' > d/sub/m.rhai
cd d
__luish_internal plugin load -c '
sh::builtin("greet", |argv| { import "sub/m" as m; print(`${m::hi()} ${argv}`); 3 });
sh::builtin("where", |argv| print(sh::plugin_dir().ends_with("/d")));
sh::hook("chpwd", |from, to| print(`chpwd ${to.ends_with("/d")}`));
' greeter; echo "load: $?"
cd ..
greet a b; echo "greet: $?"
type greet
where
__luish_internal plugin load -c 'print("other")' other
__luish_internal plugin list-loaded
# Again: the new code replaces the old, hooks and all.
__luish_internal plugin load -c 'sh::builtin("greet", |argv| print("v2"))' greeter; echo "reload: $?"
greet; cd d; cd ..
# Code that doesn't compile leaves it loaded.
__luish_internal plugin load -c 'let x = ' greeter; echo "syntax: $?"
greet
# Code that fails isn't loaded.
__luish_internal plugin load -c 'throw "bad"' bad; echo "throw: $?"
__luish_internal plugin list-loaded
# savestate records the code.
__luish_internal savestate > state
grep "^__luish_internal plugin" state
$SH -c '. ./state; greet; __luish_internal plugin list-loaded'
# A file replaces it, with a warning; it doesn't replace a file.
echo 'sh::builtin("ff", |argv| print("from file"))' > greeter.rhai
__luish_internal plugin load ./greeter.rhai; echo "file over -c: $?"
type greet; ff
{ __luish_internal plugin load -c 'print("no")' greeter; echo "-c over file: $?"; } 2>&1 | sed "s|$HOME|HOME|"
ff
__luish_internal plugin unload greeter
__luish_internal plugin load -c 'print("after unload")' greeter; echo "after unload: $?"
# A name must be given, without a /.
__luish_internal plugin load -c 'x' a/b; echo "slash: $?"
__luish_internal plugin load -c 'x' ''; echo "empty: $?"
__luish_internal plugin load -c 'x'; echo "usage: $?"

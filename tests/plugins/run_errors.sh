# `plugin run`'s errors: a missing file, a syntax error, a thrown string
# (reported with the file, as for other errors), an error, and registering,
# which only a loaded plugin can do. Each is status 1, and the shell goes on.
__luish_internal plugin run missing.rhai; echo "missing: $?"
__luish_internal plugin run -c 'let x = '; echo "syntax: $?"
echo 'throw "bad input"' > t.rhai
__luish_internal plugin run ./t.rhai; echo "throw: $?"
__luish_internal plugin run -c 'no_such_function()'; echo "error: $?"
__luish_internal plugin run -c 'sh::builtin("x", |argv| 0)'; echo "builtin: $?"
__luish_internal plugin run -c 'sh::hook("precmd", || 0)'; echo "hook: $?"
type x; echo "type: $?"
__luish_internal plugin run -c; echo "usage: $?"
__luish_internal plugin run; echo "usage: $?"

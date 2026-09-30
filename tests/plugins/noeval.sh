# Rhai's `eval` is disabled in extensions: it is a syntax error, and the
# rest of the language still works.
echo 'let x = eval("1 + 1"); print(x);' > ev.rhai
__luish_internal plugin load ./ev.rhai 2>/dev/null
echo "eval: $?"
echo 'print(1 + 1);' > ok.rhai
__luish_internal plugin load ./ok.rhai
echo "ok: $?"

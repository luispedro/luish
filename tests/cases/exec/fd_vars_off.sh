# Without `setopt syntax.fd_vars`, `{name}>file` is a word and a
# redirection, as in dash.
echo {a}>out; cat out
echo "[$a]"
echo {a}<out x{b}>>out; cat out
{fd}>out 2>/dev/null; echo "status $?"
f() { echo {g}>out; }
f; cat out

# setopt syntax.fd_vars where luish differs from zsh (and so has no
# reference shell).
setopt syntax.fd_vars
# As in bash, `{name}` must be right before the operator (zsh also takes
# `{fd} >file`), and a word that isn't a name is text (an error in zsh).
echo {a} >out; cat out; echo "[$a]"
echo {1a}>out; cat out
# As in bash, also after a compound command (a parse error in zsh).
{ echo group; } {g}>out; cat out; [ -n "$g" ] && echo "g set"
for i in 1; do echo loop; done {l}>out; cat out; [ -n "$l" ] && echo "l set"
# Redirections are made in the shell for every command, as in dash, so
# for an external command too the variable is set and the fd stays open
# (bash and zsh open it in the child only).
/bin/echo ext {x}>out; cat out
[ -n "$x" ] && echo "x set" && echo still >&$x && echo "still open"
# Parsed one line at a time: after unsetopt, `{z}` is text again (zsh
# doesn't read `setopt ignorebraces` back the same way).
unsetopt syntax.fd_vars
echo {z}>out; cat out
setopt syntax.fd_vars
# The shell's own fds count as closed: the copy of stderr that `2>` saves
# takes the lowest free fd from 10 on, here the one just closed, but `>&`
# can't reach it.
exec {c}>/dev/null; exec {c}>&-
{ echo leaked >&$c; } 2>/dev/null || echo "status $?"
# As for other redirections of special built-ins, failing to open the file
# for `exec` exits a non-interactive shell (zsh's doesn't).
exec {e}>/nonexistent/file
echo not reached

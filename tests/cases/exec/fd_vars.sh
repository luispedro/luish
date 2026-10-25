# setopt syntax.fd_vars: `{name}>file` opens a new fd, 10 or above, and puts
# its number in `name`; `{name}>&-` closes it. zsh's sh emulation turns it
# off (with ignore_braces). The numbers differ (zsh starts at 11), so only
# their order is shown.
# reference: zsh -o noignorebraces
[ -n "$ZSH_NAME" ] || setopt syntax.fd_vars
exec {fd}>out
[ "$fd" -ge 10 ] && echo "at least 10"
echo one >&$fd
echo two >&$fd
cat out
# The fd is inherited by commands.
/bin/ls /proc/self/fd/$fd >/dev/null && echo inherited
exec {fd}>&-
{ echo closed >&$fd; } 2>/dev/null || echo "closed"
# Each new fd is the lowest free one.
exec {a}>/dev/null {b}>/dev/null
[ "$b" -gt "$a" ] && echo "b after a"
exec {a}>&-
exec {c}>/dev/null
[ "$c" = "$a" ] && echo "reused"
exec {b}>&- {c}>&-
# Reading, appending, read-write, duplicating, here-documents and
# here-strings.
exec {r}<out
read -r line <&$r; echo "read $line"
read -r line <&$r; echo "read $line"
exec {r}<&-
exec {w}>>out; echo three >&$w; exec {w}>&-
exec {rw}<>out; read -r line <&$rw; echo "rw $line"; exec {rw}<&-
cat out
exec {d}>&1; echo dup >&$d; exec {d}>&-
exec {h}<<EOF
here
EOF
cat <&$h; exec {h}<&-
exec {s}<<<string
cat <&$s; exec {s}<&-
# With a command other than exec, the fd stays open afterwards, also for a
# function.
true {t}>/dev/null
echo after >&$t && echo "still open"
exec {t}>&-
f() { echo "in f"; }
f {t}>/dev/null; echo after >&$t && echo "still open after f"
exec {t}>&-
# Only directly before the operator, and only for a name; elsewhere the
# braces are text.
echo x{y}>out; cat out
echo {}>out; cat out
echo "{q}">out; cat out
echo '{q}'>out; echo "[$q]"
unset a
: {a}>(cat); wait; echo "[$a]"
# In a subshell, the variable is the subshell's.
unset fd
(exec {fd}>/dev/null; [ -n "$fd" ] && echo "set in subshell")
echo "outside [$fd]"
x=$(exec {fd}>/dev/null; echo "$fd"); [ -n "$x" ] && echo "set in \$(...)"
echo "outside [$fd]"
# `local` makes the variable local.
g() { local fd; exec {fd}>/dev/null; [ -n "$fd" ] && echo "local set"; exec {fd}>&-; }
g; echo "after g [$fd]"
# Errors, after which the command isn't run: a readonly name, a variable
# that isn't an fd number, a file that can't be opened, an fd that isn't
# open.
readonly ro=x
true {ro}>/dev/null 2>/dev/null || echo "readonly: $?"
echo "ro=$ro"
bad=abc
true {bad}>&- 2>/dev/null || echo "not a number: $?"
unset nofd
true {nofd}>&- 2>/dev/null || echo "unset: $?"
true {e}>/nonexistent/file 2>/dev/null || echo "cannot create"
echo "e=[$e]"
true {e}>&7 2>/dev/null || echo "bad fd"
echo "e=[$e]"

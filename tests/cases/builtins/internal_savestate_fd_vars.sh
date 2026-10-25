# `__luish_internal savestate` sets syntax.fd_vars before the functions, so
# that functions with `{name}>file` read back, even if the option was
# turned off after they were defined.
setopt syntax.fd_vars
f() { exec {fd}>out; echo "fd set"; exec {fd}>&-; }
unsetopt syntax.fd_vars
__luish_internal savestate > state
$SH -c '. ./state; f; unsetopt | grep syntax.fd_vars'
grep -e syntax.fd_vars -e '^f()' state

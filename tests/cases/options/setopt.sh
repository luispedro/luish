# reference: zsh
# setopt and unsetopt (not POSIX, as in zsh) set options by name: case
# doesn't matter, `_` is ignored, and a `no` prefix inverts the option.
e() { case $- in *e*) echo "errexit on";; *) echo "errexit off";; esac; }
f() { case $- in *f*) echo "noglob on";; *) echo "noglob off";; esac; }
setopt ERR_EXIT; e
unsetopt errexit; e
setopt NoErrExit; e
unsetopt no_err_exit; e
set +e
setopt no_glob; f
setopt glob; f
unsetopt GLOB; f
unsetopt NOGLOB; f
setopt noclobber
echo a > file; (echo b > file) 2>/dev/null || echo "clobber refused"
setopt clobber
echo b > file && cat file
# Errors: status 1, and the other names are still set.
setopt bogus noglob 2>/dev/null; echo "status $?"; f
unsetopt noglob bogus 2>/dev/null; echo "status $?"; f
setopt interactive 2>/dev/null; echo "status $?"
unsetopt nosuch 2>/dev/null; echo "status $?"
setopt 2>/dev/null >/dev/null; echo "status $?"

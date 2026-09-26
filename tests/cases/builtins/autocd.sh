# reference: zsh
# setopt autocd (zsh's AUTO_CD): a command that is a directory's name, and
# not a command, runs cd. Only for commands read from standard input, so it
# is tested with a shell reading its commands from a pipe ($SH is zsh itself,
# not in sh emulation, when compared with zsh).
mkdir -p a/b c/sub c/a c/only bin sub
printf '#!/bin/sh\necho "ran sub"\n' > bin/sub; chmod +x bin/sub
top=$PWD
"$SH" <<'END'
top=$PWD
p() { echo "at ${PWD#"$top"}"; }
setopt autocd
a/b; p
..; p
../c; p
./sub; p
/; echo "at $PWD"
cd "$top"
# Commands, functions and executable files come first.
PATH=$top/bin:$PATH
sub
a() { echo "function a"; }
a
unset -f a
# Only a single word without redirections.
a/b x 2>/dev/null; echo "status $?"
a/b >/dev/null 2>&1; echo "status $?"; p
# CDPATH is searched for relative names, after the current directory.
CDPATH=$top/c
a; p
only; p
cd "$top"
sub/; p
cd "$top"
nosuch 2>/dev/null; echo "status $?"; p
unsetopt autocd
a 2>/dev/null; echo "status $?"; p
END
# Not in scripts or -c.
"$SH" -c 'setopt autocd; a 2>/dev/null; echo "status $?"; pwd' | sed "s|$top||"

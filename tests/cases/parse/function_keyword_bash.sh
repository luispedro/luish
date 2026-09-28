# Forms of `function` that bash has and zsh's sh emulation doesn't: a body
# that isn't `{ ... }` straight after the name.
function sub (echo "subshell $1")
sub 1
function cond if true; then echo "if body"; fi
cond
function loop for i in 1 2; do echo "for $i"; done
loop
function p () ( echo "parens and subshell" )
p

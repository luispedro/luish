# `repeat` (zsh's), a reserved word only with `setopt syntax.repeat`. zsh's
# sh emulation doesn't have it, so no reference shell runs these.
# Without the option it is an ordinary command name.
repeat() { echo "function $*"; }
repeat 2 args
setopt syntax.repeat
# Both forms of the body, with the `;` optional before `do`.
repeat 3 { echo brace }
repeat 2; do echo do-done; done
repeat 2 do echo no-semicolon; done
repeat 2 {
    echo several
    echo lines
}
# The count is expanded, then evaluated as an arithmetic expression.
n=2
repeat $n { echo "n=$n" }
repeat "$n" { echo quoted }
repeat n+1 { echo expression }
repeat 0x2 { echo hex }
repeat 0 { echo never; }; echo "none: $?"
repeat -1 { echo never; }; echo "negative: $?"
repeat no_such_var { echo never; }; echo "unset: $?"
# A `}` ends the command it follows only unquoted, and in a nested command
# it closes that one.
repeat 1 { { echo nested } }
repeat 1 { echo "}" }
repeat 1 { if true; then echo in-if; fi }
# `break` and `continue` count it as a loop.
repeat 3 { echo once; break }
repeat 2 { echo each; continue; echo never }
repeat 2 { repeat 3 { echo inner; break 2 } }
# The status is the last command's, and a redirection applies to the loop.
repeat 2 { false }; echo "status: $?"
repeat 2 { echo hidden } > /dev/null; echo "redirected: $?"
echo "substitution:" $(repeat 2 { echo s })
# A `repeat` that is not a command name is an ordinary word, and as one it
# is the keyword even though a function of that name is defined.
echo repeat
repeat 1 { echo keyword }
# In a function, written back as the `do ... done` form.
f() { repeat 2 { echo "in $1" }; }
f body
typeset -f f

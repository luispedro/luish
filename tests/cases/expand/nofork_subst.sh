# `${ list; }` and `${| list; }` (mksh and bash 5.3, bad substitutions in
# dash): the list runs in the shell itself, as a function body runs, and the
# substitution is its output (as with `$(...)`), or the value of `REPLY`.

# Side effects stay; output is captured, trailing newlines removed, and
# split like `$(...)`.
x=1
y=${ echo hi; x=2; }
echo "$y $x"
printf '[%s]' "${ printf 'a\n\n'; }" ${ echo a b; }; echo
cd /; d=${ cd /tmp; pwd; }; echo "$d $PWD"; cd "$HOME"
# More output than a pipe holds.
echo ${ i=0; while [ $i -lt 3000 ]; do echo 0123456789abcdefghijklmnopqrstuvwxyz; i=$((i+1)); done; } | wc -c
# The status, as for `$(...)`.
x=${ false; }; echo "status $?"
x=${ true; }; echo "status $?"

# Local variables and `return`, as in a function; the positional parameters
# are the caller's.
v=outer
x=${ local v=inner; echo $v; }; echo "$x $v"
f() { local w=f; x=${ local w=sub; echo $w; }; echo "$x $w"; }; f
x=${ echo a; return 3; echo b; }; echo "$x status $?"
set -- p q r
x=${ echo $1; shift; }; echo "$x|$*"
f() { x=${ echo "$1"; }; echo "f: $x"; }; f arg

# `${| ...; }`: the value of REPLY, which is local to it and starts unset.
REPLY=before
z=${| echo "REPLY=[${REPLY-unset}]"; REPLY=val; }; echo "$z $REPLY"
printf '[%s]' "${|REPLY='a
'; }" "${| }"; echo
unset REPLY; z=${| REPLY=x; }; echo "${REPLY-unset} $z"

# The closing `}` is a reserved word: after `;`, `&`, a newline, it ends the
# list even with more of the word after it; elsewhere it is an argument.
echo ${ echo a;}x "${ echo b;}" ${ echo c
}d
echo ${ echo }; } end
echo "${ echo $(echo }x); }"
x=${ : & }; wait; echo "[$x]"
echo "[${ }]"

# Nested, and with `$(...)` around and inside.
echo ${ echo ${ echo inner; } outer; }
echo $(echo ${ echo in-cmdsubst; })
echo "${ echo "$(echo quoted)"; }"
x=${ cat <<EOF
here ${ echo doc; }
EOF
}; echo "$x"
alias al='echo aliased'
eval 'x=${ al; }'; echo "$x"

# A function printed and read back.
g() { a=${ echo one; echo two; }; b=${| REPLY=r; }; c=${ : & }; echo "$a $b"; }
eval "$(typeset -f g | sed 's/^g/h/')"
h

# Errors are the shell's own: an unset `${x?}` ends the shell.
$SH -c 'x=$(echo ${unset?}); echo "after \$(): $?"; x=${ echo ${unset?}; }; echo notreached' 2>/dev/null
echo "status $?"
# `exit` exits the shell, and the EXIT trap writes to the shell's stdout.
$SH -c 'trap "echo trap" EXIT; x=${ echo lost; exit 4; }; echo notreached'
echo "status $?"
# With stdout closed.
$SH -c 'exec 3>&1 >&-; x=${ echo inner; }; echo "$x" >&3'

# Unterminated.
$SH -c 'echo ${ echo a' 2>/dev/null; echo "status $?"
$SH -c 'echo ${ echo a }' 2>/dev/null; echo "status $?"
# Read line by line from standard input.
printf 'x=${ echo a\necho b\n}; echo "$x"\n' | $SH

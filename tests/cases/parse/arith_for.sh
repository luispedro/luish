# reference: zsh
# `for ((init; cond; step))`, as in bash and zsh (a syntax error in dash).
for ((i = 0; i < 3; i++)); do echo "i=$i"; done
echo "status $? after, i=$i"
# No space after `for`, no `;` before `do`, and newlines in and after it.
for((j=0;j<2;j++))do echo "j=$j";done
for ((k = 0;
      k < 2;
      k++))
do
    echo "k=$k"
done
# The body as a brace group.
for ((i = 0; i < 2; i++)) { echo "brace $i"; }
# Empty expressions: an empty condition is true.
i=0
for ((;;)); do i=$((i + 1)); [ $i -ge 3 ] && break; done
echo "forever stopped at $i"
for ((; i < 5;)); do i=$((i + 1)); done
echo "i=$i"
# The expressions are expanded each time: parameters, command
# substitutions, and quotes are removed.
n=3
for ((i = $(echo 1); i <= $n; i += "1")); do printf '%s ' "$i"; done
echo
for ((i = n; i > 0; i -= 2)); do printf '%s ' "$i"; done
echo
step=2
for ((i = 0; i < 6; i += step)); do step=$((step + 1)); printf '%s ' "$i"; done
echo
# Nested parentheses, commas inside them, and comparisons.
for ((i = (1 + 1) * 2; i != 0 && (i > 1 || 0); i--)); do printf '%s ' "$i"; done
echo
# No iteration: status 0, even after a failure.
false
for ((i = 0; i < 0; i++)); do echo never; done
echo "status $?"
# The status of the last command of the body.
for ((i = 0; i < 2; i++)); do false; done
echo "status $?"
# break, continue (the step still runs) and nested loops.
for ((i = 0; i < 5; i++)); do
    [ $i = 1 ] && continue
    [ $i = 3 ] && break
    echo "loop $i"
done
echo "status $? i=$i"
for ((i = 0; i < 3; i++)); do
    for ((j = 0; j < 3; j++)); do
        [ $j = 1 ] && continue 2
        [ $i = 2 ] && break 2
        echo "$i$j"
    done
done
# In a pipeline, a function and a subshell.
for ((i = 0; i < 3; i++)); do echo "piped $i"; done | tail -n 1
f() { for ((x = 0; x < $1; x++)); do printf '%s ' "$x"; done; echo; }
f 4
(for ((i = 0; i < 2; i++)); do echo "sub $i"; done)
echo "i=$i"
# An arithmetic error stops the shell (here, the subshell).
(for ((i = 0; i < 1 +; i++)); do echo never; done; echo not reached) 2>/dev/null || echo error
# Syntax errors.
(eval 'for ((i = 0; i < 1)); do :; done') 2>/dev/null || echo "two expressions"
(eval 'for ((i = 0; i < 1; i++)) echo x') 2>/dev/null || echo "no do"
(eval 'for (( ; ; ) ); do :; done') 2>/dev/null || echo "split parens"

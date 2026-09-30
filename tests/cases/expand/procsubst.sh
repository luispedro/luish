# reference: zsh
# Process substitution: <(...) is a file that reads what the list writes.
cat <(echo hi)
cat <(echo a) <(echo b)
diff <(echo a) <(echo b)
echo status=$?
# Not split, and quoted the same way as any expansion.
set -- <(echo x)
echo $#
case $1 in /dev/fd/*|/proc/self/fd/*) echo path;; esac
# As a redirection target, and in a loop.
while read -r l; do echo "got $l"; done < <(printf '%s\n' 1 2 3)
cat < <(echo redirected)
# Nested, and in a function.
cat <(cat <(echo nested))
f() { cat "$1"; }
f <(echo in function)
# Text after the `)` is part of the word.
echo <(true) | grep -c /
# The list's status is not the command's.
true <(exit 3)
echo status=$?
# Many, one after the other, do not use up descriptors.
i=0
while [ $i -lt 300 ]; do
    i=$((i + 1))
    cat <(echo $i) > /dev/null
done
echo $i

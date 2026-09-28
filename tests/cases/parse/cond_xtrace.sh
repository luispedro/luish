# `set -x` shows `[[ ... ]]` on one line, with the expanded words of the
# parts that were evaluated, as zsh does (zsh writes `=~` as
# `-regex-match`, and `=` as written).
exec 2>&1
x='a b'
set -x
[[ $x = "a b" && -n $x ]]
[[ a = b && -n $(echo not run) ]]
[[ ! ( a < b || c ) && 1+1 -eq 2 ]]
[[ $x =~ ^a ]]
[[ lone ]]
set +x

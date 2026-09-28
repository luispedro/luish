# zsh's parameter flags where luish differs from zsh (or zsh has no
# equivalent to compare with).
# A flag luish doesn't have is a bad substitution, when expanded (zsh: an
# error in flags).
f() { echo ${(P)x}; }
echo defined
(f) 2>/dev/null || echo "bad $?"
(echo ${(j:,)x}) 2>/dev/null || echo "bad $?"
(echo ${(U)}) 2>/dev/null || echo "bad $?"
(echo ${(U)a[}) 2>/dev/null || echo "bad $?"
# typeset -f prints the flags as written.
g() { echo "${(j:,:)a[@]}" ${(s(,)o)x:-d} "${(@kv)h[*]}" ${(U)1} ${(Oa)@}; }
typeset -f g
eval "$(typeset -f g | sed 's/^g/g2/')"
a=(x y) x=b,a
typeset -A h
h=(k v)
g2 z w

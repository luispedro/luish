# A letter after `:` would start one of zsh's modifiers, which luish lacks:
# as in dash, it's a bad substitution when expanded.
x=abc
f() { echo ${x:h}; }
echo ${x:}
echo never

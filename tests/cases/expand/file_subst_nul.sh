# As for any `$(...)`, NUL bytes are dropped from `$(<file)`, as in dash
# and bash (zsh keeps them).
printf '.\000.\000\n\000\n' > f
s=$(<f); echo len=${#s}; printf %s "$s" | od -A n -t x1

# As in dash, NUL bytes are dropped from command substitution output and
# from the input of read.
show_bytes() { printf %s "$1" | od -A n -t x1; }
s=$(printf '.\001.'); echo len=${#s}; show_bytes "$s"
s=$(printf '.\000.'); echo len=${#s}; show_bytes "$s"
s=$(printf '\000'); echo len=${#s}
s=$(printf '\000.\000\n\000\n'); echo len=${#s}; show_bytes "$s"
printf '.\000.\n' | { read -r s; echo len=${#s}; show_bytes "$s"; }
printf 'a\\\000b\n' | { read s; echo "$s"; }
printf '\000\n' | { read s; echo len=${#s}; }

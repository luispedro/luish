# Where luish's `read -A` and `read -a` differ from zsh's.
show() { printf '<%s>' "$@"; echo; }
# The fields are split as in field splitting, as in bash: no empty element
# after a trailing delimiter (zsh adds one, even after whitespace), and none
# for an empty line or at the end of the input (zsh has one empty element).
echo 'a::b:c:' | { IFS=: read -A a; show "${a[@]}"; }
echo 'x y ' | { read -A a; echo "${#a[@]}"; }
echo | { read -A a; echo "$? ${#a[@]}"; }
: | { read -A a; echo "$? ${#a[@]}"; }
# bash's `read -a NAME`: the name is the option's argument, and other names
# are ignored.
echo 'x\y z' | { read -a a; show "${a[@]}"; }
echo 'x\y z' | { read -ra a; show "${a[@]}"; }
echo 'x\y z' | { read -a a -r; show "${a[@]}"; }
echo 'x y' | { read -aa b; show "${a[@]}" "[${b-unset}]"; }
# Errors.
echo x | { read -A a b; echo "status $?"; }
echo x | { read -a; echo "status $?"; }
echo x | { read -A; echo "status $?"; }
echo x | { read -a 1a; echo "status $?"; }
readonly r
echo x | { read -A r; echo never; }
echo "status $?"

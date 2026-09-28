# reference: zsh
# `read -A` (zsh; bash's `read -a NAME`) reads the fields into an array.
show() { printf '<%s>' "$@"; echo; }
echo '  x  y\ z  w' | { read -A a; echo $?; show "${a[@]}"; }
echo 'x y\ z' | { read -r -A a; show "${a[@]}"; }
echo 'x\y z' | { read -rA a; show "${a[@]}"; }
echo 'a::b:c' | { IFS=: read -A a; show "${a[@]}"; }
echo ':a' | { IFS=: read -A a; show "${a[@]}"; }
echo 'a : b ::c' | { IFS=': ' read -A a; show "${a[@]}"; }
echo ' a b ' | { IFS= read -A a; show "${a[@]}"; }
printf 'x\\\ny z\n' | { read -A a; show "${a[@]}"; }
printf 'no newline' | { read -A a; echo $?; show "${a[@]}"; }
# The array replaces the old value.
a=(9 9 9)
read -A a <<EOT
1
EOT
show "${a[@]}"
f() {
  local -a l
  read -A l
  echo "${l[1]}"
}
echo 'x y' | f
echo "[${l-unset}]"

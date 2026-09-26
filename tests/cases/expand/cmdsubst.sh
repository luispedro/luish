echo "[$(printf 'a\n\n\n')]"
echo "[$(printf '\n\na')]"
x=$(echo one; echo two); echo "$x"
echo $(echo a    b)
echo "$(echo a    b)"
echo `echo \$HOME_x`
echo $(exit 3)$?
f() { echo from-func; }; echo $(f)
echo $(echo $(echo $(echo deep)))

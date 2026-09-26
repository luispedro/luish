# umask is a port of dash's: symbolic modes (including X, s, u/g/o copies
# and several clauses), and its errors.
for start in 0124 0246 0022 0777 0000; do
  for m in '' ' ' a=X a-X a+X a-s a+s a=s a=u a=g a=o a=,a=u u=rwx,go=rx u+w g-w o= -rwx -wx -=+ 'u=, g+, o-' ug+x o-rwx =r +x -x u=g g=o,o=u a=rw- 'u=rwx,' , u=q 08 0999 1a 777 7 02 00022; do
    umask $start
    umask "$m" 2>/dev/null
    st=$?
    printf '%s %s -> %s %s\n' "$start" "[$m]" "$(umask)" "$st"
  done
done
umask 0027; umask -S
umask -S 2>/dev/null; echo "st $?"
umask -- 0077; umask

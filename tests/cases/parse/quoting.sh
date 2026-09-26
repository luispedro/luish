echo 'single $x' "double $HOME_UNSET" \$escaped
echo "a\"b" 'it'\''s' "back\\slash" "\$" "\x"
echo a\
b
echo "multi
line"
echo ''"" x
echo "$(echo "nested \"quotes\"")"
echo `echo back\`tick\``
echo "`echo "in dq"`"
echo \# not a comment # comment
echo a#b

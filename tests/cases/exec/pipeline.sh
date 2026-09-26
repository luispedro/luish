echo hello | tr a-z A-Z
echo a b c | tr ' ' '\n' | sort -r | head -2
false | true; echo $?
true | false; echo $?
x=outer; echo | x=inner; echo $x
echo one | { read v; echo "got $v"; }
printf 'a\nb\n' | while read l; do echo "[$l]"; done

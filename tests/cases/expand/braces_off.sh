# Without setopt expand.braces, braces are text, as in POSIX.
echo {a,b} f/{a,b} {1..3} a{,b} {a..c}
echo hi > o{1,2}
cat 'o{1,2}'
for i in {1..2}; do echo $i; done

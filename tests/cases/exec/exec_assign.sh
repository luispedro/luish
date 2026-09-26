# Assignments before exec persist and are exported to the command.
printf 'echo hello $BLAH\n' > snork
chmod +x snork
$SH -c 'BLAH=123; ./snork'
$SH -c 'BLAH=123; exec ./snork'
$SH -c 'BLAH=123 exec ./snork'
$SH -c 'BLAH=123 exec 3>/dev/null; echo "BLAH=$BLAH"; env | grep BLAH'

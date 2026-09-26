# Deviation from dash: fd numbers of more than one digit (POSIX allows
# them; dash only recognises a single digit, so `20>f` is the word 20).
exec 20> file20
echo hello20 >&20
exec 20>&-
cat file20
echo hi 99>&1
echo hi 12>&1 1>/dev/null >&12

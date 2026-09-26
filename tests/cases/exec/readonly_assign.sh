# An assignment before an external command is made in the shell, as in
# dash, so a read-only variable is an error of the shell, which exits.
readonly x=1
x=2 /bin/echo not-run
echo not-reached

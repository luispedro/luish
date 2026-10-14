# Aliases whose text has newlines, in a script read from standard input: the
# commands after the first newline run too
printf '%s\n' "alias e_='echo 1" "echo 2" "echo 3'" 'e_ x' \
    "alias t='echo 1" "echo 2" "'" 't' 'echo after' \
    "alias a='echo one && echo two;" "echo four'" 'a' \
    "alias r='echo r" "r'" 'r' \
    "alias n='echo n" "e_ y'" 'n' \
    "alias q='echo \"a" "b\"" "echo c'" 'q' 2>&1 | $SH 2>/dev/null
echo status=$?

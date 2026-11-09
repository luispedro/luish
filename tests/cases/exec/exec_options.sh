# reference: zsh
# `exec -a NAME`, `-l` and `-c`, as in zsh and bash.
(exec -a myname cat /proc/self/cmdline) | tr '\0' ' '; echo
(exec -amyname cat /proc/self/cmdline) | tr '\0' ' '; echo
(exec -l cat /proc/self/cmdline) | tr '\0' ' '; echo
(exec -l -- cat /proc/self/cmdline) | tr '\0' ' '; echo
# zsh ignores -l with -a (bash would give -myname).
(exec -la myname cat /proc/self/cmdline) | tr '\0' ' '; echo
(exec -l -a myname cat /proc/self/cmdline) | tr '\0' ' '; echo
# The command is looked up by its name, not by the name given with -a.
(exec -a nosuch-command-xyz cat /proc/self/cmdline) | tr '\0' ' '; echo
(exec -a cat nosuch-command-xyz) 2>/dev/null
echo "not found: $?"
# -c clears the environment, also of the command's own assignments.
export X=1
(Y=2 exec -c env) | wc -l | tr -d ' '
(Y=2 exec env) | grep -c '^[XY]='
(exec -c -a foo env) | wc -l | tr -d ' '
# A script without #! is run by the shell, without environment.
printf 'echo "[$X]"\n' >script
chmod +x script
(exec -c ./script)
(exec ./script)
# Errors: an unknown option, -a without its argument, options without a command.
(exec -x true) 2>/dev/null || echo failed
(exec -a) 2>/dev/null || echo failed
(exec -c) 2>/dev/null || echo failed
(exec -c >/dev/null; echo not reached) 2>/dev/null || echo failed
echo still here
# A lone - is a command.
(exec - 2>/dev/null) 2>/dev/null
echo "dash: $?"

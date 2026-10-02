# precmd hooks run before each prompt (not PS2) with the last status,
# preexec hooks before each command read at the prompt with its text, and
# exit hooks when the main shell exits, after the EXIT trap, with the exit
# status. $? is kept around them all. Only exit hooks run in scripts, and
# not in subshells; a shell with exit hooks doesn't exec its last command.
cat > hooks.rhai <<'P'
sh::hook("precmd", |status| print(`precmd ${status}, $?=${sh::last_status()}`));
sh::hook("preexec", |line| print(`preexec [${line}]`));
sh::hook("exit", |status| print(`exit ${status}`));
P
echo '--- interactive'
$SH -i 2>/dev/null <<'I'
__luish_internal plugin load ./hooks.rhai
false

echo "\$? kept: $?"
alias say='echo said'
say it
echo two \
lines
(exit 3)
trap 'echo "EXIT trap, $?"' EXIT
exit 5
I
echo "status $?"
echo '--- end of input'
printf '%s\n' '__luish_internal plugin load ./hooks.rhai' 'echo last; (exit 2)' | $SH -i 2>/dev/null
echo "status $?"
echo '--- a script'
$SH -c '__luish_internal plugin load ./hooks.rhai; (echo subshell); echo $(echo cmdsub); sh -c "exit 4"'
echo "status $?"
echo '--- exit in a hook'
cat > exits.rhai <<'P'
sh::hook("precmd", |status| if status == 6 { sh::run("exit 7"); });
sh::hook("exit", |status| { print(`exit ${status}`); if status == 1 { sh::run("exit 8"); } });
sh::hook("exit", |status| print(`second exit hook ${status}`));
P
printf '%s\n' '__luish_internal plugin load ./exits.rhai' '(exit 6)' 'echo not reached' | $SH -i 2>/dev/null
echo "status $?"
$SH -c '__luish_internal plugin load ./exits.rhai; false'
echo "status $?"

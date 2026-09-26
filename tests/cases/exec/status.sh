true; echo $?
false; echo $?
sh -c 'exit 7'; echo $?
nonexistent_cmd_xyz 2>/dev/null; echo $?
./ 2>/dev/null; echo $?
touch notexec; ./notexec 2>/dev/null; echo $?
(exit 300); echo $?
sh -c 'kill -9 $$'; echo $?

# External commands in a non-interactive shell are started with posix_spawn,
# with redirections and assignments made in the shell around it.
# Redirections apply to the command and are undone afterwards.
/bin/echo to-file >out; echo after; cat out
/bin/echo err >&2 2>/dev/null; echo st=$?
/bin/cat <<EOF2
heredoc $HOME
EOF2
# The shell's saved copies of fds are close-on-exec.
ls /proc/self/fd >fds 2>/dev/null; tr '\n' ' ' <fds; echo
exec 3>three; sh -c 'echo on-three >&3'; exec 3>&-; cat three
# Redirection errors and exec errors, reported with the redirections applied.
/bin/echo x >/nonexistent/dir/f; echo st=$?
nonexistent_cmd_xyz 2>err; echo st=$?; wc -l <err
mkdir d; ./d 2>/dev/null; echo st=$?
PATH=/nonexistent ls 2>/dev/null; echo st=$?
# Assignments go to the command's environment only.
x=1 PATH=/bin:$PATH sh -c 'echo x=$x'; echo "x=$x"
export y=outer; y=inner sh -c 'echo y=$y'; echo y=$y
# The child gets the default action for trapped signals, and keeps
# ignored ones ignored.
trap 'echo trapped' USR1
sh -c 'kill -USR1 $$; echo survived'; echo st=$?
trap '' USR2
sh -c 'kill -USR2 $$; echo survived'; echo st=$?
# command runs the external command the same way.
command /bin/echo via-command

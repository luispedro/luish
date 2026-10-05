# `&!` followed by a command is `&` and then `!`, as in dash (zsh reads
# zsh's `&!`, disowning the job, which luish does only where a command
# can't follow; see exec/amp_disown.sh).
true &! false
echo "status $?"
true &!true
echo "status $?"
true &! (exit 3)
echo "status $?"
true &! >/dev/null
echo "status $?"
true &! "}"
echo "status $?"
wait

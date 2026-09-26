# The shell looks up (and remembers) the commands of a pipeline itself,
# not in the forked processes.
ls >/dev/null | cat >/dev/null
hash | sed 's,.*/,,' | sort

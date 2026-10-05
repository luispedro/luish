# reference: zsh
# `a |& b` pipes both standard output and standard error, as `a 2>&1 | b`.
{ echo out; echo err >&2; } |& sed 's/^/> /'
sh -c 'echo out; echo err >&2' |& sort
f() { echo "from f" >&2; }
f |& tr a-z A-Z

# The redirection comes after the command's own, so standard error still
# goes to the pipe.
{ echo err >&2; } 2>/dev/null |& sed 's/^/2: /'

# In longer pipelines, after `!`, across a newline and in `$(...)`.
echo a |& { cat; echo b >&2; } |& cat
! { echo err >&2; false; } |& cat
echo "negated: $?"
echo x |&
cat
v=$( { echo out; echo err >&2; } |& tr '\n' ' ')
echo "v=$v"

# Only the command on the left of `|&` is affected.
{ echo e1 >&2; } |& { cat; echo e2 >&2; } 2>/dev/null | cat

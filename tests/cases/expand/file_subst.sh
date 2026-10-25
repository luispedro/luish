# `$(<file)` is the file's contents, as in ksh, bash and zsh (in dash it
# only opens the file, so it is empty).
# reference: zsh
printf 'one\ntwo\n\n\n' > f
printf 'no newline' > nonl
echo "[$(<f)]"
echo "[$(< f )] [$(0<f)] [`<f`] [$(<f;)] [$(
<f
)]"
echo "[a$(<nonl)b]"
set -- $(<f); echo $#
x=$(<f); echo "$? [$x]"
echo "[$(</dev/null)]"
# The name is expanded as a redirection's: no splitting or globbing.
printf 'spaced\n' > 'a b'
s='a b'; echo "[$(<$s)] [$(<"$s")] [$(<~/f)]"
x=$(<a*); echo "$? [$x]"
# It is expanded in the shell itself, not in a subshell.
unset n; echo "[$(<${n:=nonl})] $n"
# A fifo.
mkfifo p
printf 'piped\n' > p &
echo "[$(<p)]"
wait
# Failures: status 1.
x=$(<missing); echo "$? [$x]"
mkdir d
x=$(<d); echo "$? [$x]"
# Anything more than one redirection of standard input runs as usual, so
# its value is empty.
echo "[$(<f 2>/dev/null)] [$(<f <nonl)] [$(1<f)] [$(<<<hi)] [$( (<f) )] [$({ <f; })]"
set -e
x=$(<missing)
echo not reached

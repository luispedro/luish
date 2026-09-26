# With promptpercent (luish's own option, as in zsh), PS1, PS2 and PS4
# expand `%` sequences, after parameter expansion. An interactive shell
# reading a pipe writes its prompts to stderr.
mkdir -p a/b/c
PS1='[%~ %?] ' $SH -i +m <<'EOF2' 2>&1 | sed "s|$HOME|H|g" | cat -v
setopt promptpercent
false
cd a/b/c
PS1='[%2~|%-1~|%1/|%C|%c|%/|%%|%)] '
PS1='[%(?.ok.bad %?) %(3?.three.not3) %3(?.three.not3) %(4~.deep.shallow)] '
(exit 3)
PS1='[%j %(1j.jobs.nojobs)] '
: | sleep 10 &
kill $!; wait
PS1='[%5<..<%~%<<|%6>>%~%>>|%(?.%3<<abcdef.x)g] '
PS1='[%F{red}r%f%K{12}k%k%F{#00ff80}%B%U%S%s%u%b%f%{x%}%Q%E] '
PS2='%F{2}more%f '
echo 'a
b'
x='%?'
PS1='[$x]%% '
unsetopt promptpercent
EOF2
echo
echo '--- PS4'
$SH -c 'setopt promptpercent; PS4="+%? %1(?.x.y) "; set -x; false; echo hi' 2>&1

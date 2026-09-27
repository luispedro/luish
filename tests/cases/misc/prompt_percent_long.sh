# With promptpercent, `%[name]` is the long form of a `%` sequence, and
# `%[name:arg]` gives it an argument; `%([name].yes.no)` does the same for
# conditions. Case, `_` and `-` don't matter in names. Unknown names are
# errors that suggest a close name, or else list them.
mkdir -p a/b/c
cd a/b/c
PS4='+ ' $SH -c 'setopt promptpercent
PS4="[%[dir]|%[dir:2]|%2[dir]|%[pwd_tail]|%[Dir-Tail:2]|%[status]|%[percent]] "
set -x; false; : x
PS4="[%[fg:red]r%[fg_off]%[bg:12]k%[bg_off]%[bold]%[underline]%[standout]%[standout_off]%[underline_off]%[bold_off]%[clear_eol]] "
: colours
PS4="[%([status].ok.bad)|%([status:1].one.not1)|%2([status].two.not2)|%([dir:3].deep.shallow)|%([jobs:1].jobs.nojobs)] "
: conditions; (exit 1); : after
PS4="[%[8<..]%[pwd]%[<]|%6[>>]abcdefgh] "
: zsh truncation
PS4="[%[date:%Y]|%[jobs]] "
: date
set +x
PS4="[%[hostname]|%[host:2]|%[user]] "
exec 3>&1
out=$( (set -x; :) 2>&1 >&3)
[ "$out" = "[$(uname -n)|$(uname -n | cut -d. -f1-2)|$(id -un)] :" ] && echo host and user
' sh 2>&1 | sed "s|$HOME|H|g; s|$(date +%Y)|YEAR|" | cat -v
echo '--- errors'
$SH -c 'setopt promptpercent
PS4="[%[hostnam]|%[Usr]|%[branch]|%[dir:x]|%([stauts].a.b)|%([bogus].a.b)] "
set -x; : one
PS4="[%[host"
: two' sh 2>&1

# Startup files whose effects are cached, as zsh's .zshrc and .zlogin:
# ~/.config/luish/rc.d/*.lsh for every interactive shell, then
# ~/.config/luish/login.d/*.lsh for login shells. Later shells use the
# cache; when a file changes, they rerun the files and rewrite the cache
# without asking.
umask 022
mkdir -p .config/luish/login.d .config/luish/rc.d
cd .config/luish/login.d
cat > 10-a.lsh <<'X'
echo running 10-a
export A=1
f() { echo "f: $A $1"; }
alias ll='echo ll' origin='echo login'
unset Z
readonly R=r
umask 027
set -o noclobber
. "$HOME/extra.sh"
X
echo 'echo running 20-b; B=$A-b' > 20-b.lsh
echo 'echo uncached; U=u' > _uncached.lsh
echo 'echo not a startup file' > 30-c.sh
cd ../rc.d
echo "echo running rc; alias origin='echo rc'; P=p" > rc.lsh
echo 'echo uncached rc' > _uncached.lsh
cd
echo 'E=extra' > extra.sh
show='echo "A=$A B=$B E=$E U=$U T=$T Z=${Z-unset} P=$P"; f x; ll; origin; umask; case $- in *C*) echo noclobber;; esac'
echo '--- an interactive login shell builds both caches, rc.d first'
T=t Z=z $SH -il -c "$show" 2>/dev/null
echo '--- and uses them'
T=t2 Z=z $SH -il -c "$show; env | grep ^A=; R=2; echo not reached" 2>/dev/null
echo "status $?"
ls -l .cache/luish | grep -c '^-rw------- .* login-'
ls -l .cache/luish | grep -c '^-rw------- .* rc-'
# The directory is tagged for backup tools and explains itself.
head -c 43 .cache/luish/CACHEDIR.TAG; echo
grep -c 'removed' .cache/luish/README
echo '--- a non-interactive login shell runs only login.d'
$SH -l -c "$show" 2>/dev/null
echo '--- an interactive shell runs only rc.d'
$SH -i -c "$show" 2>/dev/null
echo '--- a non-interactive shell runs neither'
$SH -c "$show" 2>/dev/null
echo '--- a sourced file changed'
echo 'E=extra2' > extra.sh
$SH -l -c 'echo $E'
$SH -l -c 'echo $E'
# Only the files from the one added on run; once it is removed, the entries
# from before are used again.
echo '--- a file added, then removed'
echo 'echo running 15-d; D=d' > .config/luish/login.d/15-d.lsh
$SH -l -c 'echo "B=$B D=$D"'
$SH -l -c 'echo "B=$B D=$D"'
rm .config/luish/login.d/15-d.lsh
$SH -l -c 'echo "B=$B D=${D-unset}"'
$SH -l -c 'echo "B=$B D=${D-unset}"'
echo '--- a cache written by another build of luish'
# The cache records the build (in a field \`b LEN\`, then the id on a line
# of its own, which starts with the git revision); on a mismatch, the files
# rerun, silently (stderr is shown here). The other build's id has the same
# length.
set -- .cache/luish/login-*
rev=$(__luish_internal print-git-rev)
other=$(printf %s "$rev" | tr 0-9a-f g-v)
grep -c "^$rev" "$1"
sed "s/^$rev/$other/" "$1" > ../other
cat ../other > "$1"
grep -c "^$other" "$1"
$SH -l -c 'echo $E' 2>&1
$SH -l -c 'echo $E' 2>&1
grep -c "^$other" "$1"
echo '--- a file changed, in an interactive shell'
echo "echo running rc; alias origin='echo rc2'" > .config/luish/rc.d/rc.lsh
$SH -i -c 'origin' 2>/dev/null
$SH -i -c 'origin' 2>/dev/null
echo '--- without login.d, ~/.profile is read (after rc.d)'
rm -r .config/luish/login.d
echo 'echo profile' > .profile
# /etc/profile runs too, and may print something (/etc/profile.d varies).
$SH -il -c 'echo "A=$A"; origin' 2>/dev/null | grep -x -E 'uncached rc|profile|A=.*|rc2'

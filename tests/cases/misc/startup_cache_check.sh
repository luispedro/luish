# `__luish_internal check-cache` reruns the startup files, in the
# environment and with the options of the shell that built each cache, and
# compares the state they give with the one the cache restores: this finds
# the changes that the files' fingerprints miss. A current cache is
# touched; one that differs is rebuilt. Status 0, 1 if any was rebuilt.
export TZ=UTC
mkdir -p .config/luish/login.d .config/luish/rc.d bin
cd .config/luish/login.d
cat > 10-a.lsh <<'X'
echo running 10-a
export PATH=$HOME/bin:$PATH
V=$(cat "$HOME/value")
[ -e "$HOME/flag" ] && export FLAG=yes
f() { echo f; }
alias ll='echo ll'
readonly R=r
X
echo 'echo uncached; touch "$HOME/uncached-ran"' > _uncached.lsh
cd ../rc.d
echo "alias origin=\"echo \$(cat \"\$HOME/origin\")\"" > rc.lsh
cd
echo 1 > value
echo rc > origin
# Hide the cache's directory, its host and the times (a check within
# the minute may or may not come a second after the cache was written).
report() {
    sed -e "s|^$SH:|luish:|" -e "s|$HOME|~|" -e 's|\(cache/luish/[a-z]*\)-[^:]*|\1-HOST|' \
        -e 's/[0-9]* days ago/N days ago/' -e 's/generated .* (just now)$/generated NOW/' \
        -e '/last checked .* (just now)$/d'
}
PATH=/usr/bin:/bin $SH -il -c 'echo "$PATH V=$V" | sed "s|$HOME|~|"' 2>/dev/null
rm uncached-ran
echo '--- both are current, whatever the PATH and directory of the check'
cd bin
PATH=$HOME/bin:$PATH $SH -c '__luish_internal check-cache; echo "status $?"' | report
cd
# The startup files' output isn't shown, and _uncached.lsh doesn't run.
[ -e uncached-ran ] || echo 'no _uncached.lsh'
echo '--- quiet: no output when current'
$SH -c '__luish_internal check-cache -q; echo "status $?"'
echo '--- the cache is touched, and keeps its time'
set -- .cache/luish/login-*
login=$1
touch -t 200001010000 "$login"
touch -t 200101010000 reference
t=$(grep '^t ' "$login")
$SH -c '__luish_internal check-cache -q login'
[ "$login" -nt reference ] && echo touched
[ "$(grep '^t ' "$login")" = "$t" ] && echo 'same time'
echo '--- output that no fingerprint shows'
echo 2 > value
$SH -l -c 'echo "V=$V"'
$SH -c '__luish_internal check-cache -q; echo "status $?"' | report
$SH -l -c 'echo "V=$V"'
$SH -c '__luish_internal check-cache -q; echo "status $?"'
echo '--- a variable added, then removed'
touch flag
$SH -c '__luish_internal check-cache --quiet' | report
rm flag
$SH -c '__luish_internal check-cache -q login' | report
echo '--- an alias, in rc'
echo rc2 > origin
$SH -c '__luish_internal check-cache -q; echo "status $?"' | report
$SH -i -c 'origin' 2>/dev/null
echo '--- the time it was generated and last checked, as local time'
sed 's/^t .*/t 1000000000/' "$login" > tmp
cat tmp > "$login"
touch -t 200109100000 "$login"
$SH -c '__luish_internal check-cache login' | report
echo '--- a file changed, and another build of luish'
echo 'W=w' >> .config/luish/login.d/10-a.lsh
sed 's/^b .*/b another-build/' "$login" > tmp
cat tmp > "$login"
$SH -c '__luish_internal check-cache -q' | report
$SH -c '__luish_internal check-cache -q; echo "status $?"'
echo '--- built by a non-interactive login shell: without rc'
rm "$login"
$SH -l -c 'echo "V=$V"'
grep '^m ' "$login"
echo rc3 > origin
$SH -c '__luish_internal check-cache -q login; echo "status $?"'
echo '--- errors'
$SH -c '__luish_internal check-cache -x; echo "status $?"' 2>&1 | report
$SH -c '__luish_internal check-cache other; echo "status $?"' 2>&1 | report
$SH --no-plugins -c '__luish_internal check-cache; echo "status $?"' 2>&1 | report
echo '# luish startup cache 1' > "$login"
$SH -c '__luish_internal check-cache -q login; echo "status $?"' 2>&1 | report
rm "$login"
$SH -c '__luish_internal check-cache login; echo "status $?"' 2>&1 | report
rm -r .config/luish/rc.d
$SH -c '__luish_internal check-cache; echo "status $?"' 2>&1 | report
rm -r .cache
$SH -c '__luish_internal check-cache; echo "status $?"'

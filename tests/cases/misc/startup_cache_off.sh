# Without startup.auto_cache (the default), only `__luish_cache` blocks are
# cached: config.toml, the plugins it enables and the files of rc.d and
# login.d run in every shell, as _uncached.lsh does. The option is read from
# config.toml, also by a login shell that isn't interactive (for login.d).
PATH=/usr/bin:/bin
mkdir -p .config/luish/login.d .config/luish/rc.d
# A file with a block runs every time anyway: the blocks are in files of
# their own.
echo 'echo running rc a; A=a' > .config/luish/rc.d/a.lsh
echo '__luish_cache { echo building rc block; B=b; }' > .config/luish/rc.d/b.lsh
echo 'echo running rc uncached; U=u' > .config/luish/rc.d/_uncached.lsh
echo 'echo running login l; L=l' > .config/luish/login.d/l.lsh
echo '__luish_cache { echo building login block; M=m; }' > .config/luish/login.d/m.lsh
show='echo "A=$A B=$B U=$U L=$L M=$M"'
entries() {
    for c in rc login; do
        printf '%s: %s files, %s blocks\n' $c "$(grep -c '^file ' .cache/luish/$c-*)" \
            "$(grep -c '^block ' .cache/luish/$c-*)"
    done
}
echo '--- off: the files run every time, the blocks once'
$SH -il -c "$show" 2>/dev/null
$SH -il -c "$show" 2>/dev/null
entries
echo '--- on: whole files are cached'
printf '[options.startup]\nauto_cache = true\n' > .config/luish/config.toml
$SH -il -c "$show" 2>/dev/null
$SH -il -c "$show" 2>/dev/null
entries
echo '--- a login shell that is not interactive reads the option from config.toml'
$SH -l -c "$show"
$SH -l -c "$show"
echo '--- off again: the entries of files are dropped, those of blocks kept'
printf '[options.startup]\nauto_cache = false\n' > .config/luish/config.toml
$SH -il -c "$show" 2>/dev/null
$SH -il -c "$show" 2>/dev/null
entries
$SH -l -c "$show"
echo '--- in a login shell that is not interactive, only that option is read'
printf '[options]\nglob.star = true\nnosuch = 1\nstartup.auto_cache = true\n[nogroup]\n' > .config/luish/config.toml
$SH -l -c 'setopt | grep -E "^(glob.star|startup)"' 2>&1

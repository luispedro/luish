# `__luish_internal default-config` prints the recommended config.toml, as the
# first run writes it, with luish-extra's completion and themes (`--extra`,
# which used to add them, is accepted for older install scripts). Every
# setting in it is valid: an interactive shell reads it without a message.
__luish_internal default-config >plain; echo "status $?"
__luish_internal default-config --extra >extra; echo "status $?"
cmp plain extra && echo same
grep -x 'autosuggest = true' plain
grep -x 'extra.complete.all = "\*".*' plain >/dev/null && echo complete
grep -x 'extra.themes = "\*".*' plain >/dev/null && echo themes
grep -x 'extra = { gh = "luispedro/luish-extra", release = "luish" }' plain
grep -x 'std.completion = "\*".*' plain >/dev/null && echo std
__luish_internal default-config --extra x 2>/dev/null; echo "status $?"
__luish_internal default-config -x 2>/dev/null; echo "status $?"
echo '--- the options it sets'
mkdir -p .config/luish && cp plain .config/luish/config.toml
$SH --no-plugins -i -c 'setopt' 2>&1 | grep -v 'job control'

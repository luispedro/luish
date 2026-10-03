# `__luish_internal default-config` prints the recommended config.toml, as the
# first run writes it; with `--extra`, also luish-extra's completion and themes
# (install.sh writes that one).
__luish_internal default-config >plain; echo "status $?"
__luish_internal default-config --extra >extra; echo "status $?"
grep -c extra plain
grep -x 'autosuggest = true' plain
grep -x 'extra.complete.all = "\*".*' extra >/dev/null && echo complete
grep -x 'extra.themes = "\*".*' extra >/dev/null && echo themes
grep -x 'extra = { gh = "luispedro/luish-extra" }' extra
grep -x 'std.completion = "\*".*' extra >/dev/null && echo std
__luish_internal default-config --extra x 2>/dev/null; echo "status $?"
__luish_internal default-config -x 2>/dev/null; echo "status $?"

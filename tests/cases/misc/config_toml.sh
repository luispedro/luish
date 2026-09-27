# ~/.config/luish/config.toml sets luish's settings, for interactive shells
# only, before rc.d: each table under `options` is a group, and each key is
# a setting in it, as for `setopt -p GROUP KEY=VALUE` (a value directly under
# `options` is a setting by its own name); the `alias` table defines aliases
# and the `bindkey` table key bindings.
# Errors are reported with their lines, in the file's order, and skipped.
run() { $SH -i -c "$1" luish 2>&1 | grep -v 'job control' | sed "s|$HOME|~|g"; }
mkdir -p .config/luish
cat > .config/luish/config.toml <<'X'
# Comments are allowed.
[options.history]
file = "~/hist"
size = 50
share = true
no_ignore_space = true   # the same as ignore_space = false

[options.glob]
star = true
X
show='setopt -p history; setopt -p glob; echo "HISTFILE=$HISTFILE"'
echo '--- an interactive shell reads it'
run "$show"
echo '--- a script and -c do not'
$SH -c "$show" | sed "s|$HOME|~|g"
echo '--- nor --no-rcs'
$SH -i --no-rcs -c 'setopt -p glob' 2>/dev/null
echo '--- errors'
cat > .config/luish/config.toml <<'X'
top = 1
[options.history]
share = 1
size = -3
file = 7
bogus = true
save_size = 20
reduce_blanks = "yes"
[options.nosuch]
x = true
[options]
glob.star = true
glob.bare_qualifiers = false
cd = 3
X
run 'setopt -p history; setopt -p glob'
echo '--- not TOML'
printf '[options.glob]\nstar = true\nstar = false\n' > .config/luish/config.toml
run 'setopt -p glob'
printf '[options.glob\n' > .config/luish/config.toml
run 'setopt -p glob'
printf '[options]\nhistory = true\n[x]\n' > .config/luish/config.toml
run 'setopt -p glob'
echo '--- with rc.d: before it, and cached with it'
mkdir .config/luish/rc.d
printf '[options.glob]\nstar = true\nbogus = 1\n[options.history]\nsize = 50\n' > .config/luish/config.toml
echo 'echo "rc.d sees star=$(setopt | grep -c glob.star) HISTSIZE=$HISTSIZE"; HISTSIZE=5' > .config/luish/rc.d/a.lsh
show='setopt -p glob; echo "HISTSIZE=$HISTSIZE"'
run "$show"
# The cache is used: neither config.toml nor rc.d is read (so there is no
# error and no output from rc.d).
run "$show"
echo '--- a changed file is read again'
printf '[options.glob]\nbare_qualifiers = true\n' > .config/luish/config.toml
run "$show"
run "$show"
echo '--- and so is a removed one'
rm .config/luish/config.toml
run "$show"
echo '--- or a new one'
printf '[options.cd]\nauto = true\n' > .config/luish/config.toml
run 'setopt -p cd'
run 'setopt -p cd'
echo '--- a setting by its own name, directly under options'
printf '[options]\nautosuggest = true\nglob.star = true\n' > .config/luish/config.toml
run 'setopt -p editor; setopt -p glob'
echo '--- aliases, with global and suffix ones in their own tables'
cat > .config/luish/config.toml <<'X'
[alias]
ll = "echo ll"
".." = "echo up"
bad = 3
"a=b" = "x"
[alias.global]
U = "| tr a-z A-Z"
N = 1
[alias.suffix]
txt = "echo TXT"
X
show='alias -L; alias -Ls; ll U; ..; a.txt'
run "$show"
# From the cache.
run "$show"
# Without those tables, `global` and `suffix` can name regular aliases.
printf '[alias]\nglobal = "echo g"\nsuffix = [1]\n' > .config/luish/config.toml
run 'alias'
echo '--- key bindings, by name or as sequences'
cat > .config/luish/config.toml <<'Y'
[bindkey]
Up = "up-line-or-history"
"Ctrl-X Ctrl-E" = "undo"
"^[[B" = "down-line-or-history"
Down = 3
"Ctrl-Tab" = "undo"
"^W" = "nosuch"
Y
show='bindkey -L Up; bindkey "^X^E"; bindkey Down'
run "$show"
# From the cache.
run "$show"

# A directory plugin's plugin.toml can set options and define aliases and
# key bindings, in the options, alias and bindkey tables of config.toml. Its
# options override the user's (config.toml's), so that a plugin can package
# a set of options. As rc.lsh, the tables are for interactive shells only,
# and come after extension.rhai (not if it fails) and just before rc.lsh.
# Errors are reported with their lines and skipped; plugin.toml's
# dependencies still load first. The startup cache keeps what they did, and
# a changed plugin.toml invalidates it, also for a plugin that rc.d loads.
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$C/rc.d" "$P/p" "$P/q" "$P/bad"
run() { $SH -i -c "$1" 2>&1 | grep -v 'job control' | sed "s|$SH|luish|; s|$HOME|~|g"; }
printf '[options]\nautosuggest = false\n[plugins.enabled]\np = "*"\n' > "$C/config.toml"
cat > "$P/p/plugin.toml" <<'X'
description = "Aliases and key bindings."
[dependencies]
dep = "*"
[options]
autosuggest = true
glob.star = true
nosuch = 1
[alias]
ll = "echo ll"
bad = 3
[alias.global]
U = "| tr a-z A-Z"
[alias.suffix]
txt = "echo TXT"
[bindkey]
"Ctrl-X Ctrl-E" = "undo"
Up = "nosuch"
X
echo 'echo dep' > "$P/dep.lsh"
echo 'echo "p rc.lsh: $(alias ll) $(setopt -p glob | grep star)"' > "$P/p/rc.lsh"
echo 'print("p extension");' > "$P/p/extension.rhai"
# q is loaded by rc.d, and has no rc.lsh.
echo 'echo q' > "$P/q/init.lsh"
printf '[alias]\nqq = "echo qq"\n' > "$P/q/plugin.toml"
echo '__luish_internal plugin load q' > "$C/rc.d/a.lsh"
show='setopt -p editor; alias -L; alias -Ls; ll U; a.txt; qq; bindkey "^X^E"'
echo '--- builds the cache'
run "$show"
echo '--- uses it'
run "$show"
echo '--- a changed plugin.toml, of a plugin that rc.d loads, is read again'
printf '[alias]\nqq = "echo qq2"\n' > "$P/q/plugin.toml"
run 'qq'
run 'qq'
echo '--- not in a script'
$SH -c '__luish_internal plugin load ./.config/luish/plugins/p; alias; setopt -p glob; echo "status $?"'
echo '--- not if the extension fails'
printf '[alias]\nbb = "echo bb"\n' > "$P/bad/plugin.toml"
echo 'throw "no";' > "$P/bad/extension.rhai"
run '__luish_internal plugin load bad; alias bb' | sed 's/(line.*//'
echo '--- not TOML: reported once'
printf '[alias\n' > "$P/q/plugin.toml"
run 'alias qq'

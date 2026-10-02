# A theme is a plugin: its plugin.toml can define colour schemes, in a
# `colorscheme` table as config.toml's, which the user can then choose; its
# `style` table sets defaults for names (its own), under any scheme, and it
# can't choose the scheme, nor whether the terminal's colours are set, but a
# scheme can give them. The startup cache keeps what they did.
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$C/rc.d" "$P/theme"
run() { $SH -i -c "$1" 2>&1 | grep -v 'job control' | sed "s|$SH|luish|; s|$HOME|~|g"; }
cat > "$C/config.toml" <<'T'
[plugins.enabled]
theme = "*"
[style]
colorscheme = "solar"
command = "italic"
T
cat > "$P/theme/plugin.toml" <<'T'
description = "A theme."
[colorscheme.solar]
inherits = "default-dark"
keyword = "#268bd2"
string = "#b58900"
terminal.background = "#002b36"
[style]
"git.branch" = "magenta"
keyword = "red"
colorscheme = "solar"
terminal-colors = false
T
show='s() { __luish_internal style "$@"; }; s -c; s keyword; s string; s command.unknown; s git.branch; s command; s --terminal-colors; s -s solar terminal.background'
echo '--- builds the cache'
run "$show"
echo '--- uses it'
run "$show"

# config.toml's `colorscheme` table defines colour schemes, each a table of
# styles (as `style` takes them) that may inherit from another; its `style`
# table chooses one (or a dark and light pair) as `colorscheme`, and sets
# styles over it. Errors are reported with their lines and skipped. The
# startup cache keeps the result.
run() { $SH -i -c "$1" luish 2>&1 | grep -v 'job control' | sed "s|$HOME|~|g"; }
mkdir -p .config/luish/rc.d
cat > .config/luish/config.toml <<'T'
[colorscheme.blue]
keyword = "bold blue"
command = "blue"
"command.unknown" = "bold red"
string = "yellow"

[colorscheme.green]
inherits = "blue"
keyword = "bold green"
subst = "magenta"
var.unset = "dim cyan"     # the same as "var.unset"
[colorscheme.green.menu]
selected = "reverse"

[style]
colorscheme = "green"
"command.function" = "bold"
T
show='s() { __luish_internal style "$@"; }; s -c; s keyword; s command.unknown; s var.unset; s menu.selected; s command.function'
echo '--- an interactive shell reads it'
run "$show"
echo '--- and then from the cache'
run "$show"
echo '--- a pair, with a default'
cat > .config/luish/config.toml <<'T'
[colorscheme.light]
string = "136"
[style]
colorscheme = { dark = "default-dark", light = "light", default = "light" }
T
run '__luish_internal style -c'
LUISH_BACKGROUND=dark run '__luish_internal style -c'
echo '--- a scheme replaces an earlier one of the same name'
cat > .config/luish/config.toml <<'T'
[colorscheme.default-dark]
keyword = "italic"
T
run '__luish_internal style keyword; __luish_internal style command'
echo '--- errors'
cat > .config/luish/config.toml <<'T'
[colorscheme]
x = 3
[colorscheme.a]
inherits = "a"
keyword = "bolt"
"command.nope" = "red"
comand = "red"
x = 3
[colorscheme."b c"]
[style]
colorscheme = { dark = "a" }
keyword = "red blue"
T
run '__luish_internal style -p'
cat > .config/luish/config.toml <<'T'
[style]
colorscheme = { dark = "a", light = "b", other = "c" }
T
run '__luish_internal style -p'
echo '--- terminal colours, and whether to set them'
cat > .config/luish/config.toml <<'T'
[colorscheme.g]
keyword = "red"
[colorscheme.g.terminal]
background = "#282828"
palette = ["#282828", "#cc241d"]
cursor = 3
bold = "#000000"
foreground = "red"

[colorscheme.h]
inherits = "g"
terminal.background = "#fbf1c7"

[style]
colorscheme = "h"
terminal-colours = false
T
run '__luish_internal style -p'
cat > .config/luish/config.toml <<'T'
[style]
terminal-colors = "no"
T
run '__luish_internal style --terminal-colors'

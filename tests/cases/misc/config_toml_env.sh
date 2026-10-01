# config.toml's `env` table sets exported variables, for interactive and
# login shells, its `interactive` table exported variables for interactive
# shells only, and `vars` shell variables that aren't exported (interactive
# shells only). `path` adds directories to PATH, `before` or `after` those
# it has, skipping any that it has already, wherever it is (so that a shell
# started by `pixi shell` keeps the environment's directories first).
# Values are literal but for a leading `~`.
export PATH=/usr/bin:/bin
# An empty login.d, so that login shells don't read /etc/profile.
mkdir -p .config/luish/login.d
cat > .config/luish/config.toml <<'X'
[path]
before = ["~/bin", "/b1", "/usr/bin", "~/bin"]
after = ["/a1", "/b1", "/a2"]

[env]
EDITOR = "nvim"
GOPATH = "~/go:~/x"
NUM = 42
PATH = "/usr/bin:/bin"

[env.interactive]
LESS = "-R"

[vars]
WORDCHARS = "*?_-."
X
show='for v in EDITOR GOPATH NUM LESS WORDCHARS; do
  eval "printf \"%s=%s \" $v \"\${$v-unset}\""
  if /usr/bin/env | /bin/grep -q "^$v="; then echo exported; else echo local; fi
done
echo "PATH=$PATH"'
run() { "$@" 2>&1 | grep -v "job control" | sed "s|$HOME|~|g; s|^$SH:|luish:|"; }
echo '--- interactive'
run $SH -i -c "$show"
echo '--- from the cache'
run $SH -i -c "$show"
echo '--- a login shell that is not interactive: env and path only'
run $SH -l -c "$show"
echo '--- not -c or a script'
run $SH -c "$show"
echo '--- --no-rcs'
run $SH -l --no-rcs -c "$show"
echo '--- directories that PATH has already stay where they are'
cat > .config/luish/config.toml <<'X'
[path]
before = ["~/bin", "~/.local/bin"]
after = ["/opt/bin"]
X
run env PATH="/env/bin:$HOME/bin:/usr/bin:/bin" $SH -i -c 'echo "$PATH"'
run env PATH="/env/bin:$HOME/bin:/usr/bin:/bin" $SH -i -c 'echo "$PATH"'
run env PATH="/usr/bin:/bin" $SH -i -c 'echo "$PATH"'
# A shell started from one that has read it adds nothing.
run env PATH="/usr/bin:/bin" $SH -l -c '$SH -i -c "echo \"\$PATH\""'
echo '--- errors'
cat > .config/luish/config.toml <<'X'
[env]
A = true
"B-C" = "x"
D = "ok"
interactive = "x"   # not a table: a variable
[vars]
E = [1]
[path]
before = "~/bin"
after = ["/x", 3, "", "/y"]
around = ["/z"]
X
run env PATH=/usr/bin:/bin $SH -i -c 'echo "D=$D interactive=$interactive PATH=$PATH"'
run env PATH=/usr/bin:/bin $SH -l -c 'echo "D=$D interactive=$interactive PATH=$PATH"'
printf 'env = 1\nvars = "x"\npath = []\n' > .config/luish/config.toml
run $SH -i -c 'echo ok'

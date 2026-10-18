# `release = "luish"`: a git source at the newest tag `vX.Y` or `vX.Y.N` for
# this luish's version X.Y, else for the newest older one; pre-releases and
# tags for newer versions are skipped. The lock records X.Y, `plugin check`
# and `plugin update` follow newer tags, and a source with no such tag is an
# error.
export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=a GIT_AUTHOR_EMAIL=a@b GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@b
export GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z'
C=$HOME/.config/luish
mkdir -p "$C"
# This luish's X.Y, shown as X.Y, and the next version.
v=$($SH --version | sed 's/^luish \([0-9]*\)\.\([0-9]*\).*/\1 \2/')
set -- $v
M=$1.$2 NEXT=$1.$(($2 + 1))
show() { sed "s|$SH|luish|; s|$HOME|~|g; s|luish $M at|luish X.Y at|; s|tag v$M or v$M\.N|tag vX.Y or vX.Y.N|; s|v$M\.|vX.Y.|g; s|\"$M\"|\"X.Y\"|; s|v$NEXT\.|vNEXT.|g; s|commit = \".*\"|commit = ...|; s|at [0-9a-f]\{7\}|at ...|; s|[0-9a-f]\{7\}\.\.[0-9a-f]\{7\}|...|"; }
run() { $SH -i -c "$1" 2>&1 | grep -v 'job control' | show; }
cmd() { $SH -c "$1" 2>&1 | show; }
release() { # REPO TAG [-a]: commits a plugin that says which tag it is, and tags it.
    mkdir -p "$1"
    echo "echo '$1 at $2'" | sed "s/v$M\./vX.Y./; s/v$NEXT\./vNEXT./" > "$1/init.lsh"
    (cd "$1" && { [ -d .git ] || git init -q -b main; } && git add -A && git commit -qm "$2" && git tag $3 ${3:+-m "$2"} "$2")
}
release p v0.0.1
release p "v$M.0"
release p "v$M.1" -a
release p "v$M.2-rc1"
release p "v$NEXT.0"
release p vlatest
# Only a tag for an older version.
release old v0.0.1
# Only tags for newer versions.
release new "v$NEXT.0"
cat > "$C/config.toml" <<X
[plugins.enabled]
p = { git = "file://$HOME/p", release = "luish" }
old = { git = "file://$HOME/old", release = "luish" }
X
echo '--- sync takes the newest tag for this version, or for an older one'
cmd '__luish_internal plugin sync; echo "status $?"'
grep -A1 '^url' "$C/plugins.lock" | show
run 'true'
echo '--- a newer tag for this version: check and update'
release p "v$M.2"
cmd '__luish_internal plugin check; echo "status $?"'
cmd '__luish_internal plugin update p; echo "status $?"'
run 'true'
echo '--- no tag for this version or an older one'
echo "new = { git = \"file://$HOME/new\", release = \"luish\" }" >> "$C/config.toml"
cmd '__luish_internal plugin sync; echo "status $?"'
echo '--- bad sources'
cat > "$C/config.toml" <<X
[plugins.enabled]
a = { git = "file://$HOME/p", release = "latest" }
b = { git = "file://$HOME/p", release = "luish", branch = "main" }
c = { path = "~/p", release = "luish" }
X
cmd '__luish_internal plugin sync; echo "status $?"'

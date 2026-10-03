#!/bin/sh
# Tests install.sh against the packages that scripts/dist.sh left in
# target/dist/, under dash, bash and the packaged luish itself:
#
#   pixi run dist && sh scripts/test-install.sh
set -eu

cd "$(dirname "$0")/.."
top=$PWD
case $(uname -m) in
x86_64 | amd64) arch=x86_64 ;;
aarch64 | arm64) arch=aarch64 ;;
esac
dist=file://$top/target/dist
unset LUISH_INSTALL_DIR LUISH_LIBC XDG_CONFIG_HOME
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# The configuration is written to $HOME/.config/luish.
export HOME="$tmp/home"
mkdir "$HOME"
failed=0

fail() {
    echo "FAIL: $*" >&2
    failed=1
}

# check NAME EXPECTED-STATUS SHELL [install.sh options]: runs install.sh into
# a fresh directory, downloading from $url, and checks its status and, if it
# succeeded, the result.
check() {
    name=$1 want=$2 sh=$3
    shift 3
    rm -rf "$tmp/bin"
    status=0
    LUISH_DOWNLOAD_URL=$url "$sh" install.sh --dir "$tmp/bin" "$@" >"$tmp/out" 2>&1 || status=$?
    if [ "$status" != "$want" ]; then
        fail "$name ($sh): status $status, expected $want"
        sed 's/^/    /' "$tmp/out" >&2
        return
    fi
    if [ "$want" = 0 ]; then
        [ "$("$tmp/bin/luish" -c 'echo $((6 * 7))')" = 42 ] || fail "$name ($sh): the installed luish doesn't run"
    else
        [ ! -e "$tmp/bin/luish" ] || fail "$name ($sh): installed despite failing"
    fi
    echo "ok: $name ($sh)"
}

url=$dist
builds=
for libc in gnu musl; do
    [ -e "target/dist/luish-$arch-linux-$libc.tar.gz" ] && builds="$builds $libc"
done
[ -n "$builds" ] || {
    echo "test-install.sh: no packages in target/dist; run pixi run dist" >&2
    exit 1
}
first=${builds# }
first=${first%% *}
tar -xzf "target/dist/luish-$arch-linux-$first.tar.gz" -C "$tmp"
packaged=$tmp/luish-$arch-linux-$first/luish

for sh in dash bash "$packaged"; do
    command -v "$sh" >/dev/null || continue
    for libc in $builds; do
        check "--$libc" 0 "$sh" "--$libc"
        grep -q "($libc build)" "$tmp/out" || fail "--$libc ($sh): installed another build: $(cat "$tmp/out")"
    done
    check "unknown option" 1 "$sh" --bogus
    url=file://$tmp/none
    check "missing download" 1 "$sh"
    url=$dist
done

# The default on a glibc system is the gnu build (when there is one).
case $builds in *gnu*)
    check "default build" 0 sh
    grep -q '(gnu build)' "$tmp/out" || fail "default build: $(cat "$tmp/out")"
    ;;
esac

# Reinstalling replaces the binary that is running install.sh.
libc=$first
check "first install" 0 sh "--$libc"
cp "$tmp/bin/luish" "$tmp/running"
LUISH_DOWNLOAD_URL=$url "$tmp/bin/luish" install.sh --dir "$tmp/bin" "--$libc" >"$tmp/out" 2>&1 || fail "reinstall over the running luish"
cmp -s "$tmp/running" "$tmp/bin/luish" || fail "reinstall: the binary differs"
echo "ok: reinstall over the running luish"

# A corrupt download is refused.
mkdir "$tmp/bad"
cp "target/dist/luish-$arch-linux-$libc.tar.gz" "target/dist/luish-$arch-linux-$libc.tar.gz.sha256" "$tmp/bad/"
printf 'x' >>"$tmp/bad/luish-$arch-linux-$libc.tar.gz"
url=file://$tmp/bad
check "corrupt download" 1 sh "--$libc"

# When the gnu build doesn't run (as on NixOS), the default falls back to musl.
case $builds in *gnu*musl*)
    mkdir "$tmp/broken" "$tmp/broken/luish-$arch-linux-gnu"
    printf '#!/nonexistent\n' >"$tmp/broken/luish-$arch-linux-gnu/luish"
    chmod +x "$tmp/broken/luish-$arch-linux-gnu/luish"
    tar -C "$tmp/broken" -czf "$tmp/broken/luish-$arch-linux-gnu.tar.gz" "luish-$arch-linux-gnu"
    (cd "$tmp/broken" && sha256sum "luish-$arch-linux-gnu.tar.gz" >"luish-$arch-linux-gnu.tar.gz.sha256")
    cp "target/dist/luish-$arch-linux-musl.tar.gz" "target/dist/luish-$arch-linux-musl.tar.gz.sha256" "$tmp/broken/"
    url=file://$tmp/broken
    check "fallback to musl" 0 sh
    grep -q '(musl build)' "$tmp/out" || fail "fallback to musl: $(cat "$tmp/out")"
    check "no fallback with --gnu" 1 sh --gnu
    ;;
esac

# The configuration. With stderr not a terminal, nothing is asked or written.
conf=$HOME/.config/luish
url=$dist
check "no configuration without a terminal" 0 sh "--$libc"
[ ! -e "$conf" ] || fail "no configuration without a terminal: wrote $conf"
grep -q 'first time it runs' "$tmp/out" || fail "no configuration without a terminal: $(cat "$tmp/out")"
# --config writes it; the plugins are fetched with a git that fails here, so that nothing is downloaded.
mkdir "$tmp/fakegit"
printf '#!/bin/sh\nexit 1\n' >"$tmp/fakegit/git"
chmod +x "$tmp/fakegit/git"
PATH=$tmp/fakegit:$PATH check "--config" 0 sh "--$libc" --config
grep -qx 'extra.complete.all = "\*".*' "$conf/config.toml" && grep -qx 'extra.themes = "\*".*' "$conf/config.toml" ||
    fail "--config: $(cat "$conf/config.toml" 2>&1)"
grep -q 'run plugin sync' "$tmp/out" || fail "--config: no warning that plugin sync failed: $(cat "$tmp/out")"
# An existing configuration is left alone.
mkdir -p "$conf"
echo '# mine' >"$conf/config.toml"
check "--config with a configuration" 0 sh "--$libc" --config
[ "$(cat "$conf/config.toml")" = '# mine' ] || fail "--config with a configuration: replaced it"
rm -r "$conf"
check "--no-config" 0 sh "--$libc" --no-config
[ ! -e "$conf" ] || fail "--no-config: wrote $conf"

exit $failed

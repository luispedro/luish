#!/bin/sh
# Builds the release binaries for this machine's architecture, checks them,
# and packages them in target/dist/ as luish-ARCH-linux-LIBC.tar.gz with a
# .sha256 file each (the names install.sh downloads).
#
#   pixi run dist                 # both builds
#   pixi run dist gnu             # only the glibc build (or: musl)
#
# gnu: linked against glibc 2.17 (conda-forge's sysroot, set up by the dist
# environment in pixi.toml), so it runs on any distribution with glibc 2.17
# or later. musl: static, built with rustup's toolchain of the same Rust
# version, since conda-forge has no musl Rust standard library.
set -eu

cd "$(dirname "$0")/.."
top=$PWD

case $(uname -m) in
x86_64 | amd64) arch=x86_64 ;;
aarch64 | arm64) arch=aarch64 ;;
*)
    echo "dist.sh: unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac
glibc_max=2.17
out=target/dist
mkdir -p "$out"

die() {
    echo "dist.sh: $*" >&2
    exit 1
}

# Runs the built binary, including a Rhai extension.
smoke() {
    tmp=$(mktemp -d)
    echo 'print(`rhai ${6 * 7}`);' >"$tmp/t.rhai"
    got=$(cd "$tmp" && "$top/$1" -c 'echo "sh $((6 * 7))"; __luish_internal plugin load ./t.rhai' 2>&1)
    rm -rf "$tmp"
    [ "$got" = "sh 42
rhai 42" ] || die "$1 doesn't run: $got"
    "$1" --version
}

package() { # binary libc
    name=luish-$arch-linux-$2
    rm -rf "${out:?}/$name" "$out/$name.tar.gz"
    mkdir "$out/$name"
    cp "$1" README.md COPYING.MIT "$out/$name/"
    tar -C "$out" -czf "$out/$name.tar.gz" "$name"
    rm -r "${out:?}/$name"
    (cd "$out" && sha256sum "$name.tar.gz" >"$name.tar.gz.sha256")
    echo "dist.sh: $out/$name.tar.gz"
}

build_gnu() {
    target=$arch-unknown-linux-gnu
    cargo build --release --locked --target "$target"
    bin=target/$target/release/luish
    # The newest glibc symbol version must be at most $glibc_max.
    newest=$(${OBJDUMP:-objdump} -T "$bin" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -Vu | tail -n 1)
    [ -n "$newest" ] || die "no glibc symbol versions in $bin"
    [ "$(printf '%s\n' "$newest" "$glibc_max" | sort -V | tail -n 1)" = "$glibc_max" ] ||
        die "$bin needs glibc $newest (at most $glibc_max expected)"
    smoke "$bin"
    package "$bin" gnu
}

build_musl() {
    target=$arch-unknown-linux-musl
    command -v rustup >/dev/null || die "the musl build needs rustup (https://rustup.rs)"
    rust=$(rustc --version | cut -d' ' -f2)
    rustup toolchain install "$rust" --profile minimal --target "$target"
    rustup run "$rust" cargo build --release --locked --target "$target"
    bin=target/$target/release/luish
    ! ${READELF:-readelf} -l "$bin" | grep -q INTERP || die "$bin is not static"
    smoke "$bin"
    package "$bin" musl
}

[ $# -gt 0 ] || set -- gnu musl
for libc; do
    case $libc in
    gnu) build_gnu ;;
    musl) build_musl ;;
    *) die "unknown build: $libc (gnu or musl)" ;;
    esac
done

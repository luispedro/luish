#!/bin/sh
# Installs luish from its GitHub releases, for Linux on x86_64 or aarch64:
#
#   curl -fsSL https://raw.githubusercontent.com/luispedro/luish/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/luispedro/luish/main/install.sh | sh -s -- --musl
#
# Options, most also settable with an environment variable:
#
#   --dir DIR        LUISH_INSTALL_DIR  where to put `luish` (default: ~/.local/bin)
#   --version TAG                       a release tag such as v0.1.0 (default: the latest release)
#   --gnu, --musl    LUISH_LIBC         which build: gnu (dynamically linked against glibc 2.17 or later) or musl
#                                       (static, runs anywhere but is slower); by default gnu where it can run
#   --config         write the recommended configuration (with luish-extra) without asking, if there is none
#   --no-config      don't offer to write a configuration
#
# Without a configuration (~/.config/luish missing or empty), it asks on the terminal whether to write the recommended
# one, and fetches its plugins; with no terminal to ask on, it leaves that to luish's first run.
#
# LUISH_DOWNLOAD_URL replaces the release's download URL (for testing, e.g. file:///path/to/target/dist).

set -eu

repo=luispedro/luish

# Colours, when stderr is a terminal and NO_COLOR (https://no-color.org) isn't set.
if [ -t 2 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-dumb}" != dumb ]; then
    esc=$(printf '\033')
    bold="$esc[1m" red="$esc[31m" green="$esc[32m" yellow="$esc[33m" cyan="$esc[36m" off="$esc[m"
else
    bold= red= green= yellow= cyan= off=
fi

say() {
    printf '%s\n' "${bold}install.sh:${off} $*" >&2
}

ok() {
    say "$green$*$off"
}

warn() {
    say "${yellow}warning:$off $*"
}

die() {
    say "${red}error:$off $*"
    exit 1
}

# A command for the user to type, in a message.
cmd() {
    printf '%s' "$cyan$*$off"
}

usage() {
    cat <<'END'
usage: install.sh [--dir DIR] [--version TAG] [--gnu | --musl] [--config | --no-config]

  --dir DIR      where to put luish (default: ~/.local/bin; or LUISH_INSTALL_DIR)
  --version TAG  a release tag such as v0.1.0 (default: the latest)
  --gnu          the build linked against glibc 2.17 or later (the default where it runs)
  --musl         the static build, which runs anywhere but is slower (or LUISH_LIBC=gnu/musl)
  --config       write the recommended configuration, with luish-extra, if there is none (asked by default)
  --no-config    don't offer to write a configuration
END
}

# Whether the glibc build can run: glibc 2.17 or later, with its dynamic loader
# where the binary looks for it (NixOS and Alpine don't have it there).
glibc_ok() {
    case $arch in
    x86_64) [ -e /lib64/ld-linux-x86-64.so.2 ] || return 1 ;;
    aarch64) [ -e /lib/ld-linux-aarch64.so.1 ] || return 1 ;;
    esac
    v=$(getconf GNU_LIBC_VERSION 2>/dev/null) || v=
    v=${v#glibc }
    if [ -z "$v" ]; then
        # glibc's ldd prints "ldd (GNU libc) 2.17" or "ldd (Ubuntu GLIBC 2.39-0ubuntu8) 2.39"; musl's has no version.
        line=$(ldd --version 2>&1 | head -n 1) || line=
        case $line in *GLIBC* | *"GNU libc"*) v=${line##* } ;; esac
    fi
    major=${v%%.*}
    minor=${v#*.}
    minor=${minor%%.*}
    case $major$minor in '' | *[!0-9]*) return 1 ;; esac
    [ "$major" -gt 2 ] || { [ "$major" -eq 2 ] && [ "$minor" -ge 17 ]; }
}

download() { # url file
    if command -v curl >/dev/null; then
        curl -fsSL -o "$2" "$1"
    elif command -v wget >/dev/null; then
        wget -q -O "$2" "$1"
    else
        die "neither curl nor wget is installed"
    fi
}

# Downloads, checks and unpacks the $libc build into $tmp/$name.
fetch() {
    name=luish-$arch-linux-$libc
    say "downloading $base/$name.tar.gz"
    download "$base/$name.tar.gz" "$tmp/$name.tar.gz" || die "download failed: $base/$name.tar.gz"
    download "$base/$name.tar.gz.sha256" "$tmp/$name.tar.gz.sha256" || die "download failed: $base/$name.tar.gz.sha256"
    if command -v sha256sum >/dev/null; then
        (cd "$tmp" && sha256sum -c "$name.tar.gz.sha256" >/dev/null 2>&1) || die "checksum mismatch for $name.tar.gz"
    elif command -v shasum >/dev/null; then
        (cd "$tmp" && shasum -a 256 -c "$name.tar.gz.sha256" >/dev/null 2>&1) || die "checksum mismatch for $name.tar.gz"
    else
        warn "no sha256sum or shasum, so the download is not verified"
    fi
    tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
}

# Whether the user can be asked a question: on the terminal, as stdin is the script with curl | sh.
can_ask() {
    [ -t 2 ] && { true </dev/tty; } 2>/dev/null
}

# Offers to write the recommended configuration, with luish-extra (its completion and themes), where luish
# looks for it, unless there is one (the directory isn't empty, as for luish's first run). $config is yes to
# write it without asking, no to do nothing, or empty to ask.
setup_config() {
    luish=$1
    [ "$config" != no ] || return 0
    case ${XDG_CONFIG_HOME:-} in
    /*) confdir=$XDG_CONFIG_HOME/luish ;;
    *) confdir=$HOME/.config/luish ;;
    esac
    shown=$confdir
    case $confdir in "$HOME"/*) shown="~${confdir#"$HOME"}" ;; esac
    if [ -n "$(ls -A "$confdir" 2>/dev/null)" ]; then
        [ "$config" != yes ] || say "$shown already has a configuration, so not writing one"
        return 0
    fi
    # Older releases can't write it (and luish-extra needs luish 0.4.0 or newer).
    if ! "$luish" -c '__luish_internal default-config --extra' >"$tmp/config.toml" 2>/dev/null; then
        say "this luish can't write a configuration from here; it offers one the first time it runs"
        return 0
    fi
    if [ -z "$config" ]; then
        if ! can_ask; then
            say "there is no configuration in $shown: luish offers one the first time it runs"
            return 0
        fi
        say "there is no configuration in $shown yet. The recommended one has:"
        say "  - Tab completion for about 500 commands, with luish-extra's (science, bioinformatics, ...)"
        say "  - colour schemes for the command line ($(cmd style -c) lists them)"
        say "  - suggestions from the history as you type (Right accepts them)"
        say "  - zsh's % sequences in prompts, and history expansion ($(cmd '!!'), $(cmd '!$'))"
        printf '%s' "${bold}install.sh:${off} write it, and fetch its plugins? [Y/n] " >&2
        answer=
        read -r answer </dev/tty || { echo >&2; answer=n; }
        case $answer in
        '' | [Yy]*) ;;
        *)
            say "not writing it: luish offers it again the first time it runs"
            return 0
            ;;
        esac
    fi
    mkdir -p "$confdir"
    cp "$tmp/config.toml" "$confdir/config.toml"
    ok "wrote $shown/config.toml"
    if ! command -v git >/dev/null; then
        warn "fetching the plugins needs git: install it, then run $(cmd plugin sync) in luish"
    elif ! "$luish" -c '__luish_internal plugin sync' >&2; then
        warn "the plugins couldn't be fetched: run $(cmd plugin sync) in luish to try again"
    fi
}

main() {
    dir=${LUISH_INSTALL_DIR:-${HOME:?}/.local/bin}
    version=latest
    libc=${LUISH_LIBC:-}
    config=
    while [ $# -gt 0 ]; do
        case $1 in
        --dir) [ $# -ge 2 ] || die "--dir needs a directory"; dir=$2; shift ;;
        --dir=*) dir=${1#*=} ;;
        --version) [ $# -ge 2 ] || die "--version needs a tag"; version=$2; shift ;;
        --version=*) version=${1#*=} ;;
        --gnu | --glibc) libc=gnu ;;
        --musl) libc=musl ;;
        --config) config=yes ;;
        --no-config) config=no ;;
        -h | --help) usage; exit 0 ;;
        *) die "unknown option: $1 (see --help)" ;;
        esac
        shift
    done

    [ "$(uname -s)" = Linux ] || die "luish's releases are for Linux; on $(uname -s), build it from source"
    case $(uname -m) in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) die "there is no release for $(uname -m); build luish from source" ;;
    esac
    auto=
    case $libc in
    '')
        auto=1
        if glibc_ok; then libc=gnu; else libc=musl; fi
        ;;
    glibc) libc=gnu ;;
    gnu | musl) ;;
    *) die "unknown build: $libc (gnu or musl)" ;;
    esac

    if [ -n "${LUISH_DOWNLOAD_URL:-}" ]; then
        base=$LUISH_DOWNLOAD_URL
    elif [ "$version" = latest ]; then
        base=https://github.com/$repo/releases/latest/download
    else
        base=https://github.com/$repo/releases/download/$version
    fi
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM

    fetch
    if ! "$tmp/$name/luish" -c : 2>/dev/null; then
        # Such as on NixOS, whose /lib64/ld-linux-x86-64.so.2 only prints an error.
        [ -n "$auto" ] && [ "$libc" = gnu ] || die "the $libc build doesn't run on this system"
        say "the gnu build doesn't run on this system, so using the musl build"
        libc=musl
        fetch
        "$tmp/$name/luish" -c : 2>/dev/null || die "the musl build doesn't run on this system"
    fi

    # Replace any existing luish by renaming, so that running shells keep theirs.
    mkdir -p "$dir"
    cp "$tmp/$name/luish" "$dir/.luish.new.$$"
    chmod 755 "$dir/.luish.new.$$"
    mv -f "$dir/.luish.new.$$" "$dir/luish"
    ok "installed $("$dir/luish" --version) ($libc build) as $dir/luish"

    setup_config "$dir/luish"

    case :$PATH: in
    *:"$dir":*) ;;
    *) warn "$dir is not in your PATH; add it in your shell's startup file: $(cmd "export PATH=\"$dir:\$PATH\"")" ;;
    esac
    if ! grep -qx "$dir/luish" /etc/shells 2>/dev/null; then
        say "to make luish your login shell:"
        say "  $(cmd "echo $dir/luish | sudo tee -a /etc/shells && chsh -s $dir/luish")"
    fi
}

main "$@"

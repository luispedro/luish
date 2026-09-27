# luish

[![CI](https://github.com/luispedro/luish/actions/workflows/ci.yml/badge.svg)](https://github.com/luispedro/luish/actions/workflows/ci.yml)
[![Documentation](https://readthedocs.org/projects/luish/badge/?version=latest)](https://luish.readthedocs.io/en/latest/)
[![Latest release](https://img.shields.io/github/v/release/luispedro/luish)](https://github.com/luispedro/luish/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](COPYING.MIT)

luish is a POSIX `sh` for Linux, written in Rust, meant to replace zsh as a daily driver, built on a core that is as
fast as [dash](http://gondor.apana.org.au/~herbert/dash/).

- **POSIX first.** luish implements the POSIX Shell Command Language and its required built-ins, plus `local`.
  Where POSIX is ambiguous, luish matches dash.
- **As fast as dash, with zsh's features.** Scripts run as fast as under dash (faster on arithmetic and function
  calls), and up to five times faster than under bash or zsh. Interactively: zsh's keys, syntax highlighting,
  autosuggestions, shared history in zsh's format, completion with a menu, zsh's prompts and glob qualifiers.
- **A modern plugin architecture**: plugins in shell and [Rhai](https://rhai.rs), listed in `config.toml`, fetched
  from git and pinned in a lock file. Anything beyond POSIX is opt-in and costs nothing when it is not used.
- **Instant startup**: luish caches the effect of your startup files, so a shell starts in milliseconds even with
  `conda`, `nvm` and the like set up.
- **Modern configuration** in a TOML file, with options named in groups (`history.share`) and settings in layers
  (`config.toml`, plugins, your own startup files).

luish is usable as a daily shell. The user documentation is in [docs/](docs/index.md), with the benchmarks in
[docs/performance.md](docs/performance.md); [docs/compatibility.md](docs/compatibility.md) lists the differences
from dash and the known limitations, and [DEVELOPING.md](DEVELOPING.md) has how luish is implemented and tested.

## Installing

Releases have binaries for Linux on x86_64 and aarch64. This installs the right one in `~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/luispedro/luish/main/install.sh | sh
```

[docs/installation.md](docs/installation.md) has its options, such as the static musl build (`sh -s -- --musl`), and
how to install luish with Nix (`nix profile install github:luispedro/luish`).

## Building

luish uses [pixi](https://pixi.sh) to provide the Rust toolchain:

```sh
pixi run release                  # release build: target/release/luish
pixi run luish -c 'echo hello'    # run a debug build
```

Plain `cargo build --release` also works with a recent Rust toolchain (edition 2024).

## Usage

luish accepts the usual `sh` invocations:

```sh
luish script.sh [args...]
luish -c 'command' [arg0 [args...]]
luish -s [args...]                # read commands from stdin
luish                             # interactive when stdin is a terminal
```

Options can be given as letters (`-e`, `-x`, ...) or with `-o name` / `+o name`, where the name can be any option as
`setopt` names it (such as `-o err_exit` or `-o prompt_percent`). `luish --help` lists the options, and `--no-rcs`
starts a shell without reading any startup files.

## Differences from dash

luish differs from dash in a few deliberate places, for example `$LINENO` is supported, and a script read from a
pipe is never read past the current command, so commands in it can read the rest of stdin. The full list is in
[docs/compatibility.md](docs/compatibility.md).

## Testing

Most tests are differential: each script in `tests/cases/` is run under luish and under the system dash, and their
output and exit status must match. Interactive behaviour (job control, signals, terminal modes) is tested on a
pseudo-terminal. dash must be installed from the system (`/usr/bin/dash`).

```sh
pixi run check                    # rustfmt, clippy, and all tests
LUISH_CASE=expand/ pixi run test  # only the differential cases whose path contains "expand/"
```

[DEVELOPING.md](DEVELOPING.md) has more for developers: design decisions, implementation notes and their tests.

## Roadmap

The goals are grouped into stages, described in [GOALS.md](GOALS.md):

1. **Reproduce existing functionality** (current focus): POSIX conformance, dash-level speed, and a usable
   interactive shell.
2. **Beyond POSIX**: lazily loaded plugins, opt-in arrays and process substitution, modern terminal features, better
   error messages and strict mode, and richer history and completion.
3. **New capabilities**: caching the effects of login scripts for near-instant startup, and a built-in SSH mode
   where the line editor runs locally.

The implementation plan is in [PLAN.md](PLAN.md).

## License

luish is licensed under the [MIT License](COPYING.MIT).

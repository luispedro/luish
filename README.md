# luish

luish is a POSIX `sh` for Linux, written in Rust. The long-term aim is a shell that can replace zsh as a daily
driver, built on a core that is as fast as dash.

- **POSIX first.** luish implements the POSIX Shell Command Language and its required built-ins, plus `local`.
  Where POSIX is ambiguous, luish matches [dash](http://gondor.apana.org.au/~herbert/dash/).
- **As fast as dash** for scripts and `sh -c`, both at startup and while running.
- **Pay only for what you use.** Anything beyond POSIX is opt-in and costs nothing when it is not used.
- **Usable interactively**: line editing, history (with `fc`), tab completion, and job control.

luish is at an early stage. Most of POSIX is implemented, but expect rough edges. See [STATUS.md](STATUS.md) for
what works, what is missing, and current benchmark numbers.

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
[DEVIATIONS.md](DEVIATIONS.md).

## Testing

Most tests are differential: each script in `tests/cases/` is run under luish and under the system dash, and their
output and exit status must match. Interactive behaviour (job control, signals, terminal modes) is tested on a
pseudo-terminal. dash must be installed from the system (`/usr/bin/dash`).

```sh
pixi run check                    # rustfmt, clippy, and all tests
LUISH_CASE=expand/ pixi run test  # only the differential cases whose path contains "expand/"
```

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

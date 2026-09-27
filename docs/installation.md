# Installation

## From a release

Releases have binaries for Linux on x86_64 and aarch64. The install script picks the right one and puts it in
`~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/luispedro/luish/main/install.sh | sh
```

Options go after `sh -s --`, such as `curl -fsSL .../install.sh | sh -s -- --musl --dir /usr/local/bin`:

| Option | Environment variable | Meaning |
|---|---|---|
| `--dir DIR` | `LUISH_INSTALL_DIR` | Where to put `luish` (default: `~/.local/bin`) |
| `--version TAG` | `LUISH_VERSION` | A release tag such as `v0.1.0` (default: the latest release) |
| `--gnu`, `--musl` | `LUISH_LIBC` (`gnu` or `musl`) | Which build (see below) |

The script checks the download's SHA-256 checksum, and replaces an existing `luish` by renaming, so shells that are
running keep working. Running it again updates luish. To uninstall, remove the file.

There are two builds:

- **gnu** (the default wherever it runs) is linked dynamically against glibc, and needs glibc 2.17 or later: any
  distribution from 2014 on (RHEL and CentOS 7, Debian 8, Ubuntu 14.04), except NixOS, whose dynamic loader is
  elsewhere. Unless you choose a build, the script installs the musl build where this one doesn't run.
- **musl** is a static binary, which runs on any Linux (Alpine, NixOS, containers without a C library). It starts
  faster, but scripts that do much work inside the shell take up to twice as long, since musl's `malloc` is slower.
  It also differs in a few details, listed in [](compatibility.md#known-limitations).

To make luish your login shell, add it to `/etc/shells` and use `chsh`, as the install script suggests:

```sh
echo ~/.local/bin/luish | sudo tee -a /etc/shells
chsh -s ~/.local/bin/luish
```

## From source

luish uses [pixi](https://pixi.sh) to provide the Rust toolchain:

```sh
pixi run release                  # release build: target/release/luish
```

Plain `cargo build --release` also works with a recent Rust toolchain (edition 2024).

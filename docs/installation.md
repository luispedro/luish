# Installation

## From a release

Releases have binaries for Linux on x86_64 and aarch64. The install script picks the right one and puts it in
`~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/luispedro/luish/main/install.sh | sh
```

### Advanced options for the install script

Options go after `sh -s --`, such as `curl -fsSL .../install.sh | sh -s --
--musl --dir /usr/local/bin`:

| Option | Environment variable | Meaning |
|---|---|---|
| `--dir DIR` | `LUISH_INSTALL_DIR` | Where to put `luish` (default: `~/.local/bin`) |
| `--version TAG` | `LUISH_VERSION` | A release tag such as `v0.1.0` (default: the latest release) |
| `--gnu`, `--musl` | `LUISH_LIBC` (`gnu` or `musl`) | Which build (see below) |

The script checks the download's SHA-256 checksum, and replaces an existing
`luish` by renaming, so shells that are running keep working. Running it again
updates luish. To uninstall, remove the file.

There are two builds:

- **gnu** (the default wherever it runs) is linked dynamically against glibc, and needs glibc 2.17 or later: any
  distribution from 2014 on (RHEL and CentOS 7, Debian 8, Ubuntu 14.04), except NixOS, whose dynamic loader is
  elsewhere. Unless you choose a build, the script installs the musl build where this one doesn't run.
- **musl** is a static binary, which runs on any Linux (Alpine, NixOS, containers without a C library). It starts
  faster, but it runs a bit slower, since musl's `malloc` is slower.

## With Nix

The repository is a Nix flake (`flake.nix`), for x86_64 and aarch64 Linux.

```sh
nix run github:luispedro/luish                  # run it without installing
nix profile install github:luispedro/luish      # install it in your profile
```

The first time you run it, it might take a few minutes to download the dependencies and build the binary, afterwards it be cached and run quickly.

On NixOS, add the flake to your system's inputs (`inputs.luish.url = "github:luispedro/luish";`) and use its package,
here as a login shell:

```nix
{ pkgs, inputs, ... }:
let
  luish = inputs.luish.packages.${pkgs.stdenv.hostPlatform.system}.default;
in
{
  environment.systemPackages = [ luish ];
  environment.shells = [ luish ];
  users.users.alice.shell = luish;
}
```

With Home Manager, add the same package to `home.packages`. `nix develop` gives
a shell with the Rust toolchain and the tests' reference shells (see
`DEVELOPING.md`).

## From source

luish uses [pixi](https://pixi.sh) to provide the Rust toolchain:

```sh
pixi run release                  # release build: target/release/luish
```

Plain `cargo build --release` also works, with Rust 1.95 or later.

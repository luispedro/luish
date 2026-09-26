# Installation

luish is built from source. It uses [pixi](https://pixi.sh) to provide the Rust toolchain:

```sh
pixi run release                  # release build: target/release/luish
```

Plain `cargo build --release` also works with a recent Rust toolchain (edition 2024).

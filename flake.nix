{
  description = "luish: a POSIX shell written in Rust, with optional plugins and Rhai extensions";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # The dev shell's Rust toolchain, pinned to the version in pixi.toml (clippy's lints change between releases).
    # The package is built with nixpkgs's Rust, so that users get it from the binary cache.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      inherit (nixpkgs) lib;
      # luish is a shell for Linux (as in pixi.toml's platforms).
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems =
        f:
        lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
      # Keep in step with `rust` in pixi.toml.
      rustVersion = "1.98.1";
      toolchain = pkgs: pkgs.rust-bin.stable.${rustVersion}.default;
    in
    {
      packages = forAllSystems (pkgs: {
        luish = pkgs.rustPlatform.buildRustPackage {
          pname = "luish";
          version = (lib.importTOML ./Cargo.toml).package.version;
          # Only what the binary is built from (help compiles in docs/builtins), so that other changes don't
          # trigger a rebuild. There is no .git here, so build.rs reports the revision as `unknown`.
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./build.rs
              ./src
              ./docs/builtins
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          # The tests compare against dash and zsh and drive a pty, which the build sandbox doesn't allow:
          # run them in the dev shell (`cargo test`).
          doCheck = false;
          passthru.shellPath = "/bin/luish";
          meta = {
            description = "A POSIX shell with optional plugins and Rhai extensions";
            homepage = "https://github.com/luispedro/luish";
            license = lib.licenses.mit;
            platforms = lib.platforms.linux;
            mainProgram = "luish";
          };
        };
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.luish;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            ((toolchain pkgs).override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
            })
            # The reference shells of the differential tests. nixpkgs's dash is upstream's, not Debian's (which
            # the tests are written against, see DEVELOPING.md), so a few cases may differ.
            pkgs.dash
            pkgs.zsh
            pkgs.bash
            pkgs.git
          ];
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}

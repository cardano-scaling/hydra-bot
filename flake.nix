{
  description = "A Chocolate Doom based bot client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";

    cardano-node.url = "github:intersectmbo/cardano-node/9.0.0";
    hydra.url = "github:cardano-scaling/hydra/0.17.0";

    gitignore = {
      url = "github:hercules-ci/gitignore.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];

        pkgs = import nixpkgs {
          inherit system overlays;
        };

        inherit (pkgs) makeRustPlatform mkShell rust-bin;
        rust = rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        platform = makeRustPlatform {
          rustc = rust;
          cargo = rust;
        };
      in
      {
        packages.default = platform.buildRustPackage {
          name = "hydra-bot";
          src = ./.;

          buildInputs = with pkgs; [
            pkg-config
            openssl
          ];

          cargoLock.lockFile = ./Cargo.lock;
        };

        devShells.default = mkShell {
          buildInputs = with pkgs; [
            rust

            chocolate-doom
            openssl
            pkg-config
            python312Packages.virtualenvwrapper
          ];

          shellHook =
            let
              lib-path = pkgs.lib.makeLibraryPath [
                pkgs.libffi
                pkgs.openssl
                pkgs.stdenv.cc.cc
              ];
            in
            ''
              # Augment the dynamic linker path
              export "LD_LIBRARY_PATH=$LD_LIBRARY_PATH:${lib-path}"
              SOURCE_DATE_EPOCH=$(date +%s)

              if test ! -d .venv; then
                virtualenv .venv
              fi

              source ./.venv/bin/activate
              export PYTHONPATH=`pwd`/.venv/${pkgs.python312.sitePackages}/:$PYTHONPATH
              [ -e .venv/bin/aider ] || pip install aider
            '';
        };
      }
    );
}

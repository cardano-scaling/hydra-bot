{
  description = "A Chocolate Doom based bot client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
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

        run-server = pkgs.writeShellApplication {
          name = "hydra-bot-run-server";

          runtimeInputs = with pkgs; [
            chocolate-doom
          ];

          text = ''
            exec chocolate-server -privateserver -dedicated -port 2342 -nodes 2
          '';
        };

        run-client = pkgs.writeShellApplication {
          name = "hydra-bot-run-client";

          runtimeInputs = with pkgs; [
            chocolate-doom
          ];

          text = ''
            exec chocolate-doom -window -nosound -iwad DOOM.WAD -connect 127.0.0.1
          '';
        };
      in
      {
        packages.default = platform.buildRustPackage {
          name = "hydra-bot";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };

        devShells.default = mkShell {
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
              [ -e .venv/bin/aider ] || pip install git+https://github.com/paul-gauthier/aider.git
            '';

          buildInputs = with pkgs; [
            run-client
            run-server
            rust

            chocolate-doom
            python312Packages.virtualenvwrapper
            tshark
          ];
        };
      }
    );
}

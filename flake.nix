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
      in
      {
        packages.default = platform.buildRustPackage {
          name = "hydra-bot";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };

        devShells.default = mkShell {
          buildInputs = with pkgs; [
            rust
            chocolate-doom
          ];
        };
      }
    );
}

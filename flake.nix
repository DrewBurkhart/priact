{
  description = "A prioritized actor crate for Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, crane, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      craneLib = crane.mkLib pkgs;

      # Clean the source to only include relevant files for the build.
      src = craneLib.cleanCargoSource self;

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI or locally.
      cargoArtifacts = craneLib.buildDepsOnly {
        inherit src;
        # buildInputs = [ pkgs.someDependency ];
      };

      # Build the actual crate itself, reusing the dependency artifacts from above.
      priact = craneLib.buildPackage {
        inherit src cargoArtifacts;
        # doCheck = true;
      };

    in {
      # This makes `nix build` build the crate
      packages.default = priact;

      # This makes `nix flake check` run tests and other checks
      checks = {
        inherit priact; # Checks if the package builds
        priact-tests = craneLib.cargoTest { inherit src cargoArtifacts; };
        priact-clippy = craneLib.cargoClippy { inherit src cargoArtifacts; };
        priact-fmt = craneLib.cargoFmt { inherit src; };
      };

      # A development shell to work on the project
      devShells.default = pkgs.mkShell {
        inputsFrom = [
          priact
        ];
        packages = with pkgs; [
          rustup        # for rust toolchain
          cargo-watch   # for auto-recompilation
          nixd          # for nix lsp
        ];
        RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
      };
    });
}

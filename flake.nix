{
  description = "Stack allocated structured concurrency";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, naersk, ... }:
    let
      forAllSystems = function:
        nixpkgs.lib.genAttrs [
          "x86_64-linux"
          "aarch64-linux"
          "riscv64-linux"
          "aarch64-darwin"
        ] (system: function nixpkgs.legacyPackages.${system});

      forEachSystem = forAllSystems (pkgs:
        let
          pkgs' = pkgs.extend rust-overlay.overlays.default;

          rust-stable = pkgs'.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "clippy" "rustfmt" ];
          };

          rust-nightly = pkgs'.rust-bin.nightly.latest.default.override {
            extensions = [ "rust-src" "clippy" "rustfmt" "miri" ];
          };

          naersk-stable = pkgs'.callPackage naersk {
            cargo = rust-stable;
            rustc = rust-stable;
          };

          naersk-nightly = pkgs'.callPackage naersk {
            cargo = rust-nightly;
            rustc = rust-nightly;
          };

          buildInputs = [];
          nativeBuildInputs = [];

          # Production package built with stable Rust
          package = naersk-stable.buildPackage {
            src = ./.;
            inherit buildInputs nativeBuildInputs;
          };

          # Development package built with nightly (for Miri compatibility)
          packageNightly = naersk-nightly.buildPackage {
            src = ./.;
            inherit buildInputs nativeBuildInputs;
          };

        in
        {
          packages = {
            default = package;
            stable = package;
            nightly = packageNightly;
          };

          devShells = {
            default = pkgs'.mkShell {
              buildInputs = buildInputs ++ [
                rust-stable
                pkgs'.rust-analyzer
              ];
              nativeBuildInputs = nativeBuildInputs;

              # RUST_SRC_PATH = "${rust-stable}/lib/rustlib/src/rust/library";
            };

            nightly = pkgs'.mkShell {
              buildInputs = buildInputs ++ [
                rust-nightly
                # pkgs'.rust-analyzer
              ];
              nativeBuildInputs = nativeBuildInputs;

              # RUST_SRC_PATH = "${rust-nightly}/lib/rustlib/src/rust/library";
              MIRIFLAGS = "-Zmiri-strict-provenance";

              shellHook = ''
                echo "Nightly Rust environment with Miri"
              '';
            };
          };

          checks = {
            test = pkgs'.runCommand "cargo-test" {
              buildInputs = [ rust-stable ] ++ buildInputs;
              nativeBuildInputs = nativeBuildInputs;
            } ''
              cargo test --release
            '';

            miri = pkgs'.runCommand "cargo-miri-test" {
              buildInputs = [ rust-nightly ] ++ buildInputs;
              nativeBuildInputs = nativeBuildInputs;
            } ''
              cargo miri test --release
            '';

            clippy = pkgs'.runCommand "cargo-clippy" {
              buildInputs = [ rust-stable ] ++ buildInputs;
              nativeBuildInputs = nativeBuildInputs;
            } ''
              cargo clippy --deny
            '';

            fmt = pkgs'.runCommand "cargo-fmt" {
              buildInputs = [ rust-stable ];
            } ''
              cargo fmt --all --check
            '';
          };
        }
      );
    in
    {
      packages = forAllSystems (pkgs: forEachSystem.${pkgs.system}.packages);
      devShells = forAllSystems (pkgs: forEachSystem.${pkgs.system}.devShells);
      checks = forAllSystems (pkgs: forEachSystem.${pkgs.system}.checks);
    };
}

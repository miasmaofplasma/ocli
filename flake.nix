{
  description = "ocli — a terminal CLI for work-ticket notes in an Obsidian vault";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane = { url = "github:ipetkov/crane"; };
    rust-overlay = { url = "github:oxalica/rust-overlay"; inputs.nixpkgs.follows = "nixpkgs"; };
  };

  outputs = { nixpkgs, crane, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
    in {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          # Pinned in rust-toolchain.toml — the single source of truth, so
          # this flake and any devenv reading the same file never drift.
          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
        in {
          default = craneLib.buildPackage {
            # `cleanCargoSource` keeps only `*.rs`/cargo files, but the test
            # fixtures (`tests/fixtures/**`) and `include_str!` targets must
            # be present for `cargo test`. `lib.fileset` is an explicit
            # allowlist, so devenv.nix/.git/target still stay out.
            src = pkgs.lib.fileset.toSource {
              root = ./.;
              fileset = pkgs.lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./src
                ./tests
              ];
            };
            # `git` is test-only (D34 fixtures build repos via the CLI); the
            # binary itself uses `gix`, not the `git` executable.
            nativeBuildInputs = [ pkgs.makeWrapper pkgs.git ];
            # `ocli open` runs `xdg-open` on Linux; on macOS the `open`
            # crate uses the system `open`, so the opener is Linux-only.
            buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.xdg-utils ];
            postInstall = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
              wrapProgram "$out/bin/ocli" --prefix PATH : ${pkgs.xdg-utils}/bin
            '';
          };
        });
    };
}

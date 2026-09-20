{
  description = "ocli — a terminal CLI for work-ticket notes in an Obsidian vault";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane = { url = "github:ipetkov/crane"; inputs.nixpkgs.follows = "nixpkgs"; };
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
            src = craneLib.cleanCargoSource ./.;
            nativeBuildInputs = [ pkgs.makeWrapper ];
            # `ocli open` runs `xdg-open` on Linux; on macOS the `open`
            # crate uses the system `open`, so the opener is Linux-only.
            buildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.xdg-utils ];
            postInstall = pkgs.lib.optionalString pkgs.stdenv.isLinux ''
              wrapProgram "$out/bin/ocli" --prefix PATH : ${pkgs.xdg-utils}/bin
            '';
          };
        });
    };
}

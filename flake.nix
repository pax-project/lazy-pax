{
  description = "A Basic Rust DevShell";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    naersk.url = "github:nix-community/naersk";
  };

  outputs =
    {
      self,
      nixpkgs,
      naersk,
    }:
    let
      pkgs = nixpkgs.legacyPackages."x86_64-linux";
      app_deps = [ pkgs.openssl ];
      naerskLib = pkgs.callPackage naersk { };
    in
    {

      packages."x86_64-linux".default = pkgs.rustPlatform.buildRustPackage {
        pname = "lazy-pax";
        version = "0.2.0";
        src = ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
          # pax-core's "1.1.0" tag has a stray committed `.direnv/` (nix-direnv
          # cache symlinks pointing into /nix/store), fixed upstream after this
          # tag but not in it. That makes the usual outputHashes fetchgit
          # fixed-output derivation illegal, since a FOD's output may not
          # itself reference other store paths:
          #   error: fixed-output derivations must not reference store paths
          # Route the fetch through builtins.fetchGit instead, which isn't
          # subject to that restriction (still pinned to the exact commit).
          allowBuiltinFetchGit = true;
        };

        buildInputs = app_deps;
        nativeBuildInputs = [ pkgs.pkg-config ];
      };

      homeModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          cfg = config.programs.lazy-pax;
        in
        {
          options.programs.lazy-pax = {
            enable = lib.mkEnableOption "lazy-pax, a TUI for pax-core";
            package = lib.mkPackageOption (pkgs // { lazy-pax = self.packages.${pkgs.system}.default; }) "lazy-pax" { };
          };

          config = lib.mkIf cfg.enable {
            home.packages = [ cfg.package ];
          };
        };

      devShells."x86_64-linux".default = pkgs.mkShell {
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs =
          with pkgs;
          [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
          ]
          ++ app_deps;
        env.RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
      };

      env = {
        RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        OPENSSL_DIR = "${pkgs.openssl.dev}";
      };

    };
}

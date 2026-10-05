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
            # Quality-gate tooling (see justfile): `just` unifies the
            # calling interface across local dev and CI.
            just
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

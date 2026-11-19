{
  description = "lazy-pax: a TUI paper manager built on pax-core";

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

      # naersk, as pax-core and pax-ai build: nixpkgs' buildRustPackage sets
      # AWS_LC_SYS_USE_SYSTEM=1, so aws-lc-sys (rustls, via reqwest) looks for
      # a system AWS-LC that isn't there and the build fails. naersk takes the
      # name and version from Cargo.toml.
      packages."x86_64-linux".default =
        (naerskLib.buildPackage {
          src = ./.;
          buildInputs = app_deps;
          nativeBuildInputs = [ pkgs.pkg-config ];
        }).overrideAttrs
          (old: {
            meta = (old.meta or { }) // {
              description = "A TUI paper manager built on pax-core";
              mainProgram = "lazy-pax";
            };
          });

      # For a consumer that prefers `pkgs.lazy-pax` over `inputs.lazy-pax.packages.<system>.default`.
      overlays.default = final: prev: {
        lazy-pax = self.packages.${prev.stdenv.hostPlatform.system}.default;
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
            package = lib.mkPackageOption (pkgs // { lazy-pax = self.packages.${pkgs.stdenv.hostPlatform.system}.default; }) "lazy-pax" { };
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

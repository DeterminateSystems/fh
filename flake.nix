{
  inputs = {
    nixpkgs.url = "https://api.flakehub.com/f/NixOS/nixpkgs/0.1.514192.tar.gz";

    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };

    fenix = {
      url = "https://api.flakehub.com/f/nix-community/fenix/0.1.1565.tar.gz";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, ... }@inputs:

    let

      lastModifiedDate = self.lastModifiedDate or self.lastModified or "19700101";

      version = "${builtins.substring 0 8 lastModifiedDate}-${self.shortRev or "dirty"}";

      forSystems = s: f: inputs.nixpkgs.lib.genAttrs s (system: f rec {
        inherit system;
        pkgs = import inputs.nixpkgs { inherit system; overlays = [ self.overlays.default ]; };
      });

      forAllSystems = forSystems [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      fenixToolchain = system: with inputs.fenix.packages.${system};
        combine ([
          stable.clippy
          stable.rustc
          stable.cargo
          stable.rustfmt
          stable.rust-src
        ] ++ inputs.nixpkgs.lib.optionals (system == "x86_64-linux") [
          targets.x86_64-unknown-linux-musl.stable.rust-std
        ] ++ inputs.nixpkgs.lib.optionals (system == "aarch64-linux") [
          targets.aarch64-unknown-linux-musl.stable.rust-std
        ]);

    in
    {
      overlays.default = final: prev: {
        toolchain = fenixToolchain final.stdenv.hostPlatform.system;
      };

      packages = forAllSystems ({ system, pkgs, ... }:
        let
          naerskLib = pkgs.callPackage inputs.naersk {
            cargo = pkgs.toolchain;
            rustc = pkgs.toolchain;
          };
        in
        rec {
          default = fh;
          fh = naerskLib.buildPackage rec {
            name = "fh-${version}";
            src = self;

            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

            nativeBuildInputs = with pkgs; [ pkg-config clang ];
            buildInputs = with pkgs; [
              libllvm.dev
            ]
            ++ lib.optionals (stdenv.isDarwin) (with darwin.apple_sdk.frameworks; [
              libiconv
              Security
              SystemConfiguration
            ]);
          };
        });

      devShells = forAllSystems ({ system, pkgs, ... }: {
        default = pkgs.mkShell {
          name = "dev";

          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

          nativeBuildInputs = with pkgs; [ pkg-config clang ];
          buildInputs = with pkgs; [
            toolchain
            cargo-watch
            libllvm.dev
          ]
          ++ lib.optionals (pkgs.stdenv.isDarwin) (with pkgs; with darwin.apple_sdk.frameworks; [
            libiconv
            Security
            SystemConfiguration
          ]);
        };
      });

    };
}

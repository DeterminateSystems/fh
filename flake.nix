{
  description = "The official CLI for FlakeHub: search for flakes, and add new inputs to your Nix flake.";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1.514192.tar.gz";

    flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.0.1.tar.gz";

    fenix = {
      url = "https://flakehub.com/f/nix-community/fenix/0.1.1565.tar.gz";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    naersk = {
      url = "https://flakehub.com/f/nix-community/naersk/0.1.332.tar.gz";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, ... }@inputs:
    let
      inherit (inputs.nixpkgs) lib;

      lastModifiedDate = self.lastModifiedDate or self.lastModified or "19700101";

      version = "${builtins.substring 0 8 lastModifiedDate}-${self.shortRev or "dirty"}";

      forSystems = s: f: lib.genAttrs s (system: f rec {
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
      overlays.default = final: prev: rec {
        fh =
          let
            rustToolchain = fenixToolchain final.stdenv.hostPlatform.system;
            naerskLib = final.callPackage inputs.naersk {
              cargo = rustToolchain;
              rustc = rustToolchain;
            };
          in
          naerskLib.buildPackage {
            name = "fh-${version}";
            src = self;

            doCheck = true;
            cargoTestOptions = x: x ++ lib.optionals final.stdenv.isDarwin [
              # These tests rely on localhost networking, but appear to be broken on darwin
              "--"
              "--skip cli::cmd::convert::test::nixpkgs_release_to_flakehub"
              "--skip cli::cmd::convert::test::nixpkgs_to_flakehub"
              "--skip cli::cmd::convert::test::old_flakehub_to_new_flakehub"
              "--skip cli::cmd::convert::test::test_flake1_convert"
              "--skip cli::cmd::convert::test::test_nixpkgs_from_registry"
              "--skip cli::cmd::eject::test::flakehub_nixpkgs_to_github"
              "--skip cli::cmd::eject::test::flakehub_to_github"
              "--skip cli::cmd::eject::test::test_flake8_eject"
              "--skip cli::cmd::eject::test::versioned_flakehub_to_github"
            ];

            LIBCLANG_PATH = "${final.libclang.lib}/lib";
            NIX_CFLAGS_COMPILE = lib.optionalString final.stdenv.isDarwin "-I${final.libcxx.dev}/include/c++/v1";

            nativeBuildInputs = with final; [
              pkg-config
              rustPlatform.bindgenHook
              installShellFiles
            ];

            buildInputs = with final; [
              gcc.cc.lib
            ]
            ++ lib.optionals (stdenv.isDarwin) (with darwin.apple_sdk.frameworks; [
              Security
            ]);

            postInstall = ''
              installShellCompletion --cmd fh \
                --bash <("$out/bin/fh" completion bash) \
                --zsh <("$out/bin/fh" completion zsh) \
                --fish <("$out/bin/fh" completion fish)
            '';
          };
      };

      packages = forAllSystems ({ system, pkgs, ... }: rec {
        inherit (pkgs) fh;
        default = pkgs.fh;
      });

      devShells = forAllSystems ({ system, pkgs, ... }:
        {
          default = pkgs.mkShell {
            name = "dev";

            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            NIX_CFLAGS_COMPILE = lib.optionalString pkgs.stdenv.isDarwin "-I${pkgs.libcxx.dev}/include/c++/v1";

            nativeBuildInputs = with pkgs; [ pkg-config clang ];
            buildInputs = with pkgs; [
              (fenixToolchain stdenv.hostPlatform.system)
              cargo-watch
              rust-analyzer
              nixpkgs-fmt
              gcc.cc.lib
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

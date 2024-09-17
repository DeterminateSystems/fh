{
  description = "The official CLI for FlakeHub: search for flakes, and add new inputs to your Nix flake.";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1.650378.tar.gz";

    fenix = {
      url = "https://flakehub.com/f/nix-community/fenix/0.1.1584.tar.gz";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    naersk = {
      url = "https://flakehub.com/f/nix-community/naersk/0.1.345.tar.gz";
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
              SystemConfiguration
            ]);

            postInstall = ''
              installShellCompletion --cmd fh \
                --bash <("$out/bin/fh" completion bash) \
                --zsh <("$out/bin/fh" completion zsh) \
                --fish <("$out/bin/fh" completion fish)
            '';

            env = {
              SSL_CERT_FILE = "${final.cacert}/etc/ssl/certs/ca-bundle.crt";
              LIBCLANG_PATH = "${final.libclang.lib}/lib";
              NIX_CFLAGS_COMPILE = final.lib.optionalString final.stdenv.isDarwin "-I${final.libcxx.dev}/include/c++/v1";
            };
          };
      };

      packages = forAllSystems ({ system, pkgs }: rec {
        inherit (pkgs) fh;
        default = pkgs.fh;
      });

      devShells = forAllSystems ({ system, pkgs }:
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              (fenixToolchain system)
              bacon
              cargo-watch
              rust-analyzer
              nixpkgs-fmt

              # For the Rust environment
              pkg-config
              clang
              gcc.cc.lib
            ]
            ++ lib.optionals (stdenv.isDarwin) ([ libiconv ] ++ (with darwin.apple_sdk.frameworks; [
              Security
              SystemConfiguration
            ]));

            env = {
              LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
              NIX_CFLAGS_COMPILE = pkgs.lib.optionalString pkgs.stdenv.isDarwin "-I${pkgs.libcxx.dev}/include/c++/v1";
            };
          };
        });
    };
}


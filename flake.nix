{
  description = "The official CLI for FlakeHub: search for flakes, and add new inputs to your Nix flake.";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/DeterminateSystems/secure/0";

    fenix = {
      url = "https://flakehub.com/f/nix-community/fenix/=0.1.2375"; # Stick with v1.89 since v1.90 can't seem to compile nixel
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane.url = "https://flakehub.com/f/ipetkov/crane/0";
  };

  outputs =
    { self, ... }@inputs:
    let
      forSystems =
        s: f:
        inputs.nixpkgs.lib.genAttrs s (
          system:
          f {
            inherit system;
            pkgs = import inputs.nixpkgs {
              inherit system;
              overlays = [ self.overlays.default ];
            };
          }
        );

      forAllSystems = forSystems [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
    in
    {
      overlays.default =
        final: prev:
        let
          system = prev.stdenv.hostPlatform.system;
          staticTarget =
            {
              "aarch64-linux" = "aarch64-unknown-linux-musl";
              "x86_64-linux" = "x86_64-unknown-linux-musl";
            }
            .${system} or null;
        in
        {
          fh =
            let
              craneLib = inputs.crane.mkLib prev;
            in
            craneLib.buildPackage {
              name = "fh";
              src = self;

              doCheck = true;

              nativeBuildInputs = with final; [
                pkg-config
                final.buildPackages.rustPlatform.bindgenHook
                installShellFiles
              ];

              buildInputs = with final; [
                gcc.cc.lib
              ];

              postInstall = final.lib.optionalString (final.stdenv.hostPlatform == final.stdenv.buildPlatform) ''
                installShellCompletion --cmd fh \
                  --bash <("$out/bin/fh" completion bash) \
                  --zsh <("$out/bin/fh" completion zsh) \
                  --fish <("$out/bin/fh" completion fish)
              '';

              LIBCLANG_PATH = "${final.buildPackages.libclang.lib}/lib";

              env = {
                SSL_CERT_FILE = "${final.cacert}/etc/ssl/certs/ca-bundle.crt";
                NIX_CFLAGS_COMPILE = final.lib.optionalString final.stdenv.isDarwin "-I${final.libcxx.dev}/include/c++/v1";
              };
            };

          rustToolchain =
            with inputs.fenix.packages.${system};
            combine (
              with stable;
              [
                clippy
                rustc
                cargo
                rustfmt
                rust-src
                rust-analyzer
              ]
              ++ inputs.nixpkgs.lib.optionals (staticTarget != null) [
                targets.${staticTarget}.stable.rust-std
              ]
            );
        };

      packages = forAllSystems (
        { system, pkgs }:
        rec {
          inherit (pkgs) fh;
          default = pkgs.fh;
        }
      );

      formatter = forAllSystems ({ pkgs, ... }: pkgs.nixfmt-rfc-style);

      devShells = forAllSystems (
        { system, pkgs }:
        {
          default = pkgs.mkShell {
            packages =
              with pkgs;
              [
                rustToolchain

                self.formatter.${system}

                # For the Rust environment
                pkg-config
                clang
                gcc.cc.lib
              ]
              ++ lib.optionals (stdenv.isDarwin) [ libiconv ];

            env = {
              LIBCLANG_PATH = "${pkgs.buildPackages.libclang.lib}/lib";
              NIX_CFLAGS_COMPILE = pkgs.lib.optionalString pkgs.stdenv.isDarwin "-I${pkgs.libcxx.dev}/include/c++/v1";
            };
          };
        }
      );
    };
}

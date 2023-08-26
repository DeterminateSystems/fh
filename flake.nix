{
  description = "fh: the FlakeHub CLI";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz";

    fenix = {
      url = "https://api.flakehub.com/f/nix-community/fenix/0.1.*.tar.gz";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, fenix, naersk }:
    let
      lastModifiedDate = self.lastModifiedDate or self.lastModified or "19700101";
      version = "${builtins.substring 0 8 lastModifiedDate}-${self.shortRev or "dirty"}";
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachSupportedSystem = f: nixpkgs.lib.genAttrs supportedSystems (system: f {
        inherit system;
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ self.overlays.default ];
        };
      });
      fenixToolchain = system: with fenix.packages.${system};
        combine ([
          stable.clippy
          stable.rustc
          stable.cargo
          stable.rustfmt
          stable.rust-src
        ] ++ nixpkgs.lib.optionals (system == "x86_64-linux") [
          targets.x86_64-unknown-linux-musl.stable.rust-std
        ] ++ nixpkgs.lib.optionals (system == "aarch64-linux") [
          targets.aarch64-unknown-linux-musl.stable.rust-std
        ]);
    in
    {
      overlays.default = final: prev:
        let
          toolchain = fenixToolchain final.hostPlatform.system;
          naerskLib = final.callPackage naersk {
            cargo = toolchain;
            rustc = toolchain;
          };
        in
        {
          fh = naerskLib.buildPackage rec {
            name = "fh-${version}";
            src = self;
          };
        };

      devShells = forEachSupportedSystem ({ pkgs, system }:
        let
          toolchain = fenixToolchain system;
        in
        {
          default = pkgs.mkShell {
            name = "fh-dev";
            packages = with pkgs; [
              toolchain
              cargo-edit
              cargo-watch
              nixpkgs-fmt
            ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (
              (with pkgs; [ libiconv ]) ++
                (with pkgs.darwin.apple_sdk.frameworks; [ Security ])
            );
          };
        });

      packages = forEachSupportedSystem ({ pkgs, ... }: {
        default = pkgs.fh;
      });
    };
}

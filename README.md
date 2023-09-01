
# `fh`, the official FlakeHub CLI.

`fh` is a scrappy CLI to search FlakeHub, and add new inputs to your flake.

## Installation

Install `fh` from FlakeHub:

```console
nix profile install "https://flakehub.com/f/DeterminateSystems/fh/*.tar.gz"
```

> **Note:** This will build fh on your computer locally.
> Pre-built binaries aren't available yet.

## Demo

### Add a flake published to FlakeHub to your flake.nix

`fh add` adds the most current release of the specified flake to your `flake.nix`, and update the `outputs` function to accept it.

```console
$ fh add nixos/nixpkgs

$ cat flake.nix
{
  description = "My new flake.";

  inputs.nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.2305.490449.tar.gz";

  outputs = { nixpkgs, ... } @ inputs: {};
}
```

### Searching published flakes
```console
$ fh search rust
+---------------------------------------------------------------------------------+
| Flake                      FlakeHub URL                                         |
+---------------------------------------------------------------------------------+
| astro/deadnix              https://flakehub.com/flake/astro/deadnix             |
| carlthome/ml-runtimes      https://flakehub.com/flake/carlthome/ml-runtimes     |
| ipetkov/crane              https://flakehub.com/flake/ipetkov/crane             |
| kamadorueda/alejandra      https://flakehub.com/flake/kamadorueda/alejandra     |
| nix-community/fenix        https://flakehub.com/flake/nix-community/fenix       |
| nix-community/lanzaboote   https://flakehub.com/flake/nix-community/lanzaboote  |
| nix-community/nix-init     https://flakehub.com/flake/nix-community/nix-init    |
| nix-community/nixpkgs-fmt  https://flakehub.com/flake/nix-community/nixpkgs-fmt |
| nix-community/patsh        https://flakehub.com/flake/nix-community/patsh       |
| ryanccn/nyoom              https://flakehub.com/flake/ryanccn/nyoom             |
+---------------------------------------------------------------------------------+
```

### Listing releases

`fh list releases` provides a list of a project's releases.

```console
$ fh list releases nixos/nixpkgs
+------------------------------------------------------------+
| Version                                                    |
+------------------------------------------------------------+
| 0.1.428801+rev-2788904d26dda6cfa1921c5abb7a2466ffe3cb8c    |
| 0.1.429057+rev-42337aad353c5efff4382d7bf99deda491459845    |
| 0.1.429304+rev-27ccd29078f974ddbdd7edc8e38c8c8ae003c877    |
| 0.1.429553+rev-5dc7114b7b256d217fe7752f1614be2514e61bb8    |
| 0.1.429868+rev-a115bb9bd56831941be3776c8a94005867f316a7    |
...
```

### Listing organizations and flakes

`fh list orgs` and `fh list flakes` enumerates orgs and flakes on FlakeHub:

```console
$ fh list orgs
+-------------------------------------------------------------------------+
| Organization            FlakeHub URL                                    |
+-------------------------------------------------------------------------+
| ajaxbits                https://flakehub.com/org/ajaxbits               |
| astro                   https://flakehub.com/org/astro                  |
...
```

```console
$ fh list flakes
+---------------------------------------------------------------------------------------------------------------+
| Flake                                     FlakeHub URL                                                        |
+---------------------------------------------------------------------------------------------------------------+
| ajaxbits/audiobookshelf                   https://flakehub.com/flake/ajaxbits/audiobookshelf                  |
| ajaxbits/tone                             https://flakehub.com/flake/ajaxbits/tone                            |
| astro/deadnix                             https://flakehub.com/flake/astro/deadnix                            |
...
```

## A note on automation

Piping `fh list` commands to another program will emit a CSV instead of the stylizide table.

## License

[Apache 2.0](https://choosealicense.com/licenses/mit/)


## Support

For support, email support@flakehub.com or [join our Discord](https://discord.gg/invite/a4EcQQ8STr).

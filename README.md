# `fh`, the official FlakeHub CLI

`fh` is a scrappy CLI for searching [FlakeHub] and adding new [inputs] to your [Nix flakes][nix-flakes].

## Usage

Using `fh` from FlakeHub:

```shell
nix shell "https://flakehub.com/f/DeterminateSystems/fh/*.tar.gz"
```

> **Note:** This builds `fh` locally on your computer.
> Pre-built binaries aren't yet available.

## Installation

### NixOS

To make the `fh` CLI readily available on a [NixOS] system:

```nix
{
  description = "My NixOS config.";

  inputs.fh.url = "https://flakehub.com/f/DeterminateSystems/fh/*.tar.gz";
  inputs.nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz";

  outputs = { nixpkgs, fh, ... } @ inputs: {
    nixosConfigurations.nixos = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        {
          environment.systemPackages = [ fh.packages.x86_64-linux.default ];
        }

        # ... the rest of your modules here ...
      ];
    };
  };
}
```

## Demo

### Add a flake published to FlakeHub to your `flake.nix`

`fh add` adds the most current release of the specified flake to your `flake.nix` and updates the `outputs` function to accept it.
This would add the current release of [Nixpkgs] to your flake:

```console
fh add nixos/nixpkgs
cat flake.nix

{
  description = "My new flake.";

  inputs.nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.2305.490449.tar.gz";

  outputs = { nixpkgs, ... } @ inputs: {
    # Fill in your outputs here
  };
}
```

### Searching published flakes

You can search publicly listed flakes using the `fh search` command and passing in a search query.
Here's an example:

```shell
fh search rust
```

```console
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

`fh search` supports arbitrary search strings.
An example:

```shell
fh search "rust nixos"
```

### Listing releases

`fh list releases` provides a list of a flake's [releases][semver].

```shell
fh list releases nixos/nixpkgs
```

```console
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

### Listing flakes, organizations, and versions

[`fh list flakes`](#list-flakes), [`fh list orgs`](#list-flakes), and [`fh list versions`](#list-versions) enumerate [flakes], [organizations][orgs], and [flake versions][semver] on FlakeHub, respectively.

#### List flakes

```shell
fh list flakes
```

```console
+---------------------------------------------------------------------------------------------------------------+
| Flake                                     FlakeHub URL                                                        |
+---------------------------------------------------------------------------------------------------------------+
| ajaxbits/audiobookshelf                   https://flakehub.com/flake/ajaxbits/audiobookshelf                  |
| ajaxbits/tone                             https://flakehub.com/flake/ajaxbits/tone                            |
| astro/deadnix                             https://flakehub.com/flake/astro/deadnix                            |
...
```


#### List orgs

```shell
fh list orgs
```

```console
+-------------------------------------------------------------------------+
| Organization            FlakeHub URL                                    |
+-------------------------------------------------------------------------+
| ajaxbits                https://flakehub.com/org/ajaxbits               |
| astro                   https://flakehub.com/org/astro                  |
...
```

#### List versions

Your can list [versions][semver] of a flake by passing the flake name and a version requirement to `fh list versions`:

```shell
fh list versions <flake> <version_req>
```

Here's an example:

```shell
fh list versions hyprwm/Hyprland "0.1.*"
```

```console
+------------------------------------------------------------------------------------------------------------------------------+
| Simplified version  FlakeHub URL                                        Full version                                         |
+------------------------------------------------------------------------------------------------------------------------------+
| 0.1.546             https://flakehub.com/flake/hyprwm/Hyprland/0.1.546  0.1.546+rev-d8c5e53c0803eb118080657734160bf3ab5127d2 |
+------------------------------------------------------------------------------------------------------------------------------+
```

## A note on automation

Piping `fh list` commands to another program emits [CSV] instead of the stylized table.

You can apply the `--json` flag to each list command to produce JSON output.

## License

[Apache 2.0](https://choosealicense.com/licenses/apache-2.0/)

## Support

For support, email support@flakehub.com or [join our Discord](https://discord.gg/invite/a4EcQQ8STr).

[csv]: https://en.wikipedia.org/wiki/Comma-separated_values
[flakehub]: https://flakehub.com
[flakes]: https://flakehub.com/flakes
[inputs]: https://zero-to-nix.com/concepts/flakes#inputs
[nix-flakes]: https://zero-to-nix.com/concepts/flakes
[nixos]: https://zero-to-nix.com/concepts/nixos
[nixpkgs]: https://zero-to-nix.com/concepts/nixpkgs
[orgs]: https://flakehub.com/orgs
[semver]: https://flakehub.com/docs#semver

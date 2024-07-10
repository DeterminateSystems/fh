# `fh`, the official FlakeHub CLI

[![FlakeHub](https://img.shields.io/endpoint?url=https://flakehub.com/f/DeterminateSystems/fh/badge)](https://flakehub.com/flake/DeterminateSystems/fh)

`fh` is a scrappy CLI for searching [FlakeHub] and adding new [inputs] to your [Nix flakes][nix-flakes].

## Usage

Using `fh` from FlakeHub:

```shell
nix shell "https://flakehub.com/f/DeterminateSystems/fh/*.tar.gz"
```

> [!NOTE]
> This builds `fh` locally on your computer.
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

## Using `fh`

You can use `fh` to:

- [Log into FlakeHub](#log-into-flakehub)
- [Initialize a new `flake.nix`](#initialize-a-new-flakenix-from-scratch)
- [Add flake inputs to your `flake.nix`](#add-a-flake-published-to-flakehub-to-your-flakenix)
- [Resolve flake references to store paths](#resolve-flake-references-to-store-paths)
- [Search FlakeHub flakes](#searching-published-flakes)
- List available [releases](#listing-releases) and [flakes, organizations, and versions](#listing-flakes-organizations-and-versions)
- List flakes by [label](#list-by-label)

### Log into FlakeHub

`fh` is the standard way to set up your local Nix to use [FlakeHub]'s advanced features like [FlakeHub Cache][cache] and private flakes:

```shell
fh login
```

This will prompt you for a FlakeHub token that you can obtain under [**Tokens**][tokens] on your [user settings page][settings].
Click **New** to create a new token, provide your desired configuration, copy the token, paste it into the prompt, and follow the remaining instructions.

### Initialize a new `flake.nix` from scratch

`fh init` generates a new [`flake.nix`][flakes] file for you using a combination of:

1. Your responses to interactive questions
1. The contents of the repository in which you run the command.

To create a `flake.nix`, navigate to the directory where you want to create it and run `fh init` (or specify a different directory using the `--root` option).
Respond to the prompts it provides you and at the end `fh` will write a `flake.nix` to disk.

`fh init` has built-in support for the following languages:

- [Elm]
- [Go]
- [Java]
- [JavaScript]
- [PHP]
- [Python]
- [Ruby]
- [Rust]
- [Zig]

> [!NOTE] > `fh init` operates on a best-guess basis and is opinionated in its suggestions.
> It's intended less as a comprehensive flake creation solution and more as a helpful kickstarter.

### Add a flake published to FlakeHub to your `flake.nix`

`fh add` adds the most current release of the specified flake to your `flake.nix` and updates the `outputs` function to accept it.
This would add the current release of [Nixpkgs] to your flake:

```shell
fh add nixos/nixpkgs
```

The resulting `flake.nix` would look something like this:

```nix
{
  description = "My new flake.";

  inputs.nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.2305.490449.tar.gz";

  outputs = { nixpkgs, ... } @ inputs: {
    # Fill in your outputs here
  };
}
```

### Resolve flake references to store paths

You can resolve flake references on FlakeHub to Nix store paths using the `fh resolve` command:

```shell
fh resolve "omnicorp/devtools/0.1.0#packages.x86_64-linux.cli"
/nix/store/1ab797rfbdcjzissxrsf25rqy0l8mksq-cli-0.1.0
```

You can only use `fh resolve` with flake releases for which [`include-output-paths`][flakehub-push-params] has been set to `true`.
Here's an example [flakehub-push] configuration:

```yaml
- name: Publish to FlakeHub
  uses: determinatesystems/flakehub-push@main
  with:
    visibility: "public" # or "unlisted" or "private"
    include-output-paths: true
```

The `fh resolve` command is most useful when used in conjunction with [FlakeHub Cache][cache].
If the cache is enabled on the flake and the current Nix user is [logged into FlakeHub](#log-into-flakehub), then resolved store paths are also available to Nix.
Under those conditions, you can, for example, apply a NixOS configuration published to FlakeHub:

```shell
# Build the derivation
nix build \
  --max-jobs 0 \
  --profile /nix/var/nix/profiles/system \
  $(fh resolve "my-org/my-nixos-configs#nixosConfigurations.my-dev-workstation")

# Apply the configuration
/nix/var/nix/profiles/system/bin/switch-to-configuration switch
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
| ...                                                        |
+------------------------------------------------------------+
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
| ...                                       ...                                                                 |
+---------------------------------------------------------------------------------------------------------------+
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
| ...                     ...                                             |
+-------------------------------------------------------------------------+
```

#### List versions

Your can list [versions][semver] of a flake by passing the flake name and a version requirement to `fh list versions`:

```shell
fh list versions <flake> <version_req>
```

Here's an example:

```shell
fh list versions DeterminateSystems/flake-checker "0.1.*"
```

```console
+------------------------------------------------------------------------------------------------------+
| Simplified version  FlakeHub URL                                                        Full version |
+------------------------------------------------------------------------------------------------------+
| 0.1.0               https://flakehub.com/flake/DeterminateSystems/flake-checker/0.1.0   0.1.0        |
| 0.1.1               https://flakehub.com/flake/DeterminateSystems/flake-checker/0.1.1   0.1.1        |
| 0.1.2               https://flakehub.com/flake/DeterminateSystems/flake-checker/0.1.2   0.1.2        |
| ...                 ...                                                                 ...          |
+------------------------------------------------------------------------------------------------------+
```

### List by label

You can list flakes by label using the `fh list label` comand:

```shell
fh list label <label>
```

Here's an example:

```shell
fh list label python
```

```console
+-------------------------------------------------------------------------------+
| Flake                     FlakeHub URL                                        |
+-------------------------------------------------------------------------------+
| nix-community/poetry2nix  https://flakehub.com/flake/nix-community/poetry2nix |
+-------------------------------------------------------------------------------+
```

## Shell completion

You can generate shell completion scripts using the `fh completion` command:

```shell
fh completion <shell>
```

Here's an example:

```shell
fh completion bash
```

These shells are supported:

- [Bash]
- [Elvish]
- [Fish]
- [Powershell]
- [zsh]

## A note on automation

Piping `fh list` commands to another program emits [CSV] instead of the stylized table.

You can apply the `--json` flag to each list command to produce JSON output.

## License

[Apache 2.0](https://choosealicense.com/licenses/apache-2.0/)

## Support

For support, email support@flakehub.com or [join our Discord](https://discord.gg/invite/a4EcQQ8STr).

[bash]: https://gnu.org/software/bash
[cache]: https://determinate.systems/posts/flakehub-cache-beta
[csv]: https://en.wikipedia.org/wiki/Comma-separated_values
[elm]: https://elm-lang.org
[elvish]: https://elv.sh
[fish]: https://fishshell.com
[flakehub]: https://flakehub.com
[flakehub-push]: https://github.com/determinateSystems/flakehub-push
[flakehub-push-params]: https://github.com/determinateSystems/flakehub-push?tab=readme-ov-file#available-parameters
[flakes]: https://flakehub.com/flakes
[go]: https://golang.org
[inputs]: https://zero-to-nix.com/concepts/flakes#inputs
[java]: https://java.com
[javascript]: https://javascript.info
[nix-flakes]: https://zero-to-nix.com/concepts/flakes
[nixos]: https://zero-to-nix.com/concepts/nixos
[nixpkgs]: https://zero-to-nix.com/concepts/nixpkgs
[orgs]: https://flakehub.com/orgs
[php]: https://php.net
[powershell]: https://learn.microsoft.com/powershell
[python]: https://python.org
[ruby]: https://ruby-lang.org
[rust]: https://rust-lang.org
[semver]: https://flakehub.com/docs/concepts/semver
[settings]: https://flakehub.com/user/settings
[tokens]: https://flakehub.com/user/settings?editview=tokens
[zig]: https://ziglang.org
[zsh]: https://zsh.org

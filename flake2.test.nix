{
  description = "cole-h's NixOS configuration";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  inputs.agenix-cli = { url = "github:cole-h/agenix-cli"; inputs.nixpkgs.follows = "nixpkgs"; };
  inputs.agenix.url = "github:ryantm/agenix";
  inputs.agenix.inputs.nixpkgs.follows = "nixpkgs";

  outputs = inputs: { };
}

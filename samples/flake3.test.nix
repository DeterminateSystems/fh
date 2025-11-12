{
  inputs.nixos-config.url = "0";
  inputs.nixpkgs1.url = "1";
  inputs.nixpkgs2 = {
    url = "2";
  };
  inputs = {
    nixpkgs3 = {
      url = "3";
    };
  };

  outputs = { self, ... }@inputs: { };
}

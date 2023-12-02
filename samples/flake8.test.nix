{
  inputs = {
    a.url = "https://flakehub.com/f/NixOS/nixpkgs/0.2311.*.tar.gz";
    b.url = "https://flakehub.com/f/DeterminateSystems/fh/*.tar.gz";
    c.url = "https://flakehub.com/f/DeterminateSystems/fh/0.0.0.tar.gz";
    d.url = "https://flakehub.com/f/edolstra/blender-bin/0.0.0.tar.gz";
    e.url = "https://flakehub.com/f/edolstra/blender-bin/*.tar.gz";
    f.url = "https://flakehub.com/f/nix-community/home-manager/0.2311.*.tar.gz";
  };

  outputs = inputs: { };
}

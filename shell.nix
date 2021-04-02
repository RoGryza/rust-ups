{ channel ? "1.50.0"
, nixpkgs ? import <nixpkgs>
}:
(import ./nix/shell-common.nix) {
  inherit channel nixpkgs;
  extra-packages = pkgs: [ pkgs.cargo-edit ];
}

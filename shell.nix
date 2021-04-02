{ channel ? "1.50.0"
, nixpkgs ? import <nixpkgs>
}:
let
  ci-shell = (import ./ci.nix) { inherit channel nixpkgs; };
  pkgs = nixpkgs { };
in ci-shell.overrideAttrs (old: {
  buildInputs = old.buildInputs ++ [ pkgs.cargo-edit ];
})

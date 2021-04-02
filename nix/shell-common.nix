{ channel ? "1.50.0"
, nixpkgs ? import <nixpkgs>
, extra-packages ? _: []
}:
let
  moz_overlay = import (builtins.fetchTarball https://github.com/cpcloud/nixpkgs-mozilla/archive/install-docs-optional.tar.gz);
  pkgs = nixpkgs { overlays = [ moz_overlay ]; };
  rustChannel = pkgs.rustChannelOf { inherit channel; installDoc = false; };
in pkgs.stdenv.mkDerivation {
  name = "rust_ups_shell";
  buildInputs = [
    rustChannel.rust.overrideAttrs({ extensions ? [] }: {
      extensions = extensions ++ [ "clippy" "tarpaulin" ];
    })
    rustChannel.cargo
  ] ++ extra-packages pkgs;
}

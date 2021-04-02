{ channel ? "1.50.0"
, nixpkgs ? import <nixpkgs>
}:
let
  moz_overlay = import (builtins.fetchTarball https://github.com/cpcloud/nixpkgs-mozilla/archive/install-docs-optional.tar.gz);
  pkgs = nixpkgs { overlays = [ moz_overlay ]; };
  rustChannel = pkgs.rustChannelOf { inherit channel; installDoc = false; };
in pkgs.stdenv.mkDerivation {
  name = "rust_ups_shell";
  buildInputs = [
    rustChannel.cargo rustChannel.rust.out
    pkgs.cargo-tarpaulin pkgs.clippy
  ];
}

{ channel ? "1.50.0" }:
let
  moz_overlay = import (builtins.fetchTarball https://github.com/cpcloud/nixpkgs-mozilla/archive/install-docs-optional.tar.gz);
  pkgs = import <nixpkgs> { overlays = [ moz_overlay ]; };
  rustChannel = pkgs.rustChannelOf { inherit channel; installDoc = false; };
in pkgs.stdenv.mkDerivation {
  name = "moz_overlay_shell";
  buildInputs = [
    rustChannel.cargo.out rustChannel.rust.out
    pkgs.cargo-edit pkgs.cargo-tarpaulin pkgs.clippy
  ];
}

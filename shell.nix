let
  moz_overlay = import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz);
  pkgs = import <nixpkgs> { overlays = [ moz_overlay ]; };
  rustChannel = pkgs.rustChannelOf { channel = "1.50.0"; };
in pkgs.stdenv.mkDerivation {
  name = "moz_overlay_shell";
  buildInputs = [
      rustChannel.cargo rustChannel.rust
    ];
  }

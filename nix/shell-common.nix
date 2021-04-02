{ channel ? "1.50.0"
, nixpkgs ? import <nixpkgs>
, extra-packages ? _: []
}:
let
  moz_overlay = import (builtins.fetchTarball https://github.com/cpcloud/nixpkgs-mozilla/archive/install-docs-optional.tar.gz);
  pkgs = nixpkgs { overlays = [ moz_overlay ]; };
  rustChannel = pkgs.rustChannelOf { inherit channel; installDoc = false; };
  rust = rustChannel.rust.overrideAttrs(_: {
      extensions = [ "clippy" "tarpaulin" ];
  });
in pkgs.mkShell {
  name = "rust_ups_shell";
  nativeBuildInputs = [
    rust rustChannel.cargo
  ] ++ (extra-packages pkgs);
}

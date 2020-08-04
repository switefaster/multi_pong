{ mozillaOverlay ? import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz)
, pkgs ? import <nixpkgs> { overlays = [ mozillaOverlay ]; } }:
let
  rust = pkgs.latest.rustChannels.nightly.rust;
  rustPlatform = (pkgs.makeRustPlatform {
    rustc = rust;
    cargo = rust;
  });
in
rustPlatform.buildRustPackage rec {
  pname = "multi_pong";
  version = "0.1.0";
  src = ./.;
  cargoSha256 = "08mm787h9ria9cbxa9yshfna623vqwv6qmvzy8a51bzd3sxidv34";
  nativeBuildInputs = (
    with pkgs; [
      alsaLib
      cmake
      freetype
      expat
      openssl
      pkgconfig
      python3
      (
        vulkan-validation-layers.overrideAttrs (
          old: {
            setupHook = writeText "setup-hook" ''
              addToSearchPath XDG_DATA_DIRS @out@/share
              export XDG_DATA_DIRS
            '';
          }
        )
      )
      xlibs.libX11
    ]
  );
  APPEND_LIBRARY_PATH = (
    with pkgs; stdenv.lib.makeLibraryPath [
      vulkan-loader
      xlibs.libXcursor
      xlibs.libXi
      xlibs.libXrandr
    ]
  );

  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$APPEND_LIBRARY_PATH"
    export RUSTFLAGS="-C target-cpu=native"
  '';
}

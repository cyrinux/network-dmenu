{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, naersk }:
    let systems = [ "x86_64-linux" "aarch64-linux" ];
    in flake-utils.lib.eachSystem systems (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
        libs = with pkgs; [
          pkg-config
          openssl
          perl
          dbus
          glib
          glib.dev
          gobject-introspection
        ];
      in
      {
        defaultPackage = naersk-lib.buildPackage {
          src = ./.;
          meta.mainProgram = "network-dmenu";
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = libs;
          # cargoBuildOptions = attrs: attrs ++ [ "--features" "gtk-ui" ];
        };
        devShell = with pkgs; mkShell {
          buildInputs = [
            # Build tools
            cargo-bloat
            cargo-bump
            cargo-deny
            cargo-feature
            clippy
            gcc
            glib
            glib.dev
            gnumake
            gobject-introspection
            dbus
            # GTK4 development dependencies
            pkg-config
            pre-commit
            rust-analyzer
            rustc
            # Rust development
            rustfmt
            rustPackages.clippy
          ] ++ libs;
          RUST_SRC_PATH = rustPlatform.rustLibSrc;
        };
      }
    );
}

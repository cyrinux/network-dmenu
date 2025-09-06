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
          dbus
          gtk4
          gtk4.dev
          libadwaita
          glib
          glib.dev
          gobject-introspection
          pango
          cairo
          gdk-pixbuf
          graphene
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
            cairo
            cargo
            cargo-bloat
            cargo-bump
            cargo-deny
            cargo-feature
            clippy
            gcc
            gdk-pixbuf
            glib
            glib.dev
            gnumake
            gobject-introspection
            graphene
            dbus
            gtk4
            gtk4.dev
            # GTK4 development dependencies
            libadwaita
            pango
            pkg-config
            pre-commit
            rust-analyzer
            rustc
            # Rust development
            rustfmt
            rustPackages.clippy
            tor
            torsocks
          ] ++ libs;
          RUST_SRC_PATH = rustPlatform.rustLibSrc;

          # Set environment variables for pkg-config to find GTK libraries
          shellHook = ''
            export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath [
              pkgs.gtk4
              pkgs.libadwaita
              pkgs.glib
            ]}:$LD_LIBRARY_PATH

            # For pkg-config to find .pc files
            export PKG_CONFIG_PATH="${pkgs.gtk4.dev}/lib/pkgconfig:${pkgs.libadwaita}/lib/pkgconfig:${pkgs.glib.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"

            echo "GTK4 development environment ready!"
          '';

        };
      }
    );
}

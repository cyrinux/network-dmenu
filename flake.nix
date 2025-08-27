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
          cargoBuildOptions = attrs: attrs ++ [ "--features" "gtk-ui" ];
        };
        devShell = with pkgs; mkShell {
          buildInputs = [ cargo rustc rustfmt pre-commit rustPackages.clippy pkg-config cargo-bump cargo-deny cargo-bloat cargo-feature ] ++ libs;
          RUST_SRC_PATH = rustPlatform.rustLibSrc;

          # Set environment variables for pkg-config to find GTK libraries
          shellHook = ''
            export LD_LIBRARY_PATH=${lib.makeLibraryPath [ gtk4 libadwaita glib ]}:$LD_LIBRARY_PATH
            export PKG_CONFIG_PATH="${gtk4.dev}/lib/pkgconfig:${libadwaita}/lib/pkgconfig:${glib.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
            echo "GTK4 development environment ready!"
          '';
        };
      }
    );
}

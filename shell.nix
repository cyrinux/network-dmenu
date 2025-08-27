{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    # Rust development
    rustc
    cargo
    rustfmt
    rust-analyzer
    clippy

    # GTK4 development dependencies
    pkg-config
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

    # Build tools
    gnumake
    gcc
  ];

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
}

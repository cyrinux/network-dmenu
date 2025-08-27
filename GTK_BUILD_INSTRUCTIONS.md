# Building network-dmenu with GTK4 UI Support

This document provides instructions for building network-dmenu with the optional GTK4-based UI. This provides a native graphical interface alternative to dmenu/fzf.

## Prerequisites

### NixOS

If you're using NixOS, you can use either the provided shell.nix or the flake.nix:

#### Using flake.nix (recommended)

```bash
# Clone the repository
git clone https://github.com/cyrinux/network-dmenu.git
cd network-dmenu

# Enter the development environment
nix develop

# Build with GTK support
cargo build --features gtk-ui
```

#### Using shell.nix

```bash
# Clone the repository
git clone https://github.com/cyrinux/network-dmenu.git
cd network-dmenu

# Enter the development environment
nix-shell

# Build with GTK support
cargo build --features gtk-ui
```

### Other Linux Distributions

For non-NixOS distributions, you'll need to install the required GTK4 development packages:

#### Debian/Ubuntu

```bash
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

#### Fedora

```bash
sudo dnf install gtk4-devel libadwaita-devel
```

#### Arch Linux

```bash
sudo pacman -S gtk4 libadwaita
```

After installing the dependencies, build the project:

```bash
cargo build --features gtk-ui
```

## Installation

To install network-dmenu with GTK4 support:

```bash
cargo install --path . --features gtk-ui
```

Or directly from crates.io:

```bash
cargo install network-dmenu --features gtk-ui
```

## Configuration

You can enable the GTK UI in your config file (~/.config/network-dmenu/config.toml):

```toml
# Use GTK UI instead of dmenu
use_gtk = true
```

Or use the command-line flag:

```bash
network-dmenu --use-gtk
```

## Troubleshooting

### Missing Libraries

If you encounter errors about missing GTK libraries:

1. Make sure you've installed the required development packages
2. Check that pkg-config can find the libraries:

```bash
pkg-config --list-all | grep gtk
```

### Runtime Issues

If you encounter runtime issues with the GTK UI:

1. Try running with debugging enabled:

```bash
RUST_BACKTRACE=1 network-dmenu --use-gtk
```

2. If the GTK UI fails, network-dmenu will attempt to use rofi as a fallback if available. You can install rofi to ensure there's always a GUI option.

## Alternative Build Options

If you're having trouble with the GTK dependencies, you can try these alternatives:

1. Build without GTK support and use the traditional dmenu interface:

```bash
cargo build
```

2. Build with only basic dependencies:

```bash
cargo build --no-default-features
```

## Contributing to GTK UI Development

If you'd like to contribute to the GTK UI:

1. The GTK UI code is located in `src/gtk_ui.rs`
2. The feature is controlled by the `gtk-ui` feature flag
3. Please test changes on multiple desktop environments
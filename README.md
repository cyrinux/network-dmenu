![](https://img.shields.io/github/issues-raw/cyrinux/network-dmenu)
![](https://img.shields.io/github/stars/cyrinux/network-dmenu)
![](https://img.shields.io/crates/d/network-dmenu)
![](https://img.shields.io/crates/v/network-dmenu)

# Network dmenu Selector

![Logo](https://github.com/user-attachments/assets/d07a6fb4-7558-4cc8-b7cd-9bb1321265c7)

A simple dmenu-based selector to manage Tailscale exit nodes, networkmanager, iwd and custom actions. This tool allows you to quickly enable or disable Tailscale, set Tailscale exit nodes including Mullvad VPN, and execute custom actions and more via a dmenu interface.

## Features

- Enable or disable Tailscale
- Set Tailscale exit nodes
- Set mullvad exit nodes
- Customizable actions via a configuration file
- Bluetooth connect and disconnect to known devices
- Connect to wifi devices with bare iwd or network-manager
- Connect to network-manager vpn networks
- Detect if behind a captive portal and open a browser to connect
- Execute custom actions

## Installation

1. Ensure you have Rust installed. If not, you can install it from [rust-lang.org](https://www.rust-lang.org/).
2. Install
   ```sh
   cargo install --locked network-dmenu
   ```

## Requirements

- `fontawesomes` and/or `joypixels` fonts.
- `pinentry-gnome3` for the wifi password prompt.
- `dmenu` or compatible.
- `nmcli` or just `iwd`, optional, for wifi.
- `bluetoothctl`, optional, for bluetooth.

## Configuration

The configuration file is located at `~/.config/network-dmenu/config.toml`. If it doesn't exist, a default configuration will be created automatically.

### Default Configuration

```toml
[[actions]]
display = "😀 Example"
cmd = "notify-send 'hello' 'world'"
```

You can add more actions by editing this file.

## Usage

Run the following command to open the dmenu selector:

```sh
network-dmenu
```

Select an action from the menu. The corresponding command will be executed.

## Dependencies

- [dmenu](https://tools.suckless.org/dmenu/)
- [Tailscale](https://tailscale.com/)
- [Rust](https://www.rust-lang.org/)

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the ISC License. See the [LICENSE](LICENSE.md) file for details.

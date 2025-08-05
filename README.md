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
- **Tailscale Lock management - view lock status and sign new locked nodes**
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

### Tailscale Lock

When Tailscale Lock is enabled on your tailnet, network-dmenu provides additional functionality to manage locked nodes:

- **🔒 Show Tailscale Lock Status**: Displays the current lock status and trusted signing keys
- **📋 List Locked Nodes**: Shows all nodes that are locked out and cannot connect
- **✅ Sign Node**: Sign individual locked nodes to allow them to connect to your tailnet

These actions will only appear in the menu when:
1. Tailscale is installed and running
2. Tailscale Lock is enabled on your tailnet
3. For signing actions: there are locked nodes that need to be signed

When you sign a node, you'll receive a notification confirming success or failure. The signing process uses your local Tailscale Lock key to authorize the node.

## Dependencies

- [dmenu](https://tools.suckless.org/dmenu/)
- [Tailscale](https://tailscale.com/)
- [Rust](https://www.rust-lang.org/)

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the ISC License. See the [LICENSE](LICENSE.md) file for details.

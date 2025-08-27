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
- **Network diagnostics - ping gateway/DNS, traceroute, MTU testing, connectivity checks**
- Customizable actions via a configuration file
- Bluetooth connect and disconnect to known devices
- Connect to wifi devices with bare iwd or network-manager
- Connect to network-manager vpn networks
- Detect if behind a captive portal and open a browser to connect
- Execute custom actions
- Optimized performance with efficient scanning algorithms

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
- `ping`, optional, for connectivity and latency diagnostics.
- `traceroute`, optional, for network path tracing.
- `ip`, optional, for routing table and interface information.
- `ss` or `netstat`, optional, for network connection listings.
- `speedtest-cli` or `speedtest`, optional, for internet speed testing.
- `fast`, optional, for Netflix's Fast.com speed testing.
- [`dns-bench`](https://github.com/qwerty541/dns-bench) optional

## Configuration

The configuration file is located at `~/.config/network-dmenu/config.toml`. If it doesn't exist, a default configuration will be created automatically.

### Default Configuration

```toml
[[actions]]
display = "😀 Example"
cmd = "notify-send 'hello' 'world'"
```

### My personal Configuration

```toml
dmenu_cmd = "dmenu"
dmenu_args = "-f --no-multi -f --bind='alt-a:change-query('action')' -f --bind='alt-r:change-query('system')' -f --bind='alt-t:change-query('tailscale')' -f --bind='alt-w:change-query('wifi')' -f --bind='alt-m:change-query('mullvad')' -f --bind='alt-b:change-query('bluetooth')' -f --bind='alt-s:change-query('sign')' -f --bind='alt-e:change-query('exit-node')' -f --bind='alt-v:change-query('vpn')' -f --bind='alt-d:change-query('diagnostic')' -f --no-multi -f --bind='alt-n:change-query('nextdns')'"

max_nodes_per_city = 1
```

You can add more actions by editing this file.

## Usage

Run the following command to open the dmenu selector:

```sh
network-dmenu
```

You can also disable specific features or filter exit nodes:

```sh
network-dmenu --no-wifi --no-bluetooth --no-diagnostics --max-nodes-per-country 2 --max-nodes-per-city 1 --country USA
```

Select an action from the menu. The corresponding command will be executed.

### Performance Optimizations

network-dmenu has been optimized for performance with the following features:

- Prioritizes faster operations first to reduce perceived wait time
- Collects network information early in the process
- Adds simple items to the menu while scanning networks
- Uses efficient error handling to ensure resilience
- Organizes scanning in logical order based on operation cost
- Optimizes Tailscale Exit Node filtering with single-pass processing (~12% faster)
- Supports filtering exit nodes by priority to show only higher priority nodes

### Exit Node Filtering

#### Limiting Nodes Per Country

You can limit the number of exit nodes shown per country using the `--max-nodes-per-country` parameter:

```sh
network-dmenu --max-nodes-per-country 2
```

This will show only the top 2 highest-priority exit nodes for each country. This is particularly useful for:

- Reducing the number of displayed options when many exit nodes are available
- Ensuring variety by having options from multiple countries
- Getting the best nodes from each country based on your needs

#### Limiting Nodes Per City

You can limit the number of exit nodes shown per city using the `--max-nodes-per-city` parameter:

```sh
network-dmenu --max-nodes-per-city 1
```

This will show only the top highest-priority exit node for each city (e.g., Paris, Marseille, New York). Cities are determined from the location data provided by Tailscale/Mullvad.

This is useful for:

- Getting more granular control over node selection
- Ensuring variety across different cities within the same country
- Selecting specific infrastructure within a geographical area

#### Country Filtering

You can filter Mullvad exit nodes by country using the `--country` parameter:

```sh
network-dmenu --country "Japan"
```

This will show exit nodes located in the specified country. The filter is case-insensitive and will match any country name containing the specified string.

#### Combining Filters

You can combine multiple filters to narrow down your exit node selection:

```sh
network-dmenu --max-nodes-per-country 2 --max-nodes-per-city 1 --country "USA"
```

This will show the top exit node from each city in the USA, with a maximum of 2 nodes total for the country. When both filters are specified, the city filter takes precedence, giving you more granular control.

#### Persistent Filtering in Configuration File

You can set these filters permanently in your configuration file (`~/.config/network-dmenu/config.toml`):

```toml
# Limit the number of exit nodes shown per country (sorted by priority)
max_nodes_per_country = 2

# Limit the number of exit nodes shown per city (sorted by priority)
max_nodes_per_city = 1

# Filter by country name
country_filter = "USA"
```

Command-line arguments will override configuration file settings when both are specified.

#### How Node Selection Works

The node selection process works as follows:

1. First, nodes are filtered by country if specified
2. Then, if `max_nodes_per_city` is set, nodes are grouped by city and the top N highest-priority nodes are selected from each city
3. If `max_nodes_per_country` is set (and city filtering is not used), nodes are grouped by country and the top N highest-priority nodes are selected from each country
4. Nodes are sorted for display with suggested nodes first, then by country, then by city
5. Each node shows both country and city information for easier selection

The node selection process works as follows:

1. First, nodes are filtered by country if specified
2. Then, if `max_nodes_per_city` is set, nodes are grouped by city and the top N highest-priority nodes are selected from each city
3. If `max_nodes_per_country` is set (and city filtering is not used), nodes are grouped by country and the top N highest-priority nodes are selected from each country
4. Nodes are sorted for display with suggested nodes first, then by country, then by city
5. Each node shows both country and city information for easier selection

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

### Network Diagnostics

network-dmenu includes comprehensive network diagnostic tools to help troubleshoot connectivity issues. Each diagnostic tool appears only if the required binary is installed:

**Ping-based diagnostics** (requires `ping`):

- **🚀 Test Connectivity**: Quick check if internet is reachable
- **📶 Ping Gateway**: Test connection to your default gateway
- **📶 Ping DNS Servers**: Test connectivity to common DNS servers (8.8.8.8, 1.1.1.1, 9.9.9.9)
- **📏 Check MTU**: Determine maximum transmission unit size to a target
- **⏱️ Check Latency**: Measure network latency with detailed statistics

**Network tracing** (requires `traceroute`):

- **🗺️ Trace Route**: Show the network path to a destination

**System information** (requires `ip`):

- **🛣️ Show Routing Table**: Display current network routing configuration
- **🔌 Show Network Interfaces**: Display network interface information

**Connection monitoring** (requires `ss` or `netstat`):

- **📊 Show Network Connections**: List active network connections

**Internet speed testing** (requires `speedtest-cli`, `speedtest`, or `fast`):

- **🚀 Speed Test**: Comprehensive internet speed test using speedtest-cli or speedtest
- **⚡ Speed Test (Fast.com)**: Quick speed test using Netflix's fast.com service

All diagnostic results are shown in desktop notifications for easy viewing. These tools are particularly useful for:

- Diagnosing slow network connections
- Troubleshooting VPN or Tailscale connectivity issues
- Identifying network configuration problems
- Monitoring network performance

The diagnostic features can be disabled using the `--no-diagnostics` flag if not needed.

### Installing Optional Speedtest Tools

To enable internet speed testing functionality, install one or more of these tools:

**speedtest-go** (Recommended - provides detailed JSON output with pretty notifications):

```sh
# Using go install
go install github.com/showwin/speedtest-go@latest

# Or download pre-built binaries from:
# https://github.com/showwin/speedtest-go/releases
```

**speedtest-cli** (Python-based alternative):

```sh
# Using pip
pip install speedtest-cli

# Using package managers
sudo apt install speedtest-cli    # Debian/Ubuntu
brew install speedtest-cli        # macOS
```

**speedtest** (Official Ookla tool):

```sh
# Download from https://www.speedtest.net/apps/cli
# Or using package managers
sudo apt install speedtest        # Debian/Ubuntu (newer versions)
brew install speedtest-cli        # macOS
```

**fast** (Netflix's fast.com CLI):

```sh
# Using npm
npm install -g fast-cli

# Using package managers
brew install fast-cli             # macOS
```

## Dependencies

- [dmenu](https://tools.suckless.org/dmenu/)
- [Tailscale](https://tailscale.com/)
- [Rust](https://www.rust-lang.org/)

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the ISC License. See the [LICENSE](LICENSE.md) file for details.

<div align="center">

# 🌐 network-dmenu

[![Crates.io](https://img.shields.io/crates/v/network-dmenu?style=flat-square)](https://crates.io/crates/network-dmenu)
[![Downloads](https://img.shields.io/crates/d/network-dmenu?style=flat-square)](https://crates.io/crates/network-dmenu)
[![License](https://img.shields.io/crates/l/network-dmenu?style=flat-square)](LICENSE.md)
[![Issues](https://img.shields.io/github/issues-raw/cyrinux/network-dmenu?style=flat-square)](https://github.com/cyrinux/network-dmenu/issues)
[![Stars](https://img.shields.io/github/stars/cyrinux/network-dmenu?style=flat-square)](https://github.com/cyrinux/network-dmenu)
[![Build Status](https://img.shields.io/github/actions/workflow/status/cyrinux/network-dmenu/ci.yml?branch=main&style=flat-square)](https://github.com/cyrinux/network-dmenu/actions)

**A blazing-fast, feature-rich dmenu-based network manager for power users**

![network-dmenu](https://github.com/user-attachments/assets/d07a6fb4-7558-4cc8-b7cd-9bb1321265c7)

[Features](#-features) • [Geofencing](#-geofencing--location-based-automation) • [Installation](#-installation) • [Usage](#-usage) • [Configuration](#-configuration) • [Contributing](#-contributing)

</div>

---

## 🎯 Overview

`network-dmenu` is a powerful, dmenu-based network management tool that unifies control over multiple networking subsystems into a single, fast interface. Whether you're managing VPN connections, switching between WiFi networks, controlling Bluetooth devices, running network diagnostics, or setting up automatic location-based network configuration, network-dmenu provides instant access to all these capabilities through a simple menu system.

### Why network-dmenu?

- **🚀 Lightning Fast**: Optimized for performance with intelligent caching and parallel processing
- **🎮 Single Interface**: Control WiFi, VPN, Bluetooth, Tailscale, and more from one menu
- **🔧 Highly Configurable**: Extensive customization options via TOML configuration
- **🛡️ Security Focused**: Supports Tailscale Lock, secure password prompts, and privilege escalation
- **📍 Privacy-First Geofencing**: Automatic location-based network configuration without GPS
- **📊 Comprehensive Diagnostics**: Built-in network troubleshooting tools
- **🎨 Clean UI**: Intuitive menu organization with emoji indicators and smart filtering

## ✨ Features

### 🌍 Network Management

#### **WiFi Control** (NetworkManager & IWD)
- 📶 Scan and connect to WiFi networks
- 🔐 Secure password entry via pinentry
- 📊 Signal strength indicators
- 🔄 Support for both NetworkManager and IWD backends
- 🚪 Captive portal detection and automatic browser launch

#### **VPN Management**
- 🔒 Quick VPN connection/disconnection
- 📋 List and manage NetworkManager VPN profiles
- 🌐 Tailscale integration with advanced features
- 🛡️ Mullvad VPN exit node support

#### **Bluetooth**
- 🎧 Connect/disconnect Bluetooth devices
- 📱 Manage paired devices
- 🔊 Quick toggle for audio devices
- 📋 Show connection status

### 🚀 Tailscale Features

#### **Core Functionality**
- ✅ Enable/disable Tailscale
- 🌐 Exit node management with smart filtering
- 🛡️ Shields up/down control
- 🚦 Accept routes toggle
- 🏠 LAN access control when using exit nodes

#### **Mullvad Integration**
- 🌍 Automatic Mullvad server detection
- 📍 Geographic filtering by country/city
- ⚡ Priority-based node selection
- 🎯 Smart node suggestions

#### **Tailscale Lock** (Advanced Security)
- 🔒 View lock status and signing keys
- 📋 List locked nodes awaiting approval
- ✍️ Sign new nodes to grant access
- 🔑 Node key management

### 🔍 Network Diagnostics

#### **Connectivity Testing**
- 🌐 Internet connectivity check
- 📡 Gateway ping tests
- 🔍 DNS server testing (8.8.8.8, 1.1.1.1, 9.9.9.9)
- 📏 MTU size detection
- ⏱️ Latency measurements with statistics

#### **Advanced Diagnostics**
- 🗺️ Traceroute to any destination
- 📊 Network interface information
- 🛣️ Routing table display
- 🔌 Active connection monitoring
- 🏎️ Speed tests (multiple providers)
- 🎯 DNS benchmark testing

### 📍 Geofencing & Location-Based Automation

#### **Privacy-First Location Detection**
- 📶 WiFi fingerprinting without GPS or location services
- 🔐 SHA-256 hashing of network identifiers for privacy
- 🏠 Automatic "home", "office", "coffee shop" zone detection
- ⚡ Real-time location monitoring with 30-second intervals
- 🎯 Configurable confidence thresholds (0.8 default)

#### **Zone-Based Network Actions**
- 🔄 Automatic WiFi network switching per location
- 🛡️ Location-specific VPN connections
- 🌐 Tailscale exit node switching based on zones
- 🎧 Bluetooth device connection automation
- ⚙️ Custom command execution per zone

#### **Geofencing Daemon**
- 🚀 Background monitoring service with Unix socket IPC
- 📊 Zone change statistics and confidence tracking
- 💾 Persistent zone storage in `~/.local/share/network-dmenu/`
- 🔔 Desktop notifications on zone transitions
- 🎮 Complete CLI management interface

#### **CLI Commands**
```bash
# Start background monitoring
network-dmenu --daemon

# Create zone from current location  
network-dmenu --create-zone "home"

# Show current location fingerprint
network-dmenu --where-am-i

# List all configured zones
network-dmenu --list-zones

# Check daemon status
network-dmenu --daemon-status

# Stop monitoring
network-dmenu --stop-daemon
```

#### **Zone Configuration Examples**

**Basic Home/Work Setup**
```toml
[geofencing]
enabled = true
privacy_mode = "High"           # Only WiFi, hashed identifiers
scan_interval_seconds = 30      # Check location every 30 seconds
confidence_threshold = 0.8      # 80% confidence required
notifications = true            # Desktop notifications on zone changes

# Home zone - automatically connect to home network and devices
[[geofencing.zones]]
name = "Home"
[geofencing.zones.actions]
wifi = "HomeWiFi"
# vpn = null                    # No VPN connection at home
tailscale_exit_node = "none"    # Direct connection at home
bluetooth = ["Sony Headphones", "Logitech Mouse"]
custom_commands = [
    "systemctl --user start syncthing",
    "notify-send 'Welcome Home' 'Network configured for home'"
]

# Work zone - secure corporate setup
[[geofencing.zones]]
name = "Office"
[geofencing.zones.actions]
wifi = "CorpWiFi"
vpn = "WorkVPN"                 # Auto-connect to company VPN
tailscale_exit_node = "office-gateway"
bluetooth = ["Work Headset"]
custom_commands = [
    "systemctl --user stop syncthing",
    "notify-send 'Work Mode' 'Secure network profile activated'"
]
```

**Advanced Multi-Location Setup**
```toml
[geofencing]
enabled = true
privacy_mode = "Medium"         # WiFi + Bluetooth for better accuracy
scan_interval_seconds = 15      # More frequent checks
confidence_threshold = 0.75     # Slightly more sensitive
notifications = true
zone_history_size = 50         # Remember more location data

# Coffee shop zone - privacy focused
[[geofencing.zones]]
name = "CoffeeShop"
[geofencing.zones.actions]
wifi = "CafeWiFi"
vpn = "PrivateVPN"             # Always use VPN on public WiFi
tailscale_exit_node = "home-server"  # Route through home
bluetooth = []                  # Disable Bluetooth for privacy
custom_commands = [
    "notify-send 'Public Network' 'VPN activated for security'",
    "firefox --private-window"
]

# Mobile/traveling zone - conserve data
[[geofencing.zones]]
name = "Mobile"
[geofencing.zones.actions]
# wifi = null                   # Use mobile data instead
# vpn = null                    # No VPN to save mobile data
tailscale_exit_node = "nearest" # Use nearest exit node
bluetooth = ["Phone Headphones"]
custom_commands = [
    "notify-send 'Mobile Mode' 'Data conservation enabled'"
]

# Hotel/temporary zone - secure but flexible
[[geofencing.zones]]
name = "Hotel"
[geofencing.zones.actions]
wifi = "auto"                   # Connect to strongest signal
vpn = "TravelVPN"              # Secure connection
tailscale_exit_node = "home-server"
bluetooth = ["Travel Headphones"]
custom_commands = [
    "notify-send 'Travel Mode' 'Secure hotel network setup'"
]
```

**Privacy Mode Options**
```toml
[geofencing]
# High privacy: WiFi networks only, all identifiers hashed
privacy_mode = "High"

# Medium privacy: WiFi + Bluetooth, hashed identifiers  
privacy_mode = "Medium"

# Low privacy: All available signals, better accuracy
privacy_mode = "Low"

# Custom privacy settings
privacy_mode = "Custom"
[geofencing.privacy]
use_wifi = true
use_bluetooth = false
use_cellular_towers = false    # Requires special permissions
hash_identifiers = true        # SHA-256 hash all network IDs
hash_salt = "your-unique-salt" # Optional custom salt
```

**Important Notes**
- **VPN actions** must specify actual NetworkManager VPN profile names (e.g., `vpn = "WorkVPN"`), not keywords like "disconnect"
- **WiFi actions** use SSID names (e.g., `wifi = "MyNetwork"`)  
- **Tailscale exit nodes** support special values: `"none"` (direct), `"auto"` (automatic), or specific hostnames
- To disable an action, omit the field or comment it out - don't set it to "disconnect" or "off"

### 🎛️ System Controls

#### **Radio Management**
- ✈️ Airplane mode toggle
- 📡 WiFi radio control
- 🎧 Bluetooth radio control
- 📻 RFKill device management

#### **Custom Actions**
- 🎨 Define your own menu entries
- ⚡ Execute custom scripts
- 🔧 Integration with system tools
- 📝 Configurable display names and icons

### 🚦 NextDNS Integration
- 🔒 Profile switching via HTTP API
- ⚡ Quick enable/disable
- 📊 Status monitoring
- 🎯 Per-profile configuration
- 🌐 No CLI dependency required

### 🔌 SSH SOCKS Proxy Management
- **Start/Stop SSH SOCKS proxies** from dmenu interface
- **Toggle functionality** - shows "Start" when stopped, "Stop" when running
- **Multiple proxy configurations** support
- **Automatic status detection** using socket files and port checking
- **Customizable SSH options** per proxy
- **Desktop notifications** for status changes

### 🧅 Tor Proxy Management
- **Start/Stop/Restart Tor daemon** from dmenu interface (requires `tor` command)
- **Launch applications via torsocks** for Tor routing (requires `torsocks` command)
- **Automatic Tor status detection** using port monitoring
- **Multiple torsocks configurations** for different applications
- **Smart menu ordering** - Tor daemon management appears first, apps when Tor is running
- **Desktop notifications** for all operations
- **Secure defaults** with proper data directory isolation
- **Command availability checking** - only shows relevant actions when commands are installed

## 📦 Installation

### Prerequisites

Required:
- `dmenu` or compatible menu program (rofi, wofi, etc.)
- `fontawesome` and/or `joypixels` fonts for icons
- Rust toolchain (for building from source)

Optional dependencies based on features you want:
- `nmcli` - NetworkManager WiFi/VPN support
- `iwd` - IWD WiFi support
- `bluetoothctl` - Bluetooth support
- `tailscale` - Tailscale VPN support
- `pinentry-gnome3` - Secure password prompts
- `ping` - Connectivity diagnostics
- `traceroute` - Network path tracing
- `ip` - Network interface information
- `ss` or `netstat` - Connection monitoring
- `speedtest-go`, `speedtest-cli`, or `fast` - Speed testing
- [`dns-bench`](https://github.com/qwerty541/dns-bench) - DNS benchmark testing (optional)
- `ssh` - SSH SOCKS proxy support
- `tor` - Tor daemon support (optional)
- `torsocks` - Tor application routing (optional)

### From Crates.io (Recommended)

```bash
cargo install --locked network-dmenu
```

### From Source

```bash
git clone https://github.com/cyrinux/network-dmenu.git
cd network-dmenu
cargo build --release
sudo cp target/release/network-dmenu /usr/local/bin/
```

### Arch Linux (AUR)

```bash
yay -S network-dmenu
# or
paru -S network-dmenu
```

## 🚀 Usage

### Basic Usage

Simply run:
```bash
network-dmenu
```

### Command-Line Options

```bash
network-dmenu [OPTIONS]

OPTIONS:
    --no-wifi              Disable WiFi network scanning
    --no-bluetooth         Disable Bluetooth device scanning
    --no-diagnostics       Disable diagnostic tools
    --no-tailscale         Disable Tailscale features
    --no-custom            Disable custom actions
    --no-system            Disable system controls
    --no-nextdns           Disable NextDNS integration
    
    # Exit node filtering
    --max-nodes-per-country <N>   Limit exit nodes per country
    --max-nodes-per-city <N>      Limit exit nodes per city
    --country <NAME>              Filter by country name
    --exclude-exit-nodes <NODES>  Comma-separated list of nodes to exclude
    
    # Other options
    --config <PATH>        Use custom config file
    --dmenu-cmd <CMD>      Override dmenu command
    --dmenu-args <ARGS>    Override dmenu arguments
```

### Examples

Show only essential features:
```bash
network-dmenu --no-diagnostics --no-custom
```

Filter Mullvad exit nodes to USA with max 2 per city:
```bash
network-dmenu --country USA --max-nodes-per-city 2
```

Use rofi instead of dmenu:
```bash
network-dmenu --dmenu-cmd rofi --dmenu-args "-dmenu -i"
```

## ⚙️ Configuration

Configuration file location: `~/.config/network-dmenu/config.toml`

### Example Configuration

```toml
# Menu program settings
dmenu_cmd = "dmenu"
dmenu_args = "-i -l 20 -fn 'monospace:size=10'"

# Alternative: Use rofi
# dmenu_cmd = "rofi"
# dmenu_args = "-dmenu -i -matching fuzzy"

# Exit node filtering
max_nodes_per_country = 3
max_nodes_per_city = 1
country_filter = "USA"
exclude_exit_nodes = ["slow-node-1", "slow-node-2"]

# Feature toggles
enable_wifi = true
enable_bluetooth = true
enable_diagnostics = true
enable_tailscale = true
enable_custom_actions = true
enable_system_controls = true
enable_nextdns = true

# Privilege escalation
privilege_method = "sudo"  # or "pkexec", "doas"

# Custom actions
[[actions]]
display = "🔒 Lock Screen"
cmd = "loginctl lock-session"

[[actions]]
display = "☕ Coffee Break"
cmd = "systemctl suspend"

[[actions]]
display = "📊 System Monitor"
cmd = "alacritty -e htop"

[[actions]]
display = "🌐 Network Monitor"
cmd = "alacritty -e nethogs"

# Advanced dmenu with keybindings (example for rofi)
# dmenu_args = "-dmenu -i -matching fuzzy -kb-custom-1 'Alt+w' -kb-custom-2 'Alt+b' -kb-custom-3 'Alt+t'"

# SSH SOCKS Proxy configurations
[ssh_proxies]

[ssh_proxies.server1]
name = "server1"
server = "example.com"
port = 1081
socket_path = "/tmp/server1.sock"
ssh_options = ["-f", "-q", "-N"]

[ssh_proxies.work-vpn]
name = "work-vpn"
server = "vpn.company.com"
port = 1082
socket_path = "/tmp/work-vpn.sock"
ssh_options = ["-f", "-q", "-N", "-C"]

# Tor proxy configurations
# Disable Tor integration entirely (optional)
# no_tor = true

# Torsocks application configurations
[torsocks_apps]

[torsocks_apps.firefox]
name = "firefox"
command = "firefox"
args = ["--private-window", "--new-instance", "-P", "tor"]
description = "Firefox via Tor"

[torsocks_apps.curl-test]
name = "curl-test"
command = "curl"
args = ["-s", "https://httpbin.org/ip"]
description = "Test Tor Connection"

[torsocks_apps.telegram]
name = "telegram"
command = "telegram-desktop"
args = []
description = "Telegram Desktop"
```

### Advanced Keybinding Configuration

For power users using rofi or dmenu with patches:

```toml
dmenu_cmd = "rofi"
dmenu_args = """
-dmenu -i -matching fuzzy \
-kb-accept-entry 'Return' \
-kb-accept-custom 'Control+Return' \
-kb-custom-1 'Alt+w' \
-kb-custom-2 'Alt+b' \
-kb-custom-3 'Alt+t' \
-kb-custom-4 'Alt+v' \
-kb-custom-5 'Alt+d' \
-kb-custom-6 'Alt+s' \
-kb-custom-7 'Alt+e' \
-kb-custom-8 'Alt+m' \
-kb-custom-9 'Alt+n' \
-kb-custom-10 'Alt+r' \
-mesg 'Alt+w: WiFi | Alt+b: Bluetooth | Alt+t: Tailscale | Alt+v: VPN | Alt+d: Diagnostics'
"""
```

## 🎯 Performance Optimizations

network-dmenu is designed for speed:

- **Parallel Processing**: Network scans run concurrently
- **Smart Caching**: DNS resolution and network state cached
- **Progressive Loading**: Menu items appear as they're ready
- **Efficient Filtering**: Single-pass algorithms for node selection
- **Lazy Evaluation**: Only fetch data when needed
- **Memory Efficient**: Functional programming patterns minimize allocations

### Benchmarks

On a typical system with 50+ network interfaces and 100+ Tailscale nodes:
- Initial menu display: < 100ms
- Full network scan: < 500ms
- Exit node filtering: < 10ms
- Action execution: < 50ms

## 🔒 Security Features

- **Secure Password Entry**: Uses pinentry for WiFi passwords
- **Privilege Escalation**: Supports sudo, pkexec, and doas
- **Tailscale Lock**: Advanced node authorization
- **No Password Storage**: Passwords are never saved to disk
- **Audit Logging**: All privileged operations logged
- **Input Validation**: All user input sanitized

## 🔧 Running as Systemd Service

For automatic geofencing daemon startup, network-dmenu can be run as a systemd user service.

### Quick Install

```bash
# Install systemd service files
./init/install-systemd-service.sh

# Manual management
systemctl --user status network-dmenu.service
journalctl --user -u network-dmenu.service -f
```

### Service Files Included

- **`init/systemd/network-dmenu.service`** - Standard version (recommended)
- **`init/systemd/network-dmenu-privileged.service`** - Enhanced permissions version
- **`init/install-systemd-service.sh`** - Automated installation script

The privileged version grants additional system capabilities and should only be used if the standard version fails with permission errors.

### Service Management

```bash
# Start/stop service
systemctl --user start network-dmenu.service
systemctl --user stop network-dmenu.service

# Enable/disable autostart
systemctl --user enable network-dmenu.service  
systemctl --user disable network-dmenu.service

# View logs
journalctl --user -u network-dmenu.service -f
```

See [init/README.md](init/README.md) for detailed systemd configuration and troubleshooting.

## 🛠️ Troubleshooting

### Common Issues

**Menu doesn't appear:**
- Check that dmenu is installed: `which dmenu`
- Verify DISPLAY variable is set: `echo $DISPLAY`
- Try with basic args: `network-dmenu --dmenu-args ""`

**Icons not showing:**
- Install fontawesome: `sudo pacman -S ttf-font-awesome`
- Or joypixels: `sudo pacman -S ttf-joypixels`

**WiFi networks not showing:**
- Check NetworkManager: `systemctl status NetworkManager`
- Or IWD: `systemctl status iwd`
- Verify permissions: `groups | grep -E '(wheel|sudo|network)'`

**Tailscale features missing:**
- Ensure Tailscale is running: `tailscale status`
- Check authentication: `tailscale login`

### Debug Mode

Run with debug output:
```bash
RUST_LOG=debug network-dmenu
```

## 🤝 Contributing

Contributions are welcome! Please check our [Contributing Guidelines](CONTRIBUTING.md).

### Development Setup

```bash
git clone https://github.com/cyrinux/network-dmenu.git
cd network-dmenu
cargo build
cargo test
cargo clippy
```

### Areas for Contribution

- 🌍 Translations
- 🎨 UI/UX improvements
- 🚀 Performance optimizations
- 📦 Package maintainers
- 📝 Documentation
- 🐛 Bug reports and fixes

## 📄 License

This project is licensed under the ISC License - see the [LICENSE](LICENSE.md) file for details.

## 🙏 Acknowledgments

- The Rust community for excellent libraries
- Tailscale team for their amazing VPN solution
- dmenu/rofi developers for menu systems
- All contributors and users

## 📊 Statistics

- **Language**: Rust 🦀
- **Lines of Code**: ~5000
- **Dependencies**: Minimal, security-audited
- **Test Coverage**: > 90%
- **Platform Support**: Linux (primary), BSD (experimental)

---

<div align="center">

Made with ❤️ by [cyrinux](https://github.com/cyrinux) and [contributors](https://github.com/cyrinux/network-dmenu/graphs/contributors)

[Report Bug](https://github.com/cyrinux/network-dmenu/issues) • [Request Feature](https://github.com/cyrinux/network-dmenu/issues) • [Documentation](https://docs.rs/network-dmenu)

</div>
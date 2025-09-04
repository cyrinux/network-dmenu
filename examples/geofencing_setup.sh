#!/bin/bash
# 
# Geofencing Setup Example
# 
# This script demonstrates how to set up automatic location-based
# network configuration using network-dmenu's geofencing features.
#

set -e

echo "🌐 Network-dmenu Geofencing Setup Example"
echo "==========================================="
echo

# Check if network-dmenu is installed
if ! command -v network-dmenu &> /dev/null; then
    echo "❌ network-dmenu not found. Please install it first:"
    echo "   cargo install network-dmenu"
    exit 1
fi

echo "📍 Step 1: Show current location fingerprint"
echo "This displays the WiFi networks visible from your current location:"
echo
network-dmenu --where-am-i
echo

read -p "🏠 Step 2: Create a 'home' zone from current location? [y/N]: " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "Creating home zone..."
    network-dmenu --create-zone "home"
    echo "✅ Home zone created!"
    echo
fi

read -p "🏢 Move to a different location (office/cafe) and create another zone? [y/N]: " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "📍 After you move to a different location, run:"
    echo "   network-dmenu --create-zone \"office\""
    echo "   # or"
    echo "   network-dmenu --create-zone \"cafe\""
    echo
fi

echo "🚀 Step 3: Start the geofencing daemon"
echo "This will monitor your location and automatically switch network settings:"
echo
echo "To start in foreground (for testing):"
echo "   network-dmenu --daemon"
echo
echo "To start in background:"
echo "   network-dmenu --daemon &"
echo
echo "To check daemon status:"
echo "   network-dmenu --daemon-status"
echo
echo "To list all zones:"
echo "   network-dmenu --list-zones"
echo
echo "To stop daemon:"
echo "   network-dmenu --stop-daemon"
echo

echo "⚙️ Step 4: Configure zone actions (optional)"
echo "Edit your config file to specify what should happen in each zone:"
echo "   ~/.config/network-dmenu/config.toml"
echo
echo "Example zone configuration:"
cat << 'EOF'
[[geofencing.zones]]
name = "Home"
[geofencing.zones.actions]
wifi = "HomeWiFi"                    # Auto-connect to home WiFi
vpn = "HomeVPN"                      # Connect to home VPN
tailscale_exit_node = "home-server"  # Use home Tailscale exit
bluetooth = ["Headphones", "Mouse"]  # Connect home Bluetooth devices
custom_commands = [
    "systemctl --user start syncthing",
    "notify-send 'Welcome Home' 'Network configured'"
]

[[geofencing.zones]]
name = "Office"
[geofencing.zones.actions]
wifi = "OfficeWiFi"
vpn = "CorporateVPN"
tailscale_shields = true  # Enable shields at office
custom_commands = [
    "systemctl --user stop personal-services",
    "notify-send 'At Office' 'Work mode activated'"
]
EOF
echo

echo "🔒 Privacy Notice:"
echo "• Only WiFi networks are used for location detection (no GPS)"
echo "• Network names are hashed with SHA-256 for privacy"
echo "• All processing happens locally on your device"
echo "• No data is sent to external servers"
echo

echo "🎉 Setup complete! Your network will now automatically configure"
echo "   itself based on your location. Enjoy seamless connectivity!"
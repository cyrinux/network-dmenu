#!/bin/bash

echo "=== Testing NextDNS Actions ==="
echo

echo "Building the project..."
cargo build 2>/dev/null || { echo "Build failed"; exit 1; }

echo
echo "Getting list of NextDNS actions (simulating dmenu output)..."
echo

# Create a temporary script that acts like dmenu but just prints all input
cat > /tmp/fake_dmenu.sh << 'EOF'
#!/bin/bash
# Fake dmenu that prints all input lines
while IFS= read -r line; do
    echo "$line"
done
EOF
chmod +x /tmp/fake_dmenu.sh

# Create a temporary config file with fake dmenu
cat > /tmp/test_config.toml << EOF
dmenu_cmd = "/tmp/fake_dmenu.sh"
dmenu_args = ""
use_dns_cache = false
EOF

# Run the program with the fake dmenu to see all actions
echo "Available NextDNS actions:"
echo "------------------------"
./target/debug/network-dmenu \
    --no-wifi \
    --no-vpn \
    --no-bluetooth \
    --no-tailscale \
    --no-diagnostics 2>/dev/null || true

echo
echo "Now testing with empty input (should exit silently):"
echo "" | ./target/debug/network-dmenu --stdin \
    --no-wifi \
    --no-vpn \
    --no-bluetooth \
    --no-tailscale \
    --no-diagnostics 2>&1

echo
echo "Now testing with ShowCurrent action selected:"
# First get the exact string for the ShowCurrent action
ACTION=$(./target/debug/network-dmenu \
    --no-wifi \
    --no-vpn \
    --no-bluetooth \
    --no-tailscale \
    --no-diagnostics 2>/dev/null | grep "Show Current Profile" || echo "")

if [ -n "$ACTION" ]; then
    echo "Sending action: '$ACTION'"
    echo "$ACTION" | ./target/debug/network-dmenu --stdin \
        --no-wifi \
        --no-vpn \
        --no-bluetooth \
        --no-tailscale \
        --no-diagnostics 2>&1
else
    echo "Could not find Show Current Profile action"
fi

# Cleanup
rm -f /tmp/fake_dmenu.sh /tmp/test_config.toml

echo
echo "=== Test Complete ==="

#!/bin/bash

# Test script to verify DNS commands work correctly
# This tests the actual command structure that network-dmenu generates

echo "Testing DNS Command Structure"
echo "=============================="
echo

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test function
test_command() {
    local description="$1"
    local command="$2"

    echo -e "${YELLOW}Test:${NC} $description"
    echo "Command: $command"

    # Dry run - just check syntax
    if bash -n -c "$command" 2>/dev/null; then
        echo -e "${GREEN}✓${NC} Syntax is valid"
    else
        echo -e "${RED}✗${NC} Syntax error!"
        return 1
    fi

    # Check if command would require privileges
    if [[ "$command" == *"sudo"* ]] || [[ "$command" == *"pkexec"* ]]; then
        echo "  Note: Command requires privileges (contains sudo/pkexec)"
    fi

    echo
}

# Test 1: Basic DNS command with interface detection
echo "1. Basic DNS Configuration"
echo "--------------------------"
test_command "DNS with interface detection" \
    "sudo sh -c 'iface=\$(ip route show default | grep -oP '\''dev \\K\\S+'\'' | head -1); iface=\${iface:-wlan0}; resolvectl dns \"\${iface}\" '\''8.8.8.8'\'' && resolvectl dnsovertls \"\${iface}\" no'"

# Test 2: DNS over TLS command
echo "2. DNS over TLS Configuration"
echo "-----------------------------"
test_command "DNS over TLS with Cloudflare" \
    "sudo sh -c 'iface=\$(ip route show default | grep -oP '\''dev \\K\\S+'\'' | head -1); iface=\${iface:-wlan0}; resolvectl dns \"\${iface}\" '\''1.1.1.1#cloudflare-dns.com'\'' && resolvectl dnsovertls \"\${iface}\" yes'"

# Test 3: IPv6 DNS command
echo "3. IPv6 DNS Configuration"
echo "-------------------------"
test_command "IPv6 DNS with interface detection" \
    "sudo sh -c 'iface=\$(ip -6 route show default | grep -oP '\''dev \\K\\S+'\'' | head -1); iface=\${iface:-wlan0}; resolvectl dns \"\${iface}\" '\''2001:4860:4860::8888'\'' && resolvectl dnsovertls \"\${iface}\" no'"

# Test 4: DHCP revert command
echo "4. DHCP Revert Command"
echo "----------------------"
test_command "Revert to DHCP" \
    "sudo sh -c 'iface=\$(ip route show default | grep -oP '\''dev \\K\\S+'\'' | head -1); iface=\${iface:-wlan0}; resolvectl revert \"\${iface}\"'"

# Test 5: Complex DNS with special characters
echo "5. Complex DNS Configuration"
echo "----------------------------"
test_command "NextDNS with custom ID" \
    "sudo sh -c 'iface=\$(ip route show default | grep -oP '\''dev \\K\\S+'\'' | head -1); iface=\${iface:-wlan0}; resolvectl dns \"\${iface}\" '\''45.90.28.100#dns.nextdns.io'\'' && resolvectl dnsovertls \"\${iface}\" yes'"

# Test actual interface detection (non-destructive)
echo "6. Interface Detection Test"
echo "---------------------------"
echo "Testing interface detection command..."
DETECTED_IFACE=$(ip route show default | grep -oP 'dev \K\S+' | head -1)
if [ -n "$DETECTED_IFACE" ]; then
    echo -e "${GREEN}✓${NC} Detected interface: $DETECTED_IFACE"
else
    echo -e "${YELLOW}!${NC} No default route found, would use fallback: wlan0"
fi
echo

# Summary
echo "=============================="
echo "Test Summary"
echo "=============================="
echo "All syntax tests completed."
echo "These commands would modify DNS settings if executed with privileges."
echo "To actually test DNS changes, run individual commands with sudo."
echo
echo "Example to test (dry-run with echo):"
echo "  sudo sh -c 'iface=\$(ip route show default | grep -oP '\''dev \\K\\S+'\'' | head -1); iface=\${iface:-wlan0}; echo \"Would set DNS on \${iface} to 8.8.8.8\"'"

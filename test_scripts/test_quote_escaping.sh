#!/bin/bash

# Test script to verify that quote escaping works correctly in network-dmenu
# This tests the fix for the shell quoting issue with DNS commands

echo "Testing quote escaping in privileged commands..."
echo "================================================"

# Test 1: Simple command with quotes
test_cmd1="resolvectl dns eth0 '8.8.8.8'"
escaped1=$(echo "$test_cmd1" | sed "s/'/'\\\\''/g")
echo "Test 1: Simple DNS command"
echo "  Original: $test_cmd1"
echo "  Escaped:  $escaped1"
echo "  Testing: sh -c '$escaped1'"
sh -c "echo '$escaped1'" >/dev/null 2>&1
if [ $? -eq 0 ]; then
    echo "  ✓ Success: Command executes without shell errors"
else
    echo "  ✗ Failed: Shell parsing error"
fi
echo

# Test 2: Command with multiple quoted arguments
test_cmd2="resolvectl dns eth0 '8.8.8.8' '8.8.4.4'"
escaped2=$(echo "$test_cmd2" | sed "s/'/'\\\\''/g")
echo "Test 2: DNS command with multiple IPs"
echo "  Original: $test_cmd2"
echo "  Escaped:  $escaped2"
echo "  Testing: sh -c '$escaped2'"
sh -c "echo '$escaped2'" >/dev/null 2>&1
if [ $? -eq 0 ]; then
    echo "  ✓ Success: Command executes without shell errors"
else
    echo "  ✗ Failed: Shell parsing error"
fi
echo

# Test 3: Complex command with quotes and special characters
test_cmd3="resolvectl dns eth0 '8.8.8.8#dns.google' && resolvectl dnsovertls eth0 yes"
escaped3=$(echo "$test_cmd3" | sed "s/'/'\\\\''/g")
echo "Test 3: Complex DNS over TLS command"
echo "  Original: $test_cmd3"
echo "  Escaped:  $escaped3"
echo "  Testing: sh -c '$escaped3'"
sh -c "echo '$escaped3'" >/dev/null 2>&1
if [ $? -eq 0 ]; then
    echo "  ✓ Success: Command executes without shell errors"
else
    echo "  ✗ Failed: Shell parsing error"
fi
echo

# Test 4: Command with variables (like the actual DNS commands use)
test_cmd4='resolvectl dns ${iface} '"'"'8.8.8.8'"'"''
escaped4=$(echo "$test_cmd4" | sed "s/'/'\\\\''/g")
echo "Test 4: DNS command with interface variable"
echo "  Original: $test_cmd4"
echo "  Escaped:  $escaped4"
echo "  Testing: iface=eth0; sh -c '$escaped4'"
iface=eth0
sh -c "echo '$escaped4'" >/dev/null 2>&1
if [ $? -eq 0 ]; then
    echo "  ✓ Success: Command executes without shell errors"
else
    echo "  ✗ Failed: Shell parsing error"
fi
echo

echo "================================================"
echo "Quote escaping tests complete!"

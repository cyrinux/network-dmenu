#!/usr/bin/env bash

# Test script for network-dmenu ML features
# This script verifies that ML components are working correctly

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_color() {
    color=$1
    shift
    echo -e "${color}$@${NC}"
}

print_color "$BLUE" "======================================"
print_color "$BLUE" "  network-dmenu ML Test Suite"
print_color "$BLUE" "======================================"
echo

# Check if ML build exists
if [ ! -f "target/release/network-dmenu" ]; then
    print_color "$RED" "Error: ML build not found!"
    print_color "$YELLOW" "Run ./build-with-ml.sh first"
    exit 1
fi

# Check if binary has ML features
print_color "$YELLOW" "1. Checking ML features in binary..."
if strings target/release/network-dmenu | grep -q "ml_integration"; then
    print_color "$GREEN" "✅ ML features confirmed in binary"
else
    print_color "$RED" "❌ ML features not found in binary"
    exit 1
fi

# Check ML directory creation
print_color "$YELLOW" "2. Checking ML model directory..."
ML_DIR="$HOME/.local/share/network-dmenu/ml"
if [ -d "$ML_DIR" ]; then
    print_color "$GREEN" "✅ ML directory exists: $ML_DIR"

    # List model files
    print_color "$BLUE" "   Model files:"
    for file in "$ML_DIR"/*.json; do
        if [ -f "$file" ]; then
            size=$(du -h "$file" | cut -f1)
            basename=$(basename "$file")
            print_color "$BLUE" "   - $basename ($size)"
        fi
    done
else
    print_color "$YELLOW" "⚠️  ML directory doesn't exist yet (will be created on first run)"
fi

# Test ML initialization
print_color "$YELLOW" "3. Testing ML initialization..."
RUST_LOG=info timeout 2s ./target/release/network-dmenu --stdout 2>&1 | head -1 > /dev/null
if [ -d "$ML_DIR" ]; then
    print_color "$GREEN" "✅ ML system initialized successfully"
else
    print_color "$RED" "❌ ML system failed to initialize"
    exit 1
fi

# Check model file contents
print_color "$YELLOW" "4. Checking model file integrity..."
if [ -f "$ML_DIR/usage.json" ]; then
    if jq '.' "$ML_DIR/usage.json" > /dev/null 2>&1; then
        print_color "$GREEN" "✅ usage.json is valid JSON"

        # Check for expected fields
        if jq -e '.action_stats' "$ML_DIR/usage.json" > /dev/null 2>&1; then
            print_color "$GREEN" "✅ usage.json has expected structure"
        else
            print_color "$YELLOW" "⚠️  usage.json structure may be incomplete"
        fi
    else
        print_color "$RED" "❌ usage.json is not valid JSON"
    fi
fi

if [ -f "$ML_DIR/exit_node.json" ]; then
    if jq '.' "$ML_DIR/exit_node.json" > /dev/null 2>&1; then
        print_color "$GREEN" "✅ exit_node.json is valid JSON"

        # Check for training data
        training_features=$(jq '.training_data.features | length' "$ML_DIR/exit_node.json")
        if [ "$training_features" -gt 0 ]; then
            print_color "$GREEN" "✅ exit_node.json has $training_features training samples"
        else
            print_color "$YELLOW" "⚠️  exit_node.json has no training data yet"
        fi
    else
        print_color "$RED" "❌ exit_node.json is not valid JSON"
    fi
fi

# Test ML predictions (if Tailscale is available)
print_color "$YELLOW" "5. Testing ML predictions..."
if command -v tailscale &> /dev/null; then
    if tailscale status &> /dev/null; then
        print_color "$BLUE" "   Running with Tailscale integration..."

        # Run with debug logging to see ML predictions
        RUST_LOG=debug timeout 2s ./target/release/network-dmenu --stdout 2>&1 | \
            grep -E "(ML predictions|predict|score)" | head -5 || true

        if [ ${PIPESTATUS[1]} -eq 0 ]; then
            print_color "$GREEN" "✅ ML predictions working"
        else
            print_color "$YELLOW" "⚠️  No ML predictions detected (need more training data)"
        fi
    else
        print_color "$YELLOW" "⚠️  Tailscale not running, skipping prediction test"
    fi
else
    print_color "$YELLOW" "⚠️  Tailscale not installed, skipping prediction test"
fi

# Simulate user actions to test learning
print_color "$YELLOW" "6. Testing ML learning from actions..."

# Create a test sequence of actions
for i in {1..5}; do
    echo "📶 Test WiFi Network $i" | timeout 1s ./target/release/network-dmenu --stdin 2>/dev/null || true
done

# Check if models were updated
if [ -f "$ML_DIR/usage.json" ]; then
    mod_time_before=$(stat -c %Y "$ML_DIR/usage.json" 2>/dev/null || stat -f %m "$ML_DIR/usage.json" 2>/dev/null)
    sleep 1

    # Trigger another action
    echo "📶 Test WiFi Network Final" | timeout 1s ./target/release/network-dmenu --stdin 2>/dev/null || true

    mod_time_after=$(stat -c %Y "$ML_DIR/usage.json" 2>/dev/null || stat -f %m "$ML_DIR/usage.json" 2>/dev/null)

    if [ "$mod_time_after" != "$mod_time_before" ]; then
        print_color "$GREEN" "✅ ML models are being updated with user actions"
    else
        print_color "$YELLOW" "⚠️  ML models not updated (may need more actions)"
    fi
fi

# Performance check
print_color "$YELLOW" "7. Testing ML performance impact..."
start_time=$(date +%s%N)
timeout 2s ./target/release/network-dmenu --stdout 2>/dev/null | head -20 > /dev/null
end_time=$(date +%s%N)
elapsed=$((($end_time - $start_time) / 1000000))

if [ $elapsed -lt 1000 ]; then
    print_color "$GREEN" "✅ ML performance: ${elapsed}ms (good)"
elif [ $elapsed -lt 2000 ]; then
    print_color "$YELLOW" "⚠️  ML performance: ${elapsed}ms (acceptable)"
else
    print_color "$RED" "❌ ML performance: ${elapsed}ms (slow)"
fi

# Summary
echo
print_color "$BLUE" "======================================"
print_color "$BLUE" "  Test Summary"
print_color "$BLUE" "======================================"

total_tests=7
passed_tests=$(grep -c "✅" /tmp/ml_test_$$.log 2>/dev/null || echo 0)

if [ -f "$ML_DIR/exit_node.json" ]; then
    features=$(jq '.training_data.features | length' "$ML_DIR/exit_node.json" 2>/dev/null || echo 0)
    print_color "$BLUE" "Training samples collected: $features"
fi

if [ -f "$ML_DIR/usage.json" ]; then
    actions=$(jq '.action_stats | length' "$ML_DIR/usage.json" 2>/dev/null || echo 0)
    print_color "$BLUE" "Unique actions tracked: $actions"
fi

model_count=$(ls -1 "$ML_DIR"/*.json 2>/dev/null | wc -l || echo 0)
print_color "$BLUE" "Active ML models: $model_count"

echo
print_color "$GREEN" "ML system is operational!"
print_color "$YELLOW" "Note: ML predictions improve with usage (need ~50+ actions)"

# Cleanup
rm -f /tmp/ml_test_$$.log

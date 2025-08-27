# DNS Cache Feature

## Overview

The DNS cache feature in network-dmenu automatically benchmarks DNS servers and caches the results per network. This allows for quick DNS switching without re-running benchmarks every time you connect to a familiar network.

## How It Works

### 1. Automatic Benchmarking
When you run the DNS benchmark diagnostic (`diagnostic - 🌐 DNS benchmark`), the tool:
- Tests multiple DNS servers for response time and reliability
- Filters servers with 100% success rate
- Sorts them by average latency
- **Saves the results to a cache** associated with your current network

### 2. Cache Storage
- Results are stored in `~/.cache/network-dmenu/dns_benchmark_cache.json`
- Each network (identified by SSID for WiFi or gateway IP for wired) has its own cache
- Cache validity: 24 hours (configurable in code)
- Old caches are automatically cleaned up after 7 days

### 3. Dynamic DNS Actions
When you're on a network with cached DNS benchmark results:
- The tool automatically loads the cached results
- Removes any static DNS actions from your config
- Adds dynamic DNS actions for the top 3 fastest servers
- Shows latency in milliseconds for each server

## Generated Actions

The feature generates 4 DNS actions automatically with `[auto]` label:

1. **📡 DNS [auto]: Reset to DHCP** - Reverts to default DHCP-provided DNS
2. **🥇 DNS [auto]: [Fastest Server] (X.Xms)** - Sets the fastest DNS from benchmark
3. **🥈 DNS [auto]: [Second Server] (X.Xms)** - Sets the second fastest DNS
4. **🥉 DNS [auto]: [Third Server] (X.Xms)** - Sets the third fastest DNS

These appear alongside your custom DNS actions, making it easy to distinguish between:
- `[auto]` - Dynamically generated from benchmark results
- Regular DNS actions - Your manually configured preferences

## DNS over TLS (DoT) Support

The cache recognizes DNS servers that support DNS over TLS:
- Cloudflare (1.1.1.1)
- Google (8.8.8.8)
- Quad9 (9.9.9.9)
- NextDNS
- AdGuard
- Mullvad
- ControlD/Hagezi

When setting a DoT-capable server, the command automatically enables DNS over TLS.

## Configuration

### Working with Custom DNS Actions
The DNS cache feature works alongside your custom DNS actions:

1. **Cached actions appear first** with `[auto]` label:
   - `📡 DNS [auto]: Reset to DHCP`
   - `🥇 DNS [auto]: Cloudflare (8.5ms)`
   - `🥈 DNS [auto]: Google (12.3ms)`
   - `🥉 DNS [auto]: Quad9 (15.7ms)`

2. **Your custom actions remain available**:
   ```toml
   [[actions]]
   display = "📡 DNS: Use 1.1.1.1 server"
   cmd = "sudo resolvectl dns wlan0 '1.1.1.1#cloudflare-dns.com'; sudo resolvectl dnsovertls wlan0 yes"
   
   [[actions]]
   display = "📡 DNS: Use Hagezi's server"
   cmd = "sudo resolvectl dns wlan0 '76.76.2.11#x-hagezi-ultimate.freedns.controld.com'; sudo resolvectl dnsovertls wlan0 yes"
   ```

3. **Disable DNS cache if desired**:
   ```toml
   # Set to false to only use your custom DNS actions
   use_dns_cache = false
   ```

This gives you the best of both worlds:
- Fast access to benchmarked DNS servers for the current network
- Your preferred DNS servers always available as fallback options

### Privilege Escalation
The tool automatically detects and uses the best privilege escalation method:
- **pkexec** (if available) - Provides a graphical authentication prompt
- **sudo** (fallback) - Traditional terminal-based authentication

This means you'll get a user-friendly GUI prompt when pkexec is installed, making it easier to authorize DNS changes in desktop environments.

### Interface Detection
The generated commands automatically detect the active network interface using:
```bash
iface=$(ip route show default | grep -oP 'dev \K\S+' | head -1)
```

This ensures the DNS is set on the correct interface whether you're on WiFi or wired connection.

## Cache File Format

The cache file structure:
```json
{
  "caches": {
    "YourWiFiSSID": {
      "network_id": "YourWiFiSSID",
      "timestamp": 1234567890,
      "servers": [
        {
          "name": "Cloudflare",
          "ip": "1.1.1.1",
          "average_latency_ms": 8.5,
          "success_rate": 100.0,
          "supports_dot": true
        }
      ]
    }
  }
}
```

## Usage Example

1. Connect to a network
2. Run DNS benchmark: Select `diagnostic - 🌐 DNS benchmark`
3. Results are automatically cached
4. Next time you open network-dmenu on the same network:
   - Top 3 fastest DNS servers appear as actions
   - Each shows its average latency
   - Select one to instantly switch DNS

## Benefits

- **Speed**: No need to re-benchmark on familiar networks
- **Network-Specific**: Different DNS servers optimized per network
- **Automatic**: Seamlessly integrates with existing workflow
- **Smart**: Knows which servers support DNS over TLS
- **Dynamic**: Always shows the most relevant DNS options

## Troubleshooting

### Cache Not Loading
- Check if `~/.cache/network-dmenu/dns_benchmark_cache.json` exists
- Verify the cache isn't older than 24 hours
- Ensure you have read/write permissions for the cache directory

### DNS Changes Not Applied
- The tool uses `resolvectl` (systemd-resolved)
- Ensure you have sudo/pkexec privileges
- Check if systemd-resolved is running: `systemctl status systemd-resolved`
- If using pkexec, ensure PolicyKit is properly configured

### Wrong Interface Detected
- The tool attempts to auto-detect the active interface
- If detection fails, it falls back to `wlan0`
- You can modify the interface detection logic in `dns_cache.rs`

## Technical Details

### Dependencies
- `dns-bench` tool for benchmarking (optional, but recommended)
- `systemd-resolved` for DNS management
- `nmcli` or `iwctl` for network detection
- `pkexec` (optional) for graphical privilege escalation
- `sudo` (fallback) if pkexec is not available

### Cache Validity
- Default: 24 hours per network
- Cleanup: Removes caches older than 7 days
- Location: `~/.cache/network-dmenu/dns_benchmark_cache.json`

### Performance
- Cache loading: < 1ms
- Action generation: < 1ms
- No network calls when using cache
- Benchmark results persist across reboots

## Summary

The DNS cache feature enhances network-dmenu with intelligent DNS management:

### ✅ What It Does
- **Caches DNS benchmark results** per network (WiFi SSID or gateway IP)
- **Dynamically generates DNS actions** showing the 3 fastest servers with latency
- **Works alongside custom DNS actions** - both cached and custom actions available
- **Supports DNS over TLS** automatically for compatible providers
- **Auto-detects network interface** for both WiFi and wired connections

### 🎯 Key Benefits
- **No repeated benchmarking** - Results cached for 24 hours per network
- **Network-specific optimization** - Different optimal DNS for home/work/cafe
- **Clear labeling** - `[auto]` label distinguishes cached from custom actions
- **User control** - Can be disabled via `use_dns_cache = false` in config
- **Preserves custom actions** - Your preferred DNS servers always remain available

### 📦 How Actions Appear
When you have cached DNS results for your current network:
```
📡 DNS [auto]: Reset to DHCP           <- From cache
🥇 DNS [auto]: Cloudflare (8.5ms)      <- From cache
🥈 DNS [auto]: Google (12.3ms)         <- From cache  
🥉 DNS [auto]: Quad9 (15.7ms)          <- From cache
📡 DNS: Use 1.1.1.1 server             <- Your custom action
📡 DNS: Use Hagezi's server            <- Your custom action
```

This gives you the best of both worlds: fast access to benchmarked servers and your preferred configurations.
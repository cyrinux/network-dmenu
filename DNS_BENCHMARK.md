# DNS Benchmark Feature

## Overview

The DNS Benchmark feature in network-dmenu allows you to automatically test multiple DNS servers, find the fastest one, and configure your system to use it. This feature integrates with the `dns-bench` tool to optimize your DNS resolution performance.

## Prerequisites

Before using the DNS benchmark feature, you need to have `dns-bench` installed:

```bash
cargo install dns-bench
```

## How It Works

When you select the "DNS Benchmark & Optimize" option from the diagnostics menu, the following process occurs:

1. **DNS Server Testing**: The tool runs `dns-bench` to test multiple public DNS servers including:
   - Cloudflare (1.1.1.1, 1.0.0.1)
   - Google (8.8.8.8, 8.8.4.4)
   - Quad9 (9.9.9.9, 149.112.112.112)
   - AdGuard DNS (94.140.14.14, 94.140.15.15)
   - OpenDNS (208.67.222.222, 208.67.220.220)
   - And many more...

2. **Performance Analysis**: Each DNS server is tested with 25 queries to determine:
   - Success rate (percentage of successful queries)
   - Average response time in milliseconds
   - First query response time
   - Reliability metrics

3. **Automatic Selection**: The tool:
   - Filters out unreliable servers (less than 100% success rate)
   - Sorts servers by average response time
   - Selects the fastest reliable DNS server

4. **System Configuration**: Using `systemd-resolve`, the tool attempts to:
   - Detect your active network interface
   - Set the fastest DNS server for that interface
   - Apply the changes immediately

5. **Notification**: You receive a desktop notification showing:
   - The selected DNS server name and IP
   - Average response time
   - Success/failure status of the configuration

## Usage

### Via dmenu/rofi

1. Launch network-dmenu:
   ```bash
   network-dmenu
   ```

2. Navigate to the diagnostics section and select:
   ```
   🔍 DNS Benchmark & Optimize
   ```

3. Wait for the benchmark to complete (typically 10-30 seconds)

4. Review the results showing:
   - Top 5 fastest DNS servers
   - Their average response times
   - Configuration status

### Command Line

You can also check if the DNS benchmark feature is available:

```bash
# List all available diagnostic actions
network-dmenu | grep "DNS Benchmark"
```

## Output Example

After running the DNS benchmark, you'll see output similar to:

```
🏆 Fastest DNS: Cloudflare (1.1.1.1) - 35.00ms average

Top 5 DNS Servers:
1. Cloudflare (1.1.1.1) - 35.00ms
2. Control D (76.76.10.0) - 40.24ms
3. Google (8.8.8.8) - 42.00ms
4. Quad9 (9.9.9.9) - 42.38ms
5. AdGuard DNS (94.140.14.14) - 55.83ms

✅ DNS server set to 1.1.1.1 on interface wlan0
```

## Troubleshooting

### Permission Issues

If the DNS configuration fails, you may need elevated privileges:

```bash
# Run with sudo if systemd-resolve requires it
sudo network-dmenu
```

### Manual DNS Configuration

If automatic configuration fails, you can manually set the DNS:

```bash
# Using systemd-resolved
sudo systemd-resolve --interface <interface> --set-dns <dns-ip>

# Example:
sudo systemd-resolve --interface wlan0 --set-dns 1.1.1.1
```

### Alternative DNS Configuration Methods

For systems not using systemd-resolved:

```bash
# NetworkManager
nmcli con mod <connection-name> ipv4.dns "1.1.1.1"
nmcli con up <connection-name>

# Direct /etc/resolv.conf (temporary)
echo "nameserver 1.1.1.1" | sudo tee /etc/resolv.conf
```

## Features

- **Automatic Testing**: Tests 30+ public DNS servers
- **Smart Selection**: Only considers servers with 100% reliability
- **Performance Metrics**: Shows detailed timing information
- **System Integration**: Automatically configures your system
- **Desktop Notifications**: Provides immediate feedback
- **Fallback Information**: Shows manual configuration steps if automatic setup fails

## Technical Details

### DNS Servers Tested

The benchmark excludes system-configured DNS servers and tests well-known public DNS providers including:

- **Privacy-focused**: Cloudflare, Quad9, AdGuard DNS
- **Performance-focused**: Google DNS, Level3, OpenDNS
- **Security-focused**: CleanBrowsing, Norton ConnectSafe, Comodo Secure DNS
- **Regional providers**: Hurricane Electric, Verisign, SafeDNS

### Benchmark Methodology

1. Each DNS server receives 25 test queries for `google.com`
2. Response times are measured with nanosecond precision
3. Only servers with 100% success rate are considered
4. Results are sorted by average response time
5. The fastest server is automatically selected

### Integration with network-dmenu

The DNS benchmark feature is fully integrated into the network-dmenu diagnostic tools:

- Appears only when `dns-bench` is installed
- Follows the same UI patterns as other diagnostic tools
- Results are displayed in the dmenu/rofi interface
- Notifications use the system notification daemon

## Contributing

To contribute to the DNS benchmark feature:

1. The implementation is in `src/diagnostics.rs`
2. Tests are in `tests/dns_benchmark_test.rs`
3. The feature follows Rust functional programming patterns
4. All DNS operations are async for better performance

## License

This feature is part of network-dmenu and is licensed under the MIT License.
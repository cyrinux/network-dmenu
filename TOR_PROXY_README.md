# Tor Proxy Management for Network-dmenu

Network-dmenu now supports comprehensive Tor proxy management with daemon control and application-specific torsocks integration!

## Features

- **Start/Stop/Restart Tor daemon** from dmenu interface (requires `tor` command)
- **Launch applications via torsocks** for Tor routing (requires `torsocks` command)
- **Automatic Tor status detection** using port monitoring
- **Multiple torsocks configurations** for different applications
- **Smart menu ordering** - Tor daemon management appears first, apps when Tor is running
- **Desktop notifications** for all operations
- **Secure defaults** with proper data directory isolation
- **Command availability checking** - only shows relevant actions when commands are installed

## 🔒 Security & Privacy Benefits

**Tor Integration Advantages:**
- **Anonymity**: Routes traffic through multiple encrypted relays
- **Censorship bypass**: Access blocked websites and services  
- **Location privacy**: Masks your real IP address
- **Traffic analysis protection**: Onion routing prevents tracking
- **No central authority**: Decentralized network

**Why Better Than Open Proxy Lists:**
- ✅ **Trustworthy** - Open source, audited network
- ✅ **Secure** - Multi-layer encryption vs single proxy
- ✅ **Reliable** - Established relay network vs unreliable proxies
- ✅ **Private** - No logging vs potential data harvesting
- ✅ **Fast** - Optimized relay selection vs slow proxies

## Installation & Requirements

```bash
# Install Tor daemon
sudo pacman -S tor torsocks  # Arch Linux  
sudo apt install tor torsocks  # Debian/Ubuntu
sudo dnf install tor torsocks  # Fedora

# Optional: Verify installation
tor --version
torsocks --version
```

## Configuration

Add Tor configurations to your `~/.config/network-dmenu/config.toml`:

```toml
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

### Firefox Tor Profile Setup (Recommended)

Create a dedicated Firefox profile for Tor browsing:

```bash
# Create Tor profile
firefox -CreateProfile "tor"

# Or use Firefox Profile Manager
firefox -ProfileManager
```

Configure the Tor profile:
- Set `network.proxy.type = 0` (use system proxy settings)
- Disable DNS prefetching: `network.dns.disablePrefetch = true`
- Disable WebRTC: `media.peerconnection.enabled = false`

## How it Works

### Tor Daemon Management

**Starting Tor:**
```bash
tor --DataDirectory /tmp/network-dmenu-tor \
    --ControlPort 9051 \
    --SocksPort 9050 \
    --RunAsDaemon 1 \
    --Log "notice file /tmp/network-dmenu-tor.log"
```

**Status Detection:**
- Checks if process is listening on control port (9051) 
- Fallbacks to SOCKS port check (9050)
- Uses `lsof` or `netstat` for port monitoring

**Stopping Tor:**
- Graceful shutdown via control port: `echo 'SIGNAL SHUTDOWN' | nc localhost 9051`
- Fallback: `killall tor`
- Cleanup: Remove data directory

### Torsocks Application Routing

**How torsocks works:**
- Intercepts network calls using `LD_PRELOAD`
- Routes TCP connections through Tor SOCKS proxy (127.0.0.1:9050)
- DNS queries routed through Tor for privacy
- UDP traffic blocked by default (security feature)

**Application launching:**
```bash
torsocks firefox --private-window  # Routes all Firefox traffic through Tor
torsocks curl https://httpbin.org/ip  # Shows your Tor exit IP
```

## Menu Integration

Actions appear in dmenu based on current state and command availability:

**When `tor` command not installed:**
- No Tor actions shown

**When Tor is stopped (`tor` installed):**
- 🧅 Start Tor daemon

**When Tor is running (`tor` installed):**
- ❌ Stop Tor daemon  
- 🔄 Restart Tor daemon

**When Tor is running + `torsocks` installed:**
- 🧅 Start Firefox via Tor
- 🧅 Start Telegram Desktop  
- 🧅 Test Tor Connection

**When apps are running via Tor:**
- ✅ Stop Firefox via Tor
- ✅ Stop Telegram Desktop

## Usage Examples

### 1. Basic Tor Browsing

1. Run `network-dmenu`
2. Select "🧅 Start Tor daemon" 
3. Wait for Tor to establish circuits (~10 seconds)
4. Select "🧅 Start Firefox via Tor"
5. Browse with Tor anonymity!

### 2. Testing Your Tor Connection

```bash
# Check your Tor IP
torsocks curl -s https://httpbin.org/ip

# Verify Tor is working
torsocks curl -s https://check.torproject.org/api/ip
```

### 3. Advanced Usage

**Command line integration:**
```bash
# Start Tor and launch app in one command
network-dmenu --stdout | grep "Start Tor" && tor &
sleep 10 && torsocks firefox --private-window
```

**Browser configuration check:**
Visit https://check.torproject.org/ to verify proper Tor configuration.

## Security Best Practices

### ✅ Do's
- **Use dedicated Firefox profile** for Tor browsing
- **Disable JavaScript** in Tor Browser when possible  
- **Never download files** over Tor without scanning
- **Don't log in to personal accounts** over Tor
- **Keep Tor Browser updated** regularly
- **Use HTTPS Everywhere** extension

### ❌ Don'ts
- **Don't use BitTorrent** over Tor (degrades network)
- **Don't enable browser plugins** (Flash, Java, etc.)
- **Don't maximize browser window** (fingerprinting risk)
- **Don't mix Tor and non-Tor traffic** in same session
- **Don't use Tor for illegal activities**

## Troubleshooting

### Tor Won't Start
```bash
# Check if Tor is already running
sudo netstat -tulpn | grep :9050

# Check Tor logs  
tail -f /tmp/network-dmenu-tor.log

# Manual Tor test
tor --DataDirectory /tmp/tor-test --SocksPort 9051
```

### Applications Not Using Tor
```bash
# Verify torsocks is working
torsocks curl -s https://httpbin.org/ip

# Check if app supports SOCKS proxy
torsocks telnet httpbin.org 80
```

### Connection Issues
```bash
# Test Tor connectivity
curl --socks5 127.0.0.1:9050 -s https://check.torproject.org/api/ip

# Check Tor circuit status (requires control port access)
echo -e 'AUTHENTICATE\nGETINFO circuit-status\nQUIT' | nc localhost 9051
```

## Performance Notes

- **First connection**: 10-30 seconds to establish circuits
- **Subsequent connections**: Usually under 5 seconds
- **Speed**: Expect 50-80% of normal browsing speed
- **Latency**: Additional 200-500ms due to relay routing

## Integration with Existing Features

Network-dmenu's Tor support integrates seamlessly with:

- **SSH Proxies**: Use both simultaneously for defense in depth
- **VPN connections**: Tor-over-VPN or VPN-over-Tor configurations  
- **WiFi management**: Automatic Tor startup on untrusted networks
- **ML features**: Learns your Tor usage patterns for smart ordering
- **Diagnostics**: Connection testing works with Tor routing

## Comparison: Tor vs SSH Proxies vs VPNs

| Feature | Tor | SSH Proxies | VPNs |
|---------|-----|-------------|------|
| **Anonymity** | ★★★★★ | ★★☆☆☆ | ★★★☆☆ |
| **Speed** | ★★☆☆☆ | ★★★★☆ | ★★★★★ |
| **Setup** | ★★★★★ | ★★★☆☆ | ★★★★☆ |
| **Censorship Resistance** | ★★★★★ | ★★★☆☆ | ★★★★☆ |
| **Trust Model** | Decentralized | Single server | Single provider |
| **Cost** | Free | Server costs | Subscription |

**Use Cases:**
- **Tor**: Maximum privacy, accessing .onion sites, bypassing censorship
- **SSH**: Accessing internal networks, port forwarding, development
- **VPN**: General privacy, streaming geo-blocks, consistent performance

Perfect for comprehensive network privacy management! 🧅✨
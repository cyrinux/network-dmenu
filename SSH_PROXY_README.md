# SSH SOCKS Proxy Management

Network-dmenu now supports managing SSH SOCKS proxies with toggle functionality! 

## Features

- **Start/Stop SSH SOCKS proxies** from dmenu interface
- **Toggle functionality** - shows "Start" when stopped, "Stop" when running
- **Multiple proxy configurations** support
- **Automatic status detection** using socket files and port checking
- **Customizable SSH options** per proxy
- **Desktop notifications** for status changes

## Configuration

Add SSH proxy configurations to your `~/.config/network-dmenu/config.toml`:

```toml
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
```

## How it works

The SSH proxy manager:

1. **Detects status** by checking:
   - Socket file existence (`/tmp/server1.sock`)
   - Port listening status (`lsof` or `netstat`)

2. **Starts proxies** using:
   ```bash
   ssh -S /tmp/server1.sock -f -D 1081 -q -N server1
   ```

3. **Stops proxies** using:
   ```bash
   ssh -S /tmp/server1.sock -O exit server1
   ```

## Menu Integration

SSH proxy actions appear in the dmenu with:
- **🔌 Start SOCKS proxy server1 (example.com:1081)** - when stopped
- **✓ Stop SOCKS proxy server1 (example.com:1081)** - when running

## Requirements

- `ssh` command available
- SSH key-based authentication configured for servers
- Optional: `lsof` for better status detection (falls back to `netstat`)

## Example Usage

1. Configure your SSH proxies in the config file
2. Run `network-dmenu`
3. Select "Start SOCKS proxy server1" to start
4. Use proxy: `curl --socks5 127.0.0.1:1081 httpbin.org/ip`
5. Select "Stop SOCKS proxy server1" to stop

## Browser Configuration

Configure your browser to use the SOCKS proxy:
- **Proxy Type**: SOCKS v5
- **Server**: 127.0.0.1
- **Port**: 1081 (or your configured port)

Perfect for bypassing geo-restrictions or accessing internal networks through jump servers!
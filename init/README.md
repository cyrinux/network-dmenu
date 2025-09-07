# Network-dmenu Init Scripts

This directory contains initialization scripts and service files for running network-dmenu as a system service.

## Systemd User Service

### Quick Install

Run the installation script to automatically set up the systemd service:

```bash
./init/install-systemd-service.sh
```

### Manual Installation

1. **Copy service file:**
   ```bash
   mkdir -p ~/.config/systemd/user
   cp init/systemd/network-dmenu.service ~/.config/systemd/user/
   ```

2. **Reload systemd and enable service:**
   ```bash
   systemctl --user daemon-reload
   systemctl --user enable --now network-dmenu.service
   ```

3. **Check service status:**
   ```bash
   systemctl --user status network-dmenu.service
   ```

### Service Versions

#### Standard Version (`network-dmenu.service`)
- **Recommended for most users**
- Minimal permissions required
- Should work on most systems

#### Privileged Version (`network-dmenu-privileged.service`)
- **Use only if the standard version fails**
- Grants additional system capabilities and device access
- Required for systems with strict security policies

### Troubleshooting

#### Service fails to start with "Permission denied" errors

1. Try the privileged version:
   ```bash
   cp init/systemd/network-dmenu-privileged.service ~/.config/systemd/user/network-dmenu.service
   systemctl --user daemon-reload
   systemctl --user restart network-dmenu.service
   ```

2. Check that your user is in the `netdev` group:
   ```bash
   groups
   # If netdev is missing, add it (requires root):
   sudo usermod -a -G netdev $USER
   # Then log out and back in
   ```

#### Service starts but WiFi scanning fails

1. **Check if nmcli/iwctl are accessible:**
   ```bash
   nmcli device wifi list
   iwctl station wlan0 get-networks
   ```

2. **Verify network interface detection:**
   ```bash
   journalctl --user -u network-dmenu.service -f
   ```

3. **Test daemon directly:**
   ```bash
   # Stop systemd service first
   systemctl --user stop network-dmenu.service
   # Run manually with debug logs
   RUST_LOG=debug network-dmenu --daemon
   ```

#### Socket permission errors

1. **Check socket file permissions:**
   ```bash
   ls -la /tmp/network-dmenu-daemon.sock
   ```

2. **Clear old socket files:**
   ```bash
   rm -f /tmp/network-dmenu-daemon.sock
   systemctl --user restart network-dmenu.service
   ```

### Service Management

```bash
# Start service
systemctl --user start network-dmenu.service

# Stop service
systemctl --user stop network-dmenu.service

# Restart service
systemctl --user restart network-dmenu.service

# Enable service (start on login)
systemctl --user enable network-dmenu.service

# Disable service
systemctl --user disable network-dmenu.service

# View service status
systemctl --user status network-dmenu.service

# View service logs (live)
journalctl --user -u network-dmenu.service -f

# View service logs (recent)
journalctl --user -u network-dmenu.service -n 50
```

### Configuration

The service reads configuration from:
- `~/.config/network-dmenu/config.toml`
- Default configuration if config file doesn't exist

### Data Storage

The daemon stores data in:
- `~/.local/share/network-dmenu/zones.json` - Zone definitions and fingerprints
- `~/.local/share/network-dmenu/daemon-state.json` - Daemon state
- `~/.local/share/network-dmenu/ml/` - ML models (if ML feature enabled)

### Environment Variables

The service sets these environment variables:
- `HOME` - User home directory
- `RUST_LOG=debug` - Enable debug logging
- `XDG_*` directories (privileged version only)

### Security Considerations

#### Standard Version
- Minimal system access
- Network device access only
- No elevated capabilities

#### Privileged Version
- Network management capabilities (`CAP_NET_ADMIN`, `CAP_NET_RAW`)
- Device access permissions
- Additional group memberships (`netdev`, `wheel`)

Choose the version with the minimum permissions needed for your system.
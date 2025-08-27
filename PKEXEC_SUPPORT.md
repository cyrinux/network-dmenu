# PolicyKit (pkexec) Support

## Overview

network-dmenu now automatically detects and uses the best available privilege escalation method for commands requiring elevated permissions. This provides a better user experience, especially in desktop environments.

## Automatic Detection

The application automatically detects which privilege escalation method is available:

1. **pkexec** (preferred) - Provides graphical authentication dialog
2. **sudo** (fallback) - Traditional terminal-based authentication

## Benefits

### With pkexec (GUI Authentication)
- ✅ **Graphical prompt** - No terminal required
- ✅ **Desktop integration** - Native look and feel
- ✅ **User-friendly** - Works seamlessly with dmenu/rofi
- ✅ **Secure** - PolicyKit handles authentication
- ✅ **No terminal interruption** - Doesn't break the dmenu flow

### With sudo (Terminal Authentication)
- ✅ **Universal** - Works everywhere (SSH, console, etc.)
- ✅ **Standard** - Traditional Unix privilege escalation
- ✅ **Credential caching** - May remember password for a period
- ✅ **Scriptable** - Works in automated scenarios

## Affected Features

All features requiring elevated permissions now support both methods:

### DNS Management
- Setting custom DNS servers
- Enabling/disabling DNS over TLS
- Reverting to DHCP DNS
- All cached DNS actions

### Network Operations
- Connecting to protected networks
- Modifying network settings
- Managing VPN connections
- System-level network changes

## Installation

### Installing pkexec

**Debian/Ubuntu:**
```bash
sudo apt install policykit-1
```

**Fedora/RHEL:**
```bash
sudo dnf install polkit
```

**Arch Linux:**
```bash
sudo pacman -S polkit
```

**openSUSE:**
```bash
sudo zypper install polkit
```

### Verification

Check if pkexec is available:
```bash
which pkexec
```

Check which method network-dmenu will use:
```bash
if command -v pkexec &> /dev/null; then
    echo "Will use: pkexec (GUI authentication)"
else
    echo "Will use: sudo (terminal authentication)"
fi
```

## How It Works

### Command Wrapping

Simple commands:
- **pkexec**: `pkexec resolvectl revert wlan0`
- **sudo**: `sudo resolvectl revert wlan0`

Complex shell commands:
- **pkexec**: `pkexec sh -c 'resolvectl dns wlan0 "1.1.1.1" && resolvectl dnsovertls wlan0 yes'`
- **sudo**: `sudo sh -c 'resolvectl dns wlan0 "1.1.1.1" && resolvectl dnsovertls wlan0 yes'`

### DNS Actions Example

When you select a DNS action from the menu:

1. **Interface detection** runs first (no privileges needed)
2. **Privilege escalation** is applied to the DNS command
3. **Authentication prompt** appears (GUI with pkexec, terminal with sudo)
4. **Command executes** after successful authentication

## Custom Actions

Your custom actions in `config.toml` can use either method:

```toml
[[actions]]
display = "📡 DNS: Custom Server"
# Generic - will use pkexec if available, sudo otherwise
cmd = "sudo resolvectl dns wlan0 '1.2.3.4'"

[[actions]]
display = "📡 DNS: Force pkexec"
# Explicitly use pkexec
cmd = "pkexec resolvectl dns wlan0 '1.2.3.4'"
```

### Best Practice for Custom Actions

Use the generic `sudo` in your config. The system will automatically use `pkexec` if available:

```toml
[[actions]]
display = "🔧 System Update"
cmd = "sudo apt update && sudo apt upgrade"
# This will use pkexec if available
```

## Troubleshooting

### pkexec Not Working

1. **Check PolicyKit is installed:**
   ```bash
   systemctl status polkit
   ```

2. **Verify authentication agent is running:**
   ```bash
   ps aux | grep -E 'polkit-.*-authentication-agent'
   ```

3. **Check PolicyKit rules:**
   ```bash
   ls -la /usr/share/polkit-1/actions/
   ```

### Fallback to sudo

If pkexec fails, the application automatically falls back to sudo. You can force sudo by:

1. Uninstalling PolicyKit (not recommended)
2. Setting an alias: `alias pkexec=sudo`
3. Modifying PATH to exclude pkexec location

### Permission Denied

If you get permission denied with pkexec:

1. Ensure your user is in the appropriate group:
   ```bash
   groups $USER
   ```

2. Check PolicyKit policies:
   ```bash
   pkcheck --action-id org.freedesktop.policykit.exec --process $$
   ```

## Implementation Details

### Module: `src/privilege.rs`

The privilege escalation functionality is implemented in a dedicated module:

- `get_privilege_command()` - Detects available method
- `wrap_privileged_command()` - Wraps single commands
- `wrap_privileged_commands()` - Wraps multiple commands
- `create_privileged_network_command()` - Network commands with interface detection
- `has_privilege_escalation()` - Checks if any method is available
- `has_gui_privilege_escalation()` - Checks if pkexec is available

### DNS Cache Integration

The DNS cache module (`src/dns_cache.rs`) automatically uses the privilege module for all DNS commands:

- DHCP revert commands
- DNS server changes
- DNS over TLS configuration

### Command Format

Commands requiring shell features are wrapped appropriately:

```rust
// Simple command
pkexec resolvectl revert wlan0

// Complex command with shell features
pkexec sh -c 'command1 && command2'

// With variable substitution
pkexec sh -c 'resolvectl dns ${iface} "1.1.1.1"'
```

## Security Considerations

### PolicyKit Advantages
- Fine-grained permission control
- Centralized authentication
- Audit logging
- Session management
- No password in process list

### Best Practices
1. Never hardcode passwords
2. Use PolicyKit rules for specific permissions
3. Avoid running entire shell sessions with privileges
4. Minimize privileged command scope

## Future Enhancements

Potential improvements for privilege escalation:

1. **Custom PolicyKit actions** - Define specific actions for network-dmenu
2. **Privilege caching** - Remember authentication for a session
3. **Fallback chains** - Try multiple methods in order
4. **User preferences** - Allow forcing specific method via config
5. **Notification integration** - Show status of privileged operations

## Summary

The automatic pkexec/sudo detection provides:

- **Better UX** - GUI prompts in desktop environments
- **Flexibility** - Works in both GUI and terminal contexts  
- **Compatibility** - Fallback ensures it always works
- **Security** - Leverages system authentication mechanisms
- **Transparency** - Clear indication of privileged operations

This enhancement makes network-dmenu more user-friendly while maintaining security and compatibility across different system configurations.
# NextDNS Integration

Network-dmenu now includes built-in support for managing NextDNS profiles directly from the menu.

## Features

- **Profile Switching**: Quickly switch between different NextDNS profiles
- **Service Management**: Enable, disable, or restart the NextDNS service
- **Status Monitoring**: View current profile and service status
- **Profile Toggle**: Configure quick toggle between two frequently used profiles

## Prerequisites

1. **NextDNS CLI**: Install the NextDNS CLI tool from https://github.com/nextdns/nextdns
   ```bash
   # Install on Linux
   sh -c 'sh -c "$(curl -sL https://nextdns.io/install)"'
   ```

2. **Initial Setup**: Configure NextDNS with your profile ID
   ```bash
   sudo nextdns install -config YOUR_PROFILE_ID -report-client-info
   ```

## Configuration

Add NextDNS settings to your `~/.config/network-dmenu/config.toml`:

```toml
# NextDNS configuration

# API key for fetching profiles from NextDNS API (optional)
# Get your API key from: https://my.nextdns.io/account
# This enables dynamic listing of all your NextDNS profiles
nextdns_api_key = "YOUR_API_KEY_HERE"

# Quick toggle between two profiles (optional)
# Useful for switching between different filtering levels
# Find your profile IDs at: https://my.nextdns.io
nextdns_toggle_profiles = ["abc123", "xyz789"]
```

## Available Actions

When NextDNS is installed, the following actions will appear in the menu:

### Service Control
- **ℹ️ NextDNS: Show Status** - Display current profile and service status
- **✅ NextDNS: Enable** - Activate NextDNS DNS filtering
- **❌ NextDNS: Disable** - Deactivate NextDNS (revert to system DNS)
- **🔄 NextDNS: Restart Service** - Restart the NextDNS service

### Profile Management
- **🔄 NextDNS: Switch to [Profile Name]** - Switch to a specific profile (requires API key)
- **🔄 NextDNS: Toggle between Profile1 ↔ Profile2** - Quick toggle between configured profiles

## Usage Examples

### Basic Usage
1. Run `network-dmenu`
2. Look for NextDNS actions (they're prefixed with "nextdns")
3. Select an action to execute it

### Command Line Options
```bash
# Include NextDNS actions (default)
network-dmenu

# Exclude NextDNS actions
network-dmenu --no-nextdns
```

### Profile Switching Workflow

1. **With API Key**: All your NextDNS profiles will be listed automatically
2. **With Toggle Profiles**: Quick switch between two predefined profiles
3. **Manual**: Use `nextdns config set -profile=PROFILE_ID` directly

## Finding Your Profile ID

1. Go to https://my.nextdns.io
2. Click on your profile
3. The profile ID is in the URL: `https://my.nextdns.io/PROFILE_ID/setup`

## Use Cases

### Home vs Work Profiles
Configure different filtering rules for different environments:
```toml
nextdns_toggle_profiles = ["home_profile_id", "work_profile_id"]
```

### Adult vs Kids Profiles
Switch between different content filtering levels:
```toml
nextdns_toggle_profiles = ["adult_profile_id", "kids_profile_id"]
```

### Testing vs Production
Toggle between profiles for development work:
```toml
nextdns_toggle_profiles = ["testing_profile_id", "production_profile_id"]
```

## Troubleshooting

### NextDNS Actions Not Appearing
- Ensure NextDNS CLI is installed: `which nextdns`
- Check if service is running: `sudo nextdns status`
- Verify you're not using `--no-nextdns` flag

### Profile Switching Not Working
- Check profile ID is correct
- Ensure you have proper permissions (may require sudo)
- Verify NextDNS service is running

### API Key Issues
- Ensure API key is valid and has proper permissions
- Check network connectivity to api.nextdns.io
- Note: API features require building with `nextdns-api` feature flag

## Security Considerations

- **API Key**: Store your API key securely in the config file with appropriate permissions
- **Profile IDs**: These are not sensitive, but keep them private to avoid unwanted profile switching
- **Permissions**: NextDNS operations typically require sudo/admin privileges

## Integration with DNS Cache

NextDNS integration works alongside the DNS cache feature:
- DNS cache provides benchmarking of public DNS servers
- NextDNS provides filtering and security features
- You can switch between cached DNS servers and NextDNS profiles as needed

## Future Enhancements

Planned improvements for NextDNS integration:
- [ ] Profile creation from menu
- [ ] Statistics display (queries blocked, etc.)
- [ ] Temporary profile switching with auto-revert
- [ ] Profile-specific settings management
- [ ] Integration with network detection (auto-switch profiles based on network)
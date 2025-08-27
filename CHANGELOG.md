# Changelog

All notable changes to network-dmenu will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **NextDNS Profile Management** - Integrated support for managing NextDNS profiles
  - Switch between different NextDNS profiles from the menu
  - Enable/disable NextDNS service
  - Restart NextDNS service
  - View current profile and service status
  - Configure quick toggle between two frequently used profiles
  - Optional API integration for dynamic profile listing
  - New `--no-nextdns` flag to disable NextDNS actions
  - Configuration options: `nextdns_api_key` and `nextdns_toggle_profiles`

### Fixed
- **Fixed critical shell command execution issues in DNS configuration**
  
  1. **Shell quote escaping error** - "unexpected EOF while looking for matching `''"
     - Occurred when selecting DNS entries with `[auto]` labels from the menu
     - Root cause: Single quotes weren't properly escaped in shell commands wrapped with `sh -c`
     - Solution: Changed quote escaping from `\'` to `'\''` (proper POSIX shell escaping)
     - Affected: `wrap_privileged_command`, `wrap_privileged_commands` in `privilege.rs`
  
  2. **Interface variable substitution error** - "Failed to resolve interface '45.90.30.100#dns.nextdns.io': No such device"
     - Occurred because DNS server address was being used as interface name
     - Root cause: `$iface` variable set in outer shell wasn't available in `sudo sh -c` subshell
     - Solution: Moved interface detection inside the privileged command for proper variable scope
     - Affected: DNS command generation in `dns_cache.rs`
  
  - Added comprehensive test suite (`tests/quote_escaping_test.rs`) and shell test scripts
  - All DNS commands now use pattern: `sudo sh -c 'iface=$(detect); command "${iface}"'`

## [1.11.0] - Previous Release

### Added
- DNS cache feature for automatic DNS benchmarking and selection
- Support for DNS over TLS configuration
- Tailscale exit node management
- Performance profiling with `--profile` flag

### Changed
- Improved performance of network scanning operations
- Enhanced error handling for privileged commands

### Fixed
- Various minor bug fixes and improvements
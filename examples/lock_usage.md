# Tailscale Lock Management with network-dmenu

This example demonstrates how to use the new Tailscale Lock functionality in network-dmenu.

## Overview

Tailscale Lock is a security feature that requires explicit approval for new devices to join your tailnet. When enabled, any new device attempting to connect will be "locked out" until an authorized user signs it with their lock key.

## Prerequisites

1. Tailscale installed and running
2. Tailscale Lock enabled on your tailnet
3. You have a signing key (you're an authorized signer)

## Setting up Tailscale Lock

First, enable Tailscale Lock on your tailnet:

```bash
# Initialize lock on your tailnet (run this on a trusted device)
tailscale lock init

# Check lock status
tailscale lock
```

## Using network-dmenu for Lock Management

When you run `network-dmenu`, you'll see these new lock-related actions in the menu (when applicable):

### 1. Show Lock Status
- **Menu item**: `🔒 Show Tailscale Lock Status`
- **Action**: Displays the complete lock status including:
  - Whether lock is enabled/disabled
  - Your node's signature information
  - List of trusted signing keys
  - Any locked out nodes

### 2. List Locked Nodes
- **Menu item**: `📋 List Locked Nodes`
- **Action**: Shows a notification with all nodes that are currently locked out
- **Format**: `hostname - ip_addresses - machine_name (nodekey...)`

### 3. Sign Individual Nodes
- **Menu items**: `✅ Sign Node: hostname - machine_name (nodekey...)`
- **Action**: Signs a specific locked node using your signing key to allow it to connect
- **Result**: Success/failure notification
- **Note**: Automatically uses your node's signing key from the lock status

## Example Workflow

1. **Check lock status**:
   ```
   Run network-dmenu → Select "🔒 Show Tailscale Lock Status"
   ```

2. **See locked nodes**:
   ```
   Run network-dmenu → Select "📋 List Locked Nodes"
   ```

3. **Sign a trusted node**:
   ```
   Run network-dmenu → Select "✅ Sign Node: us-atl-wg-302 - ncqp5kyPF311CNTRL (38e0e68c...)"
   ```

## Sample Lock Output

When you view lock status, you might see something like:

```
Tailnet lock is ENABLED.

This node is accessible under tailnet lock. Node signature:
SigKind: direct
Pubkey: [9Y8Bj]
KeyID: tlpub:46e70618b88ba73354ee325db315a1bb08e070f64e8829a5d7b13ca0813f74af
WrappingPubkey: tlpub:2cf55e11a9f652206c8a8145bed240907c1fcac690f1aee845e5a2446d1a0c30

Trusted signing keys:
	tlpub:9a44608f7ffeb782e3c95c03e469e960a89e2989e90273c9c369515bb517ebde	1	
	tlpub:46e70618b88ba73354ee325db315a1bb08e070f64e8829a5d7b13ca0813f74af	1	
	tlpub:2cf55e11a9f652206c8a8145bed240907c1fcac690f1aee845e5a2446d1a0c30	1	(self)

The following nodes are locked out:
	us-atl-wg-302.mullvad.ts.net.	100.117.10.73,fd7a:115c:a1e0::cc01:a51	ncqp5kyPF311CNTRL	nodekey:38e0e68cc940b9a51719e4d4cf06a01221b8d861779b46651e1fb74acc350a48
```

## Security Considerations

- **Only sign nodes you trust**: Signing a node allows it full access to your tailnet
- **Verify node identity**: Check that the hostname, machine name, and IP addresses make sense before signing
- **Comprehensive information**: Each locked node shows hostname, IP addresses, machine name, and partial node key
- **Use short node key preview**: The menu shows only the first 8 characters for readability
- **Monitor lock events**: Regularly check lock status to see new signing requests
- **Automatic key detection**: network-dmenu automatically uses your node's signing key from lock status

## Manual Commands

You can also use these Tailscale commands directly:

```bash
# View lock status
tailscale lock

# Sign a specific node (requires both node key and signing key)
tailscale lock sign nodekey:38e0e68cc940b9a51719e4d4cf06a01221b8d861779b46651e1fb74acc350a48 tlpub:2cf55e11a9f652206c8a8145bed240907c1fcac690f1aee845e5a2446d1a0c30

# List all signing keys
tailscale lock status
```

## Troubleshooting

### No lock actions in menu
- Check that Tailscale is installed and running
- Verify that Tailscale Lock is enabled on your tailnet
- Ensure you have signing privileges

### Signing fails
- Verify you have a valid signing key
- Check that the node key is correct
- Ensure network connectivity to Tailscale coordination server

### Example Demo

Run the included demo to test the functionality:

```bash
cargo run --example lock_demo
```

This will show:
- Current lock status
- List of locked nodes (if any)
- What menu actions would be available

## Integration with Existing Workflow

The lock management integrates seamlessly with existing network-dmenu functionality:

1. **Network switching**: Manage exit nodes and lock signing in one interface
2. **Notifications**: Consistent notification system for all actions
3. **Menu organization**: Lock actions appear logically grouped with other Tailscale options
4. **Error handling**: Graceful failure handling with informative messages

The lock functionality enhances network-dmenu's role as a comprehensive network management tool, making Tailscale Lock administration as easy as selecting exit nodes or connecting to WiFi.
use network_dmenu::tailscale::*;
use network_dmenu::command::RealCommandRunner;

fn main() {
    let command_runner = RealCommandRunner;

    println!("🔒 Tailscale Lock Demo");
    println!("=====================");

    // Check if Tailscale is installed
    if !network_dmenu::command::is_command_installed("tailscale") {
        println!("❌ Tailscale is not installed or not in PATH");
        return;
    }

    // Check if Tailscale Lock is enabled
    match is_tailscale_lock_enabled(&command_runner) {
        Ok(true) => {
            println!("✅ Tailscale Lock is ENABLED");

            // Get locked nodes
            match get_locked_nodes(&command_runner) {
                Ok(nodes) => {
                    if nodes.is_empty() {
                        println!("📋 No locked nodes found");
                    } else {
                        println!("📋 Found {} locked node(s):", nodes.len());
                        for (i, node) in nodes.iter().enumerate() {
                            println!("  {}. {} - {} - {} ({}...)",
                                i + 1,
                                extract_short_hostname(&node.hostname),
                                node.ip_addresses,
                                node.machine_name,
                                &node.node_key[..8]
                            );
                            println!("     Full hostname: {}", node.hostname);
                            println!("     Node key: {}", node.node_key);
                            println!();
                        }

                        // Demonstrate what the menu actions would look like
                        println!("🎯 Available menu actions:");
                        println!("  • 🔒 Show Tailscale Lock Status");
                        println!("  • 📋 List Locked Nodes");
                        for node in &nodes {
                            println!("  • ✅ Sign Node: {} ({}...)",
                                extract_short_hostname(&node.hostname),
                                &node.node_key[..8]
                            );
                        }

                        // Show signing key information
                        println!("\n🔑 Signing information:");
                        match get_signing_key(&command_runner) {
                            Ok(signing_key) => {
                                println!("  Your signing key: {}", signing_key);
                                println!("  Manual command example:");
                                if let Some(first_node) = nodes.first() {
                                    println!("    tailscale lock sign nodekey:{} {}",
                                        first_node.node_key, signing_key);
                                }
                            }
                            Err(e) => {
                                println!("  ❌ Could not get signing key: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Failed to get locked nodes: {}", e);
                }
            }
        }
        Ok(false) => {
            println!("🔓 Tailscale Lock is DISABLED");
            println!("📋 Only the lock status action would be available in the menu:");
            println!("  • 🔒 Show Tailscale Lock Status");
        }
        Err(e) => {
            println!("❌ Failed to check Tailscale Lock status: {}", e);
        }
    }

    println!("\n💡 To use this functionality:");
    println!("  1. Run `network-dmenu` to open the menu");
    println!("  2. Look for Tailscale Lock actions (🔒, 📋, ✅)");
    println!("  3. Select an action to execute it");
    println!("  4. Notifications will show the results");
}

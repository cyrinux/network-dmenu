use crate::privilege::wrap_privileged_command;
use dirs::cache_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_DIR_NAME: &str = "network-dmenu";
const CACHE_FILE_NAME: &str = "dns_benchmark_cache.json";
const CACHE_VALIDITY_HOURS: u64 = 24; // Cache valid for 24 hours

/// Represents a cached DNS server with its benchmark results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDnsServer {
    pub name: String,
    pub ip: String,
    pub average_latency_ms: f64,
    pub success_rate: f64,
    pub supports_dot: bool, // DNS over TLS support
}

/// Represents the DNS benchmark cache for a specific network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsBenchmarkCache {
    pub network_id: String, // SSID for WiFi or network identifier
    pub timestamp: u64,     // Unix timestamp when cached
    pub servers: Vec<CachedDnsServer>,
}

/// Cache storage for multiple networks
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DnsCacheStorage {
    #[serde(default)]
    pub caches: HashMap<String, DnsBenchmarkCache>,
}

impl DnsCacheStorage {
    /// Load cache from disk
    pub fn load() -> Result<Self, Box<dyn Error>> {
        let cache_path = get_cache_path()?;

        if !cache_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&cache_path)?;
        let storage: Self = serde_json::from_str(&content)?;

        Ok(storage)
    }

    /// Save cache to disk
    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let cache_path = get_cache_path()?;

        // Ensure parent directory exists
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&cache_path, content)?;

        Ok(())
    }

    /// Get cached benchmark for a specific network
    pub fn get_cache(&self, network_id: &str) -> Option<&DnsBenchmarkCache> {
        self.caches.get(network_id).filter(|cache| {
            // Check if cache is still valid
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let cache_age_hours = (current_time - cache.timestamp) / 3600;
            cache_age_hours < CACHE_VALIDITY_HOURS
        })
    }

    /// Store benchmark results for a network
    pub fn store_cache(&mut self, network_id: String, servers: Vec<CachedDnsServer>) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let cache = DnsBenchmarkCache {
            network_id: network_id.clone(),
            timestamp,
            servers,
        };

        self.caches.insert(network_id, cache);
    }

    /// Clean up old caches
    pub fn cleanup_old_caches(&mut self) {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.caches.retain(|_, cache| {
            let cache_age_hours = (current_time - cache.timestamp) / 3600;
            cache_age_hours < CACHE_VALIDITY_HOURS * 7 // Keep for a week max
        });
    }
}

/// Get the cache file path
fn get_cache_path() -> Result<PathBuf, Box<dyn Error>> {
    let cache_dir = cache_dir().ok_or("Failed to get cache directory")?;
    Ok(cache_dir.join(CACHE_DIR_NAME).join(CACHE_FILE_NAME))
}

/// Get current network identifier (SSID for WiFi, or default for wired)
pub async fn get_current_network_id(
    command_runner: &dyn crate::command::CommandRunner,
) -> Result<String, Box<dyn Error>> {
    // Try to get WiFi SSID first
    if let Ok(output) =
        command_runner.run_command("nmcli", &["-t", "-f", "active,ssid", "dev", "wifi"])
    {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.starts_with("yes:") {
                    if let Some(ssid) = line.strip_prefix("yes:") {
                        if !ssid.is_empty() {
                            return Ok(ssid.to_string());
                        }
                    }
                }
            }
        }
    }

    // Try iwctl as fallback
    if let Ok(output) = command_runner.run_command("iwctl", &["station", "wlan0", "show"]) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("Connected network") {
                    if let Some(ssid) = line.split_whitespace().last() {
                        return Ok(ssid.to_string());
                    }
                }
            }
        }
    }

    // Default to gateway IP for wired or unidentified networks
    if let Ok(output) = command_runner.run_command("ip", &["route", "show", "default"]) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = output_str.lines().next() {
                if let Some(gateway) = line.split_whitespace().nth(2) {
                    return Ok(format!("network_{}", gateway));
                }
            }
        }
    }

    Ok("default_network".to_string())
}

/// Custom action structure (matching the one in main.rs)
#[derive(Debug, Clone)]
pub struct CustomAction {
    pub display: String,
    pub cmd: String,
}

/// Generate DNS change actions from cached benchmark results
pub fn generate_dns_actions_from_cache(cache: &DnsBenchmarkCache) -> Vec<CustomAction> {
    let mut actions = Vec::new();

    // Always add DHCP revert option first
    // Include interface detection inside the privileged command
    let revert_cmd = wrap_privileged_command(
        "iface=$(ip route show default | grep -oP 'dev \\K\\S+' | head -1); iface=${iface:-$(cat /proc/net/route | awk '$2 == \"00000000\" && $3 == \"00000000\" {print $1}' | head -1)}; iface=${iface:-wlan0}; resolvectl revert \"${iface}\"",
        true
    );

    actions.push(CustomAction {
        display: "📡 DNS [auto]: Reset to DHCP".to_string(),
        cmd: revert_cmd,
    });

    // Get top 3 fastest DNS servers
    let mut sorted_servers = cache.servers.clone();
    sorted_servers.sort_by(|a, b| {
        a.average_latency_ms
            .partial_cmp(&b.average_latency_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (i, server) in sorted_servers.iter().take(3).enumerate() {
        let icon = match i {
            0 => "🥇",
            1 => "🥈",
            2 => "🥉",
            _ => "📡",
        };

        let display = format!(
            "{} DNS [auto]: {} ({:.1}ms)",
            icon, server.name, server.average_latency_ms
        );

        let cmd = if server.supports_dot {
            // DNS over TLS
            let dns_cmd = format!(
                "resolvectl dns \"${{iface}}\" '{}#{}'",
                server.ip,
                get_dot_hostname(&server.name)
            );
            let dot_cmd = "resolvectl dnsovertls \"${iface}\" yes";
            // Include interface detection inside the privileged command
            let full_cmd = format!(
                "iface=$(ip {} route show default | grep -oP 'dev \\K\\S+' | head -1); iface=${{iface:-wlan0}}; {} && {}",
                if server.ip.contains(':') { "-6" } else { "" },
                dns_cmd,
                dot_cmd
            );
            wrap_privileged_command(&full_cmd, true)
        } else {
            // Regular DNS
            let dns_cmd = format!("resolvectl dns \"${{iface}}\" '{}'", server.ip);
            let dot_cmd = "resolvectl dnsovertls \"${iface}\" no";
            // Include interface detection inside the privileged command
            let full_cmd = format!(
                "iface=$(ip {} route show default | grep -oP 'dev \\K\\S+' | head -1); iface=${{iface:-wlan0}}; {} && {}",
                if server.ip.contains(':') { "-6" } else { "" },
                dns_cmd,
                dot_cmd
            );
            wrap_privileged_command(&full_cmd, true)
        };

        actions.push(CustomAction { display, cmd });
    }

    actions
}

/// Get DNS over TLS hostname for known providers
fn get_dot_hostname(server_name: &str) -> &'static str {
    let name_lower = server_name.to_lowercase();

    if name_lower.contains("cloudflare") {
        "cloudflare-dns.com"
    } else if name_lower.contains("google") {
        "dns.google"
    } else if name_lower.contains("quad9") {
        "dns.quad9.net"
    } else if name_lower.contains("nextdns") {
        "dns.nextdns.io"
    } else if name_lower.contains("adguard") {
        "dns.adguard.com"
    } else if name_lower.contains("mullvad") {
        "dns.mullvad.net"
    } else if name_lower.contains("controld") || name_lower.contains("hagezi") {
        "x-hagezi-ultimate.freedns.controld.com"
    } else {
        "dns.example.com" // Fallback, won't work but better than empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_storage_default() {
        let storage = DnsCacheStorage::default();
        assert!(storage.caches.is_empty());
    }

    #[test]
    fn test_cache_validity() {
        let mut storage = DnsCacheStorage::default();

        let servers = vec![CachedDnsServer {
            name: "Cloudflare".to_string(),
            ip: "1.1.1.1".to_string(),
            average_latency_ms: 10.5,
            success_rate: 100.0,
            supports_dot: true,
        }];

        storage.store_cache("test_network".to_string(), servers);

        // Should get cache immediately
        assert!(storage.get_cache("test_network").is_some());

        // Modify timestamp to simulate old cache
        if let Some(cache) = storage.caches.get_mut("test_network") {
            cache.timestamp -= (CACHE_VALIDITY_HOURS + 1) * 3600;
        }

        // Should not get old cache
        assert!(storage.get_cache("test_network").is_none());
    }

    #[test]
    fn test_generate_dns_actions() {
        let cache = DnsBenchmarkCache {
            network_id: "test_wifi".to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            servers: vec![
                CachedDnsServer {
                    name: "Cloudflare".to_string(),
                    ip: "1.1.1.1".to_string(),
                    average_latency_ms: 10.5,
                    success_rate: 100.0,
                    supports_dot: true,
                },
                CachedDnsServer {
                    name: "Google".to_string(),
                    ip: "8.8.8.8".to_string(),
                    average_latency_ms: 15.2,
                    success_rate: 100.0,
                    supports_dot: true,
                },
                CachedDnsServer {
                    name: "Quad9".to_string(),
                    ip: "9.9.9.9".to_string(),
                    average_latency_ms: 20.1,
                    success_rate: 100.0,
                    supports_dot: true,
                },
            ],
        };

        let actions = generate_dns_actions_from_cache(&cache);

        // Should have 4 actions: DHCP + top 3 servers
        assert_eq!(actions.len(), 4);

        // First should be DHCP
        assert!(actions[0].display.contains("DHCP"));

        // Should be sorted by latency
        assert!(actions[1].display.contains("Cloudflare"));
        assert!(actions[2].display.contains("Google"));
        assert!(actions[3].display.contains("Quad9"));

        // Verify commands use either sudo or pkexec
        for action in &actions {
            assert!(
                action.cmd.contains("sudo") || action.cmd.contains("pkexec"),
                "Command should use sudo or pkexec for privilege escalation"
            );
        }
    }

    #[test]
    fn test_dot_hostname_mapping() {
        assert_eq!(get_dot_hostname("Cloudflare DNS"), "cloudflare-dns.com");
        assert_eq!(get_dot_hostname("Google Public DNS"), "dns.google");
        assert_eq!(get_dot_hostname("Quad9"), "dns.quad9.net");
        assert_eq!(get_dot_hostname("NextDNS"), "dns.nextdns.io");
        assert_eq!(get_dot_hostname("AdGuard"), "dns.adguard.com");
        assert_eq!(get_dot_hostname("Unknown Provider"), "dns.example.com");
    }

    #[test]
    fn test_dns_actions_with_pkexec() {
        // Test that pkexec commands are properly formatted
        let cache = DnsBenchmarkCache {
            network_id: "test".to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            servers: vec![CachedDnsServer {
                name: "Test DNS".to_string(),
                ip: "1.2.3.4".to_string(),
                average_latency_ms: 5.0,
                success_rate: 100.0,
                supports_dot: true,
            }],
        };

        let actions = generate_dns_actions_from_cache(&cache);

        // Check that commands contain proper privilege escalation
        for action in &actions {
            if action.cmd.contains("pkexec") {
                // pkexec commands should use sh -c for complex commands
                assert!(
                    action.cmd.contains("sh -c"),
                    "pkexec commands should use sh -c for complex shell operations"
                );
            }
            // All commands should have resolvectl
            if !action.display.contains("Reset to DHCP") || !action.cmd.contains("pkexec") {
                assert!(
                    action.cmd.contains("resolvectl"),
                    "DNS commands should use resolvectl"
                );
            }
        }
    }
}

use crate::command::{is_command_installed, CommandRunner};
use crate::constants::{ACTION_TYPE_DIAGNOSTIC, ICON_CHECK, ICON_SIGNAL};
use crate::dns_cache::{CachedDnsServer, DnsCacheStorage, get_current_network_id};
use crate::format_entry;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::net::IpAddr;
use std::str::FromStr;

/// Network diagnostic actions that can be performed
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticAction {
    PingGateway,
    PingDns,
    TraceRoute(String),
    CheckMtu(String),
    TestConnectivity,
    ShowRouting,
    CheckLatency(String),
    ShowNetstat,
    ShowInterfaces,
    SpeedTest,
    SpeedTestFast,
    DnsBenchmark,
}

/// Result of a network diagnostic operation
#[derive(Debug)]
pub struct DiagnosticResult {
    pub success: bool,
    pub output: String,
}

// DNS Benchmark JSON structures
#[derive(Debug, Serialize, Deserialize)]
struct DnsBenchResult {
    name: String,
    ip: String,
    #[serde(default)]
    last_resolved_ip: String,
    total_requests: u32,
    successful_requests: u32,
    successful_requests_percentage: f32,
    first_duration: DnsDuration,
    average_duration: DnsDuration,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum DnsDuration {
    Succeeded { succeeded: DnsTime },
    Failed { failed: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct DnsTime {
    secs: u64,
    nanos: u32,
}

impl DnsTime {
    fn to_millis(&self) -> f64 {
        self.secs as f64 * 1000.0 + self.nanos as f64 / 1_000_000.0
    }
}

// Speedtest-go JSON structures
#[derive(Debug, Serialize, Deserialize)]
struct SpeedtestGoResult {
    timestamp: String,
    user_info: UserInfo,
    servers: Vec<ServerResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserInfo {
    #[serde(rename = "IP")]
    ip: String,
    #[serde(rename = "Lat")]
    lat: String,
    #[serde(rename = "Lon")]
    lon: String,
    #[serde(rename = "Isp")]
    isp: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServerResult {
    url: String,
    lat: String,
    lon: String,
    name: String,
    country: String,
    sponsor: String,
    id: String,
    host: String,
    distance: f64,
    latency: i64,
    max_latency: i64,
    min_latency: i64,
    jitter: i64,
    dl_speed: f64,
    ul_speed: f64,
    test_duration: TestDuration,
    packet_loss: PacketLoss,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestDuration {
    ping: i64,
    download: i64,
    upload: i64,
    total: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PacketLoss {
    sent: i32,
    dup: i32,
    max: i32,
}

/// Get available network diagnostic actions based on installed tools
pub fn get_diagnostic_actions() -> Vec<DiagnosticAction> {
    let mut actions = Vec::new();

    // Ping-based actions (require ping command)
    let ping_available = is_command_installed("ping");

    if ping_available {
        actions.push(DiagnosticAction::TestConnectivity);
        actions.push(DiagnosticAction::PingGateway);
        actions.push(DiagnosticAction::PingDns);
        actions.push(DiagnosticAction::CheckMtu("8.8.8.8".to_string()));
        actions.push(DiagnosticAction::CheckLatency("8.8.8.8".to_string()));
        actions.push(DiagnosticAction::CheckLatency("1.1.1.1".to_string()));
    }

    // Traceroute actions (require traceroute command)
    let traceroute_available = is_command_installed("traceroute");

    if traceroute_available {
        actions.push(DiagnosticAction::TraceRoute("8.8.8.8".to_string()));
        actions.push(DiagnosticAction::TraceRoute("1.1.1.1".to_string()));
    }

    // IP-based actions (require ip command)
    let ip_available = is_command_installed("ip");

    if ip_available {
        actions.push(DiagnosticAction::ShowRouting);
        actions.push(DiagnosticAction::ShowInterfaces);
    }

    // Network connections (prefer ss, fallback to netstat)
    let ss_available = is_command_installed("ss");
    let netstat_available = is_command_installed("netstat");

    if ss_available || netstat_available {
        actions.push(DiagnosticAction::ShowNetstat);
    }

    // Speedtest actions (check for various speedtest tools)
    let speedtest_go_available = is_command_installed("speedtest-go");
    let speedtest_cli_available = is_command_installed("speedtest-cli");
    let speedtest_available = is_command_installed("speedtest");
    let fast_available = is_command_installed("fast");

    if speedtest_go_available || speedtest_cli_available || speedtest_available {
        actions.push(DiagnosticAction::SpeedTest);
    }

    if fast_available {
        actions.push(DiagnosticAction::SpeedTestFast);
    }

    // DNS benchmark action (requires dns-bench command)
    let dns_bench_available = is_command_installed("dns-bench");

    if dns_bench_available {
        actions.push(DiagnosticAction::DnsBenchmark);
    }

    actions
}

/// Execute a network diagnostic action
pub async fn handle_diagnostic_action(
    action: &DiagnosticAction,
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    match action {
        DiagnosticAction::PingGateway => ping_gateway(command_runner).await,
        DiagnosticAction::PingDns => ping_dns_servers(command_runner).await,
        DiagnosticAction::TraceRoute(target) => trace_route(command_runner, target).await,
        DiagnosticAction::CheckMtu(target) => check_mtu(command_runner, target).await,
        DiagnosticAction::TestConnectivity => test_connectivity(command_runner).await,
        DiagnosticAction::ShowRouting => show_routing_table(command_runner).await,
        DiagnosticAction::CheckLatency(target) => check_latency(command_runner, target).await,
        DiagnosticAction::ShowNetstat => show_netstat(command_runner).await,
        DiagnosticAction::ShowInterfaces => show_network_interfaces(command_runner).await,
        DiagnosticAction::SpeedTest => run_speedtest(command_runner).await,
        DiagnosticAction::SpeedTestFast => run_speedtest_fast(command_runner).await,
        DiagnosticAction::DnsBenchmark => run_dns_benchmark(command_runner).await,
    }
}

/// Convert diagnostic action to display string
pub fn diagnostic_action_to_string(action: &DiagnosticAction) -> String {
    match action {
        DiagnosticAction::PingGateway => {
            format_entry(ACTION_TYPE_DIAGNOSTIC, ICON_SIGNAL, "Ping Gateway")
        }
        DiagnosticAction::PingDns => {
            format_entry(ACTION_TYPE_DIAGNOSTIC, ICON_SIGNAL, "Ping DNS Servers")
        }
        DiagnosticAction::TraceRoute(target) => format_entry(
            ACTION_TYPE_DIAGNOSTIC,
            "🗺️",
            &format!("Trace Route to {}", target),
        ),
        DiagnosticAction::CheckMtu(target) => format_entry(
            ACTION_TYPE_DIAGNOSTIC,
            "📏",
            &format!("Check MTU to {}", target),
        ),
        DiagnosticAction::TestConnectivity => {
            format_entry(ACTION_TYPE_DIAGNOSTIC, ICON_CHECK, "Test Connectivity")
        }
        DiagnosticAction::ShowRouting => {
            format_entry(ACTION_TYPE_DIAGNOSTIC, "🛣️", "Show Routing Table")
        }
        DiagnosticAction::CheckLatency(target) => format_entry(
            ACTION_TYPE_DIAGNOSTIC,
            "⏱️",
            &format!("Check Latency to {}", target),
        ),
        DiagnosticAction::ShowNetstat => {
            format_entry(ACTION_TYPE_DIAGNOSTIC, "📊", "Show Network Connections")
        }
        DiagnosticAction::ShowInterfaces => {
            format_entry(ACTION_TYPE_DIAGNOSTIC, "🔌", "Show Network Interfaces")
        }
        DiagnosticAction::SpeedTest => format_entry(ACTION_TYPE_DIAGNOSTIC, "🚀", "Speed Test"),
        DiagnosticAction::SpeedTestFast => {
            format_entry(ACTION_TYPE_DIAGNOSTIC, "⚡", "Speed Test (Fast.com)")
        }
        DiagnosticAction::DnsBenchmark => {
            format_entry(ACTION_TYPE_DIAGNOSTIC, "🔍", "DNS Benchmark & Optimize")
        }
    }
}

/// Test general internet connectivity
async fn test_connectivity(
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    let output = command_runner.run_command("ping", &["-c", "3", "-W", "3", "8.8.8.8"])?;

    let result = if output.status.success() {
        DiagnosticResult {
            success: true,
            output: "Internet connectivity: OK".to_string(),
        }
    } else {
        DiagnosticResult {
            success: false,
            output: "Internet connectivity: FAILED".to_string(),
        }
    };

    Ok(result)
}

/// Ping the default gateway
async fn ping_gateway(
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    // First, get the default gateway
    let route_output = command_runner.run_command("ip", &["route", "show", "default"])?;

    if !route_output.status.success() {
        return Ok(DiagnosticResult {
            success: false,
            output: "Could not determine default gateway".to_string(),
        });
    }

    let route_output_str = String::from_utf8_lossy(&route_output.stdout);
    let gateway = extract_gateway_ip(&route_output_str);

    match gateway {
        Some(gw_ip) => {
            let ping_output =
                command_runner.run_command("ping", &["-c", "3", "-W", "2", &gw_ip])?;

            if ping_output.status.success() {
                let output_str = String::from_utf8_lossy(&ping_output.stdout);
                let summary = extract_ping_summary(&output_str);
                Ok(DiagnosticResult {
                    success: true,
                    output: format!("Gateway {} is reachable\n{}", gw_ip, summary),
                })
            } else {
                Ok(DiagnosticResult {
                    success: false,
                    output: format!("Gateway {} is unreachable", gw_ip),
                })
            }
        }
        None => Ok(DiagnosticResult {
            success: false,
            output: "Could not extract gateway IP address".to_string(),
        }),
    }
}

/// Ping DNS servers
async fn ping_dns_servers(
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    let dns_servers = vec!["8.8.8.8", "1.1.1.1", "9.9.9.9"];
    let mut results = Vec::new();
    let mut all_success = true;

    for dns in dns_servers {
        let output = command_runner.run_command("ping", &["-c", "2", "-W", "2", dns])?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let summary = extract_ping_summary(&output_str);
            results.push(format!("✅ {} - {}", dns, summary));
        } else {
            results.push(format!("❌ {} - Unreachable", dns));
            all_success = false;
        }
    }

    Ok(DiagnosticResult {
        success: all_success,
        output: results.join("\n"),
    })
}

/// Trace route to target
async fn trace_route(
    command_runner: &dyn CommandRunner,
    target: &str,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    let output = command_runner.run_command("traceroute", &["-n", "-m", "15", target])?;

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        Ok(DiagnosticResult {
            success: true,
            output: format!("Traceroute to {}:\n{}", target, output_str),
        })
    } else {
        Ok(DiagnosticResult {
            success: false,
            output: format!("Traceroute to {} failed", target),
        })
    }
}

/// Check MTU to target
async fn check_mtu(
    command_runner: &dyn CommandRunner,
    target: &str,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    // Try different packet sizes to find MTU
    let test_sizes = vec![1500, 1472, 1400, 1300, 1200, 1000];
    let mut working_mtu = 0;

    for size in test_sizes {
        let size_str = size.to_string();
        let output = command_runner.run_command(
            "ping",
            &["-c", "1", "-M", "do", "-s", &size_str, "-W", "3", target],
        )?;

        if output.status.success() {
            working_mtu = size + 28; // Add IP + ICMP headers
            break;
        }
    }

    if working_mtu > 0 {
        Ok(DiagnosticResult {
            success: true,
            output: format!("Maximum working MTU to {}: {} bytes", target, working_mtu),
        })
    } else {
        Ok(DiagnosticResult {
            success: false,
            output: format!("Could not determine MTU to {}", target),
        })
    }
}

/// Check latency to target
async fn check_latency(
    command_runner: &dyn CommandRunner,
    target: &str,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    let output = command_runner.run_command("ping", &["-c", "10", "-W", "3", target])?;

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        let summary = extract_ping_summary(&output_str);
        let latency_stats = extract_latency_stats(&output_str);

        Ok(DiagnosticResult {
            success: true,
            output: format!("Latency to {}:\n{}\n{}", target, summary, latency_stats),
        })
    } else {
        Ok(DiagnosticResult {
            success: false,
            output: format!("Latency test to {} failed", target),
        })
    }
}

/// Show routing table
async fn show_routing_table(
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    let output = command_runner.run_command("ip", &["route", "show"])?;

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        Ok(DiagnosticResult {
            success: true,
            output: format!("Routing Table:\n{}", output_str),
        })
    } else {
        Ok(DiagnosticResult {
            success: false,
            output: "Failed to get routing table".to_string(),
        })
    }
}

/// Show network connections
async fn show_netstat(
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    let output = command_runner.run_command("ss", &["-tuln"])?;

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        Ok(DiagnosticResult {
            success: true,
            output: format!("Active Network Connections:\n{}", output_str),
        })
    } else {
        // Fallback to netstat if ss is not available
        let netstat_output = command_runner.run_command("netstat", &["-tuln"])?;

        if netstat_output.status.success() {
            let output_str = String::from_utf8_lossy(&netstat_output.stdout);
            Ok(DiagnosticResult {
                success: true,
                output: format!("Active Network Connections:\n{}", output_str),
            })
        } else {
            Ok(DiagnosticResult {
                success: false,
                output: "Failed to get network connections".to_string(),
            })
        }
    }
}

/// Show network interfaces
async fn show_network_interfaces(
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    let output = command_runner.run_command("ip", &["addr", "show"])?;

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        let formatted = format_interface_output(&output_str);
        Ok(DiagnosticResult {
            success: true,
            output: format!("Network Interfaces:\n{}", formatted),
        })
    } else {
        Ok(DiagnosticResult {
            success: false,
            output: "Failed to get network interfaces".to_string(),
        })
    }
}

/// Extract gateway IP from route output
fn extract_gateway_ip(route_output: &str) -> Option<String> {
    for line in route_output.lines() {
        if line.contains("default") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if *part == "via" && i + 1 < parts.len() {
                    let ip_str = parts[i + 1];
                    if IpAddr::from_str(ip_str).is_ok() {
                        return Some(ip_str.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Extract ping summary from ping output
fn extract_ping_summary(ping_output: &str) -> String {
    for line in ping_output.lines() {
        if line.contains("packets transmitted") {
            return line.to_string();
        }
    }
    "No summary available".to_string()
}

/// Extract latency statistics from ping output
fn extract_latency_stats(ping_output: &str) -> String {
    for line in ping_output.lines() {
        if line.contains("rtt min/avg/max/mdev") {
            return format!(
                "RTT Statistics: {}",
                line.split('=').next_back().unwrap_or("").trim()
            );
        }
    }
    "No latency statistics available".to_string()
}

/// Format interface output for better readability
fn format_interface_output(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let mut formatted = Vec::new();

    for line in lines {
        if line.starts_with(char::is_numeric) {
            // Interface line
            if let Some(colon_pos) = line.find(':') {
                let interface_part = &line[..colon_pos];
                let rest = &line[colon_pos..];
                formatted.push(format!("📡 {}{}", interface_part, rest));
            } else {
                formatted.push(line.to_string());
            }
        } else if line.trim().starts_with("inet ") {
            // IP address line
            formatted.push(format!("  🌐 {}", line.trim()));
        } else if line.trim().starts_with("link/") {
            // MAC address line
            formatted.push(format!("  🔗 {}", line.trim()));
        } else {
            formatted.push(line.to_string());
        }
    }

    formatted.join("\n")
}

/// Run internet speed test using speedtest-go, speedtest-cli, or speedtest
async fn run_speedtest(
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    // Try speedtest-go first for better JSON output
    if is_command_installed("speedtest-go") {
        let output = command_runner.run_command("speedtest-go", &["--json"])?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);

            // Try to parse JSON for pretty output
            if let Ok(result) = serde_json::from_str::<SpeedtestGoResult>(&output_str) {
                if let Some(server) = result.servers.first() {
                    // Convert speeds from bps to Mbps
                    let download_mbps = server.dl_speed / 1_000_000.0;
                    let upload_mbps = server.ul_speed / 1_000_000.0;
                    let ping_ms = server.latency as f64 / 1_000_000.0;
                    let jitter_ms = server.jitter as f64 / 1_000_000.0;

                    let pretty_output = format!(
                        "🌐 Speed Test Results\n\n\
                        📥 Download: {:.2} Mbps\n\
                        📤 Upload: {:.2} Mbps\n\
                        🏓 Ping: {:.2} ms\n\
                        📊 Jitter: {:.2} ms\n\n\
                        🏢 Server: {} - {}\n\
                        📍 Location: {}, {}\n\
                        🌍 Distance: {:.1} km\n\n\
                        🏠 Your ISP: {}\n\
                        🔢 Your IP: {}",
                        download_mbps,
                        upload_mbps,
                        ping_ms,
                        jitter_ms,
                        server.sponsor,
                        server.name,
                        server.name,
                        server.country,
                        server.distance,
                        result.user_info.isp,
                        result.user_info.ip
                    );

                    return Ok(DiagnosticResult {
                        success: true,
                        output: pretty_output,
                    });
                }
            }

            // Fallback to raw output if JSON parsing fails
            return Ok(DiagnosticResult {
                success: true,
                output: format!("Speed Test Results:\n{}", output_str),
            });
        }
    }

    // Fallback to other speedtest tools
    let output = if is_command_installed("speedtest-cli") {
        command_runner.run_command("speedtest-cli", &["--simple"])?
    } else if is_command_installed("speedtest") {
        command_runner.run_command("speedtest", &[])?
    } else {
        return Ok(DiagnosticResult {
            success: false,
            output: "No speedtest tool available (speedtest-go, speedtest-cli, or speedtest)"
                .to_string(),
        });
    };

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        Ok(DiagnosticResult {
            success: true,
            output: format!("Internet Speed Test Results:\n{}", output_str),
        })
    } else {
        Ok(DiagnosticResult {
            success: false,
            output: "Speed test failed".to_string(),
        })
    }
}

/// Run internet speed test using fast (Netflix's fast.com)
async fn run_speedtest_fast(
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    let output = command_runner.run_command("fast", &[])?;

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        Ok(DiagnosticResult {
            success: true,
            output: format!("Fast.com Speed Test Results:\n{}", output_str),
        })
    } else {
        Ok(DiagnosticResult {
            success: false,
            output: "Fast.com speed test failed".to_string(),
        })
    }
}

/// Run DNS benchmark and set the fastest DNS server
async fn run_dns_benchmark(
    command_runner: &dyn CommandRunner,
) -> Result<DiagnosticResult, Box<dyn Error>> {
    // Run dns-bench with JSON output
    let output = command_runner.run_command(
        "dns-bench",
        &["--format", "json", "--skip-system-servers"],
    )?;

    if !output.status.success() {
        return Ok(DiagnosticResult {
            success: false,
            output: "DNS benchmark failed to run".to_string(),
        });
    }

    let json_output = String::from_utf8_lossy(&output.stdout);

    // Parse the JSON output
    let dns_results: Vec<DnsBenchResult> = match serde_json::from_str(&json_output) {
        Ok(results) => results,
        Err(e) => {
            return Ok(DiagnosticResult {
                success: false,
                output: format!("Failed to parse DNS benchmark results: {}", e),
            });
        }
    };

    // Filter and sort DNS servers by average response time
    let mut valid_results: Vec<_> = dns_results
        .into_iter()
        .filter_map(|result| {
            // Only consider servers with 100% success rate
            if result.successful_requests_percentage >= 100.0 {
                if let DnsDuration::Succeeded { succeeded } = result.average_duration {
                    return Some((
                        result.name,
                        result.ip,
                        succeeded.to_millis(),
                        result.successful_requests_percentage,
                    ));
                }
            }
            None
        })
        .collect();

    // Sort by average response time (fastest first)
    valid_results.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    if valid_results.is_empty() {
        return Ok(DiagnosticResult {
            success: false,
            output: "No reliable DNS servers found".to_string(),
        });
    }

    // Save results to cache
    let network_id = get_current_network_id(command_runner).await.unwrap_or_else(|_| "default_network".to_string());

    let cached_servers: Vec<CachedDnsServer> = valid_results
        .iter()
        .map(|(name, ip, latency, success_rate)| {
            // Determine if server supports DNS over TLS
            let supports_dot = name.to_lowercase().contains("cloudflare")
                || name.to_lowercase().contains("google")
                || name.to_lowercase().contains("quad9")
                || name.to_lowercase().contains("nextdns")
                || name.to_lowercase().contains("adguard")
                || name.to_lowercase().contains("mullvad")
                || name.to_lowercase().contains("controld")
                || name.to_lowercase().contains("hagezi");

            CachedDnsServer {
                name: name.clone(),
                ip: ip.clone(),
                average_latency_ms: *latency,
                success_rate: *success_rate as f64,
                supports_dot,
            }
        })
        .collect();

    // Store in cache
    if let Ok(mut cache_storage) = DnsCacheStorage::load() {
        cache_storage.store_cache(network_id.clone(), cached_servers);
        cache_storage.cleanup_old_caches();
        let _ = cache_storage.save();
    }

    // Get the fastest DNS server
    let (name, ip, avg_ms, _) = &valid_results[0];

    // Get current network interface (assuming default route)
    let route_output = command_runner.run_command("ip", &["route", "show", "default"])?;
    let route_str = String::from_utf8_lossy(&route_output.stdout);

    // Extract the interface name from the default route
    let interface = route_str
        .lines()
        .next()
        .and_then(|line| {
            line.split_whitespace()
                .skip_while(|&word| word != "dev")
                .nth(1)
        })
        .unwrap_or("eth0");

    // Set the DNS using systemd-resolved
    let set_dns_output = command_runner.run_command(
        "systemd-resolve",
        &["--interface", interface, "--set-dns", ip],
    )?;

    let success = set_dns_output.status.success();

    // Prepare the results summary
    let mut summary = format!(
        "🏆 Fastest DNS: {} ({}) - {:.2}ms average\n\n",
        name, ip, avg_ms
    );

    summary.push_str("Top 5 DNS Servers:\n");
    for (i, (name, ip, avg_ms, _)) in valid_results.iter().take(5).enumerate() {
        summary.push_str(&format!(
            "{}. {} ({}) - {:.2}ms\n",
            i + 1,
            name,
            ip,
            avg_ms
        ));
    }

    if success {
        summary.push_str(&format!(
            "\n✅ DNS server set to {} on interface {}",
            ip, interface
        ));

        // Send notification about the DNS change
        let _ = Notification::new()
            .summary("DNS Optimized")
            .body(&format!(
                "Set fastest DNS: {} ({})\nAverage response: {:.2}ms",
                name, ip, avg_ms
            ))
            .timeout(5000)
            .show();
    } else {
        summary.push_str(&format!(
            "\n❌ Failed to set DNS. You may need to manually set DNS to {}",
            ip
        ));

        // Send error notification
        let _ = Notification::new()
            .summary("DNS Benchmark Complete")
            .body(&format!(
                "Fastest DNS found: {} ({})\nManual configuration may be required.",
                name, ip
            ))
            .timeout(5000)
            .show();
    }

    Ok(DiagnosticResult {
        success,
        output: summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_action_to_string() {
        assert_eq!(
            diagnostic_action_to_string(&DiagnosticAction::PingGateway),
            "diagnostic- 📶 Ping Gateway"
        );

        assert_eq!(
            diagnostic_action_to_string(&DiagnosticAction::TraceRoute("8.8.8.8".to_string())),
            "diagnostic- 🗺️ Trace Route to 8.8.8.8"
        );

        assert_eq!(
            diagnostic_action_to_string(&DiagnosticAction::TestConnectivity),
            "diagnostic- ✅ Test Connectivity"
        );

        assert_eq!(
            diagnostic_action_to_string(&DiagnosticAction::CheckMtu("1.1.1.1".to_string())),
            "diagnostic- 📏 Check MTU to 1.1.1.1"
        );

        assert_eq!(
            diagnostic_action_to_string(&DiagnosticAction::ShowRouting),
            "diagnostic- 🛣️ Show Routing Table"
        );

        assert_eq!(
            diagnostic_action_to_string(&DiagnosticAction::ShowInterfaces),
            "diagnostic- 🔌 Show Network Interfaces"
        );

        assert_eq!(
            diagnostic_action_to_string(&DiagnosticAction::SpeedTest),
            "diagnostic- 🚀 Speed Test"
        );

        assert_eq!(
            diagnostic_action_to_string(&DiagnosticAction::SpeedTestFast),
            "diagnostic- ⚡ Speed Test (Fast.com)"
        );

        assert_eq!(
            diagnostic_action_to_string(&DiagnosticAction::DnsBenchmark),
            "diagnostic- 🔍 DNS Benchmark & Optimize"
        );
    }

    #[test]
    fn test_extract_gateway_ip() {
        let route_output = "default via 192.168.1.1 dev wlan0 proto dhcp metric 600";
        assert_eq!(
            extract_gateway_ip(route_output),
            Some("192.168.1.1".to_string())
        );

        let no_gateway = "192.168.1.0/24 dev wlan0 proto kernel scope link src 192.168.1.100";
        assert_eq!(extract_gateway_ip(no_gateway), None);

        let multiple_routes = "default via 10.0.0.1 dev eth0\ndefault via 192.168.1.1 dev wlan0";
        assert_eq!(
            extract_gateway_ip(multiple_routes),
            Some("10.0.0.1".to_string())
        );
    }

    #[test]
    fn test_extract_ping_summary() {
        let ping_output = "PING 8.8.8.8 (8.8.8.8) 56(84) bytes of data.\n64 bytes from 8.8.8.8: icmp_seq=1 ttl=118 time=15.2 ms\n\n--- 8.8.8.8 ping statistics ---\n3 packets transmitted, 3 received, 0% packet loss, time 2003ms";

        assert_eq!(
            extract_ping_summary(ping_output),
            "3 packets transmitted, 3 received, 0% packet loss, time 2003ms"
        );

        let no_summary = "PING 8.8.8.8 (8.8.8.8) 56(84) bytes of data.";
        assert_eq!(extract_ping_summary(no_summary), "No summary available");
    }

    #[test]
    fn test_extract_latency_stats() {
        let ping_output = "PING 8.8.8.8 (8.8.8.8) 56(84) bytes of data.\n64 bytes from 8.8.8.8: icmp_seq=1 ttl=118 time=15.2 ms\nrtt min/avg/max/mdev = 10.123/15.456/20.789/2.345 ms";

        assert_eq!(
            extract_latency_stats(ping_output),
            "RTT Statistics: 10.123/15.456/20.789/2.345 ms"
        );

        let no_stats = "PING 8.8.8.8 (8.8.8.8) 56(84) bytes of data.";
        assert_eq!(
            extract_latency_stats(no_stats),
            "No latency statistics available"
        );
    }

    #[test]
    fn test_get_diagnostic_actions() {
        let actions = get_diagnostic_actions();

        // Test ping-based actions (only if ping is available)
        if is_command_installed("ping") {
            assert!(actions.contains(&DiagnosticAction::TestConnectivity));
            assert!(actions.contains(&DiagnosticAction::PingGateway));
            assert!(actions.contains(&DiagnosticAction::PingDns));
        }

        // Test traceroute actions (only if traceroute is available)
        if is_command_installed("traceroute") {
            assert!(actions.contains(&DiagnosticAction::TraceRoute("8.8.8.8".to_string())));
        }

        // Test ip-based actions (only if ip is available)
        if is_command_installed("ip") {
            assert!(actions.contains(&DiagnosticAction::ShowRouting));
            assert!(actions.contains(&DiagnosticAction::ShowInterfaces));
        }

        // Test network connections (only if ss or netstat is available)
        if is_command_installed("ss") || is_command_installed("netstat") {
            assert!(actions.contains(&DiagnosticAction::ShowNetstat));
        }

        // Test speedtest actions (only if speedtest tools are available)
        if is_command_installed("speedtest-cli") || is_command_installed("speedtest") {
            assert!(actions.contains(&DiagnosticAction::SpeedTest));
        }

        if is_command_installed("fast") {
            assert!(actions.contains(&DiagnosticAction::SpeedTestFast));
        }

        // Test DNS benchmark action (only if dns-bench is available)
        if is_command_installed("dns-bench") {
            assert!(actions.contains(&DiagnosticAction::DnsBenchmark));
        }

        // At least some actions should be available on most systems (ping is very common)
        // If no diagnostic tools are available, actions can be empty
        // This is acceptable behavior
    }

    #[test]
    fn test_format_interface_output() {
        let input = "1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN\n    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00\n    inet 127.0.0.1/8 scope host lo";
        let output = format_interface_output(input);

        assert!(output.contains("📡 1"));
        assert!(output.contains("🔗 link/loopback"));
        assert!(output.contains("🌐 inet 127.0.0.1/8"));
    }
}

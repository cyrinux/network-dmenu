//! Example demonstrating ML-enhanced network management with network-dmenu
//!
//! This example shows how to use the machine learning features to:
//! - Select optimal exit nodes based on performance predictions
//! - Diagnose network issues intelligently
//! - Personalize menu ordering based on usage patterns
//! - Predict best WiFi networks to connect to

use network_dmenu::{
    command::RealCommandRunner,
    ml_integration::{
        analyze_network_issues, get_performance_summary, get_personalized_menu_order,
        predict_best_exit_nodes, predict_best_wifi_network, record_exit_node_performance,
        record_user_action, record_wifi_performance,
    },
    tailscale::{get_mullvad_actions, TailscaleState},
};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logger
    env_logger::init();

    println!("🤖 Network-dmenu ML Integration Example\n");

    // Create command runner
    let command_runner = RealCommandRunner;

    // Example 1: ML-Enhanced Exit Node Selection
    println!("📍 Example 1: Intelligent Exit Node Selection");
    println!("{}", "=".repeat(50));

    // Get current Tailscale state
    let tailscale_state = TailscaleState::new(&command_runner);

    // Get all available peers
    let peers: Vec<_> = tailscale_state
        .status
        .peer
        .values()
        .filter(|p| p.exit_node_option)
        .cloned()
        .collect();

    println!("Found {} available exit nodes", peers.len());

    // Use ML to predict best exit nodes
    #[cfg(feature = "ml")]
    {
        let ml_predictions = predict_best_exit_nodes(&peers, 5);

        if !ml_predictions.is_empty() {
            println!("\n🎯 ML-Predicted Best Exit Nodes:");
            for (i, (node, score)) in ml_predictions.iter().enumerate() {
                println!("  {}. {} (Score: {:.2})", i + 1, node, score);
            }
        } else {
            println!(
                "ℹ️  ML predictions not available (low confidence), using traditional selection"
            );
        }
    }

    // Traditional selection for comparison
    let traditional_nodes = get_mullvad_actions(
        &tailscale_state,
        &command_runner,
        &[],
        Some(3),
        Some(1),
        None,
    );

    println!("\n📋 Traditional Selection (first 5):");
    for (i, node) in traditional_nodes.iter().take(5).enumerate() {
        println!("  {}. {}", i + 1, node);
    }

    // Simulate recording performance for learning
    #[cfg(feature = "ml")]
    {
        println!("\n📊 Recording performance metrics for learning...");
        record_exit_node_performance("us-nyc-wg-001.mullvad.ts.net", 25.5, 0.001);
        record_exit_node_performance("ca-tor-wg-002.mullvad.ts.net", 35.2, 0.002);
    }

    // Example 2: Intelligent Network Diagnostics
    println!("\n\n🔍 Example 2: Intelligent Network Diagnostics");
    println!("{}", "=".repeat(50));

    #[cfg(feature = "ml")]
    {
        // Simulate detecting network issues
        let symptoms = vec!["high_latency", "packet_loss"];
        println!("Detected symptoms: {:?}", symptoms);

        let (probable_cause, recommended_tests) = analyze_network_issues(symptoms);

        println!("\n🎯 ML Analysis:");
        println!("  Probable cause: {}", probable_cause);
        println!("  Recommended tests:");
        for test in recommended_tests {
            println!("    - {}", test);
        }
    }

    // Example 3: Personalized Menu Ordering
    println!("\n\n🎨 Example 3: Personalized Menu Ordering");
    println!("{}", "=".repeat(50));

    let menu_items = vec![
        "🌐 Connect to WiFi Network".to_string(),
        "🔒 Enable Tailscale VPN".to_string(),
        "🎧 Connect Bluetooth Device".to_string(),
        "📊 Run Network Diagnostics".to_string(),
        "🚀 Select Exit Node".to_string(),
        "✈️ Toggle Airplane Mode".to_string(),
    ];

    println!("Original menu order:");
    for (i, item) in menu_items.iter().enumerate() {
        println!("  {}. {}", i + 1, item);
    }

    #[cfg(feature = "ml")]
    {
        // Record some usage to train the model
        record_user_action("🔒 Enable Tailscale VPN");
        record_user_action("🚀 Select Exit Node");
        record_user_action("🔒 Enable Tailscale VPN");
        record_user_action("📊 Run Network Diagnostics");
        record_user_action("🚀 Select Exit Node");

        let personalized = get_personalized_menu_order(menu_items.clone());

        println!("\n🎯 ML-Personalized menu order:");
        for (i, item) in personalized.iter().enumerate() {
            println!("  {}. {}", i + 1, item);
        }
    }

    // Example 4: WiFi Network Prediction
    println!("\n\n📶 Example 4: WiFi Network Selection");
    println!("{}", "=".repeat(50));

    let available_networks = vec![
        ("HomeWiFi-5G".to_string(), -45, "WPA3".to_string()),
        ("HomeWiFi-2.4G".to_string(), -55, "WPA2".to_string()),
        ("Guest-Network".to_string(), -65, "WPA2".to_string()),
        ("PublicWiFi".to_string(), -70, "Open".to_string()),
    ];

    println!("Available networks:");
    for (ssid, signal, security) in &available_networks {
        println!(
            "  📶 {} (Signal: {}dBm, Security: {})",
            ssid, signal, security
        );
    }

    #[cfg(feature = "ml")]
    {
        if let Some(best_network) = predict_best_wifi_network(available_networks.clone()) {
            println!("\n🎯 ML recommends connecting to: {}", best_network);
        } else {
            println!("\nℹ️  No ML recommendation available");
        }

        // Record performance for learning
        record_wifi_performance("HomeWiFi-5G", 15.0, 250.0);
    }

    // Example 5: Performance Summary
    println!("\n\n📈 Example 5: Performance Summary");
    println!("{}", "=".repeat(50));

    #[cfg(feature = "ml")]
    {
        // Simulate recording more metrics
        for i in 0..10 {
            record_exit_node_performance(
                "us-nyc-wg-001.mullvad.ts.net",
                20.0 + (i as f32 * 2.0),
                0.001 * (1.0 + (i as f32 * 0.1)),
            );
        }

        if let Some(summary) = get_performance_summary("us-nyc-wg-001.mullvad.ts.net") {
            println!("{}", summary);
        } else {
            println!("No performance data available yet");
        }
    }

    // Feature flag notice
    #[cfg(not(feature = "ml"))]
    {
        println!("\n⚠️  Note: ML features are disabled. Build with --features=ml to enable.");
    }

    println!("\n✅ Example completed successfully!");

    Ok(())
}

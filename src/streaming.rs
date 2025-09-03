use crate::{
    ActionType, Args, Config, CustomAction, SystemAction, TailscaleAction,
    VpnAction, WifiAction,
    format_entry, ACTION_TYPE_SYSTEM, ICON_CROSS, ICON_SIGNAL,
};
use network_dmenu::{
    bluetooth::get_paired_bluetooth_devices,
    command::{CommandRunner, RealCommandRunner, is_command_installed},
    diagnostics,
    dns_cache,
    iwd::get_iwd_networks,
    networkmanager::{get_nm_vpn_networks, get_nm_wifi_networks},
    nextdns,
    rfkill,
    ssh,
    tailscale::{get_mullvad_actions, is_exit_node_active, is_tailscale_lock_enabled, get_locked_nodes, TailscaleState},
    tor,
    tailscale_prefs::parse_tailscale_prefs,
};
use std::error::Error;
use std::process::Stdio;
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;

/// Stream actions to dmenu as they become available for faster responsiveness
pub async fn select_action_from_menu_streaming(
    config: &Config,
    args: &Args,
    _command_runner: &impl CommandRunner,
    use_stdin: bool,
    use_stdout: bool,
) -> Result<(String, Vec<ActionType>), Box<dyn Error>> {
    let mut collected_actions = Vec::new();

    // Handle stdout mode - collect all actions first
    if use_stdout {
        #[allow(unused_mut)] // Needed for ML feature
        let mut actions = collect_all_actions(args.clone(), config.clone()).await?;

        // Apply ML personalization if enabled
        #[cfg(feature = "ml")]
        {
            let action_strings: Vec<String> = actions.iter()
                .map(crate::action_to_string)
                .collect();
            let personalized = network_dmenu::get_personalized_menu_order(action_strings);

            // Reorder actions based on personalized order
            let mut reordered = Vec::new();
            for action_str in personalized {
                if let Some(pos) = actions.iter().position(|a| crate::action_to_string(a) == action_str) {
                    reordered.push(actions.remove(pos));
                }
            }
            // Add any remaining actions that weren't in the personalized list
            reordered.extend(actions);
            actions = reordered;
        }
        for (i, action) in actions.iter().enumerate() {
            println!("{}: {}", i + 1, crate::action_to_string(action));
        }
        std::process::exit(0);
    }

    // Handle stdin mode - collect all actions first
    if use_stdin {
        #[allow(unused_mut)] // Needed for ML feature
        let mut actions = collect_all_actions(args.clone(), config.clone()).await?;

        // Apply ML personalization if enabled
        #[cfg(feature = "ml")]
        {
            let action_strings: Vec<String> = actions.iter()
                .map(crate::action_to_string)
                .collect();
            let personalized = network_dmenu::get_personalized_menu_order(action_strings);

            // Reorder actions based on personalized order
            let mut reordered = Vec::new();
            for action_str in personalized {
                if let Some(pos) = actions.iter().position(|a| crate::action_to_string(a) == action_str) {
                    reordered.push(actions.remove(pos));
                }
            }
            // Add any remaining actions that weren't in the personalized list
            reordered.extend(actions);
            actions = reordered;
        }
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let selected = line.trim().to_string();
        return Ok((selected, actions));
    }

    // Normal mode - stream to dmenu
    let (tx, mut rx) = mpsc::unbounded_channel::<ActionType>();

    // Spawn dmenu immediately using async process
    let dmenu_args: Vec<String> = config.dmenu_args.split_whitespace().map(|s| s.to_string()).collect();
    let mut child = tokio::process::Command::new(&config.dmenu_cmd)
        .args(&dmenu_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("Failed to open stdin");

    // Start producing actions in background
    let args_clone = args.clone();
    let config_clone = config.clone();

    // Start producer task
    let producer_handle = tokio::spawn(async move {
        stream_actions_simple(args_clone, config_clone, tx).await
    });

    // Collect all actions first for ML personalization
    let mut all_actions = Vec::new();
    while let Some(action) = rx.recv().await {
        all_actions.push(action);
    }

    // Apply ML personalization if enabled
    #[cfg(feature = "ml")]
    {
        let action_strings: Vec<String> = all_actions.iter()
            .map(crate::action_to_string)
            .collect();
        let personalized = network_dmenu::get_personalized_menu_order(action_strings);

        // Reorder actions based on personalized order
        let mut reordered = Vec::new();
        for action_str in personalized {
            if let Some(pos) = all_actions.iter().position(|a| crate::action_to_string(a) == action_str) {
                reordered.push(all_actions.remove(pos));
            }
        }
        // Add any remaining actions that weren't in the personalized list
        reordered.extend(all_actions);
        all_actions = reordered;
    }

    // Stream personalized actions to dmenu
    for action in all_actions {
        let action_string = crate::action_to_string(&action);
        if stdin.write_all(format!("{}\n", action_string).as_bytes()).await.is_err() {
            break; // dmenu closed
        }
        if stdin.flush().await.is_err() {
            break; // dmenu closed
        }
        collected_actions.push(action);
    }

    // Ensure producer finishes
    let _ = producer_handle.await;

    // Close stdin to signal we're done
    drop(stdin);

    // Wait for dmenu selection
    let output = child.wait_with_output().await?;
    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok((selected, collected_actions))
}

/// Simple streaming function that avoids Send issues
async fn stream_actions_simple(
    args: Args,
    config: Config,
    tx: mpsc::UnboundedSender<ActionType>,
) {
    // Send custom actions first
    for action in config.actions {
        let _ = tx.send(ActionType::Custom(action));
    }

    // Send system actions
    if !args.no_wifi && is_command_installed("nmcli") && is_command_installed("nm-connection-editor") {
        let _ = tx.send(ActionType::System(SystemAction::EditConnections));
    }

    // Run simple collection tasks
    let mut handles = vec![];

    // Bluetooth
    if !args.no_bluetooth && is_command_installed("bluetoothctl") {
        let tx_clone = tx.clone();
        handles.push(tokio::spawn(async move {
            send_bluetooth_actions(&tx_clone).await;
        }));
    }

    // VPN
    if !args.no_vpn && is_command_installed("nmcli") {
        let tx_clone = tx.clone();
        handles.push(tokio::spawn(async move {
            send_vpn_actions(&tx_clone).await;
        }));
    }

    // WiFi
    if !args.no_wifi {
        let tx_clone = tx.clone();
        let wifi_interface = args.wifi_interface.clone();
        handles.push(tokio::spawn(async move {
            send_wifi_actions(&tx_clone, &wifi_interface).await;
        }));
    }

    // Tailscale
    if !args.no_tailscale && is_command_installed("tailscale") {
        let tx_clone = tx.clone();
        let max_nodes_per_country = args.max_nodes_per_country.or(config.max_nodes_per_country);
        let max_nodes_per_city = args.max_nodes_per_city.or(config.max_nodes_per_city);
        let country_filter = args.country.clone().or(config.country_filter.clone());
        let exclude_exit_node = config.exclude_exit_node.clone();
        handles.push(tokio::spawn(async move {
            send_tailscale_actions_simple(&tx_clone, exclude_exit_node, max_nodes_per_country, max_nodes_per_city, country_filter).await;
        }));
    }

    // NextDNS
    if !args.no_nextdns {
        let tx_clone = tx.clone();
        let api_key = if !args.nextdns_api_key.is_empty() {
            Some(args.nextdns_api_key.clone())
        } else {
            config.nextdns_api_key.clone()
        }.map(|k| k.trim().to_string());

        debug!("NextDNS: Setting up with API key: {:?}",
               api_key.as_ref().map(|k| if k.len() > 4 { &k[0..4] } else { k }));
        debug!("NextDNS: Toggle profiles: {:?}", config.nextdns_toggle_profiles);

        let toggle_profiles = config.nextdns_toggle_profiles.clone();
        handles.push(tokio::spawn(async move {
            send_nextdns_actions(&tx_clone, api_key, toggle_profiles).await;
        }));
    }

    // Diagnostics
    if !args.no_diagnostics {
        let tx_clone = tx.clone();
        handles.push(tokio::spawn(async move {
            send_diagnostic_actions(&tx_clone);
        }));
    }

    // Rfkill
    if rfkill::is_rfkill_available() {
        let tx_clone = tx.clone();
        let no_wifi = args.no_wifi;
        let no_bluetooth = args.no_bluetooth;
        handles.push(tokio::spawn(async move {
            send_rfkill_actions(&tx_clone, no_wifi, no_bluetooth).await;
        }));
    }

    // SSH proxies
    let tx_clone = tx.clone();
    let ssh_proxies = config.ssh_proxies.clone();
    handles.push(tokio::spawn(async move {
        send_ssh_actions(&tx_clone, &ssh_proxies).await;
    }));

    // Tor proxies
    if !args.no_tor {
        let tx_clone = tx.clone();
        let torsocks_apps = config.torsocks_apps.clone();
        handles.push(tokio::spawn(async move {
            send_tor_actions(&tx_clone, &torsocks_apps).await;
        }));
    }

    // Wait for all tasks
    for handle in handles {
        let _ = handle.await;
    }
}

/// Collect all actions without streaming (fallback)
async fn collect_all_actions(
    args: Args,
    config: Config,
) -> Result<Vec<ActionType>, Box<dyn Error>> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let command_runner = RealCommandRunner;

    // Produce actions directly
    let _ = produce_actions_streaming(&args, &config, &command_runner, tx).await;

    let mut actions = Vec::new();
    while let Some(action) = rx.recv().await {
        actions.push(action);
    }

    Ok(actions)
}

/// Produce actions and send them through the channel as they become available
async fn produce_actions_streaming(
    args: &Args,
    config: &Config,
    command_runner: &impl CommandRunner,
    tx: mpsc::UnboundedSender<ActionType>,
) -> Result<(), Box<dyn Error>> {
    // Send custom actions immediately - these are already available
    send_custom_actions(config, command_runner, &tx).await?;

    // Send system actions
    if !args.no_wifi && is_command_installed("nmcli") && is_command_installed("nm-connection-editor") {
        let _ = tx.send(ActionType::System(SystemAction::EditConnections));
    }

    // Start parallel tasks for slower operations
    let mut tasks = vec![];

    // Bluetooth devices (usually fast)
    if !args.no_bluetooth && is_command_installed("bluetoothctl") {
        let tx_clone = tx.clone();
        tasks.push(tokio::spawn(async move {
            send_bluetooth_actions(&tx_clone).await;
        }));
    }

    // VPN networks (usually fast)
    if !args.no_vpn && is_command_installed("nmcli") {
        let tx_clone = tx.clone();
        tasks.push(tokio::spawn(async move {
            send_vpn_actions(&tx_clone).await;
        }));
    }

    // WiFi networks (can be slow)
    if !args.no_wifi {
        let tx_clone = tx.clone();
        let wifi_interface = args.wifi_interface.clone();
        tasks.push(tokio::spawn(async move {
            send_wifi_actions(&tx_clone, &wifi_interface).await;
        }));
    }

    // Tailscale (can be slow)
    // Handle Tailscale
    if !args.no_tailscale && is_command_installed("tailscale") {
        let tx_clone = tx.clone();
        let max_nodes_per_country = args.max_nodes_per_country.or(config.max_nodes_per_country);
        let max_nodes_per_city = args.max_nodes_per_city.or(config.max_nodes_per_city);
        let country_filter = args.country.clone().or(config.country_filter.clone());
        let exclude_exit_node = config.exclude_exit_node.clone();

        tasks.push(tokio::spawn(async move {
            send_tailscale_actions_simple(&tx_clone, exclude_exit_node, max_nodes_per_country, max_nodes_per_city, country_filter).await;
        }));
    }

    // NextDNS
    if !args.no_nextdns && is_command_installed("nextdns") {
        let tx_clone = tx.clone();
        let api_key = if !args.nextdns_api_key.is_empty() {
            Some(args.nextdns_api_key.clone())
        } else {
            config.nextdns_api_key.clone()
        }.map(|k| k.trim().to_string());
        let toggle_profiles = config.nextdns_toggle_profiles.clone();
        tasks.push(tokio::spawn(async move {
            send_nextdns_actions(&tx_clone, api_key, toggle_profiles).await;
        }));
    }

    // Diagnostics
    if !args.no_diagnostics {
        let tx_clone = tx.clone();
        tasks.push(tokio::spawn(async move {
            send_diagnostic_actions(&tx_clone);
        }));
    }

    // Rfkill actions
    if rfkill::is_rfkill_available() {
        let tx_clone = tx.clone();
        let no_wifi = args.no_wifi;
        let no_bluetooth = args.no_bluetooth;
        tasks.push(tokio::spawn(async move {
            send_rfkill_actions(&tx_clone, no_wifi, no_bluetooth).await;
        }));
    }

    // SSH proxies
    let tx_clone = tx.clone();
    let ssh_proxies = config.ssh_proxies.clone();
    tasks.push(tokio::spawn(async move {
        send_ssh_actions(&tx_clone, &ssh_proxies).await;
    }));

    // Wait for all tasks to complete
    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

async fn send_custom_actions(
    config: &Config,
    command_runner: &impl CommandRunner,
    tx: &mpsc::UnboundedSender<ActionType>,
) -> Result<(), Box<dyn Error>> {
    let mut custom_actions = config.actions.clone();

    // Add DNS cache actions if available
    if config.use_dns_cache {
        match dns_cache::DnsCacheStorage::load() {
            Ok(cache_storage) => {
                if let Ok(network_id) = dns_cache::get_current_network_id(command_runner).await {
                    if let Some(dns_cache) = cache_storage.get_cache(&network_id) {
                        let dns_actions = dns_cache::generate_dns_actions_from_cache(dns_cache);
                        for dns_action in dns_actions.into_iter().rev() {
                            custom_actions.insert(
                                0,
                                CustomAction {
                                    display: dns_action.display,
                                    cmd: dns_action.cmd,
                                },
                            );
                        }
                    }
                }
            }
            Err(_) => {
                // Ignore DNS cache errors
            }
        }
    }

    // Send all custom actions
    for custom_action in custom_actions {
        let _ = tx.send(ActionType::Custom(custom_action));
    }

    Ok(())
}

async fn send_bluetooth_actions(tx: &mpsc::UnboundedSender<ActionType>) {
    let command_runner = RealCommandRunner;

    if let Ok(devices) = get_paired_bluetooth_devices(&command_runner) {
        for device in devices {
            let _ = tx.send(ActionType::Bluetooth(device));
        }
    }
}

async fn send_vpn_actions(tx: &mpsc::UnboundedSender<ActionType>) {
    let command_runner = RealCommandRunner;

    if let Ok(actions) = get_nm_vpn_networks(&command_runner) {
        for action in actions {
            // Convert library VpnAction to main VpnAction
            let main_action = match action {
                network_dmenu::VpnAction::Connect(name) => VpnAction::Connect(name),
                network_dmenu::VpnAction::Disconnect(name) => VpnAction::Disconnect(name),
            };
            let _ = tx.send(ActionType::Vpn(main_action));
        }
    }
}

async fn send_wifi_actions(tx: &mpsc::UnboundedSender<ActionType>, wifi_interface: &str) {
    let command_runner = RealCommandRunner;

    if is_command_installed("nmcli") {
        if let Ok(actions) = get_nm_wifi_networks(&command_runner) {
            for action in actions {
                // Convert library WifiAction to main WifiAction
                let main_action = match action {
                    network_dmenu::WifiAction::Connect => WifiAction::Connect,
                    network_dmenu::WifiAction::ConnectHidden => WifiAction::ConnectHidden,
                    network_dmenu::WifiAction::Disconnect => WifiAction::Disconnect,
                    network_dmenu::WifiAction::Network(name) => WifiAction::Network(name),
                };
                let _ = tx.send(ActionType::Wifi(main_action));
            }
        }
    } else if is_command_installed("iwctl") {
        if let Ok(actions) = get_iwd_networks(wifi_interface, &command_runner) {
            for action in actions {
                // Convert library WifiAction to main WifiAction
                let main_action = match action {
                    network_dmenu::WifiAction::Connect => WifiAction::Connect,
                    network_dmenu::WifiAction::ConnectHidden => WifiAction::ConnectHidden,
                    network_dmenu::WifiAction::Disconnect => WifiAction::Disconnect,
                    network_dmenu::WifiAction::Network(name) => WifiAction::Network(name),
                };
                let _ = tx.send(ActionType::Wifi(main_action));
            }
        }
    }
}

// Simplified tailscale action sender
async fn send_tailscale_actions_simple(
    tx: &mpsc::UnboundedSender<ActionType>,
    exclude_exit_node: Vec<String>,
    max_nodes_per_country: Option<i32>,
    max_nodes_per_city: Option<i32>,
    country_filter: Option<String>,
) {
    let command_runner = RealCommandRunner;

    // Get Tailscale preferences
    if let Some(prefs) = parse_tailscale_prefs(&command_runner) {
        // Send basic Tailscale actions first (these are simple and fast)
        let _ = tx.send(ActionType::Tailscale(TailscaleAction::SetShields(!prefs.ShieldsUp)));
        let _ = tx.send(ActionType::Tailscale(TailscaleAction::SetAllowLanAccess(!prefs.ExitNodeAllowLANAccess)));
        let _ = tx.send(ActionType::Tailscale(TailscaleAction::SetAcceptRoutes(!prefs.RouteAll)));
        let _ = tx.send(ActionType::Tailscale(TailscaleAction::ShowLockStatus));

        // Create TailscaleState to get exit node information
        let tailscale_state = TailscaleState::new(&command_runner);

        // Get and send Mullvad/exit node actions
        let mullvad_actions = get_mullvad_actions(
            &tailscale_state,
            &command_runner,
            &exclude_exit_node,
            max_nodes_per_country,
            max_nodes_per_city,
            country_filter.as_deref(),
        );

        for action_str in mullvad_actions {
            let _ = tx.send(ActionType::Tailscale(TailscaleAction::SetExitNode(action_str)));
        }

        if is_exit_node_active(&tailscale_state) {
            let _ = tx.send(ActionType::Tailscale(TailscaleAction::DisableExitNode));
        }

        let _ = tx.send(ActionType::Tailscale(TailscaleAction::SetEnable(!prefs.WantRunning)));

        // Add Tailscale Lock actions if enabled
        if is_tailscale_lock_enabled(&command_runner, Some(&tailscale_state)).unwrap_or(false) {
            let _ = tx.send(ActionType::Tailscale(TailscaleAction::ListLockedNodes));

            // Add sign all nodes action and individual node actions
            if let Ok(locked_nodes) = get_locked_nodes(&command_runner, Some(&tailscale_state)) {
                if !locked_nodes.is_empty() {
                    let _ = tx.send(ActionType::Tailscale(TailscaleAction::SignAllNodes));

                    for node in locked_nodes {
                        let _ = tx.send(ActionType::Tailscale(TailscaleAction::SignLockedNode(node.node_key)));
                    }
                }
            }
        }
    }
}

async fn send_nextdns_actions(
    tx: &mpsc::UnboundedSender<ActionType>,
    api_key: Option<String>,
    toggle_profiles: Option<(String, String)>,
) {
    debug!("NextDNS: Preparing to send actions with API key: {:?}, toggle profiles: {:?}",
           api_key.as_ref().map(|k| if k.len() > 4 { &k[0..4] } else { k }),
           toggle_profiles);

    // Convert toggle_profiles tuple to the format needed by get_nextdns_actions
    let toggle_tuple = toggle_profiles.as_ref().map(|(a, b)| (a.as_str(), b.as_str()));

    // Get NextDNS actions
    match nextdns::get_nextdns_actions(api_key.as_deref(), toggle_tuple).await {
        Ok(actions) => {
            debug!("NextDNS: Successfully got {} actions", actions.len());
            for action in actions {
                debug!("NextDNS: Sending action: {:?}", action);
                let _ = tx.send(ActionType::NextDns(action));
            }
        }
        Err(e) => {
            error!("NextDNS: Failed to get actions: {}", e);
        }
    }
}

fn send_diagnostic_actions(tx: &mpsc::UnboundedSender<ActionType>) {
    for action in diagnostics::get_diagnostic_actions() {
        let _ = tx.send(ActionType::Diagnostic(action));
    }
}

async fn send_ssh_actions(
    tx: &mpsc::UnboundedSender<ActionType>,
    ssh_proxies: &std::collections::HashMap<String, network_dmenu::SshProxyConfig>,
) {
    if is_command_installed("ssh") {
        let actions = network_dmenu::get_ssh_proxy_actions(ssh_proxies);
        for action in actions {
            let _ = tx.send(ActionType::Ssh(action));
        }
    }
}

async fn send_rfkill_actions(
    tx: &mpsc::UnboundedSender<ActionType>,
    no_wifi: bool,
    no_bluetooth: bool,
) {
    // Check if all devices are blocked (airplane mode is on)
    // Use get_device_type_summary to efficiently check device status
    let device_summary = rfkill::get_device_type_summary().await.unwrap_or_default();

    if !device_summary.is_empty() {
        // Check if all radio devices are blocked
        let radio_types = ["wlan", "bluetooth", "wwan", "fm", "nfc", "gps"];
        let radio_devices: Vec<_> = device_summary
            .iter()
            .filter(|(device_type, _)| radio_types.contains(&device_type.as_str()))
            .collect();

        if !radio_devices.is_empty() {
            // Check if all radio devices are blocked
            // (blocked_count, unblocked_count)
            let all_blocked = radio_devices
                .iter()
                .all(|(_, (blocked, unblocked))| *blocked > 0 && *unblocked == 0);

            if all_blocked {
                // All devices are blocked, offer to disable airplane mode
                let _ = tx.send(ActionType::System(SystemAction::AirplaneMode(false)));
            } else {
                // Not all devices are blocked, offer to enable airplane mode
                let _ = tx.send(ActionType::System(SystemAction::AirplaneMode(true)));
            }
        }
    }

    // Add WiFi rfkill actions
    if !no_wifi {
        if let Ok(devices) = rfkill::get_rfkill_devices_by_type("wlan").await {
            for device in devices {
                let device_display = format!("{} ({})", device.device_type_display(), device.device);
                if device.is_unblocked() {
                    let display_text = format_entry(
                        ACTION_TYPE_SYSTEM,
                        ICON_CROSS,
                        &format!("Turn OFF {}", device_display),
                    );
                    let _ = tx.send(ActionType::System(SystemAction::RfkillBlock(
                        device.id.to_string(),
                        display_text,
                    )));
                } else {
                    let display_text = format_entry(
                        ACTION_TYPE_SYSTEM,
                        ICON_SIGNAL,
                        &format!("Turn ON {}", device_display),
                    );
                    let _ = tx.send(ActionType::System(SystemAction::RfkillUnblock(
                        device.id.to_string(),
                        display_text,
                    )));
                }
            }
        }
    }

    // Add Bluetooth rfkill actions
    if !no_bluetooth {
        if let Ok(devices) = rfkill::get_rfkill_devices_by_type("bluetooth").await {
            for device in devices {
                let device_display = format!("{} ({})", device.device_type_display(), device.device);
                if device.is_unblocked() {
                    let display_text = format_entry(
                        ACTION_TYPE_SYSTEM,
                        ICON_CROSS,
                        &format!("Turn OFF {}", device_display),
                    );
                    let _ = tx.send(ActionType::System(SystemAction::RfkillBlock(
                        device.id.to_string(),
                        display_text,
                    )));
                } else {
                    let display_text = format_entry(
                        ACTION_TYPE_SYSTEM,
                        ICON_SIGNAL,
                        &format!("Turn ON {}", device_display),
                    );
                    let _ = tx.send(ActionType::System(SystemAction::RfkillUnblock(
                        device.id.to_string(),
                        display_text,
                    )));
                }
            }
        }
    }
}

/// Send Tor proxy actions  
async fn send_tor_actions(
    tx: &mpsc::UnboundedSender<ActionType>,
    torsocks_apps: &std::collections::HashMap<String, network_dmenu::TorsocksConfig>,
) {
    // Only show Tor daemon actions if tor command is available
    if is_command_installed("tor") {
        let actions = tor::get_tor_actions(torsocks_apps);
        for action in actions {
            let _ = tx.send(ActionType::Tor(action));
        }
    }
}

use super::*;
use crate::command::{CommandRunner, RealCommandRunner};
use crate::constants::{commands, nmcli_commands};
use crate::errors::{NetworkMenuError, Result};
use crate::parsers::vpn::parse_networkmanager_vpn;
use crate::types::{ActionContext, VpnConnection};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// VPN service for managing VPN connections
#[derive(Debug)]
pub struct VpnService {
    command_runner: Box<dyn CommandRunner>,
}

impl VpnService {
    pub fn new() -> Self {
        Self {
            command_runner: Box::new(RealCommandRunner::new()),
        }
    }
    
    pub fn with_command_runner(mut self, runner: Box<dyn CommandRunner>) -> Self {
        self.command_runner = runner;
        self
    }
    
    async fn get_vpn_connections(&self) -> Result<Vec<VpnConnection>> {
        debug!("Getting VPN connections from NetworkManager");
        
        let output = self.command_runner
            .run_command(commands::NMCLI, nmcli_commands::VPN_LIST)
            .await?;
        
        if !output.status.success() {
            return Err(NetworkMenuError::command_failed(
                "nmcli connection show",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_networkmanager_vpn(&stdout)
    }
}

impl Default for VpnService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkService for VpnService {
    async fn get_actions(&self, _context: &ActionContext) -> Result<Vec<ActionType>> {
        let mut actions = Vec::new();
        
        match self.get_vpn_connections().await {
            Ok(connections) => {
                for connection in connections {
                    if connection.is_active {
                        actions.push(ActionType::Vpn(VpnAction::Disconnect(connection.name.clone())));
                    } else {
                        actions.push(ActionType::Vpn(VpnAction::Connect(connection.name.clone())));
                    }
                }
            }
            Err(e) => {
                warn!("Failed to get VPN connections: {}", e);
            }
        }
        
        debug!("VPN service generated {} actions", actions.len());
        Ok(actions)
    }
    
    async fn is_available(&self) -> bool {
        self.command_runner.is_command_available(commands::NMCLI).await
    }
    
    fn service_name(&self) -> &'static str {
        "VPN"
    }
    
    async fn initialize(&mut self) -> Result<()> {
        info!("VPN service initialized");
        Ok(())
    }
}
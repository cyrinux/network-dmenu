use super::*;
use crate::command::{CommandRunner, RealCommandRunner};
use crate::constants::{commands, tailscale_commands};
use crate::errors::{NetworkMenuError, Result};
use crate::types::ActionContext;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Tailscale service for managing Tailscale VPN connections
#[derive(Debug)]
pub struct TailscaleService {
    command_runner: Box<dyn CommandRunner>,
}

impl TailscaleService {
    pub fn new() -> Self {
        Self {
            command_runner: Box::new(RealCommandRunner::new()),
        }
    }
    
    pub fn with_command_runner(mut self, runner: Box<dyn CommandRunner>) -> Self {
        self.command_runner = runner;
        self
    }
    
    async fn is_enabled(&self) -> Result<bool> {
        let output = self.command_runner
            .run_command(commands::TAILSCALE, tailscale_commands::STATUS)
            .await?;
        
        if !output.status.success() {
            return Ok(false);
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(!stdout.contains("Tailscale is stopped"))
    }
    
    async fn get_exit_nodes(&self) -> Result<Vec<String>> {
        let output = self.command_runner
            .run_command(commands::TAILSCALE, tailscale_commands::EXIT_NODE_LIST)
            .await?;
        
        if !output.status.success() {
            return Ok(Vec::new());
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let nodes: Vec<String> = stdout
            .lines()
            .filter(|line| line.contains("ts.net"))
            .map(|line| line.trim().to_string())
            .collect();
        
        Ok(nodes)
    }
    
    async fn has_active_exit_node(&self) -> Result<bool> {
        let output = self.command_runner
            .run_command(commands::TAILSCALE, tailscale_commands::STATUS)
            .await?;
        
        if !output.status.success() {
            return Ok(false);
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains("exit node:"))
    }
}

impl Default for TailscaleService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkService for TailscaleService {
    async fn get_actions(&self, context: &ActionContext) -> Result<Vec<ActionType>> {
        let mut actions = Vec::new();
        
        // Check if Tailscale is enabled
        match self.is_enabled().await {
            Ok(enabled) => {
                if enabled {
                    actions.push(ActionType::Tailscale(TailscaleAction::Disable));
                    actions.push(ActionType::Tailscale(TailscaleAction::SetShields(true)));
                    actions.push(ActionType::Tailscale(TailscaleAction::SetShields(false)));
                    
                    // Add exit node options
                    if let Ok(has_exit_node) = self.has_active_exit_node().await {
                        if has_exit_node {
                            actions.push(ActionType::Tailscale(TailscaleAction::DisableExitNode));
                        }
                    }
                    
                    // Add available exit nodes (filtered by exclusion list)
                    if let Ok(nodes) = self.get_exit_nodes().await {
                        for node in nodes {
                            let should_exclude = context.config.exclude_exit_node
                                .iter()
                                .any(|excluded| node.contains(excluded));
                            
                            if !should_exclude {
                                actions.push(ActionType::Tailscale(TailscaleAction::SetExitNode(
                                    Arc::from(node.as_str())
                                )));
                            }
                        }
                    }
                } else {
                    actions.push(ActionType::Tailscale(TailscaleAction::Enable));
                }
            }
            Err(e) => {
                warn!("Failed to check Tailscale status: {}", e);
            }
        }
        
        debug!("Tailscale service generated {} actions", actions.len());
        Ok(actions)
    }
    
    async fn is_available(&self) -> bool {
        self.command_runner.is_command_available(commands::TAILSCALE).await
    }
    
    fn service_name(&self) -> &'static str {
        "Tailscale"
    }
    
    async fn initialize(&mut self) -> Result<()> {
        info!("Tailscale service initialized");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::MockCommandRunner;
    use crate::types::Config;
    use std::process::{ExitStatus, Output};
    
    fn create_mock_output(success: bool, stdout: &str, stderr: &str) -> Output {
        Output {
            status: if success {
                ExitStatus::from_raw(0)
            } else {
                ExitStatus::from_raw(1)
            },
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }
    
    #[tokio::test]
    async fn test_tailscale_service() {
        let status_output = create_mock_output(true, "Tailscale is running", "");
        let exit_nodes_output = create_mock_output(true, "node1.ts.net\nnode2.ts.net", "");
        
        let runner = MockCommandRunner::new()
            .with_response("tailscale", tailscale_commands::STATUS, status_output)
            .with_response("tailscale", tailscale_commands::EXIT_NODE_LIST, exit_nodes_output)
            .with_available_command("tailscale");
        
        let service = TailscaleService::new()
            .with_command_runner(Box::new(runner));
        
        assert!(service.is_available().await);
        
        let context = ActionContext::new(Config::default());
        let actions = service.get_actions(&context).await.unwrap();
        
        // Should have disable action since Tailscale is running
        assert!(!actions.is_empty());
        let has_disable_action = actions.iter().any(|action| {
            matches!(action, ActionType::Tailscale(TailscaleAction::Disable))
        });
        assert!(has_disable_action);
    }
}
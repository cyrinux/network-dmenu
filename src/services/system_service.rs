use super::*;
use crate::command::{CommandRunner, RealCommandRunner};
use crate::constants::{commands, rfkill_commands};
use crate::errors::Result;
use crate::types::ActionContext;
use async_trait::async_trait;
use tracing::{debug, info, warn};

/// System service for managing system-level network operations
#[derive(Debug)]
pub struct SystemService {
    command_runner: Box<dyn CommandRunner>,
}

impl SystemService {
    pub fn new() -> Self {
        Self {
            command_runner: Box::new(RealCommandRunner::new()),
        }
    }
    
    pub fn with_command_runner(mut self, runner: Box<dyn CommandRunner>) -> Self {
        self.command_runner = runner;
        self
    }
    
    async fn has_nm_connection_editor(&self) -> bool {
        self.command_runner.is_command_available(commands::NM_CONNECTION_EDITOR).await
    }
    
    async fn has_rfkill(&self) -> bool {
        self.command_runner.is_command_available(commands::RFKILL).await
    }
}

impl Default for SystemService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkService for SystemService {
    async fn get_actions(&self, _context: &ActionContext) -> Result<Vec<ActionType>> {
        let mut actions = Vec::new();
        
        // Add network connection editor if available
        if self.has_nm_connection_editor().await {
            actions.push(ActionType::System(SystemAction::EditConnections));
        }
        
        // Add rfkill actions if available
        if self.has_rfkill().await {
            actions.push(ActionType::System(SystemAction::RfkillBlock));
            actions.push(ActionType::System(SystemAction::RfkillUnblock));
        }
        
        // Add network manager restart action
        actions.push(ActionType::System(SystemAction::RestartNetworkManager));
        actions.push(ActionType::System(SystemAction::ShowNetworks));
        
        debug!("System service generated {} actions", actions.len());
        Ok(actions)
    }
    
    async fn is_available(&self) -> bool {
        // System service is always available as it provides basic system functions
        true
    }
    
    fn service_name(&self) -> &'static str {
        "System"
    }
    
    async fn initialize(&mut self) -> Result<()> {
        info!("System service initialized");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::MockCommandRunner;
    use crate::types::Config;
    
    #[tokio::test]
    async fn test_system_service() {
        let runner = MockCommandRunner::new()
            .with_available_command(commands::NM_CONNECTION_EDITOR)
            .with_available_command(commands::RFKILL);
        
        let service = SystemService::new()
            .with_command_runner(Box::new(runner));
        
        assert!(service.is_available().await);
        
        let context = ActionContext::new(Config::default());
        let actions = service.get_actions(&context).await.unwrap();
        
        // Should have at least system actions
        assert!(!actions.is_empty());
        
        let has_edit_connections = actions.iter().any(|action| {
            matches!(action, ActionType::System(SystemAction::EditConnections))
        });
        assert!(has_edit_connections);
        
        let has_rfkill_block = actions.iter().any(|action| {
            matches!(action, ActionType::System(SystemAction::RfkillBlock))
        });
        assert!(has_rfkill_block);
    }
    
    #[tokio::test]
    async fn test_system_service_limited_tools() {
        let runner = MockCommandRunner::new();
        
        let service = SystemService::new()
            .with_command_runner(Box::new(runner));
        
        let context = ActionContext::new(Config::default());
        let actions = service.get_actions(&context).await.unwrap();
        
        // Should still have basic system actions even without external tools
        assert!(!actions.is_empty());
        
        let has_restart_nm = actions.iter().any(|action| {
            matches!(action, ActionType::System(SystemAction::RestartNetworkManager))
        });
        assert!(has_restart_nm);
    }
}
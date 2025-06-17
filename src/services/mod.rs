use crate::errors::Result;
use crate::types::{ActionContext, Config};
use async_trait::async_trait;
use std::sync::Arc;

pub mod wifi_service;
pub mod vpn_service;
pub mod tailscale_service;
pub mod bluetooth_service;
pub mod system_service;

/// Core trait for network services
#[async_trait]
pub trait NetworkService: Send + Sync + ServiceDowncast {
    /// Get available actions for this service
    async fn get_actions(&self, context: &ActionContext) -> Result<Vec<ActionType>>;
    
    /// Check if this service is available on the system
    async fn is_available(&self) -> bool;
    
    /// Get the service name for logging/debugging
    fn service_name(&self) -> &'static str;
    
    /// Initialize the service (optional)
    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
    
    /// Cleanup the service (optional)
    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Action types that can be performed
#[derive(Debug, Clone)]
pub enum ActionType {
    Wifi(WifiAction),
    Vpn(VpnAction),
    Tailscale(TailscaleAction),
    Bluetooth(BluetoothAction),
    System(SystemAction),
    Custom(CustomAction),
}

#[derive(Debug, Clone)]
pub enum WifiAction {
    Connect(Arc<str>),
    ConnectHidden,
    Disconnect,
    Scan,
    ForgetNetwork(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum VpnAction {
    Connect(Arc<str>),
    Disconnect(Arc<str>),
    Toggle(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum TailscaleAction {
    Enable,
    Disable,
    SetExitNode(Arc<str>),
    DisableExitNode,
    SetShields(bool),
    Netcheck,
}

#[derive(Debug, Clone)]
pub enum BluetoothAction {
    Connect(Arc<str>),
    Disconnect(Arc<str>),
    Pair(Arc<str>),
    Unpair(Arc<str>),
    Trust(Arc<str>),
    Scan,
}

#[derive(Debug, Clone)]
pub enum SystemAction {
    EditConnections,
    RfkillBlock,
    RfkillUnblock,
    RestartNetworkManager,
    ShowNetworks,
}

#[derive(Debug, Clone)]
pub struct CustomAction {
    pub display: String,
    pub command: String,
    pub args: Vec<String>,
    pub confirm: bool,
}

/// Service manager that coordinates all network services
#[derive(Debug)]
pub struct NetworkServiceManager {
    services: Vec<Box<dyn NetworkService>>,
    initialized: bool,
}

impl NetworkServiceManager {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            initialized: false,
        }
    }
    
    /// Add a service to the manager
    pub fn add_service(mut self, service: Box<dyn NetworkService>) -> Self {
        self.services.push(service);
        self
    }
    
    /// Initialize all services
    pub async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }
        
        for service in &mut self.services {
            if let Err(e) = service.initialize().await {
                tracing::warn!(
                    "Failed to initialize service {}: {}", 
                    service.service_name(), 
                    e
                );
            }
        }
        
        self.initialized = true;
        Ok(())
    }
    
    /// Get all available actions from all services
    pub async fn get_all_actions(&self, context: &ActionContext) -> Result<Vec<ActionType>> {
        let mut all_actions = Vec::new();
        
        // Add custom actions first
        for custom_action in &context.config.actions {
            all_actions.push(ActionType::Custom(CustomAction {
                display: custom_action.display.clone(),
                command: custom_action.cmd.clone(),
                args: custom_action.args.clone(),
                confirm: custom_action.confirm,
            }));
        }
        
        // Get actions from all available services
        for service in &self.services {
            if service.is_available().await {
                match service.get_actions(context).await {
                    Ok(mut actions) => all_actions.append(&mut actions),
                    Err(e) => tracing::warn!(
                        "Failed to get actions from service {}: {}", 
                        service.service_name(), 
                        e
                    ),
                }
            }
        }
        
        Ok(all_actions)
    }
    
    /// Get actions from a specific service type
    pub async fn get_service_actions<T>(&self, context: &ActionContext) -> Result<Vec<ActionType>>
    where
        T: NetworkService + 'static,
    {
        for service in &self.services {
            if service.as_any().is::<T>() && service.is_available().await {
                return service.get_actions(context).await;
            }
        }
        Ok(Vec::new())
    }
    
    /// Cleanup all services
    pub async fn cleanup(&mut self) -> Result<()> {
        for service in &mut self.services {
            if let Err(e) = service.cleanup().await {
                tracing::warn!(
                    "Failed to cleanup service {}: {}", 
                    service.service_name(), 
                    e
                );
            }
        }
        Ok(())
    }
}

impl Default for NetworkServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper trait for downcasting services
pub trait ServiceDowncast {
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: NetworkService + 'static> ServiceDowncast for T {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Factory for creating a fully configured service manager
pub struct ServiceManagerFactory;

impl ServiceManagerFactory {
    pub async fn create_default() -> Result<NetworkServiceManager> {
        let mut manager = NetworkServiceManager::new()
            .add_service(Box::new(wifi_service::WifiService::new()))
            .add_service(Box::new(vpn_service::VpnService::new()))
            .add_service(Box::new(tailscale_service::TailscaleService::new()))
            .add_service(Box::new(bluetooth_service::BluetoothService::new()))
            .add_service(Box::new(system_service::SystemService::new()));
        
        manager.initialize().await?;
        Ok(manager)
    }
    
    pub async fn create_with_config(config: &Config) -> Result<NetworkServiceManager> {
        let mut manager = NetworkServiceManager::new();
        
        // Add services based on configuration
        if config.wifi.enabled {
            manager = manager.add_service(Box::new(wifi_service::WifiService::new()));
        }
        
        if config.vpn.enabled {
            manager = manager.add_service(Box::new(vpn_service::VpnService::new()));
        }
        
        if config.tailscale.enabled {
            manager = manager.add_service(Box::new(tailscale_service::TailscaleService::new()));
        }
        
        if config.bluetooth.enabled {
            manager = manager.add_service(Box::new(bluetooth_service::BluetoothService::new()));
        }
        
        manager = manager.add_service(Box::new(system_service::SystemService::new()));
        
        manager.initialize().await?;
        Ok(manager)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ActionContext;
    
    struct MockService {
        name: &'static str,
        available: bool,
        actions: Vec<ActionType>,
    }
    
    impl MockService {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                available: true,
                actions: Vec::new(),
            }
        }
        
        fn with_availability(mut self, available: bool) -> Self {
            self.available = available;
            self
        }
        
        fn with_actions(mut self, actions: Vec<ActionType>) -> Self {
            self.actions = actions;
            self
        }
    }
    
    #[async_trait]
    impl NetworkService for MockService {
        async fn get_actions(&self, _context: &ActionContext) -> Result<Vec<ActionType>> {
            Ok(self.actions.clone())
        }
        
        async fn is_available(&self) -> bool {
            self.available
        }
        
        fn service_name(&self) -> &'static str {
            self.name
        }
    }
    
    #[tokio::test]
    async fn test_service_manager() {
        let mock_actions = vec![
            ActionType::System(SystemAction::EditConnections),
        ];
        
        let service = MockService::new("test")
            .with_actions(mock_actions);
        
        let mut manager = NetworkServiceManager::new()
            .add_service(Box::new(service));
        
        manager.initialize().await.unwrap();
        
        let context = ActionContext::new(Config::default());
        let actions = manager.get_all_actions(&context).await.unwrap();
        
        assert!(!actions.is_empty());
    }
    
    #[tokio::test]
    async fn test_unavailable_service() {
        let service = MockService::new("unavailable")
            .with_availability(false);
        
        let mut manager = NetworkServiceManager::new()
            .add_service(Box::new(service));
        
        manager.initialize().await.unwrap();
        
        let context = ActionContext::new(Config::default());
        let actions = manager.get_all_actions(&context).await.unwrap();
        
        // Should only have custom actions from config, no service actions
        assert_eq!(actions.len(), 0);
    }
}
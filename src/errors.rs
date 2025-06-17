use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum NetworkMenuError {
    #[error("Command execution failed: {command} - {message}")]
    CommandFailed { command: String, message: String },
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Network operation failed: {0}")]
    NetworkError(String),
    
    #[error("Parsing error: {0}")]
    ParseError(String),
    
    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: String },
    
    #[error("Action execution failed: {action} - {reason}")]
    ActionExecutionFailed { action: String, reason: String },
    
    #[error("Menu selection failed: {0}")]
    MenuSelectionFailed(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Timeout occurred: {operation}")]
    TimeoutError { operation: String },
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),
    
    #[error("TOML parsing error: {0}")]
    TomlError(#[from] toml::de::Error),
    
    #[error("Regex error: {0}")]
    RegexError(#[from] regex::Error),
    
    #[error("HTTP request error: {0}")]
    RequestError(#[from] reqwest::Error),
    
    #[error("Notification error: {0}")]
    NotificationError(#[from] notify_rust::error::Error),
    
    #[error("Join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
    
    #[error("Elapsed error: {0}")]
    ElapsedError(#[from] tokio::time::error::Elapsed),
}

pub type Result<T> = std::result::Result<T, NetworkMenuError>;

impl NetworkMenuError {
    pub fn command_failed(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::CommandFailed {
            command: command.into(),
            message: message.into(),
        }
    }

    pub fn config_error(message: impl Into<String>) -> Self {
        Self::ConfigError(message.into())
    }

    pub fn network_error(message: impl Into<String>) -> Self {
        Self::NetworkError(message.into())
    }

    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::ParseError(message.into())
    }

    pub fn service_unavailable(service: impl Into<String>) -> Self {
        Self::ServiceUnavailable {
            service: service.into(),
        }
    }

    pub fn action_execution_failed(action: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ActionExecutionFailed {
            action: action.into(),
            reason: reason.into(),
        }
    }

    pub fn menu_selection_failed(message: impl Into<String>) -> Self {
        Self::MenuSelectionFailed(message.into())
    }

    pub fn validation_error(message: impl Into<String>) -> Self {
        Self::ValidationError(message.into())
    }

    pub fn timeout_error(operation: impl Into<String>) -> Self {
        Self::TimeoutError {
            operation: operation.into(),
        }
    }
}

// Helper trait for converting std::error::Error to NetworkMenuError
pub trait IntoNetworkMenuError<T> {
    fn into_network_menu_error(self) -> Result<T>;
}

impl<T, E> IntoNetworkMenuError<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn into_network_menu_error(self) -> Result<T> {
        self.map_err(|e| NetworkMenuError::NetworkError(e.to_string()))
    }
}
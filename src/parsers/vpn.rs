use crate::constants::FIELD_SEPARATOR;
use crate::errors::{NetworkMenuError, Result};
use crate::parsers::{NetworkParser, ParsingPipeline, utils};
use crate::types::VpnConnection;
use std::sync::Arc;
use tracing::{debug, trace};

/// Parser for VPN connection information from NetworkManager
#[derive(Debug)]
pub struct VpnParser {
    format: VpnFormat,
}

#[derive(Debug, Clone)]
pub enum VpnFormat {
    NetworkManager,
    OpenVpn,
    Auto,
}

impl VpnParser {
    pub fn new(format: VpnFormat) -> Self {
        Self { format }
    }
    
    pub fn networkmanager() -> Self {
        Self::new(VpnFormat::NetworkManager)
    }
    
    pub fn openvpn() -> Self {
        Self::new(VpnFormat::OpenVpn)
    }
    
    pub fn auto() -> Self {
        Self::new(VpnFormat::Auto)
    }
    
    fn detect_format(&self, line: &str) -> VpnFormat {
        if line.contains("ACTIVE") || line.contains("TYPE") || line.contains("NAME") {
            VpnFormat::NetworkManager
        } else if line.contains("openvpn") || line.contains("vpn") {
            VpnFormat::OpenVpn
        } else {
            VpnFormat::NetworkManager
        }
    }
}

impl NetworkParser<VpnConnection> for VpnParser {
    fn parse(&self, input: &str) -> Result<VpnConnection> {
        let lines: Vec<&str> = input.lines().collect();
        if lines.is_empty() {
            return Err(NetworkMenuError::parse_error("Empty VPN input"));
        }
        
        // For multi-line input, parse the first valid line
        for line in lines {
            if let Some(connection) = self.parse_line(line)? {
                return Ok(connection);
            }
        }
        
        Err(NetworkMenuError::parse_error("No valid VPN connection found in input"))
    }
    
    fn parse_line(&self, line: &str) -> Result<Option<VpnConnection>> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        
        let format = match self.format {
            VpnFormat::Auto => self.detect_format(line),
            ref f => f.clone(),
        };
        
        match format {
            VpnFormat::NetworkManager => self.parse_networkmanager_line(line),
            VpnFormat::OpenVpn => self.parse_openvpn_line(line),
            VpnFormat::Auto => unreachable!("Auto format should be resolved"),
        }
    }
    
    fn validate(&self, connection: &VpnConnection) -> Result<()> {
        if connection.name.is_empty() {
            return Err(NetworkMenuError::validation_error("VPN connection name cannot be empty"));
        }
        
        if connection.connection_type.is_empty() {
            return Err(NetworkMenuError::validation_error("VPN connection type cannot be empty"));
        }
        
        Ok(())
    }
}

impl VpnParser {
    fn parse_networkmanager_line(&self, line: &str) -> Result<Option<VpnConnection>> {
        // NetworkManager format: ACTIVE:TYPE:NAME
        // Example: "yes:vpn:MyVPN"
        let fields = utils::split_and_trim(line, FIELD_SEPARATOR);
        
        if fields.len() < 3 {
            trace!("Skipping NetworkManager VPN line with insufficient fields: {}", line);
            return Ok(None);
        }
        
        let active = &fields[0];
        let connection_type = &fields[1];
        let name = &fields[2];
        
        // Skip header lines or non-VPN connections
        if name.is_empty() || 
           name.to_uppercase() == "NAME" || 
           !connection_type.to_lowercase().contains("vpn") {
            return Ok(None);
        }
        
        let is_active = utils::parse_bool(active).unwrap_or(false);
        
        let connection = VpnConnection::new(
            Arc::from(name.as_str()),
            Arc::from(connection_type.as_str())
        ).with_active_state(is_active);
        
        debug!("Parsed NetworkManager VPN: {} ({})", name, if is_active { "active" } else { "inactive" });
        Ok(Some(connection))
    }
    
    fn parse_openvpn_line(&self, line: &str) -> Result<Option<VpnConnection>> {
        // OpenVPN format is more flexible, try to extract name and status
        let trimmed = line.trim();
        
        // Skip empty lines or common non-connection lines
        if trimmed.is_empty() || 
           trimmed.starts_with('#') || 
           trimmed.starts_with("OpenVPN") {
            return Ok(None);
        }
        
        // Look for connection indicators
        let is_active = trimmed.to_lowercase().contains("connected") ||
                       trimmed.to_lowercase().contains("active") ||
                       trimmed.to_lowercase().contains("up");
        
        // Extract connection name (assume it's the first word or quoted string)
        let name = if let Some(quoted) = utils::extract_quoted_string(trimmed) {
            quoted
        } else {
            trimmed.split_whitespace().next().unwrap_or("Unknown").to_string()
        };
        
        if name.is_empty() || name == "Unknown" {
            return Ok(None);
        }
        
        let connection = VpnConnection::new(
            Arc::from(name.as_str()),
            Arc::from("openvpn")
        ).with_active_state(is_active);
        
        debug!("Parsed OpenVPN connection: {} ({})", name, if is_active { "active" } else { "inactive" });
        Ok(Some(connection))
    }
}

/// Create a parsing pipeline for VPN connections
pub fn create_vpn_pipeline(format: VpnFormat) -> ParsingPipeline<VpnConnection> {
    ParsingPipeline::new(Box::new(VpnParser::new(format)))
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !line.trim().starts_with('#'))
        .transform(|line| utils::strip_ansi_codes(&line))
        .transform(|line| utils::normalize_whitespace(&line))
}

/// Convenience function to parse NetworkManager VPN output
pub fn parse_networkmanager_vpn(output: &str) -> Result<Vec<VpnConnection>> {
    create_vpn_pipeline(VpnFormat::NetworkManager).process(output)
}

/// Convenience function to parse OpenVPN output
pub fn parse_openvpn_vpn(output: &str) -> Result<Vec<VpnConnection>> {
    create_vpn_pipeline(VpnFormat::OpenVpn).process(output)
}

/// Auto-detect format and parse VPN output
pub fn parse_vpn_auto(output: &str) -> Result<Vec<VpnConnection>> {
    create_vpn_pipeline(VpnFormat::Auto).process(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_networkmanager_vpn_parsing() {
        let input = r#"yes:vpn:MyWorkVPN
no:vpn:HomeVPN
yes:vpn:TravelVPN"#;
        
        let connections = parse_networkmanager_vpn(input).unwrap();
        assert_eq!(connections.len(), 3);
        
        let work_vpn = &connections[0];
        assert_eq!(work_vpn.name.as_ref(), "MyWorkVPN");
        assert!(work_vpn.is_active);
        assert_eq!(work_vpn.connection_type.as_ref(), "vpn");
        
        let home_vpn = &connections[1];
        assert_eq!(home_vpn.name.as_ref(), "HomeVPN");
        assert!(!home_vpn.is_active);
    }

    #[test]
    fn test_openvpn_parsing() {
        let input = r#"MyVPN connected
AnotherVPN disconnected
# This is a comment
"Complex VPN Name" active"#;
        
        let connections = parse_openvpn_vpn(input).unwrap();
        assert_eq!(connections.len(), 3);
        
        let my_vpn = &connections[0];
        assert_eq!(my_vpn.name.as_ref(), "MyVPN");
        assert!(my_vpn.is_active);
        
        let another_vpn = &connections[1];
        assert_eq!(another_vpn.name.as_ref(), "AnotherVPN");
        assert!(!another_vpn.is_active);
        
        let complex_vpn = &connections[2];
        assert_eq!(complex_vpn.name.as_ref(), "Complex VPN Name");
        assert!(complex_vpn.is_active);
    }

    #[test]
    fn test_vpn_validation() {
        let parser = VpnParser::networkmanager();
        
        let valid_connection = VpnConnection::new("TestVPN".into(), "vpn".into());
        assert!(parser.validate(&valid_connection).is_ok());
        
        let empty_name_connection = VpnConnection::new("".into(), "vpn".into());
        assert!(parser.validate(&empty_name_connection).is_err());
        
        let empty_type_connection = VpnConnection::new("TestVPN".into(), "".into());
        assert!(parser.validate(&empty_type_connection).is_err());
    }

    #[test]
    fn test_format_detection() {
        let parser = VpnParser::auto();
        
        let nm_line = "yes:vpn:MyVPN";
        assert!(matches!(parser.detect_format(nm_line), VpnFormat::NetworkManager));
        
        let openvpn_line = "MyVPN connected";
        assert!(matches!(parser.detect_format(openvpn_line), VpnFormat::OpenVpn));
    }

    #[test]
    fn test_skip_non_vpn_connections() {
        let input = r#"ACTIVE:TYPE:NAME
yes:ethernet:Wired connection 1
no:wifi:WiFi connection
yes:vpn:MyVPN"#;
        
        let connections = parse_networkmanager_vpn(input).unwrap();
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name.as_ref(), "MyVPN");
    }
}
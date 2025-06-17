use crate::constants::FIELD_SEPARATOR;
use crate::errors::{NetworkMenuError, Result};
use crate::parsers::{NetworkParser, ParsingPipeline, utils};
use crate::types::{SecurityType, WifiNetwork};
use regex::Regex;
use std::sync::Arc;
use tracing::{debug, trace};

/// Parser for WiFi network information from NetworkManager and iwd
#[derive(Debug)]
pub struct WifiParser {
    format: WifiFormat,
}

#[derive(Debug, Clone)]
pub enum WifiFormat {
    NetworkManager,
    Iwd,
    Auto,
}

impl WifiParser {
    pub fn new(format: WifiFormat) -> Self {
        Self { format }
    }
    
    pub fn networkmanager() -> Self {
        Self::new(WifiFormat::NetworkManager)
    }
    
    pub fn iwd() -> Self {
        Self::new(WifiFormat::Iwd)
    }
    
    pub fn auto() -> Self {
        Self::new(WifiFormat::Auto)
    }
    
    fn detect_format(&self, line: &str) -> WifiFormat {
        if line.contains("IN-USE") || line.contains("SSID") || line.contains("BARS") {
            WifiFormat::NetworkManager
        } else if line.contains("Available networks") || line.trim().starts_with('>') {
            WifiFormat::Iwd
        } else {
            WifiFormat::NetworkManager
        }
    }
}

impl NetworkParser<WifiNetwork> for WifiParser {
    fn parse(&self, input: &str) -> Result<WifiNetwork> {
        let lines: Vec<&str> = input.lines().collect();
        if lines.is_empty() {
            return Err(NetworkMenuError::parse_error("Empty WiFi input"));
        }
        
        // For multi-line input, parse the first valid line
        for line in lines {
            if let Some(network) = self.parse_line(line)? {
                return Ok(network);
            }
        }
        
        Err(NetworkMenuError::parse_error("No valid WiFi network found in input"))
    }
    
    fn parse_line(&self, line: &str) -> Result<Option<WifiNetwork>> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        
        let format = match self.format {
            WifiFormat::Auto => self.detect_format(line),
            ref f => f.clone(),
        };
        
        match format {
            WifiFormat::NetworkManager => self.parse_networkmanager_line(line),
            WifiFormat::Iwd => self.parse_iwd_line(line),
            WifiFormat::Auto => unreachable!("Auto format should be resolved"),
        }
    }
    
    fn validate(&self, network: &WifiNetwork) -> Result<()> {
        if network.ssid.is_empty() {
            return Err(NetworkMenuError::validation_error("WiFi SSID cannot be empty"));
        }
        
        if let Some(strength) = network.signal_strength {
            if strength > 100 {
                return Err(NetworkMenuError::validation_error(
                    format!("Invalid signal strength: {}", strength)
                ));
            }
        }
        
        Ok(())
    }
}

impl WifiParser {
    fn parse_networkmanager_line(&self, line: &str) -> Result<Option<WifiNetwork>> {
        // NetworkManager format: IN-USE:SSID:BARS:SECURITY
        // Example: "*:MyNetwork:▂▄▆█:WPA2"
        let fields = utils::split_and_trim(line, FIELD_SEPARATOR);
        
        if fields.len() < 4 {
            trace!("Skipping NetworkManager line with insufficient fields: {}", line);
            return Ok(None);
        }
        
        let in_use = &fields[0];
        let ssid = &fields[1];
        let bars = &fields[2];
        let security = &fields[3];
        
        // Skip empty SSIDs or header lines
        if ssid.is_empty() || ssid.to_uppercase() == "SSID" {
            return Ok(None);
        }
        
        let is_connected = in_use == "*" || in_use.to_lowercase() == "yes";
        let signal_strength = self.parse_networkmanager_signal(bars);
        let security_type = SecurityType::from(security.as_str());
        
        let network = WifiNetwork::new(Arc::from(ssid.as_str()))
            .with_security(security_type)
            .with_signal_strength(signal_strength.unwrap_or(0))
            .with_connection_state(is_connected);
        
        debug!("Parsed NetworkManager WiFi: {} ({})", ssid, if is_connected { "connected" } else { "available" });
        Ok(Some(network))
    }
    
    fn parse_iwd_line(&self, line: &str) -> Result<Option<WifiNetwork>> {
        // iwd format examples:
        // "  NetworkName                   psk"
        // "> ConnectedNetwork             psk"
        let trimmed = line.trim();
        
        // Skip header lines
        if trimmed.starts_with("Available networks") || 
           trimmed.starts_with("---") || 
           trimmed.is_empty() {
            return Ok(None);
        }
        
        let is_connected = trimmed.starts_with('>');
        let line_without_indicator = if is_connected {
            &trimmed[1..].trim()
        } else {
            trimmed
        };
        
        // Split by whitespace, SSID is everything except the last field (security)
        let parts: Vec<&str> = line_without_indicator.split_whitespace().collect();
        if parts.len() < 2 {
            trace!("Skipping iwd line with insufficient parts: {}", line);
            return Ok(None);
        }
        
        let security_field = parts.last().unwrap();
        let ssid_parts = &parts[..parts.len() - 1];
        let ssid = ssid_parts.join(" ");
        
        if ssid.is_empty() {
            return Ok(None);
        }
        
        let security_type = match security_field {
            "open" => SecurityType::None,
            "psk" => SecurityType::Wpa2,
            "8021x" => SecurityType::Enterprise,
            _ => SecurityType::Unknown(security_field.to_string()),
        };
        
        let network = WifiNetwork::new(Arc::from(ssid.as_str()))
            .with_security(security_type)
            .with_connection_state(is_connected);
        
        debug!("Parsed iwd WiFi: {} ({})", ssid, if is_connected { "connected" } else { "available" });
        Ok(Some(network))
    }
    
    fn parse_networkmanager_signal(&self, bars: &str) -> Option<u8> {
        // Try different signal strength formats
        if let Some(strength) = utils::parse_signal_strength(bars) {
            return Some(strength);
        }
        
        // Handle NetworkManager's bar symbols
        if bars.contains('█') || bars.contains('▆') || bars.contains('▄') || bars.contains('▂') {
            let filled_bars = bars.chars()
                .filter(|&c| c == '█' || c == '▆' || c == '▄' || c == '▂')
                .count();
            let total_bars = bars.chars()
                .filter(|&c| c == '█' || c == '▆' || c == '▄' || c == '▂' || c == '_')
                .count();
            
            if total_bars > 0 {
                let percentage = (filled_bars * 100) / total_bars;
                return Some(percentage as u8);
            }
        }
        
        None
    }
}

/// Create a parsing pipeline for WiFi networks
pub fn create_wifi_pipeline(format: WifiFormat) -> ParsingPipeline<WifiNetwork> {
    ParsingPipeline::new(Box::new(WifiParser::new(format)))
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !line.to_uppercase().contains("DEVICE"))
        .transform(|line| utils::strip_ansi_codes(&line))
        .transform(|line| utils::normalize_whitespace(&line))
}

/// Convenience function to parse NetworkManager WiFi output
pub fn parse_networkmanager_wifi(output: &str) -> Result<Vec<WifiNetwork>> {
    create_wifi_pipeline(WifiFormat::NetworkManager).process(output)
}

/// Convenience function to parse iwd WiFi output
pub fn parse_iwd_wifi(output: &str) -> Result<Vec<WifiNetwork>> {
    create_wifi_pipeline(WifiFormat::Iwd).process(output)
}

/// Auto-detect format and parse WiFi output
pub fn parse_wifi_auto(output: &str) -> Result<Vec<WifiNetwork>> {
    create_wifi_pipeline(WifiFormat::Auto).process(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_networkmanager_parsing() {
        let input = r#"*:MyHomeNetwork:▂▄▆█:WPA2
:PublicWiFi:▂▄__:
:SecureNetwork:▂▄▆_:WPA2"#;
        
        let networks = parse_networkmanager_wifi(input).unwrap();
        assert_eq!(networks.len(), 3);
        
        let home_network = &networks[0];
        assert_eq!(home_network.ssid.as_ref(), "MyHomeNetwork");
        assert!(home_network.is_connected);
        assert!(matches!(home_network.security, SecurityType::Wpa2));
        
        let public_wifi = &networks[1];
        assert_eq!(public_wifi.ssid.as_ref(), "PublicWiFi");
        assert!(!public_wifi.is_connected);
        assert!(matches!(public_wifi.security, SecurityType::None));
    }

    #[test]
    fn test_iwd_parsing() {
        let input = r#"Available networks:
> ConnectedNetwork             psk
  PublicNetwork               open
  EnterpriseNetwork           8021x"#;
        
        let networks = parse_iwd_wifi(input).unwrap();
        assert_eq!(networks.len(), 3);
        
        let connected = &networks[0];
        assert_eq!(connected.ssid.as_ref(), "ConnectedNetwork");
        assert!(connected.is_connected);
        assert!(matches!(connected.security, SecurityType::Wpa2));
        
        let public = &networks[1];
        assert_eq!(public.ssid.as_ref(), "PublicNetwork");
        assert!(!public.is_connected);
        assert!(matches!(public.security, SecurityType::None));
    }

    #[test]
    fn test_signal_strength_parsing() {
        let parser = WifiParser::networkmanager();
        
        assert_eq!(parser.parse_networkmanager_signal("▂▄▆█"), Some(100));
        assert_eq!(parser.parse_networkmanager_signal("▂▄▆_"), Some(75));
        assert_eq!(parser.parse_networkmanager_signal("▂▄__"), Some(50));
        assert_eq!(parser.parse_networkmanager_signal("▂___"), Some(25));
        assert_eq!(parser.parse_networkmanager_signal("____"), Some(0));
    }

    #[test]
    fn test_wifi_validation() {
        let parser = WifiParser::networkmanager();
        
        let valid_network = WifiNetwork::new("TestNetwork".into());
        assert!(parser.validate(&valid_network).is_ok());
        
        let empty_ssid_network = WifiNetwork::new("".into());
        assert!(parser.validate(&empty_ssid_network).is_err());
        
        let invalid_strength_network = WifiNetwork::new("Test".into())
            .with_signal_strength(150);
        assert!(parser.validate(&invalid_strength_network).is_err());
    }

    #[test]
    fn test_format_detection() {
        let parser = WifiParser::auto();
        
        let nm_line = "*:NetworkName:▂▄▆█:WPA2";
        assert!(matches!(parser.detect_format(nm_line), WifiFormat::NetworkManager));
        
        let iwd_line = "> ConnectedNetwork             psk";
        assert!(matches!(parser.detect_format(iwd_line), WifiFormat::Iwd));
    }

    #[test]
    fn test_complex_ssid_parsing() {
        // Test SSID with spaces in iwd format
        let input = "  My Complex Network Name      psk";
        let parser = WifiParser::iwd();
        
        let network = parser.parse_line(input).unwrap().unwrap();
        assert_eq!(network.ssid.as_ref(), "My Complex Network Name");
        assert!(matches!(network.security, SecurityType::Wpa2));
    }
}
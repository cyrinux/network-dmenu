use crate::errors::{NetworkMenuError, Result};
use crate::parsers::{NetworkParser, ParsingPipeline, utils};
use crate::types::TailscaleExitNode;
use std::sync::Arc;
use tracing::{debug, trace};

/// Parser for Tailscale exit node information from tailscale command
#[derive(Debug)]
pub struct TailscaleParser;

impl TailscaleParser {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TailscaleParser {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkParser<TailscaleExitNode> for TailscaleParser {
    fn parse(&self, input: &str) -> Result<TailscaleExitNode> {
        let lines: Vec<&str> = input.lines().collect();
        if lines.is_empty() {
            return Err(NetworkMenuError::parse_error("Empty Tailscale input"));
        }
        
        // For multi-line input, parse the first valid line
        for line in lines {
            if let Some(node) = self.parse_line(line)? {
                return Ok(node);
            }
        }
        
        Err(NetworkMenuError::parse_error("No valid Tailscale exit node found in input"))
    }
    
    fn parse_line(&self, line: &str) -> Result<Option<TailscaleExitNode>> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        
        // Skip header lines or non-node lines
        if trimmed.starts_with('#') || 
           trimmed.starts_with("ID") ||
           trimmed.starts_with("---") ||
           !trimmed.contains("ts.net") {
            return Ok(None);
        }
        
        // Parse exit node line
        // Expected format varies, but generally contains hostname and location info
        // Example: "  node-name   hostname.mullvad.ts.net   Amsterdam, NL   online"
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(None);
        }
        
        // Extract hostname (look for .ts.net domain)
        let hostname = parts.iter()
            .find(|part| part.contains("ts.net"))
            .map(|s| s.to_string());
        
        if let Some(hostname) = hostname {
            // Extract node name (usually first part before hostname)
            let name = if parts.len() > 1 && !parts[0].contains("ts.net") {
                parts[0].to_string()
            } else {
                // Fallback: extract from hostname
                hostname.split('.').next().unwrap_or(&hostname).to_string()
            };
            
            // Check if it's active (indicated by various markers)
            let is_active = trimmed.contains("*") || 
                           trimmed.contains("active") ||
                           trimmed.contains("current");
            
            // Extract location if present (usually between hostname and status)
            let location = extract_location(&parts, &hostname);
            
            let mut node = TailscaleExitNode::new(name, hostname)
                .with_active_state(is_active);
            
            if let Some(loc) = location {
                node = node.with_location(loc);
            }
            
            debug!("Parsed Tailscale exit node: {} ({})", node.name, node.hostname);
            return Ok(Some(node));
        }
        
        Ok(None)
    }
    
    fn validate(&self, node: &TailscaleExitNode) -> Result<()> {
        if node.name.is_empty() {
            return Err(NetworkMenuError::validation_error("Tailscale node name cannot be empty"));
        }
        
        if node.hostname.is_empty() {
            return Err(NetworkMenuError::validation_error("Tailscale hostname cannot be empty"));
        }
        
        if !node.hostname.contains("ts.net") {
            return Err(NetworkMenuError::validation_error(
                format!("Invalid Tailscale hostname: {}", node.hostname)
            ));
        }
        
        Ok(())
    }
}

/// Extract location information from parsed parts
fn extract_location(parts: &[&str], hostname: &str) -> Option<String> {
    // Find the hostname index
    let hostname_index = parts.iter().position(|&part| part == hostname)?;
    
    // Look for location info after hostname
    if hostname_index + 1 < parts.len() {
        let remaining: Vec<&str> = parts[hostname_index + 1..].iter()
            .take_while(|&&part| {
                !part.contains("online") && 
                !part.contains("offline") && 
                !part.contains("idle") &&
                !part.contains("*")
            })
            .copied()
            .collect();
        
        if !remaining.is_empty() {
            return Some(remaining.join(" ").trim_end_matches(',').to_string());
        }
    }
    
    None
}

/// Create a parsing pipeline for Tailscale exit nodes
pub fn create_tailscale_pipeline() -> ParsingPipeline<TailscaleExitNode> {
    ParsingPipeline::new(Box::new(TailscaleParser::new()))
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !line.starts_with('#'))
        .filter(|line| line.contains("ts.net"))
        .transform(|line| utils::strip_ansi_codes(&line))
        .transform(|line| utils::normalize_whitespace(&line))
}

/// Convenience function to parse Tailscale exit node output
pub fn parse_tailscale_exit_nodes(output: &str) -> Result<Vec<TailscaleExitNode>> {
    create_tailscale_pipeline().process(output)
}

/// Parse Tailscale status to check if a node is currently active
pub fn parse_tailscale_status(output: &str) -> Option<String> {
    for line in output.lines() {
        if line.contains("exit node:") {
            // Extract the exit node name/hostname
            if let Some(node_part) = line.split("exit node:").nth(1) {
                return Some(node_part.trim().to_string());
            }
        }
    }
    None
}

/// Check if Tailscale is enabled from status output
pub fn is_tailscale_enabled(output: &str) -> bool {
    !output.contains("Tailscale is stopped") && 
    !output.contains("not running") &&
    !output.contains("stopped")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tailscale_exit_node_parsing() {
        let input = r#"# ID    Name            Location        Status
amsterdam-1   amsterdam-1.mullvad.ts.net   Amsterdam, NL   online
london-2      london-2.mullvad.ts.net      London, UK      online
*sydney-3     sydney-3.mullvad.ts.net      Sydney, AU      online"#;
        
        let nodes = parse_tailscale_exit_nodes(input).unwrap();
        assert_eq!(nodes.len(), 3);
        
        let amsterdam = &nodes[0];
        assert_eq!(amsterdam.name.as_ref(), "amsterdam-1");
        assert_eq!(amsterdam.hostname.as_ref(), "amsterdam-1.mullvad.ts.net");
        assert!(amsterdam.is_mullvad);
        assert!(!amsterdam.is_active);
        assert_eq!(amsterdam.location.as_ref().unwrap().as_ref(), "Amsterdam, NL");
        
        let sydney = &nodes[2];
        assert_eq!(sydney.name.as_ref(), "sydney-3");
        assert!(sydney.is_active); // Has * marker
    }

    #[test]
    fn test_tailscale_status_parsing() {
        let status_with_exit_node = "100.64.0.1   machine-name  online  exit node: amsterdam-1.mullvad.ts.net";
        let active_node = parse_tailscale_status(status_with_exit_node);
        assert_eq!(active_node, Some("amsterdam-1.mullvad.ts.net".to_string()));
        
        let status_without_exit_node = "100.64.0.1   machine-name  online";
        let no_node = parse_tailscale_status(status_without_exit_node);
        assert_eq!(no_node, None);
    }

    #[test]
    fn test_tailscale_enabled_check() {
        assert!(is_tailscale_enabled("100.64.0.1   machine-name  online"));
        assert!(!is_tailscale_enabled("Tailscale is stopped"));
        assert!(!is_tailscale_enabled("not running"));
        assert!(!is_tailscale_enabled("stopped"));
    }

    #[test]
    fn test_tailscale_validation() {
        let parser = TailscaleParser::new();
        
        let valid_node = TailscaleExitNode::new("test-node", "test.mullvad.ts.net");
        assert!(parser.validate(&valid_node).is_ok());
        
        let empty_name_node = TailscaleExitNode::new("", "test.mullvad.ts.net");
        assert!(parser.validate(&empty_name_node).is_err());
        
        let empty_hostname_node = TailscaleExitNode::new("test", "");
        assert!(parser.validate(&empty_hostname_node).is_err());
        
        let invalid_hostname_node = TailscaleExitNode::new("test", "invalid.hostname.com");
        assert!(parser.validate(&invalid_hostname_node).is_err());
    }

    #[test]
    fn test_location_extraction() {
        let parts = vec!["amsterdam-1", "amsterdam-1.mullvad.ts.net", "Amsterdam,", "NL", "online"];
        let hostname = "amsterdam-1.mullvad.ts.net";
        
        let location = extract_location(&parts, hostname);
        assert_eq!(location, Some("Amsterdam, NL".to_string()));
        
        let parts_no_location = vec!["node", "node.ts.net", "online"];
        let location_none = extract_location(&parts_no_location, "node.ts.net");
        assert_eq!(location_none, None);
    }

    #[test]
    fn test_mullvad_detection() {
        let mullvad_input = "amsterdam-1   amsterdam-1.mullvad.ts.net   Amsterdam, NL   online";
        let nodes = parse_tailscale_exit_nodes(mullvad_input).unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].is_mullvad);
        
        let regular_input = "my-node   my-node.ts.net   online";
        let regular_nodes = parse_tailscale_exit_nodes(regular_input).unwrap();
        assert_eq!(regular_nodes.len(), 1);
        assert!(!regular_nodes[0].is_mullvad);
    }

    #[test]
    fn test_skip_invalid_lines() {
        let input = r#"# This is a header
Invalid line without ts.net
amsterdam-1   amsterdam-1.mullvad.ts.net   Amsterdam, NL   online
---
Another invalid line"#;
        
        let nodes = parse_tailscale_exit_nodes(input).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name.as_ref(), "amsterdam-1");
    }
}
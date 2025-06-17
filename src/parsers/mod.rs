use crate::errors::{NetworkMenuError, Result};
use crate::types::{BluetoothDevice, SecurityType, TailscaleExitNode, VpnConnection, WifiNetwork};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, trace, warn};

pub mod bluetooth;
pub mod tailscale;
pub mod wifi;
pub mod vpn;

/// Trait for parsing network data from command output
pub trait NetworkParser<T> {
    /// Parse input string into structured data
    fn parse(&self, input: &str) -> Result<T>;
    
    /// Parse a single line of input
    fn parse_line(&self, line: &str) -> Result<Option<T>>;
    
    /// Validate parsed data
    fn validate(&self, data: &T) -> Result<()>;
}

/// Generic parsing pipeline for processing command output
#[derive(Debug)]
pub struct ParsingPipeline<T> {
    filters: Vec<Box<dyn Fn(&str) -> bool + Send + Sync>>,
    transformers: Vec<Box<dyn Fn(String) -> String + Send + Sync>>,
    parser: Box<dyn NetworkParser<T> + Send + Sync>,
}

impl<T> ParsingPipeline<T> {
    pub fn new(parser: Box<dyn NetworkParser<T> + Send + Sync>) -> Self {
        Self {
            filters: Vec::new(),
            transformers: Vec::new(),
            parser,
        }
    }
    
    /// Add a filter to skip lines that don't match the predicate
    pub fn filter<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&str) -> bool + Send + Sync + 'static,
    {
        self.filters.push(Box::new(predicate));
        self
    }
    
    /// Add a transformer to modify lines before parsing
    pub fn transform<F>(mut self, transformer: F) -> Self
    where
        F: Fn(String) -> String + Send + Sync + 'static,
    {
        self.transformers.push(Box::new(transformer));
        self
    }
    
    /// Process the input through the pipeline
    pub fn process(&self, input: &str) -> Result<Vec<T>> {
        let mut results = Vec::new();
        
        for line in input.lines() {
            // Skip empty lines by default
            if line.trim().is_empty() {
                continue;
            }
            
            // Apply filters
            let mut skip = false;
            for filter in &self.filters {
                if !filter(line) {
                    skip = true;
                    break;
                }
            }
            if skip {
                continue;
            }
            
            // Apply transformers
            let mut processed_line = line.to_string();
            for transformer in &self.transformers {
                processed_line = transformer(processed_line);
            }
            
            // Parse the line
            match self.parser.parse_line(&processed_line)? {
                Some(item) => {
                    if let Err(e) = self.parser.validate(&item) {
                        warn!("Validation failed for parsed item: {}", e);
                        continue;
                    }
                    results.push(item);
                }
                None => continue,
            }
        }
        
        debug!("Parsed {} items from input", results.len());
        Ok(results)
    }
}

/// Utility functions for common parsing operations
pub mod utils {
    use super::*;
    use crate::constants::WIFI_STRENGTH_SYMBOLS;
    
    /// Parse signal strength from various formats
    pub fn parse_signal_strength(input: &str) -> Option<u8> {
        // Handle star-based strength (e.g., "****")
        if let Some(stars) = parse_star_strength(input) {
            return Some(stars);
        }
        
        // Handle percentage (e.g., "75%")
        if let Some(percentage) = parse_percentage_strength(input) {
            return Some(percentage);
        }
        
        // Handle dBm values (e.g., "-50 dBm")
        if let Some(dbm) = parse_dbm_strength(input) {
            return Some(dbm);
        }
        
        // Handle numeric values (0-100)
        if let Ok(value) = input.trim().parse::<u8>() {
            if value <= 100 {
                return Some(value);
            }
        }
        
        None
    }
    
    fn parse_star_strength(input: &str) -> Option<u8> {
        let stars = input.chars().filter(|&c| c == '*').count();
        if stars > 0 && stars <= 4 {
            Some((stars * 25) as u8)
        } else {
            None
        }
    }
    
    fn parse_percentage_strength(input: &str) -> Option<u8> {
        if let Some(stripped) = input.strip_suffix('%') {
            if let Ok(value) = stripped.trim().parse::<u8>() {
                if value <= 100 {
                    return Some(value);
                }
            }
        }
        None
    }
    
    fn parse_dbm_strength(input: &str) -> Option<u8> {
        let dbm_regex = Regex::new(r"-?(\d+)\s*dBm").ok()?;
        if let Some(captures) = dbm_regex.captures(input) {
            if let Ok(dbm) = captures[1].parse::<i32>() {
                // Convert dBm to percentage (rough approximation)
                let percentage = if dbm >= -30 {
                    100
                } else if dbm >= -50 {
                    75
                } else if dbm >= -70 {
                    50
                } else if dbm >= -80 {
                    25
                } else {
                    10
                };
                return Some(percentage);
            }
        }
        None
    }
    
    /// Convert signal strength to visual symbols
    pub fn strength_to_symbols(strength: u8) -> String {
        let index = match strength {
            0..=20 => 1,
            21..=40 => 2,
            41..=60 => 3,
            61..=80 => 4,
            81..=100 => 4,
            _ => 0,
        };
        
        let mut result = String::new();
        for i in 0..4 {
            if i < index {
                result.push_str(WIFI_STRENGTH_SYMBOLS[i + 1]);
            } else {
                result.push_str(WIFI_STRENGTH_SYMBOLS[0]);
            }
        }
        result
    }
    
    /// Parse boolean values from various string representations
    pub fn parse_bool(input: &str) -> Option<bool> {
        match input.to_lowercase().trim() {
            "yes" | "true" | "1" | "on" | "enabled" | "connected" | "active" => Some(true),
            "no" | "false" | "0" | "off" | "disabled" | "disconnected" | "inactive" => Some(false),
            _ => None,
        }
    }
    
    /// Split a line by delimiter and trim whitespace
    pub fn split_and_trim(line: &str, delimiter: char) -> Vec<String> {
        line.split(delimiter)
            .map(|s| s.trim().to_string())
            .collect()
    }
    
    /// Extract quoted strings from input
    pub fn extract_quoted_string(input: &str) -> Option<String> {
        let quotes = ['"', '\''];
        for quote in quotes {
            if let Some(start) = input.find(quote) {
                if let Some(end) = input[start + 1..].find(quote) {
                    return Some(input[start + 1..start + 1 + end].to_string());
                }
            }
        }
        None
    }
    
    /// Remove ANSI color codes from string
    pub fn strip_ansi_codes(input: &str) -> String {
        let ansi_regex = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        ansi_regex.replace_all(input, "").to_string()
    }
    
    /// Parse MAC address from string
    pub fn parse_mac_address(input: &str) -> Option<String> {
        let mac_regex = Regex::new(r"([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})").ok()?;
        mac_regex.find(input).map(|m| m.as_str().to_string())
    }
    
    /// Parse IP address from string
    pub fn parse_ip_address(input: &str) -> Option<String> {
        let ip_regex = Regex::new(r"\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b").ok()?;
        ip_regex.find(input).map(|m| m.as_str().to_string())
    }
    
    /// Normalize whitespace in string
    pub fn normalize_whitespace(input: &str) -> String {
        let whitespace_regex = Regex::new(r"\s+").unwrap();
        whitespace_regex.replace_all(input.trim(), " ").to_string()
    }
}

/// Field-based parser for structured command output
#[derive(Debug)]
pub struct FieldParser {
    delimiter: char,
    field_map: HashMap<String, usize>,
}

impl FieldParser {
    pub fn new(delimiter: char) -> Self {
        Self {
            delimiter,
            field_map: HashMap::new(),
        }
    }
    
    /// Set field mapping from header line
    pub fn set_fields_from_header(&mut self, header: &str) -> Result<()> {
        self.field_map.clear();
        
        for (index, field) in header.split(self.delimiter).enumerate() {
            let field_name = field.trim().to_lowercase();
            self.field_map.insert(field_name, index);
        }
        
        debug!("Field mapping: {:?}", self.field_map);
        Ok(())
    }
    
    /// Parse a line into fields
    pub fn parse_line(&self, line: &str) -> ParsedFields {
        let fields: Vec<String> = line
            .split(self.delimiter)
            .map(|s| s.trim().to_string())
            .collect();
        
        ParsedFields {
            fields,
            field_map: &self.field_map,
        }
    }
}

/// Represents parsed fields from a line
pub struct ParsedFields<'a> {
    fields: Vec<String>,
    field_map: &'a HashMap<String, usize>,
}

impl<'a> ParsedFields<'a> {
    /// Get field value by name
    pub fn get(&self, field_name: &str) -> Option<&str> {
        let index = self.field_map.get(&field_name.to_lowercase())?;
        self.fields.get(*index).map(|s| s.as_str())
    }
    
    /// Get field value by index
    pub fn get_by_index(&self, index: usize) -> Option<&str> {
        self.fields.get(index).map(|s| s.as_str())
    }
    
    /// Get all fields
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
    
    /// Check if field exists and is not empty
    pub fn has_value(&self, field_name: &str) -> bool {
        self.get(field_name)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    }
    
    /// Get field as boolean
    pub fn get_bool(&self, field_name: &str) -> Option<bool> {
        self.get(field_name).and_then(utils::parse_bool)
    }
    
    /// Get field as number
    pub fn get_number<T>(&self, field_name: &str) -> Option<T>
    where
        T: std::str::FromStr,
    {
        self.get(field_name)?.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::utils::*;

    #[test]
    fn test_signal_strength_parsing() {
        assert_eq!(parse_signal_strength("****"), Some(100));
        assert_eq!(parse_signal_strength("***"), Some(75));
        assert_eq!(parse_signal_strength("**"), Some(50));
        assert_eq!(parse_signal_strength("*"), Some(25));
        assert_eq!(parse_signal_strength("85%"), Some(85));
        assert_eq!(parse_signal_strength("-50 dBm"), Some(75));
        assert_eq!(parse_signal_strength("42"), Some(42));
        assert_eq!(parse_signal_strength("invalid"), None);
    }

    #[test]
    fn test_bool_parsing() {
        assert_eq!(parse_bool("yes"), Some(true));
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("no"), Some(false));
        assert_eq!(parse_bool("FALSE"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn test_split_and_trim() {
        let result = split_and_trim("  field1  :  field2  :  field3  ", ':');
        assert_eq!(result, vec!["field1", "field2", "field3"]);
    }

    #[test]
    fn test_extract_quoted_string() {
        assert_eq!(extract_quoted_string(r#"name="My Network""#), Some("My Network".to_string()));
        assert_eq!(extract_quoted_string("name='Another Network'"), Some("Another Network".to_string()));
        assert_eq!(extract_quoted_string("no quotes here"), None);
    }

    #[test]
    fn test_mac_address_parsing() {
        assert_eq!(parse_mac_address("Device AA:BB:CC:DD:EE:FF found"), Some("AA:BB:CC:DD:EE:FF".to_string()));
        assert_eq!(parse_mac_address("Device aa-bb-cc-dd-ee-ff found"), Some("aa-bb-cc-dd-ee-ff".to_string()));
        assert_eq!(parse_mac_address("No MAC here"), None);
    }

    #[test]
    fn test_ip_address_parsing() {
        assert_eq!(parse_ip_address("Interface has IP 192.168.1.100"), Some("192.168.1.100".to_string()));
        assert_eq!(parse_ip_address("No IP here"), None);
    }

    #[test]
    fn test_field_parser() {
        let mut parser = FieldParser::new(':');
        parser.set_fields_from_header("field1:field2:field3").unwrap();
        
        let parsed = parser.parse_line("value1:value2:value3");
        assert_eq!(parsed.get("field1"), Some("value1"));
        assert_eq!(parsed.get("field2"), Some("value2"));
        assert_eq!(parsed.get("field3"), Some("value3"));
        assert_eq!(parsed.get("nonexistent"), None);
    }

    #[test]
    fn test_strength_to_symbols() {
        assert_eq!(strength_to_symbols(100), "▂▄▆█");
        assert_eq!(strength_to_symbols(75), "▂▄▆█");
        assert_eq!(strength_to_symbols(50), "▂▄▆_");
        assert_eq!(strength_to_symbols(25), "▂▄__");
        assert_eq!(strength_to_symbols(10), "▂___");
    }
}
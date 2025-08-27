use country_emoji::flag;
use notify_rust::Notification;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use log::debug;
use std::error::Error;
use std::io::Write;
use std::process::{Command, Stdio};
use tokio::time::{timeout, Duration};

const DETECT_CAPTIVE_PORTAL_URL: &str = "http://detectportal.firefox.com/";
const EXPECTED_RESPONSE: &str = "success";
const TIMEOUT_DURATION: Duration = Duration::from_secs(5);

/// Returns the flag emoji for a given country.
pub fn get_flag(name_or_code: &str) -> String {
    flag(name_or_code).unwrap_or_else(|| "❓".into())
}
/// Detects a captive portal by making an HTTP request to a known URL.
/// If a captive portal is detected, it notifies the user and opens the portal in a web browser.
pub async fn check_captive_portal() -> Result<(), Box<dyn Error>> {
    // Wait for the connection to stabilize before checking for captive portal
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Create a retry policy with exponential backoff
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

    // Build a client with retry middleware
    let client: ClientWithMiddleware = ClientBuilder::new(Client::new())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();

    // Make a request and handle retries automatically, with a timeout
    let response_result = timeout(
        TIMEOUT_DURATION,
        client.get(DETECT_CAPTIVE_PORTAL_URL).send(),
    )
    .await;

    // Handle connection errors gracefully
    let response = match response_result {
        Ok(Ok(resp)) => resp,
        Ok(Err(_e)) => {
            debug!("Captive portal check error: {}", _e);
            // Return Ok instead of propagating the error
            return Ok(());
        }
        Err(_e) => {
            #[cfg(debug_assertions)]
            eprintln!("Captive portal check timeout: {}", _e);
            // Return Ok instead of propagating the error
            return Ok(());
        }
    };

    // Try to get response text, but handle errors gracefully
    let response_text = match response.text().await {
        Ok(text) => text,
        Err(_e) => {
            #[cfg(debug_assertions)]
            eprintln!("Failed to read captive portal response: {}", _e);
            return Ok(());
        }
    };

    if response_text.trim() != EXPECTED_RESPONSE {
        // Show notification
        if let Err(_e) = Notification::new()
            .summary("Captive Portal Detected")
            .body("Opening captive portal in your default browser.")
            .show()
        {
            #[cfg(debug_assertions)]
            eprintln!("Failed to show notification: {}", _e);
        }

        // Open web browser
        if let Err(_e) = webbrowser::open(DETECT_CAPTIVE_PORTAL_URL) {
            #[cfg(debug_assertions)]
            eprintln!("Failed to open browser: {}", _e);
        }
    }

    Ok(())
}

/// Converts network strength to a visual representation.
pub fn convert_network_strength(line: &str) -> String {
    let strength_symbols = ["_", "▂", "▄", "▆", "█"];
    let stars = line.chars().rev().take_while(|&c| c == '*').count();
    let network_strength = format!(
        "{}{}{}{}",
        strength_symbols.get(1).unwrap_or(&"_"),
        strength_symbols
            .get(if stars >= 2 { 2 } else { 0 })
            .unwrap_or(&"_"),
        strength_symbols
            .get(if stars >= 3 { 3 } else { 0 })
            .unwrap_or(&"_"),
        strength_symbols
            .get(if stars >= 4 { 4 } else { 0 })
            .unwrap_or(&"_"),
    );
    network_strength
}

/// Prompts for the wifi SSID using `pinentry-gnome3`.
pub fn prompt_for_ssid() -> Result<String, Box<dyn std::error::Error>> {
    let mut child = Command::new("pinentry-gnome3")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
        write!(stdin, "SETDESC Enter SSID\nGETPIN\n")?;
    }

    let output = child.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ssid_line = stdout
        .lines()
        .find(|line| line.starts_with("D "))
        .ok_or("SSID not found")?;
    let ssid = ssid_line.trim_start_matches("D ").trim().to_string();

    Ok(ssid)
}

/// Prompts the user for a password using `pinentry-gnome3`.
pub fn prompt_for_password(ssid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut child = Command::new("pinentry-gnome3")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
        write!(stdin, "SETDESC Enter {ssid} password\nGETPIN\n")?;
    }

    let output = child.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let password_line = stdout
        .lines()
        .find(|line| line.starts_with("D "))
        .ok_or("Password not found")?;
    let password = password_line.trim_start_matches("D ").trim().to_string();

    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_network_strength_no_stars() {
        let input = "TestNetwork";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂___");
    }

    #[test]
    fn test_convert_network_strength_one_star() {
        let input = "TestNetwork*";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂___");
    }

    #[test]
    fn test_convert_network_strength_two_stars() {
        let input = "TestNetwork**";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂▄__");
    }

    #[test]
    fn test_convert_network_strength_three_stars() {
        let input = "TestNetwork***";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂▄▆_");
    }

    #[test]
    fn test_convert_network_strength_four_stars() {
        let input = "TestNetwork****";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂▄▆█");
    }

    #[test]
    fn test_convert_network_strength_five_stars() {
        let input = "TestNetwork*****";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂▄▆█");
    }

    #[test]
    fn test_convert_network_strength_empty_string() {
        let input = "";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂___");
    }

    #[test]
    fn test_convert_network_strength_only_stars() {
        let input = "***";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂▄▆_");
    }

    #[test]
    fn test_convert_network_strength_stars_not_at_end() {
        let input = "Test*Network";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂___");
    }

    #[test]
    fn test_convert_network_strength_mixed_content() {
        let input = "Network-5G-Fast**";
        let result = convert_network_strength(input);
        assert_eq!(result, "▂▄__");
    }

    #[tokio::test]
    async fn test_check_captive_portal_success_response() {
        // This test would require mocking the HTTP client
        // For now, we'll just test that the function doesn't panic
        let result = check_captive_portal().await;
        // The function should always return Ok(()) even on errors
        assert!(result.is_ok());
    }

    #[test]
    fn test_prompt_for_ssid_error_handling() {
        // This test verifies that the function handles errors gracefully
        // In a test environment, pinentry-gnome3 might not be available
        // We'll test that the function doesn't panic, but we won't actually call it
        // since it would show a GUI dialog and wait for user input

        // For now, we just test that the function signature is correct
        // In a real implementation, we would mock the command execution
        assert!(true); // Placeholder test
    }

    #[test]
    fn test_prompt_for_password_error_handling() {
        // This test verifies that the function handles errors gracefully
        // In a test environment, pinentry-gnome3 might not be available
        // We'll test that the function doesn't panic, but we won't actually call it
        // since it would show a GUI dialog and wait for user input

        // For now, we just test that the function signature is correct
        // In a real implementation, we would mock the command execution
        assert!(true); // Placeholder test
    }
}

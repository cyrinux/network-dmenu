//! Privilege escalation utilities for commands requiring elevated permissions
//!
//! This module provides utilities to detect and use the appropriate privilege
//! escalation method (pkexec for GUI or sudo for terminal).

use which::which;

/// Determines the best available privilege escalation command
///
/// Returns "pkexec" if available (for GUI authentication), otherwise "sudo"
pub fn get_privilege_command() -> &'static str {
    if which("pkexec").is_ok() {
        "pkexec"
    } else {
        "sudo"
    }
}

/// Wraps a command with the appropriate privilege escalation
///
/// # Arguments
/// * `command` - The command to run with privileges
/// * `use_shell` - Whether the command needs shell interpretation
///
/// # Examples
/// ```
/// use network_dmenu::privilege::wrap_privileged_command;
///
/// // Simple command
/// let cmd = wrap_privileged_command("resolvectl revert wlan0", false);
/// // Returns: "pkexec resolvectl revert wlan0" or "sudo resolvectl revert wlan0"
///
/// // Complex shell command
/// let cmd = wrap_privileged_command("resolvectl dns wlan0 '1.1.1.1' && resolvectl dnsovertls wlan0 yes", true);
/// // Returns: "pkexec sh -c '...'" or "sudo sh -c '...'"
/// ```
pub fn wrap_privileged_command(command: &str, use_shell: bool) -> String {
    let priv_cmd = get_privilege_command();

    if use_shell && priv_cmd == "pkexec" {
        // pkexec needs sh -c for complex shell commands
        // Properly escape single quotes for shell: 'text' becomes '\''text'\''
        format!("{} sh -c '{}'", priv_cmd, command.replace('\'', r"'\''"))
    } else if use_shell {
        // sudo with sh -c for consistency
        // Properly escape single quotes for shell: 'text' becomes '\''text'\''
        format!("{} sh -c '{}'", priv_cmd, command.replace('\'', r"'\''"))
    } else {
        // Simple command without shell
        format!("{} {}", priv_cmd, command)
    }
}

/// Wraps multiple commands that need to be run with privileges
///
/// # Arguments
/// * `commands` - Vector of commands to run sequentially
///
/// # Examples
/// ```
/// use network_dmenu::privilege::wrap_privileged_commands;
///
/// let cmds = vec![
///     "resolvectl dns wlan0 '1.1.1.1#cloudflare-dns.com'",
///     "resolvectl dnsovertls wlan0 yes"
/// ];
/// let cmd = wrap_privileged_commands(&cmds);
/// // Returns appropriate command for pkexec or sudo
/// ```
pub fn wrap_privileged_commands(commands: &[&str]) -> String {
    let priv_cmd = get_privilege_command();
    let joined = commands.join(" && ");

    if priv_cmd == "pkexec" {
        // pkexec needs sh -c for multiple commands
        // Properly escape single quotes for shell: 'text' becomes '\''text'\''
        format!("{} sh -c '{}'", priv_cmd, joined.replace('\'', r"'\''"))
    } else {
        // sudo can handle && directly, but we use sh -c for consistency
        // Properly escape single quotes for shell: 'text' becomes '\''text'\''
        format!("{} sh -c '{}'", priv_cmd, joined.replace('\'', r"'\''"))
    }
}

/// Check if the current user has privilege escalation available
///
/// Returns true if either pkexec or sudo is available and configured
pub fn has_privilege_escalation() -> bool {
    which("pkexec").is_ok() || which("sudo").is_ok()
}

/// Check if GUI privilege escalation (pkexec) is available
pub fn has_gui_privilege_escalation() -> bool {
    which("pkexec").is_ok()
}

/// Get a user-friendly description of the privilege method
pub fn get_privilege_method_description() -> &'static str {
    if which("pkexec").is_ok() {
        "GUI authentication (pkexec)"
    } else if which("sudo").is_ok() {
        "Terminal authentication (sudo)"
    } else {
        "No privilege escalation available"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_privilege_command() {
        let cmd = get_privilege_command();
        assert!(cmd == "pkexec" || cmd == "sudo");
    }

    #[test]
    fn test_wrap_simple_command() {
        let wrapped = wrap_privileged_command("resolvectl revert wlan0", false);
        assert!(wrapped.contains("resolvectl revert wlan0"));
        assert!(wrapped.contains("pkexec") || wrapped.contains("sudo"));
    }

    #[test]
    fn test_wrap_shell_command() {
        let wrapped = wrap_privileged_command("echo 'test' && echo 'done'", true);
        assert!(wrapped.contains("sh -c"));
        assert!(wrapped.contains("pkexec") || wrapped.contains("sudo"));
    }

    #[test]
    fn test_wrap_multiple_commands() {
        let commands = vec![
            "resolvectl dns wlan0 '1.1.1.1'",
            "resolvectl dnsovertls wlan0 yes",
        ];
        let wrapped = wrap_privileged_commands(&commands);
        assert!(wrapped.contains("&&"));
        assert!(wrapped.contains("sh -c"));
        assert!(wrapped.contains("pkexec") || wrapped.contains("sudo"));
    }

    #[test]
    fn test_escape_quotes() {
        let wrapped = wrap_privileged_command("resolvectl dns eth0 '8.8.8.8'", true);
        // Should properly escape quotes for shell
        assert!(wrapped.contains("sh -c"));
        // The single quotes around '8.8.8.8' should be escaped as '\''
        assert!(wrapped.contains("resolvectl dns eth0 '\\''8.8.8.8'\\''"));
    }

    #[test]
    fn test_has_privilege_escalation() {
        // This should be true on most systems with either sudo or pkexec
        let has_priv = has_privilege_escalation();
        // Test passed if we reach this point - function doesn't panic
        let _ = has_priv; // Use the variable to avoid unused variable warning
    }

    #[test]
    fn test_privilege_method_description() {
        let desc = get_privilege_method_description();
        assert!(desc.contains("pkexec") || desc.contains("sudo") || desc.contains("No privilege"));
    }
}

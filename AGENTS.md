# 🤖 AI Agent Integration Guide

This document provides instructions for AI agents, automation tools, and LLMs to effectively work with the network-dmenu codebase.

## 📋 Quick Overview

**Project**: network-dmenu  
**Language**: Rust  
**Purpose**: dmenu-based network management tool  
**Architecture**: Modular, functional programming style  

## 🎯 Key Principles

### Code Style
- **Functional Programming**: Use functional programming patterns, avoid mutations where possible
- **Error Handling**: Use `Result<T, E>` types, handle all error cases explicitly
- **Performance**: Optimize for speed, use iterators over loops, minimize allocations
- **Testing**: Every new feature must have comprehensive tests
- **Documentation**: All public functions need doc comments with examples

### Design Patterns
```rust
// Preferred: Functional style with iterators
let filtered_nodes = nodes.iter()
    .filter(|n| n.is_exit_node)
    .map(|n| format_node(n))
    .collect::<Vec<_>>();

// Avoid: Imperative style with mutations
let mut filtered_nodes = Vec::new();
for node in nodes {
    if node.is_exit_node {
        filtered_nodes.push(format_node(node));
    }
}
```

## 🏗️ Project Structure

```
network-dmenu/
├── src/
│   ├── main.rs              # Entry point, CLI argument parsing
│   ├── lib.rs               # Main library, action handling
│   ├── bluetooth.rs         # Bluetooth device management
│   ├── command.rs           # Command execution abstraction
│   ├── config.rs            # Configuration management
│   ├── diagnostics.rs       # Network diagnostic tools
│   ├── dns_cache.rs         # DNS caching functionality
│   ├── iwd.rs              # IWD WiFi backend
│   ├── networkmanager.rs   # NetworkManager backend
│   ├── nextdns.rs          # NextDNS integration
│   ├── privilege.rs        # Privilege escalation
│   ├── rfkill.rs           # Radio device control
│   ├── tailscale.rs        # Tailscale VPN management
│   ├── tailscale_prefs.rs  # Tailscale preferences
│   └── utils.rs            # Utility functions
├── Cargo.toml              # Dependencies and metadata
├── README.md               # User documentation
└── AGENTS.md              # This file
```

## 🔧 Common Tasks

### Adding a New Feature

1. **Identify the appropriate module** or create a new one
2. **Define the action enum** in the module:
```rust
#[derive(Debug, Clone)]
pub enum MyFeatureAction {
    Enable,
    Disable,
    Configure(String),
}
```

3. **Implement the handler**:
```rust
pub async fn handle_my_feature_action(
    action: &MyFeatureAction,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn std::error::Error>> {
    match action {
        MyFeatureAction::Enable => {
            // Implementation
            Ok(true)
        }
        // ... other cases
    }
}
```

4. **Add menu entries** in `get_my_feature_actions()`:
```rust
pub fn get_my_feature_actions() -> Vec<String> {
    vec![
        "🎯 Enable My Feature".to_string(),
        "🚫 Disable My Feature".to_string(),
    ]
}
```

5. **Wire it up** in lib.rs:
   - Add to `ActionType` enum
   - Update `parse_action()` function
   - Add case in `handle_action()`

6. **Add tests**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_my_feature_enable() {
        let mock_runner = MockCommandRunner::new(
            "mycommand",
            &["--enable"],
            Output {
                status: ExitStatus::from_raw(0),
                stdout: b"success".to_vec(),
                stderr: vec![],
            },
        );
        
        let result = handle_my_feature_action(
            &MyFeatureAction::Enable,
            &mock_runner
        );
        assert!(result.is_ok());
    }
}
```

### Modifying Existing Features

1. **Locate the module** (e.g., `tailscale.rs` for Tailscale features)
2. **Find the relevant function** (use grep: `grep -n "function_name" src/*.rs`)
3. **Make changes following the existing patterns**
4. **Update or add tests**
5. **Run tests**: `cargo test`
6. **Check formatting**: `cargo fmt`
7. **Run linter**: `cargo clippy`

### Working with Dependencies

```toml
# In Cargo.toml, add dependencies with specific versions
[dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }
```

## 🧪 Testing Guidelines

### Unit Tests
- Place tests in the same file as the code
- Use `MockCommandRunner` for command execution
- Test both success and failure cases
- Use descriptive test names

### Test Structure
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandRunner;
    use std::process::{ExitStatus, Output};
    use std::os::unix::process::ExitStatusExt;

    struct MockCommandRunner {
        expected_cmd: String,
        expected_args: Vec<String>,
        output: Output,
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, command: &str, args: &[&str]) -> std::io::Result<Output> {
            // Validation and return mock output
        }
    }

    #[test]
    fn test_feature_success_case() {
        // Arrange
        let mock = setup_mock();
        
        // Act
        let result = function_under_test(&mock);
        
        // Assert
        assert!(result.is_ok());
    }
}
```

## 📝 Documentation Standards

### Function Documentation
```rust
/// Brief description of what the function does.
///
/// # Arguments
///
/// * `param1` - Description of first parameter
/// * `param2` - Description of second parameter
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// Description of possible errors
///
/// # Examples
///
/// ```
/// let result = my_function("input", 42);
/// assert_eq!(result, expected);
/// ```
pub fn my_function(param1: &str, param2: i32) -> Result<String, Error> {
    // Implementation
}
```

### Module Documentation
```rust
//! Module-level documentation goes here.
//! 
//! This module handles X functionality and provides Y capabilities.
//! 
//! # Examples
//! 
//! ```
//! use network_dmenu::my_module;
//! ```
```

## 🚀 Performance Considerations

### Do's
- Use `&str` instead of `String` when possible
- Use iterators and functional chains
- Cache expensive operations
- Use `Cow<str>` for strings that may or may not be modified
- Parallelize independent operations with `tokio::spawn`

### Don'ts
- Avoid unnecessary cloning
- Don't use `.unwrap()` in production code (except in tests)
- Avoid nested loops when possible
- Don't block the async runtime with synchronous I/O

## 🔍 Debugging Tips

### Enable Debug Logging
```bash
RUST_LOG=debug cargo run
```

### Use Debug Prints in Tests
```rust
#[test]
fn test_something() {
    let value = compute_something();
    dbg!(&value);  // Prints to stderr during test
    assert_eq!(value, expected);
}
```

### Trace Command Execution
The `CommandRunner` trait allows injecting mock implementations for testing without actual command execution.

## 🎨 UI/Menu Conventions

### Icon Usage
- 🌐 Network/Internet related
- 📶 WiFi/Signal strength
- 🔒 Security/Lock/VPN
- 🎧 Bluetooth audio
- 📱 Mobile/Device
- ⚡ Speed/Performance
- 🚀 Fast actions
- ⚠️ Warnings
- ❌ Errors/Disable
- ✅ Success/Enable
- 📊 Statistics/Monitoring
- 🔧 Configuration/Settings

### Menu Entry Format
```
"<emoji> <Action Description> [<status>]"
```

Examples:
- "🌐 Connect to WiFi Network"
- "🔒 Enable Tailscale VPN"
- "📶 WiFi Network (Signal: 75%)"
- "🎧 Bluetooth Headphones [Connected]"

## 🔒 Security Guidelines

### Password Handling
- Never log passwords
- Use `pinentry` for password prompts
- Clear sensitive data from memory after use

### Privilege Escalation
- Use the `privilege` module for operations requiring elevated permissions
- Support multiple methods: sudo, pkexec, doas
- Always validate and sanitize user input before passing to shell commands

### Command Injection Prevention
```rust
// Good: Use array of arguments
command_runner.run_command("nmcli", &["device", "wifi", "connect", &ssid]);

// Bad: String concatenation
command_runner.run_command("sh", &["-c", &format!("nmcli device wifi connect {}", ssid)]);
```

## 📦 Release Checklist

When preparing changes for release:

1. **Update version** in `Cargo.toml`
2. **Run full test suite**: `cargo test`
3. **Check formatting**: `cargo fmt -- --check`
4. **Run linter**: `cargo clippy -- -D warnings`
5. **Build release**: `cargo build --release`
6. **Test release binary**: `./target/release/network-dmenu`
7. **Update documentation** if needed
8. **Add entry to CHANGELOG** (if exists)

## 🐛 Common Issues and Solutions

### Issue: Tests failing with "Unexpected command call"
**Solution**: Ensure `MockCommandRunner` is set up with all expected commands:
```rust
let mock_runner = MockCommandRunner::with_multiple_calls(vec![
    ("command1", &["arg1"], output1),
    ("command2", &["arg2"], output2),
]);
```

### Issue: Compilation errors with missing fields
**Solution**: Check struct definitions and ensure all fields are initialized:
```rust
let state = TailscaleState {
    status: status,
    active_exit_node: String::new(),
    suggested_exit_node: String::new(),
    lock_output: None,
    can_sign_nodes: false,        // Don't forget these!
    node_signing_key: None,
};
```

### Issue: Async runtime errors
**Solution**: Ensure async functions are properly awaited and use `tokio::test` for async tests:
```rust
#[tokio::test]
async fn test_async_function() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

## 🤝 Collaboration with Humans

When working on this codebase:

1. **Ask for clarification** when requirements are unclear
2. **Suggest improvements** when you see opportunities
3. **Explain your changes** with clear commit messages
4. **Document edge cases** you discover
5. **Report potential issues** even if not directly related to current task

## 📚 External Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust Async Book](https://rust-lang.github.io/async-book/)
- [Tailscale API Docs](https://tailscale.com/kb/api/)
- [NetworkManager Documentation](https://networkmanager.dev/)
- [IWD Documentation](https://iwd.wiki.kernel.org/)

## 🎯 Quick Command Reference

```bash
# Build and test
cargo build
cargo test
cargo run -- --help

# Development workflow
cargo watch -x test                   # Auto-run tests on file change
cargo watch -x 'run -- --no-tailscale' # Auto-run with specific args

# Code quality
cargo fmt                             # Format code
cargo clippy                          # Lint code
cargo doc --open                      # Generate and open docs

# Debugging
RUST_LOG=debug cargo run              # Run with debug output
RUST_BACKTRACE=1 cargo run           # Show backtraces on panic
cargo test -- --nocapture             # Show test output

# Performance
cargo build --release                 # Build optimized binary
cargo bench                           # Run benchmarks (if any)
```

---

*This guide is maintained for AI agents and developers. Keep it updated as the codebase evolves.*
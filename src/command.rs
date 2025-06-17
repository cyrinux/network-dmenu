use crate::constants::{COMMAND_TIMEOUT, LC_ALL_ENV, LC_ALL_VALUE};
use crate::errors::{NetworkMenuError, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, error, trace, warn};

/// Trait for running shell commands asynchronously
#[async_trait]
pub trait CommandRunner: Send + Sync {
    /// Runs a shell command with the specified arguments
    async fn run_command(&self, command: &str, args: &[&str]) -> Result<Output>;
    
    /// Runs a command with custom timeout
    async fn run_command_with_timeout(
        &self,
        command: &str,
        args: &[&str],
        timeout_duration: Duration,
    ) -> Result<Output>;
    
    /// Checks if a command is available on the system
    async fn is_command_available(&self, command: &str) -> bool;
    
    /// Executes a command and returns only success/failure status
    async fn execute_command(&self, command: &str, args: &[&str]) -> bool;
}

/// Real command runner that executes actual system commands
#[derive(Debug, Clone)]
pub struct RealCommandRunner {
    default_timeout: Duration,
}

impl RealCommandRunner {
    pub fn new() -> Self {
        Self {
            default_timeout: COMMAND_TIMEOUT,
        }
    }
    
    pub fn with_timeout(timeout_duration: Duration) -> Self {
        Self {
            default_timeout: timeout_duration,
        }
    }
}

impl Default for RealCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandRunner for RealCommandRunner {
    async fn run_command(&self, command: &str, args: &[&str]) -> Result<Output> {
        self.run_command_with_timeout(command, args, self.default_timeout)
            .await
    }

    async fn run_command_with_timeout(
        &self,
        command: &str,
        args: &[&str],
        timeout_duration: Duration,
    ) -> Result<Output> {
        let args_str = args.join(" ");
        debug!("Executing command: {} {}", command, args_str);
        trace!("Command timeout: {:?}", timeout_duration);

        let mut cmd = Command::new(command);
        cmd.args(args)
            .env(LC_ALL_ENV, LC_ALL_VALUE)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let future = cmd.output();
        
        match timeout(timeout_duration, future).await {
            Ok(Ok(output)) => {
                let success = output.status.success();
                let stdout_len = output.stdout.len();
                let stderr_len = output.stderr.len();
                
                if success {
                    debug!(
                        "Command succeeded: {} {} (stdout: {} bytes, stderr: {} bytes)",
                        command, args_str, stdout_len, stderr_len
                    );
                } else {
                    let exit_code = output.status.code().unwrap_or(-1);
                    let stderr_preview = String::from_utf8_lossy(&output.stderr);
                    let stderr_preview = if stderr_preview.len() > 200 {
                        format!("{}...", &stderr_preview[..200])
                    } else {
                        stderr_preview.to_string()
                    };
                    
                    warn!(
                        "Command failed: {} {} (exit code: {}, stderr: {})",
                        command, args_str, exit_code, stderr_preview
                    );
                }
                
                Ok(output)
            }
            Ok(Err(io_error)) => {
                error!("Command IO error: {} {} - {}", command, args_str, io_error);
                Err(NetworkMenuError::command_failed(
                    format!("{} {}", command, args_str),
                    format!("IO error: {}", io_error),
                ))
            }
            Err(_) => {
                error!("Command timeout: {} {} (timeout: {:?})", command, args_str, timeout_duration);
                Err(NetworkMenuError::timeout_error(
                    format!("Command execution: {} {}", command, args_str),
                ))
            }
        }
    }

    async fn is_command_available(&self, command: &str) -> bool {
        debug!("Checking if command is available: {}", command);
        
        match which::which(command) {
            Ok(path) => {
                trace!("Command found at: {:?}", path);
                true
            }
            Err(_) => {
                trace!("Command not found: {}", command);
                false
            }
        }
    }

    async fn execute_command(&self, command: &str, args: &[&str]) -> bool {
        match self.run_command(command, args).await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
}

/// Mock command runner for testing
#[derive(Debug, Clone)]
pub struct MockCommandRunner {
    responses: HashMap<String, Output>,
    available_commands: Vec<String>,
    default_success: bool,
}

impl MockCommandRunner {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
            available_commands: Vec::new(),
            default_success: true,
        }
    }
    
    pub fn with_response(mut self, command: &str, args: &[&str], output: Output) -> Self {
        let key = format!("{} {}", command, args.join(" "));
        self.responses.insert(key, output);
        self
    }
    
    pub fn with_available_command(mut self, command: impl Into<String>) -> Self {
        self.available_commands.push(command.into());
        self
    }
    
    pub fn with_default_success(mut self, success: bool) -> Self {
        self.default_success = success;
        self
    }
    
    pub fn add_response(&mut self, command: &str, args: &[&str], output: Output) {
        let key = format!("{} {}", command, args.join(" "));
        self.responses.insert(key, output);
    }
    
    pub fn add_available_command(&mut self, command: impl Into<String>) {
        self.available_commands.push(command.into());
    }
}

impl Default for MockCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandRunner for MockCommandRunner {
    async fn run_command(&self, command: &str, args: &[&str]) -> Result<Output> {
        self.run_command_with_timeout(command, args, Duration::from_secs(1))
            .await
    }

    async fn run_command_with_timeout(
        &self,
        command: &str,
        args: &[&str],
        _timeout_duration: Duration,
    ) -> Result<Output> {
        let key = format!("{} {}", command, args.join(" "));
        
        if let Some(output) = self.responses.get(&key) {
            Ok(output.clone())
        } else {
            // Return default success/failure response
            use std::os::unix::process::ExitStatusExt;
            let status = if self.default_success {
                std::process::ExitStatus::from_raw(0)
            } else {
                std::process::ExitStatus::from_raw(256) // Non-zero exit code
            };
            
            Ok(Output {
                status,
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    }

    async fn is_command_available(&self, command: &str) -> bool {
        self.available_commands.contains(&command.to_string())
    }

    async fn execute_command(&self, command: &str, args: &[&str]) -> bool {
        match self.run_command(command, args).await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
}

/// Helper functions for command output processing
pub mod output {
    use crate::errors::{NetworkMenuError, Result};
    use std::io::{BufRead, BufReader};
    use std::process::Output;

    /// Reads the stdout of a command output and returns it as a vector of lines
    pub fn read_lines(output: &Output) -> Result<Vec<String>> {
        BufReader::new(output.stdout.as_slice())
            .lines()
            .collect::<std::result::Result<Vec<String>, _>>()
            .map_err(|e| NetworkMenuError::parse_error(format!("Failed to read output lines: {}", e)))
    }

    /// Reads the stderr of a command output and returns it as a vector of lines
    pub fn read_error_lines(output: &Output) -> Result<Vec<String>> {
        BufReader::new(output.stderr.as_slice())
            .lines()
            .collect::<std::result::Result<Vec<String>, _>>()
            .map_err(|e| NetworkMenuError::parse_error(format!("Failed to read error lines: {}", e)))
    }

    /// Gets the stdout as a string
    pub fn stdout_string(output: &Output) -> Result<String> {
        String::from_utf8(output.stdout.clone())
            .map_err(|e| NetworkMenuError::parse_error(format!("Invalid UTF-8 in stdout: {}", e)))
    }

    /// Gets the stderr as a string
    pub fn stderr_string(output: &Output) -> Result<String> {
        String::from_utf8(output.stderr.clone())
            .map_err(|e| NetworkMenuError::parse_error(format!("Invalid UTF-8 in stderr: {}", e)))
    }

    /// Checks if the command output indicates success
    pub fn is_success(output: &Output) -> bool {
        output.status.success()
    }

    /// Gets the exit code from the output
    pub fn exit_code(output: &Output) -> Option<i32> {
        output.status.code()
    }
}

/// Command builder for easier command construction
#[derive(Debug, Clone)]
pub struct CommandBuilder {
    command: String,
    args: Vec<String>,
    timeout: Option<Duration>,
}

impl CommandBuilder {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            timeout: None,
        }
    }
    
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }
    
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(|s| s.into()));
        self
    }
    
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    
    pub async fn execute(self, runner: &dyn CommandRunner) -> Result<Output> {
        let args_refs: Vec<&str> = self.args.iter().map(|s| s.as_str()).collect();
        
        if let Some(timeout) = self.timeout {
            runner.run_command_with_timeout(&self.command, &args_refs, timeout).await
        } else {
            runner.run_command(&self.command, &args_refs).await
        }
    }
    
    pub async fn execute_success(self, runner: &dyn CommandRunner) -> bool {
        match self.execute(runner).await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
}

/// Batch command executor for running multiple commands
pub struct BatchCommandExecutor<'a> {
    runner: &'a dyn CommandRunner,
    commands: Vec<CommandBuilder>,
    fail_fast: bool,
}

impl<'a> BatchCommandExecutor<'a> {
    pub fn new(runner: &'a dyn CommandRunner) -> Self {
        Self {
            runner,
            commands: Vec::new(),
            fail_fast: true,
        }
    }
    
    pub fn add_command(mut self, command: CommandBuilder) -> Self {
        self.commands.push(command);
        self
    }
    
    pub fn fail_fast(mut self, fail_fast: bool) -> Self {
        self.fail_fast = fail_fast;
        self
    }
    
    pub async fn execute_all(self) -> Result<Vec<Output>> {
        let mut results = Vec::new();
        
        for command in self.commands {
            match command.execute(self.runner).await {
                Ok(output) => {
                    let success = output.status.success();
                    results.push(output);
                    
                    if !success && self.fail_fast {
                        return Err(NetworkMenuError::command_failed(
                            "Batch execution",
                            "Command failed and fail_fast is enabled",
                        ));
                    }
                }
                Err(e) => {
                    if self.fail_fast {
                        return Err(e);
                    }
                    // Continue execution but record the error
                    error!("Command in batch failed: {}", e);
                }
            }
        }
        
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn create_mock_output(success: bool, stdout: &str, stderr: &str) -> Output {
        use std::process::ExitStatus;
        use std::os::unix::process::ExitStatusExt;
        
        Output {
            status: if success {
                ExitStatus::from_raw(0)
            } else {
                ExitStatus::from_raw(256) // Non-zero exit code
            },
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[tokio::test]
    async fn test_real_command_runner() {
        let runner = RealCommandRunner::new();
        
        // Test successful command
        let result = runner.run_command("echo", &["hello"]).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        assert!(output.status.success());
        assert_eq!(output::stdout_string(&output).unwrap().trim(), "hello");
    }

    #[tokio::test]
    async fn test_mock_command_runner() {
        let mock_output = create_mock_output(true, "test output", "");
        let runner = MockCommandRunner::new()
            .with_response("test", &["arg"], mock_output)
            .with_available_command("test");
        
        assert!(runner.is_command_available("test").await);
        assert!(!runner.is_command_available("nonexistent").await);
        
        let result = runner.run_command("test", &["arg"]).await;
        assert!(result.is_ok());
        
        let output = result.unwrap();
        assert!(output.status.success());
        assert_eq!(output::stdout_string(&output).unwrap(), "test output");
    }

    #[tokio::test]
    async fn test_command_builder() {
        let mock_output = create_mock_output(true, "builder output", "");
        let runner = MockCommandRunner::new()
            .with_response("echo", &["hello", "world"], mock_output);
        
        let result = CommandBuilder::new("echo")
            .arg("hello")
            .arg("world")
            .execute(&runner)
            .await;
        
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output::stdout_string(&output).unwrap(), "builder output");
    }

    #[tokio::test]
    async fn test_batch_executor() {
        let mock_output1 = create_mock_output(true, "output1", "");
        let mock_output2 = create_mock_output(true, "output2", "");
        
        let runner = MockCommandRunner::new()
            .with_response("cmd1", &[], mock_output1)
            .with_response("cmd2", &[], mock_output2);
        
        let results = BatchCommandExecutor::new(&runner)
            .add_command(CommandBuilder::new("cmd1"))
            .add_command(CommandBuilder::new("cmd2"))
            .execute_all()
            .await;
        
        assert!(results.is_ok());
        let outputs = results.unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(output::stdout_string(&outputs[0]).unwrap(), "output1");
        assert_eq!(output::stdout_string(&outputs[1]).unwrap(), "output2");
    }
}
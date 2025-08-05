use std::error::Error;
use std::io::{BufRead, BufReader};
use std::process::{Command, Output, Stdio};

/// Trait for running shell commands.
pub trait CommandRunner {
    /// Runs a shell command with the specified arguments.
    fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error>;
}

/// Struct for running real shell commands.
pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error> {
        Command::new(command).args(args).env("LC_ALL", "C").output()
    }
}

/// Checks if a command is installed on the system.
pub fn is_command_installed(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

/// Reads the output of a command and returns it as a vector of lines.
pub fn read_output_lines(output: &Output) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(BufReader::new(output.stdout.as_slice())
        .lines()
        .collect::<Result<Vec<String>, _>>()?)
}

/// Executes a command and returns whether it was successful.
pub fn execute_command(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .env("LC_ALL", "C")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    /// Mock command runner for testing
    pub struct MockCommandRunner {
        pub expected_command: String,
        pub expected_args: Vec<String>,
        pub return_output: Output,
    }

    impl MockCommandRunner {
        pub fn new(command: &str, args: &[&str], output: Output) -> Self {
            Self {
                expected_command: command.to_string(),
                expected_args: args.iter().map(|s| s.to_string()).collect(),
                return_output: output,
            }
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error> {
            assert_eq!(command, self.expected_command);
            assert_eq!(args, self.expected_args.as_slice());
            Ok(Output {
                status: self.return_output.status,
                stdout: self.return_output.stdout.clone(),
                stderr: self.return_output.stderr.clone(),
            })
        }
    }

    #[test]
    fn test_real_command_runner_with_echo() {
        let runner = RealCommandRunner;
        let result = runner.run_command("echo", &["hello", "world"]);

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "hello world"
        );
    }

    #[test]
    fn test_real_command_runner_with_invalid_command() {
        let runner = RealCommandRunner;
        let result = runner.run_command("nonexistent_command_12345", &[]);

        assert!(result.is_err());
    }

    #[test]
    fn test_mock_command_runner() {
        let expected_stdout = b"test output";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: expected_stdout.to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("test_cmd", &["arg1", "arg2"], output);
        let result = mock_runner.run_command("test_cmd", &["arg1", "arg2"]);

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, expected_stdout);
    }

    #[test]
    #[should_panic]
    fn test_mock_command_runner_wrong_command() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("expected_cmd", &[], output);
        let _ = mock_runner.run_command("wrong_cmd", &[]);
    }

    #[test]
    #[should_panic]
    fn test_mock_command_runner_wrong_args() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("cmd", &["arg1"], output);
        let _ = mock_runner.run_command("cmd", &["arg2"]);
    }

    #[test]
    fn test_is_command_installed_existing() {
        // Test with a command that should exist on most systems
        assert!(is_command_installed("echo"));
    }

    #[test]
    fn test_is_command_installed_nonexistent() {
        // Test with a command that definitely doesn't exist
        assert!(!is_command_installed("nonexistent_command_xyz_12345"));
    }

    #[test]
    fn test_read_output_lines_success() {
        let stdout = b"line1\nline2\nline3\n";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };

        let result = read_output_lines(&output);
        assert!(result.is_ok());

        let lines = result.unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], "line3");
    }

    #[test]
    fn test_read_output_lines_empty() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let result = read_output_lines(&output);
        assert!(result.is_ok());

        let lines = result.unwrap();
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_read_output_lines_single_line() {
        let stdout = b"single line";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };

        let result = read_output_lines(&output);
        assert!(result.is_ok());

        let lines = result.unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "single line");
    }

    #[test]
    fn test_execute_command_success() {
        // Test with echo command which should succeed
        let result = execute_command("echo", &["test"]);
        assert!(result);
    }

    #[test]
    fn test_execute_command_failure() {
        // Test with nonexistent command
        let result = execute_command("nonexistent_command_xyz_12345", &[]);
        assert!(!result);
    }

    #[test]
    fn test_execute_command_with_args() {
        // Test with a command that takes arguments
        let result = execute_command("echo", &["hello", "world"]);
        assert!(result);
    }
}

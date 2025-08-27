//! Logging utilities for network-dmenu
//!
//! This module provides a configurable logging system that's quiet by default
//! but can be enabled for debugging. It also includes profiling functionality
//! to track operation durations.
use chrono::Local;
use env_logger::{Builder, Env};
use log::{debug, log_enabled, Level};
use std::io::Write;
use std::time::{Duration, Instant};

/// Initialize the logging system
///
/// This sets up the logger to be:
/// - Quiet by default (only warnings and errors)
/// - Configurable via RUST_LOG environment variable
/// - Formatted with timestamps and log levels
pub fn init() {
    let env = Env::default().filter_or("RUST_LOG", "warn");

    Builder::from_env(env)
        .format(|buf, record| {
            let level_style = buf.default_level_style(record.level());

            writeln!(
                buf,
                "{} {} {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                level_style,
                record.args()
            )
        })
        .init();

    debug!("Logger initialized");
}

/// A simple profiler for measuring operation durations
pub struct Profiler {
    name: String,
    start: Instant,
    enabled: bool,
}

#[allow(dead_code)]
impl Profiler {
    /// Create a new profiler with the given operation name
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            start: Instant::now(),
            enabled: log_enabled!(target: module_path!(), Level::Debug),
        }
    }

    /// Log the elapsed time since profiler creation with a custom message
    pub fn log_with_message(&self, message: &str) {
        if self.enabled {
            let elapsed = self.start.elapsed();
            debug!(
                "PROFILE: {} {} took: {}",
                message,
                self.name,
                format_duration(elapsed)
            );
        }
    }

    /// Log the elapsed time since profiler creation
    pub fn log(&self) {
        if self.enabled {
            let elapsed = self.start.elapsed();
            debug!("PROFILE: {} took: {}", self.name, format_duration(elapsed));
        }
    }

    /// Reset the profiler start time
    pub fn reset(&mut self) {
        self.start = Instant::now();
    }

    /// Get the elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Format a duration in a human-readable way
fn format_duration(duration: Duration) -> String {
    let nanos = duration.as_nanos();

    if nanos < 1_000 {
        format!("{}ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.2}µs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2}ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2}s", nanos as f64 / 1_000_000_000.0)
    }
}

/// Macro for creating and logging a profiler in one line
#[macro_export]
macro_rules! profile_operation {
    ($name:expr, $operation:expr) => {{
        let profiler = $crate::logger::Profiler::new($name);
        let result = $operation;
        profiler.log();
        result
    }};
}

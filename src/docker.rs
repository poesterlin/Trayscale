use std::process::Command;
use thiserror::Error;

/// Represents the parsed status of the Docker daemon.
#[derive(Clone, Debug, Default)]
pub struct DockerStatus {
    /// The raw, multi-line output from the `systemctl status` command.
    pub raw_output: String,
    /// The value of the "Active" field, e.g., "active (running)" or "inactive (dead)".
    pub active_state: String,
    /// A simple boolean indicating if the service is currently running.
    pub is_active: bool,
    /// The value of the "Loaded" field.
    pub loaded_state: String,
    /// The main process ID of the daemon, if it's running.
    pub main_pid: Option<u32>,
    /// The peak memory usage reported by systemd.
    pub memory_peak: Option<String>,
    /// The total CPU time consumed, as reported by systemd.
    pub cpu_time: Option<String>,
}

/// Defines the possible errors that can occur when interacting with the Docker daemon.
#[derive(Error, Debug)]
pub enum DockerError {
    #[error("Failed to execute systemctl command: {0}")]
    CommandError(#[from] std::io::Error),

    #[error("Sudo/systemctl command failed with stderr: {0}")]
    CommandFailed(String),

    #[error("Failed to parse systemctl output: {0}")]
    ParseError(String),
}

/// A simple wrapper for controlling the Docker daemon via `systemctl`.
///
/// This wrapper requires that the user has passwordless sudo access
/// to the specific `systemctl start docker` and `systemctl stop docker` commands.
pub struct Docker;

impl Docker {
    /// Starts the Docker daemon by running `sudo systemctl start docker`.
    ///
    /// # Prerequisites
    /// Requires passwordless `sudo` access configured in `/etc/sudoers`.
    pub fn start() -> Result<(), DockerError> {
        let output = Command::new("sudo")
            .args(["systemctl", "start", "docker"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(DockerError::CommandFailed(stderr));
        }
        Ok(())
    }

    /// Stops the Docker daemon by running `sudo systemctl stop docker`.
    ///
    /// # Prerequisites
    /// Requires passwordless `sudo` access configured in `/etc/sudoers`.
    pub fn stop() -> Result<(), DockerError> {
        let output = Command::new("sudo")
            .args(["systemctl", "stop", "docker"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(DockerError::CommandFailed(stderr));
        }
        Ok(())
    }

    /// Toggles the Docker daemon's state. Starts it if it's inactive, stops it if it's active.
    pub fn toggle() -> Result<(), DockerError> {
        if Self::is_active()? {
            Self::stop()
        } else {
            Self::start()
        }
    }

    /// Gets the detailed status of the Docker daemon by running `systemctl status docker`.
    /// This command does not require `sudo`.
    pub fn status() -> Result<DockerStatus, DockerError> {
        // `systemctl status` returns a non-zero exit code when the service is inactive.
        // We must capture the output regardless of the exit code.
        let output = Command::new("systemctl")
            .args(["status", "docker"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut status = DockerStatus {
            raw_output: stdout.clone(),
            ..Default::default()
        };

        for line in stdout.lines() {
            let trimmed = line.trim();
            if let Some(value) = Self::parse_line_value(trimmed, "Active:") {
                status.active_state = value.to_string();
                status.is_active = status.active_state.contains("(running)");
            } else if let Some(value) = Self::parse_line_value(trimmed, "Loaded:") {
                status.loaded_state = value.to_string();
            } else if let Some(value) = Self::parse_line_value(trimmed, "Main PID:") {
                status.main_pid = value.split_whitespace().next().and_then(|v| v.parse().ok());
            } else if let Some(value) = Self::parse_line_value(trimmed, "Mem peak:") {
                status.memory_peak = Some(value.to_string());
            } else if let Some(value) = Self::parse_line_value(trimmed, "CPU:") {
                status.cpu_time = Some(value.to_string());
            }
        }

        Ok(status)
    }

    /// Checks if the Docker daemon is currently active and running.
    pub fn is_active() -> Result<bool, DockerError> {
        let status = Self::status()?;
        Ok(status.is_active)
    }

    /// Helper function to parse a "Key: Value" line.
    fn parse_line_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
        line.strip_prefix(key).map(|v| v.trim())
    }
}
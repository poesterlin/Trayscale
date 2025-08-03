use std::process::{Command, Stdio};
use thiserror::Error;

/// Represents the parsed status of the Docker daemon.
#[derive(Clone, Debug, Default)]
pub struct DockerStatus {
    /// The raw, multi-line output from the `systemctl status` command.
    pub raw_output: String,
    /// The value of the "Active" field, e.g., "active (running)" or
    /// "inactive (dead)".
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

/// Defines the possible errors that can occur when interacting with the Docker
/// daemon.
#[derive(Error, Debug)]
pub enum DockerError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("command failed (code {code:?}): {stderr}")]
    Failed { code: Option<i32>, stderr: String },
}

/// A simple wrapper for controlling the Docker daemon via `systemctl`.
///
/// By default, this tries pkexec (GUI auth) and falls back to sudo (TTY) for
/// start/stop.
pub struct Docker;

impl Docker {
    const SYSTEMCTL: &'static str = "/bin/systemctl"; // adjust if needed
    const SERVICE: &'static str = "docker";

    /// Starts the Docker daemon by running `systemctl start docker` as root.
    pub fn start() -> Result<(), DockerError> {
        Self::run_as_root(Self::SYSTEMCTL, &["start", Self::SERVICE])
    }

    /// Stops the Docker daemon by running `systemctl stop docker` as root.
    pub fn stop() -> Result<(), DockerError> {
        Self::run_as_root(Self::SYSTEMCTL, &["stop", Self::SERVICE])
    }

    /// Toggles the Docker daemon's state. Starts if inactive, stops if active.
    pub fn toggle() -> Result<(), DockerError> {
        if Self::is_active()? {
            Self::stop()
        } else {
            Self::start()
        }
    }

    /// Gets the detailed status of the Docker daemon by running
    /// `systemctl status docker`. This does not require root.
    pub fn status() -> Result<DockerStatus, DockerError> {
        // `systemctl status` returns non-zero when inactive; still parse output.
        let output = Command::new(Self::SYSTEMCTL)
            .args(["status", Self::SERVICE, "--no-pager"])
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
                // More robust: check for "active (running)" specifically
                status.is_active = value.contains("active (running)");
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

    /// Helper: parse a "Key: Value" line.
    fn parse_line_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
        line.strip_prefix(key).map(|v| v.trim())
    }

    /// Run a command as root, preferring pkexec (GUI) and falling back to sudo.
    fn run_as_root(cmd: &str, args: &[&str]) -> Result<(), DockerError> {
        // GUI dialog via pkexec
        let out = Command::new("pkexec")
            .arg(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::piped())
            .output();

        match out {
            Ok(o) if o.status.success() => Ok(()),
            Ok(o) => Err(DockerError::Failed {
                code: o.status.code(),
                stderr: String::from_utf8_lossy(&o.stderr).to_string(),
            }),
            Err(e) => Err(DockerError::Io(e)),
        }
    }
}

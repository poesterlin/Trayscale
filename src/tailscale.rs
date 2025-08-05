use std::process::Command;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct MachineData {
    pub ip: String,
    pub hostname: String,
    pub online: bool,
    pub is_exit_node: bool,
    pub advertises_exit_node: bool,
    user: String,
    os: String,
    details: String,
}

/// Defines the possible errors that can occur when interacting with the Tailscale CLI.
#[derive(Error, Debug)]
pub enum TailscaleError {
    #[error("Failed to execute tailscale command: {0}")]
    CommandError(#[from] std::io::Error),

    #[error("Tailscale command failed with stderr: {0}")]
    CommandFailed(String),

    #[error("Failed to parse tailscale output: {0}")]
    ParseError(String),

    #[error("Tailscale daemon is stopped.")]
    DaemonStopped, // Keep this error variant for specific status checks
}

/// A simple wrapper for the Tailscale CLI.
pub struct Tailscale;

impl Tailscale {
    /// Enables Tailscale by running `tailscale up`.
    /// Requires `sudo` or running as root.
    pub fn up() -> Result<(), TailscaleError> {
        let output = Command::new("tailscale").arg("up").output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(TailscaleError::CommandFailed(stderr));
        }
        Ok(())
    }

    /// Disables Tailscale by running `tailscale down`.
    pub fn down() -> Result<(), TailscaleError> {
        let output = Command::new("tailscale").arg("down").output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(TailscaleError::CommandFailed(stderr));
        }
        Ok(())
    }

    pub fn deselect_exit_node() -> Result<(), TailscaleError> {
        Self::set_exit_node("")
    }

    pub fn toggle() -> Result<(), TailscaleError> {
        if Tailscale::is_enabled().unwrap_or(false) {
            Tailscale::down()
        } else {
            Tailscale::up()
        }
    }

    pub fn set_exit_node(hostname: &str) -> Result<(), TailscaleError> {
        let arg = format!("--exit-node={}", hostname);
        let output = Command::new("tailscale").arg("set").arg(arg).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(TailscaleError::CommandFailed(stderr));
        }

        Ok(())
    }

    /// Gets the status of all machines in the network by running `tailscale status`.
    pub fn status() -> Result<Vec<MachineData>, TailscaleError> {
        let output = Command::new("tailscale").arg("status").output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(TailscaleError::CommandFailed(stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim() == "Tailscale is stopped." {
            return Err(TailscaleError::DaemonStopped);
        }

        let mut machines = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();

            // A valid machine line has at least 4 parts: IP, Hostname, User, OS
            if parts.len() < 4 {
                continue;
            }

            let details = parts[4..].join(" ");
            let machine = MachineData {
                ip: parts[0].into(),
                hostname: parts[1].into(),
                user: parts[2].into(),
                os: parts[3].into(),
                online: !details.contains("offline"),
                is_exit_node: details.contains("exit node") && !details.contains("offers exit node"),
                advertises_exit_node: details.contains("exit node"),
                details: details.into(),
            };

            if machine.online {
                machines.insert(0, machine);
            } else {
                machines.push(machine);
            }
        }

        Ok(machines)
    }

    /// A convenience function to get only the online machines.
    pub fn online_machines() -> Result<Vec<MachineData>, TailscaleError> {
        let machines = Self::status()?;
        let online = machines.into_iter().filter(|m| m.online).collect();
        Ok(online)
    }

    /// Checks if the Tailscale daemon is currently running and enabled.
    /// Returns `true` if it's running (i.e., `tailscale status` does not report "stopped"),
    /// `false` otherwise, or an error if the command itself fails to execute.
    pub fn is_enabled() -> Result<bool, TailscaleError> {
        let output = Command::new("tailscale").arg("status").output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            // If the command failed, it's probably not enabled or there's a serious issue.
            // Distinguish between command failure and the daemon being explicitly stopped.
            if stderr.contains("Tailscale is not running")
                || stderr.contains("Cannot connect to the Tailscale daemon")
            {
                Ok(false) // Consider it not enabled if it reports not running or connection issues
            } else {
                Err(TailscaleError::CommandFailed(stderr))
            }
        } else {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.trim() != "Tailscale is stopped.")
        }
    }
}

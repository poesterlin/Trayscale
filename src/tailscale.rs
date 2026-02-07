use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct MachineData {
    pub ip: String,
    pub hostname: String,
    pub online: bool,
    pub is_exit_node: bool,
}

#[derive(Clone, Debug)]
pub struct ExitNodeData {
    pub hostname: String,
    pub ip: String,
}

pub struct VPNNodeData {
    pub hostname: String,
    pub country: String,
    pub city: String,
    pub is_exit_node: bool,
}

pub enum ExitNode {
    VPN(VPNNodeData),
    Machine(ExitNodeData),
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TailscaleStatus {
    backend_state: String,
    #[serde(default, rename = "Self")]
    self_node: Option<PeerInfo>,
    #[serde(default)]
    peer: HashMap<String, PeerInfo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PeerInfo {
    host_name: String,
    #[serde(default, rename = "DNSName")]
    dns_name: String,
    #[serde(default)]
    tailscale_i_ps: Vec<String>,
    online: bool,
    exit_node: bool,
    exit_node_option: bool,
    location: Option<PeerLocation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PeerLocation {
    country: String,
    city: String,
}

/// Defines the possible errors that can occur when interacting with the Tailscale CLI.
#[derive(Error, Debug)]
pub enum TailscaleError {
    #[error("Failed to execute tailscale command: {0}")]
    CommandError(#[from] std::io::Error),

    #[error("Tailscale command failed with stderr: {0}")]
    CommandFailed(String),

    #[error("Tailscale daemon is stopped.")]
    DaemonStopped,

    #[error("Failed to parse JSON: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// A simple wrapper for the Tailscale CLI.
pub struct Tailscale;

fn get_status_json() -> Result<TailscaleStatus, TailscaleError> {
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.contains("Tailscale is not running")
            || stderr.contains("Cannot connect to the Tailscale daemon")
        {
            return Err(TailscaleError::DaemonStopped);
        }
        return Err(TailscaleError::CommandFailed(stderr));
    }

    let status: TailscaleStatus = serde_json::from_slice(&output.stdout)?;
    Ok(status)
}

impl Tailscale {
    /// Enables Tailscale by running `tailscale up`.
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

    /// Checks if the Tailscale daemon is currently running and enabled.
    pub fn is_enabled() -> Result<bool, TailscaleError> {
        match get_status_json() {
            Ok(status) => Ok(status.backend_state == "Running"),
            Err(TailscaleError::DaemonStopped) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Gets this device's own node info.
    pub fn self_node() -> Result<Option<MachineData>, TailscaleError> {
        let status = get_status_json()?;
        Ok(status.self_node.map(|node| MachineData {
            ip: node.tailscale_i_ps.first().cloned().unwrap_or_default(),
            hostname: node.host_name,
            online: node.online,
            is_exit_node: node.exit_node,
        }))
    }

    /// Gets the status of all machines in the network via `tailscale status --json`.
    pub fn status() -> Result<Vec<MachineData>, TailscaleError> {
        let status = get_status_json()?;

        if status.backend_state != "Running" {
            return Err(TailscaleError::DaemonStopped);
        }

        let mut machines = Vec::new();
        for (_key, peer) in &status.peer {
            // Skip VPN nodes (those with a location and exit_node_option)
            if peer.exit_node_option && peer.location.is_some() {
                continue;
            }

            let ip = peer.tailscale_i_ps.first().cloned().unwrap_or_default();
            let machine = MachineData {
                ip,
                hostname: peer.host_name.clone(),
                online: peer.online,
                is_exit_node: peer.exit_node,
            };

            if machine.online {
                machines.insert(0, machine);
            } else {
                machines.push(machine);
            }
        }

        Ok(machines)
    }

    /// Gets exit nodes from `tailscale status --json`.
    /// VPN nodes are deduplicated by country+city, keeping one representative per city.
    pub fn get_exit_nodes() -> Result<Vec<ExitNode>, TailscaleError> {
        let status = get_status_json()?;

        let mut exit_nodes = Vec::new();
        let mut seen_cities: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();

        // Sort peers so active exit nodes come first (preferred when deduplicating)
        let mut peers: Vec<&PeerInfo> = status.peer.values().collect();
        peers.sort_by(|a, b| b.exit_node.cmp(&a.exit_node));

        for peer in peers {
            if !peer.exit_node_option {
                continue;
            }

            let dns_name = peer.dns_name.trim_end_matches('.').to_string();

            if let Some(location) = &peer.location {
                let city_key = (location.country.clone(), location.city.clone());
                if !seen_cities.insert(city_key) {
                    continue;
                }

                exit_nodes.push(ExitNode::VPN(VPNNodeData {
                    hostname: dns_name,
                    country: location.country.clone(),
                    city: location.city.clone(),
                    is_exit_node: peer.exit_node,
                }));
            } else {
                let ip = peer.tailscale_i_ps.first().cloned().unwrap_or_default();
                exit_nodes.push(ExitNode::Machine(ExitNodeData {
                    hostname: dns_name,
                    ip,
                }));
            }
        }

        Ok(exit_nodes)
    }
}

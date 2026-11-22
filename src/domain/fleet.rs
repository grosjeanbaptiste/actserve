use std::collections::HashSet;

use crate::domain::errors::DomainError;

/// A remote host actserve administers: its name, how to reach it over ssh and
/// the hostname it must answer with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetHost {
    name: String,
    ssh_target: String,
    expected_hostname: String,
}

impl FleetHost {
    pub fn new(
        name: impl AsRef<str>,
        ssh_target: impl AsRef<str>,
        expected_hostname: impl AsRef<str>,
    ) -> Result<Self, DomainError> {
        let field = |label: &str, v: &str| -> Result<String, DomainError> {
            let v = v.trim();
            if v.is_empty() {
                return Err(DomainError::ValidationError(format!(
                    "Fleet host {label} cannot be empty"
                )));
            }
            Ok(v.to_string())
        };
        Ok(Self {
            name: field("name", name.as_ref())?,
            ssh_target: field("ssh target", ssh_target.as_ref())?,
            expected_hostname: field("expected hostname", expected_hostname.as_ref())?,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ssh_target(&self) -> &str {
        &self.ssh_target
    }

    pub fn expected_hostname(&self) -> &str {
        &self.expected_hostname
    }
}

/// Outcome of asking a host for its hostname.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostStatus {
    Ok,
    Unreachable { reason: String },
    WrongHost { got: String, expected: String },
}

/// Parse the `NAME|SSH-TARGET|EXPECTED-HOSTNAME` inventory format; `#` lines
/// and blank lines are skipped, names are unique.
pub fn parse_inventory(text: &str) -> Result<Vec<FleetHost>, DomainError> {
    let mut seen = HashSet::new();
    let mut hosts = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        let [name, target, hostname] = parts[..] else {
            return Err(DomainError::ValidationError(format!(
                "Inventory line {}: expected NAME|SSH-TARGET|EXPECTED-HOSTNAME, got {} field(s)",
                i + 1,
                parts.len()
            )));
        };
        let host = FleetHost::new(name, target, hostname)
            .map_err(|e| DomainError::ValidationError(format!("Inventory line {}: {e}", i + 1)))?;
        if !seen.insert(host.name.clone()) {
            return Err(DomainError::ValidationError(format!(
                "Inventory line {}: duplicate server name '{}'",
                i + 1,
                host.name
            )));
        }
        hosts.push(host);
    }
    Ok(hosts)
}

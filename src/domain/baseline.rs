//! The shell commands of the hardening baseline of a server (step 2 of the
//! bootstrap). Each item has a check and an apply; apply only installs or
//! enables, nothing is ever removed.

pub const FAIL2BAN_CHECK: &str = "systemctl is-active --quiet fail2ban";
pub const FAIL2BAN_APPLY: &str = "sudo -n sh -c 'DEBIAN_FRONTEND=noninteractive apt-get install -y fail2ban && systemctl enable --now fail2ban'";
pub const UPGRADES_CHECK: &str = "dpkg -s unattended-upgrades > /dev/null 2>&1";
pub const UPGRADES_APPLY: &str =
    "sudo -n sh -c 'DEBIAN_FRONTEND=noninteractive apt-get install -y unattended-upgrades'";

/// Where the sshd drop-in lives on a server.
pub const SSHD_DROPIN: &str = "/etc/ssh/sshd_config.d/00-acteble.conf";

/// Succeeds when the drop-in on the server equals the declared one (on stdin).
pub fn sshd_check_command() -> String {
    format!("sudo -n cmp -s - {SSHD_DROPIN}")
}

/// Succeeds when ufw is active, denies incoming, and allows every rule.
pub fn ufw_check_command(ports: &[String]) -> String {
    let mut cmd = String::from(
        r#"ufw status verbose | grep -q "^Status: active" && ufw status verbose | grep -q "deny (incoming)""#,
    );
    for port in ports {
        cmd.push_str(&format!(r#" && ufw status | grep -qE "^{port} +ALLOW""#));
    }
    format!("sudo -n sh -c '{cmd}'")
}

/// Allows 22/tcp first, then every other rule once, then enables ufw.
pub fn ufw_apply_command(ports: &[String]) -> String {
    let mut cmd =
        String::from("ufw default deny incoming && ufw default allow outgoing && ufw allow 22/tcp");
    for port in ports.iter().filter(|p| *p != "22/tcp") {
        cmd.push_str(&format!(" && ufw allow {port}"));
    }
    format!("sudo -n sh -c '{cmd} && ufw --force enable'")
}

//! What a server must look like after a bootstrap: the declared data of
//! `servers/<name>/` turned into ordered, checkable items. Mirrors the six
//! steps of the old shell bootstrap, minus the ones that are not ported yet.

use crate::domain::errors::DomainError;

/// The raw text of the declared files of one server.
#[derive(Debug, Clone, Default)]
pub struct Declared {
    pub sshd_conf: String,
    pub ports: String,
    pub docker_daemon: String,
    pub dirs: String,
    pub stacks: String,
}

/// A command run on the server, with what it reads on stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCommand {
    pub command: String,
    pub stdin: Vec<u8>,
}

/// One thing a bootstrap makes true. `check` succeeds (exit 0) when it already is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub step: &'static str,
    pub name: String,
    pub check: RemoteCommand,
    /// Declared but deliberately left alone.
    pub held: bool,
    /// Makes the check pass; `None` while the step is not ported.
    pub fix: Option<RemoteCommand>,
}

pub const SSHD_DROPIN: &str = "/etc/ssh/sshd_config.d/00-acteble.conf";

/// Lines of a declared file: trimmed, without blanks and `#` comments, with their number.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
}

fn refuse(problems: Vec<String>) -> Result<(), DomainError> {
    if problems.is_empty() {
        Ok(())
    } else {
        Err(DomainError::ValidationError(problems.join("; ")))
    }
}

/// `NUMBER/tcp|udp` per line; 22/tcp must be there, or enabling the firewall
/// would lock the operator out.
pub fn parse_ports(file: &str, text: &str) -> Result<Vec<String>, DomainError> {
    let (mut ports, mut problems) = (Vec::new(), Vec::new());
    for (n, l) in lines(text) {
        let ok = l.split_once('/').is_some_and(|(num, proto)| {
            (1..=5).contains(&num.len())
                && num.chars().all(|c| c.is_ascii_digit())
                && matches!(proto, "tcp" | "udp")
        });
        if ok {
            ports.push(l.to_string());
        } else {
            problems.push(format!(
                "{file}:{n}: \"{l}\" must be NUMBER/tcp or NUMBER/udp"
            ));
        }
    }
    if problems.is_empty() && !ports.iter().any(|p| p == "22/tcp") {
        problems.push(format!(
            "{file}: must allow 22/tcp, or enabling the firewall would lock the operator out"
        ));
    }
    refuse(problems).map(|()| ports)
}

/// `ABSOLUTE-PATH|OWNER` per line; no `..` component.
pub fn parse_dirs(file: &str, text: &str) -> Result<Vec<(String, String)>, DomainError> {
    let (mut dirs, mut problems) = (Vec::new(), Vec::new());
    for (n, l) in lines(text) {
        let valid = l.split_once('|').filter(|(path, owner)| {
            path.starts_with('/')
                && path.len() > 1
                && path
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_./-".contains(c))
                && !format!("{path}/").contains("/../")
                && owner.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
                && owner
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        });
        match valid {
            Some((p, o)) => dirs.push((p.to_string(), o.to_string())),
            None => problems.push(format!(
                "{file}:{n}: \"{l}\" must be ABSOLUTE-PATH|OWNER (no .. component)"
            )),
        }
    }
    refuse(problems).map(|()| dirs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackMode {
    Run,
    Held,
}

/// `NAME|MODE` per line, NAME `cdn` or `api`, MODE `run` or `held`. `api` can only
/// be held: it has no running check and its deploy is frozen.
pub fn parse_stacks(file: &str, text: &str) -> Result<Vec<(String, StackMode)>, DomainError> {
    let (mut stacks, mut problems) = (Vec::new(), Vec::new());
    for (n, l) in lines(text) {
        match l.split_once('|') {
            Some(("api", "run")) => problems.push(format!(
                "{file}:{n}: api can only be held: it has no running check and its deploy is frozen"
            )),
            Some((name @ ("cdn" | "api"), mode @ ("run" | "held"))) => stacks.push((
                name.to_string(),
                if mode == "run" {
                    StackMode::Run
                } else {
                    StackMode::Held
                },
            )),
            _ => problems.push(format!(
                "{file}:{n}: \"{l}\" must be NAME|MODE, NAME cdn or api, MODE run or held"
            )),
        }
    }
    refuse(problems).map(|()| stacks)
}

fn check(step: &'static str, name: &str, command: impl Into<String>) -> Item {
    Item {
        step,
        name: name.to_string(),
        check: RemoteCommand {
            command: command.into(),
            stdin: Vec::new(),
        },
        held: false,
        fix: None,
    }
}

fn with_fix(mut item: Item, command: &str) -> Item {
    item.fix = Some(RemoteCommand {
        command: command.to_string(),
        stdin: Vec::new(),
    });
    item
}

fn docker_ps(project: &str) -> String {
    format!("test -n \"$(docker ps -q --filter label=com.docker.compose.project={project})\"")
}

/// The ordered items of steps 2 to 5 (harden, runtime, edge, stacks) with their checks.
/// Everything is validated first: a bad declared file refuses the whole plan.
pub fn plan(d: &Declared) -> Result<Vec<Item>, DomainError> {
    let ports = parse_ports("ports.txt", &d.ports)?;
    let dirs = parse_dirs("dirs.txt", &d.dirs)?;
    let stacks = parse_stacks("stacks.txt", &d.stacks)?;

    let mut ufw = String::from(
        "ufw status verbose | grep -q \"^Status: active\" && ufw status verbose | grep -q \"deny (incoming)\"",
    );
    for p in &ports {
        ufw.push_str(&format!(" && ufw status | grep -qE \"^{p} +ALLOW\""));
    }
    let dirs_cmd = dirs
        .iter()
        .map(|(path, owner)| format!("test \"$(stat -c %U {path})\" = {owner}"))
        .collect::<Vec<_>>()
        .join(" && ");

    let mut sshd = check("harden", "sshd", format!("sudo -n cmp -s - {SSHD_DROPIN}"));
    sshd.check.stdin = d.sshd_conf.clone().into_bytes();
    let mut logs = check(
        "runtime",
        "logs",
        "sudo -n cmp -s - /etc/docker/daemon.json",
    );
    logs.check.stdin = d.docker_daemon.clone().into_bytes();

    let dirs_fix = dirs
        .iter()
        .map(|(path, owner)| format!("sudo -n mkdir -p {path} && sudo -n chown {owner} {path}"))
        .collect::<Vec<_>>()
        .join(" && ");
    let mut dirs_item = check("runtime", "dirs", dirs_cmd);
    dirs_item.fix = Some(RemoteCommand {
        command: dirs_fix,
        stdin: Vec::new(),
    });

    let mut network = check(
        "edge",
        "network",
        "docker network inspect acteble_edge > /dev/null 2>&1",
    );
    network.fix = Some(RemoteCommand {
        command: "docker network create acteble_edge".to_string(),
        stdin: Vec::new(),
    });

    let mut items = vec![
        sshd,
        check("harden", "ufw", format!("sudo -n sh -c '{ufw}'")),
        with_fix(
            check("harden", "fail2ban", "systemctl is-active --quiet fail2ban"),
            "sudo -n apt-get install -y fail2ban && sudo -n systemctl enable --now fail2ban",
        ),
        with_fix(
            check(
                "harden",
                "upgrades",
                "dpkg -s unattended-upgrades > /dev/null 2>&1",
            ),
            "sudo -n apt-get install -y unattended-upgrades",
        ),
        check(
            "runtime",
            "docker",
            "docker compose version > /dev/null 2>&1",
        ),
        logs,
        dirs_item,
        network,
        check(
            "edge",
            "caddy",
            "test -n \"$(docker ps -q --filter label=com.docker.compose.project=acteble_edge --filter label=com.docker.compose.service=caddy)\"",
        ),
    ];
    for (name, mode) in stacks {
        let mut item = check("stacks", &name, docker_ps(&format!("acteble_{name}")));
        item.held = mode == StackMode::Held;
        items.push(item);
    }
    Ok(items)
}

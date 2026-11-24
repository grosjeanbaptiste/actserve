use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use actserve::application::fleet_apply_service::FleetApplyService;
use actserve::domain::{
    bootstrap::{Declared, parse_dirs, parse_ports, parse_stacks, plan},
    errors::DomainError,
    fleet::FleetHost,
};
use actserve::ports::outbound::{
    declared_data::DeclaredDataPort,
    hostname_probe::HostnameProbePort,
    remote_shell::{RemoteShellPort, ShellOutput},
    server_inventory::ServerInventoryPort,
};

fn declared() -> Declared {
    Declared {
        sshd_conf: "PasswordAuthentication no\n".into(),
        ports: "# edge\n22/tcp\n443/tcp\n".into(),
        dirs: "/opt/acteble/edge|deploy\n".into(),
        stacks: "cdn|run\napi|held\n".into(),
    }
}

#[test]
fn ports_must_let_ssh_in_and_be_number_slash_proto() {
    assert_eq!(
        parse_ports("p", "# c\n22/tcp\n443/udp\n").unwrap(),
        vec!["22/tcp", "443/udp"]
    );
    let no_ssh = parse_ports("ports.txt", "443/tcp\n")
        .unwrap_err()
        .to_string();
    assert!(no_ssh.contains("22/tcp"), "{no_ssh}");
    let bad = parse_ports("ports.txt", "22/tcp\nhttp\n")
        .unwrap_err()
        .to_string();
    assert!(bad.contains("ports.txt:2"), "{bad}");
}

#[test]
fn dirs_are_absolute_without_dotdot_with_an_owner() {
    assert_eq!(
        parse_dirs("d", "/opt/a|deploy\n").unwrap(),
        vec![("/opt/a".to_string(), "deploy".to_string())]
    );
    for bad in [
        "opt/a|deploy",
        "/opt/../etc|deploy",
        "/opt/a",
        "/opt/a|Bad Owner",
    ] {
        assert!(parse_dirs("dirs.txt", bad).is_err(), "{bad}");
    }
}

#[test]
fn stacks_are_cdn_or_api_run_or_held_and_api_is_always_held() {
    assert_eq!(parse_stacks("s", "cdn|run\napi|held\n").unwrap().len(), 2);
    for bad in ["api|run", "web|run", "cdn|maybe", "cdn"] {
        assert!(parse_stacks("stacks.txt", bad).is_err(), "{bad}");
    }
}

#[test]
fn plan_lists_every_item_in_bootstrap_order_and_skips_held_stacks() {
    let items = plan(&declared()).unwrap();
    let names: Vec<_> = items
        .iter()
        .map(|i| format!("{} {}", i.step, i.name))
        .collect();
    assert_eq!(
        names,
        [
            "harden sshd",
            "harden ufw",
            "harden fail2ban",
            "harden upgrades",
            "runtime podman",
            "runtime dirs",
            "edge network",
            "edge caddy",
            "stacks cdn",
            "stacks api",
        ]
    );
    assert!(items.last().unwrap().held);
    assert!(!items[0].held);
}

#[test]
fn plan_checks_carry_the_declared_file_and_every_port() {
    let items = plan(&declared()).unwrap();
    let sshd = &items[0].check;
    assert_eq!(
        sshd.command,
        "sudo -n cmp -s - /etc/ssh/sshd_config.d/00-acteble.conf"
    );
    assert_eq!(sshd.stdin, b"PasswordAuthentication no\n");
    let ufw = &items[1].check.command;
    assert!(ufw.contains(r#"grep -qE "^443/tcp +ALLOW""#), "{ufw}");
    assert!(ufw.contains("deny (incoming)"), "{ufw}");
    let dirs = &items[5].check.command;
    assert!(
        dirs.contains(r#"test "$(stat -c %U /opt/acteble/edge)" = deploy"#),
        "{dirs}"
    );
}

#[test]
fn a_bad_declared_file_is_refused_before_any_check() {
    let mut d = declared();
    d.ports = "443/tcp\n".into();
    assert!(plan(&d).is_err());
}

struct Inventory;
#[async_trait]
impl ServerInventoryPort for Inventory {
    async fn hosts(&self) -> Result<Vec<FleetHost>, DomainError> {
        Ok(vec![
            FleetHost::new("acteble", "ops@vps", "cloud-1").unwrap(),
        ])
    }
}

struct Probe(&'static str);
#[async_trait]
impl HostnameProbePort for Probe {
    async fn hostname(&self, _: &str) -> Result<String, DomainError> {
        Ok(self.0.into())
    }
}

struct Data(Declared);
#[async_trait]
impl DeclaredDataPort for Data {
    async fn read(&self, _: &str, file: &str) -> Result<String, DomainError> {
        let d = &self.0;
        Ok(match file {
            "sshd.conf" => d.sshd_conf.clone(),
            "ports.txt" => d.ports.clone(),
            "dirs.txt" => d.dirs.clone(),
            "stacks.txt" => d.stacks.clone(),
            other => panic!("unexpected {other}"),
        })
    }
}

/// Fails (exit 1) every command that contains one of `failing`.
struct Shell {
    failing: Vec<&'static str>,
    calls: Arc<Mutex<Vec<String>>>,
}
#[async_trait]
impl RemoteShellPort for Shell {
    async fn run(&self, _: &str, command: &str, _: &[u8]) -> Result<ShellOutput, DomainError> {
        self.calls.lock().unwrap().push(command.to_string());
        let code = if self.failing.iter().any(|f| command.contains(f)) {
            1
        } else {
            0
        };
        Ok(ShellOutput {
            code: Some(code),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

fn service(
    host: &'static str,
    failing: Vec<&'static str>,
) -> (FleetApplyService, Arc<Mutex<Vec<String>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let s = FleetApplyService::new(
        Arc::new(Inventory),
        Arc::new(Probe(host)),
        Arc::new(Shell {
            failing,
            calls: calls.clone(),
        }),
        Arc::new(Data(declared())),
    );
    (s, calls)
}

#[tokio::test]
async fn dry_run_all_conforming_exits_0_and_lists_every_item_ok() {
    let (s, _) = service("cloud-1", vec![]);
    let r = s.dry_run("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 0, "{:?}", r.lines);
    assert!(
        r.lines.contains(&"reach      ok".to_string()),
        "{:?}",
        r.lines
    );
    assert!(
        r.lines.contains(&"harden sshd      ok".to_string()),
        "{:?}",
        r.lines
    );
    assert!(
        r.lines
            .contains(&"stacks api       skipped (held back)".to_string()),
        "{:?}",
        r.lines
    );
}

#[tokio::test]
async fn dry_run_names_what_would_change_and_still_checks_the_rest() {
    let (s, _) = service("cloud-1", vec!["systemctl is-active --quiet fail2ban"]);
    let r = s.dry_run("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 1);
    assert!(
        r.lines
            .contains(&"harden fail2ban  would apply".to_string()),
        "{:?}",
        r.lines
    );
    assert!(
        r.lines.iter().any(|l| l.starts_with("runtime podman")),
        "{:?}",
        r.lines
    );
}

#[tokio::test]
async fn dry_run_stops_after_a_failed_reach() {
    let (s, calls) = service("cloud-1", vec!["id -u"]);
    let r = s.dry_run("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 1);
    assert!(
        r.lines
            .iter()
            .any(|l| l.starts_with("reach") && l.contains("fail")),
        "{:?}",
        r.lines
    );
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn dry_run_on_another_host_sends_nothing() {
    let (s, calls) = service("other", vec![]);
    let r = s.dry_run("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 1);
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn dry_run_never_runs_a_mutating_command() {
    let (s, calls) = service(
        "cloud-1",
        vec![
            "cmp",
            "ufw",
            "fail2ban",
            "dpkg",
            "podman --version",
            "stat",
            "is-active",
            "podman network",
        ],
    );
    s.dry_run("acteble").await.unwrap();
    for c in calls.lock().unwrap().iter() {
        for forbidden in [
            "apt-get",
            "--force",
            "mv ",
            "chown",
            "install ",
            "cat >",
            "network create",
            "mkdir",
            "systemctl --user start",
            "enable",
            "reload",
            "restart",
        ] {
            assert!(!c.contains(forbidden), "{forbidden} in {c}");
        }
    }
}

#[tokio::test]
async fn unknown_server_is_not_found() {
    let (s, _) = service("cloud-1", vec![]);
    assert!(matches!(
        s.dry_run("nope").await,
        Err(DomainError::NotFound { .. })
    ));
}

// Found by mutation testing (cargo mutants): each case below was a surviving mutant.

#[test]
fn a_port_is_one_to_five_digits_and_tcp_or_udp() {
    for bad in ["ab/tcp", "123456/tcp", "80/icmp", "/tcp", "80/", "80"] {
        assert!(
            parse_ports("ports.txt", &format!("22/tcp\n{bad}\n")).is_err(),
            "{bad}"
        );
    }
    assert!(parse_ports("ports.txt", "22/tcp\n65535/udp\n7/tcp\n").is_ok());
}

#[test]
fn the_root_directory_is_refused_and_owners_may_use_digits_dash_underscore() {
    assert!(parse_dirs("dirs.txt", "/|deploy\n").is_err());
    for owner in ["web-1", "_svc", "a_b", "d9"] {
        assert!(
            parse_dirs("dirs.txt", &format!("/opt/a|{owner}\n")).is_ok(),
            "{owner}"
        );
    }
    for owner in ["Bad", "9lives", "a b", "a.b", "-x"] {
        assert!(
            parse_dirs("dirs.txt", &format!("/opt/a|{owner}\n")).is_err(),
            "{owner}"
        );
    }
    for path in ["/opt/a b", "/opt/a;rm", "/opt/$(x)"] {
        assert!(
            parse_dirs("dirs.txt", &format!("{path}|deploy\n")).is_err(),
            "{path}"
        );
    }
    assert!(parse_dirs("dirs.txt", "/opt/a-b_c.d/e|deploy\n").is_ok());
    assert!(parse_dirs("dirs.txt", "/opt/..hidden|deploy\n").is_ok());
    assert!(parse_dirs("dirs.txt", "/opt/a/..|deploy\n").is_err());
}

/// A server that drifts until a command containing `fix` has run: every command
/// containing `check` fails before that and succeeds after.
struct Drifting {
    check: &'static str,
    fix: &'static str,
    fixed: Mutex<bool>,
    calls: Arc<Mutex<Vec<String>>>,
}
#[async_trait]
impl RemoteShellPort for Drifting {
    async fn run(&self, _: &str, command: &str, _: &[u8]) -> Result<ShellOutput, DomainError> {
        self.calls.lock().unwrap().push(command.to_string());
        let mut fixed = self.fixed.lock().unwrap();
        if command.contains(self.fix) {
            *fixed = true;
        }
        let drifted = command.contains(self.check) && !*fixed;
        Ok(ShellOutput {
            code: Some(i32::from(drifted)),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

fn drifting_service(
    check: &'static str,
    fix: &'static str,
) -> (FleetApplyService, Arc<Mutex<Vec<String>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let s = FleetApplyService::new(
        Arc::new(Inventory),
        Arc::new(Probe("cloud-1")),
        Arc::new(Drifting {
            check,
            fix,
            fixed: Mutex::new(false),
            calls: calls.clone(),
        }),
        Arc::new(Data(declared())),
    );
    (s, calls)
}

#[tokio::test]
async fn apply_fixes_a_drifted_dir_then_rechecks_and_exits_0() {
    let (s, calls) = drifting_service("stat -c %U /opt/acteble/edge", "mkdir -p /opt/acteble/edge");
    let r = s.apply("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 0, "{:?}", r.lines);
    assert!(
        r.lines.contains(&"runtime dirs      applied".to_string()),
        "{:?}",
        r.lines
    );
    let calls = calls.lock().unwrap();
    assert!(
        calls
            .iter()
            .any(|c| c
                == "sudo -n mkdir -p /opt/acteble/edge && sudo -n chown deploy /opt/acteble/edge"),
        "{calls:?}"
    );
}

#[tokio::test]
async fn apply_that_does_not_heal_reports_fail_and_exits_1() {
    let (s, _) = service("cloud-1", vec!["stat -c %U /opt/acteble/edge"]);
    let r = s.apply("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 1, "{:?}", r.lines);
    assert!(
        r.lines
            .contains(&"runtime dirs      fail (still drifted after apply)".to_string()),
        "{:?}",
        r.lines
    );
}

#[tokio::test]
async fn apply_whose_fix_fails_says_so_and_does_not_recheck() {
    let (s, calls) = service(
        "cloud-1",
        vec!["stat -c %U /opt/acteble/edge", "mkdir -p /opt/acteble/edge"],
    );
    let r = s.apply("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 1, "{:?}", r.lines);
    assert!(
        r.lines
            .contains(&"runtime dirs      fail (fix failed)".to_string()),
        "{:?}",
        r.lines
    );
    let rechecks = calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.contains("stat -c %U"))
        .count();
    assert_eq!(rechecks, 1);
}

#[tokio::test]
async fn apply_creates_the_missing_edge_network() {
    let (s, calls) = drifting_service(
        "podman network exists acteble_edge",
        "systemctl --user start acteble_edge-network.service",
    );
    let r = s.apply("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 0, "{:?}", r.lines);
    assert!(
        r.lines.contains(&"edge network   applied".to_string()),
        "{:?}",
        r.lines
    );
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "systemctl --user start acteble_edge-network.service")
    );
}

#[tokio::test]
async fn apply_installs_the_missing_unattended_upgrades() {
    let (s, calls) = drifting_service(
        "dpkg -s unattended-upgrades",
        "apt-get install -y unattended-upgrades",
    );
    let r = s.apply("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 0, "{:?}", r.lines);
    assert!(
        r.lines.contains(&"harden upgrades  applied".to_string()),
        "{:?}",
        r.lines
    );
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "sudo -n apt-get install -y unattended-upgrades")
    );
}

#[tokio::test]
async fn apply_installs_and_starts_the_missing_fail2ban() {
    let (s, calls) = drifting_service(
        "systemctl is-active --quiet fail2ban",
        "apt-get install -y fail2ban",
    );
    let r = s.apply("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 0, "{:?}", r.lines);
    assert!(
        r.lines.contains(&"harden fail2ban  applied".to_string()),
        "{:?}",
        r.lines
    );
    assert!(calls.lock().unwrap().iter().any(|c| {
        c == "sudo -n apt-get install -y fail2ban && sudo -n systemctl enable --now fail2ban"
    }));
}

#[tokio::test]
async fn apply_installs_the_missing_podman() {
    let (s, calls) = drifting_service("podman --version", "apt-get install -y podman");
    let r = s.apply("acteble").await.unwrap();
    assert_eq!(r.exit_code(), 0, "{:?}", r.lines);
    assert!(
        r.lines.contains(&"runtime podman    applied".to_string()),
        "{:?}",
        r.lines
    );
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "sudo -n apt-get install -y podman")
    );
}

#[test]
fn plan_never_mentions_docker() {
    for item in plan(&declared()).unwrap() {
        let all = format!(
            "{} {}",
            item.check.command,
            item.fix.map_or(String::new(), |f| f.command)
        );
        assert!(!all.contains("docker"), "{}: {all}", item.name);
    }
}

#[test]
fn plan_checks_the_edge_and_the_stacks_through_systemd_units_not_docker() {
    let items = plan(&declared()).unwrap();
    let check = |name: &str| {
        items
            .iter()
            .find(|i| i.name == name)
            .map(|i| i.check.command.clone())
            .unwrap_or_default()
    };
    assert_eq!(check("network"), "podman network exists acteble_edge");
    assert_eq!(
        check("caddy"),
        "systemctl --user is-active --quiet acteble_edge-caddy.service"
    );
    assert_eq!(
        check("cdn"),
        "systemctl --user is-active --quiet acteble_cdn-minio.service && systemctl --user is-active --quiet acteble_cdn-checkpoint.service"
    );
    assert!(
        items.iter().all(|i| !i.check.command.contains("docker ps")),
        "{items:?}"
    );
}

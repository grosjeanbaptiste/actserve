use actserve::domain::baseline::{
    FAIL2BAN_APPLY, FAIL2BAN_CHECK, UPGRADES_APPLY, UPGRADES_CHECK, ufw_apply_command,
    ufw_check_command,
};
use actserve::domain::errors::DomainError;
use actserve::domain::ports_file::parse_ports;

fn rules(text: &str) -> Vec<String> {
    parse_ports("ports.txt", text).unwrap()
}

fn problem(text: &str) -> String {
    match parse_ports("ports.txt", text).unwrap_err() {
        DomainError::ValidationError(m) => m,
        other => panic!("{other:?}"),
    }
}

#[test]
fn ports_keep_their_order_and_skip_comments_and_blank_lines() {
    assert_eq!(
        rules("# c\n\n22/tcp\n  443/udp  \n80/tcp\n"),
        ["22/tcp", "443/udp", "80/tcp"]
    );
}

#[test]
fn a_file_without_22_tcp_is_refused_because_it_would_lock_the_operator_out() {
    let m = problem("80/tcp\n443/tcp\n");
    assert!(m.contains("ports.txt"), "{m}");
    assert!(m.contains("22/tcp"), "{m}");
}

#[test]
fn every_bad_line_is_reported_with_its_line_number_in_one_pass() {
    let m = problem("22/tcp\nssh\n80/icmp\n99999/tcp\n0/tcp\n");
    for expected in [
        "ports.txt:2:",
        "ports.txt:3:",
        "ports.txt:4:",
        "ports.txt:5:",
    ] {
        assert!(m.contains(expected), "{expected} in {m}");
    }
}

#[test]
fn the_boundary_ports_1_and_65535_are_accepted() {
    assert_eq!(rules("22/tcp\n1/udp\n65535/tcp\n").len(), 3);
}

#[test]
fn an_empty_file_is_refused() {
    assert!(problem("# nothing\n").contains("22/tcp"));
}

#[test]
fn ufw_check_demands_active_deny_incoming_and_every_rule() {
    let ports = vec!["22/tcp".to_string(), "443/tcp".to_string()];
    assert_eq!(
        ufw_check_command(&ports),
        r#"sudo -n sh -c 'ufw status verbose | grep -q "^Status: active" && ufw status verbose | grep -q "deny (incoming)" && ufw status | grep -qE "^22/tcp +ALLOW" && ufw status | grep -qE "^443/tcp +ALLOW"'"#
    );
}

#[test]
fn ufw_apply_allows_22_first_once_and_enables_without_a_prompt() {
    let ports = vec![
        "80/tcp".to_string(),
        "22/tcp".to_string(),
        "443/udp".to_string(),
    ];
    assert_eq!(
        ufw_apply_command(&ports),
        "sudo -n sh -c 'ufw default deny incoming && ufw default allow outgoing && ufw allow 22/tcp && ufw allow 80/tcp && ufw allow 443/udp && ufw --force enable'"
    );
}

#[test]
fn the_package_items_install_without_a_prompt_and_never_remove_anything() {
    assert_eq!(FAIL2BAN_CHECK, "systemctl is-active --quiet fail2ban");
    assert_eq!(
        FAIL2BAN_APPLY,
        "sudo -n sh -c 'DEBIAN_FRONTEND=noninteractive apt-get install -y fail2ban && systemctl enable --now fail2ban'"
    );
    assert_eq!(
        UPGRADES_CHECK,
        "dpkg -s unattended-upgrades > /dev/null 2>&1"
    );
    assert_eq!(
        UPGRADES_APPLY,
        "sudo -n sh -c 'DEBIAN_FRONTEND=noninteractive apt-get install -y unattended-upgrades'"
    );
    for c in [FAIL2BAN_APPLY, UPGRADES_APPLY] {
        assert!(!c.contains("remove") && !c.contains("purge") && !c.contains("rm "));
    }
}

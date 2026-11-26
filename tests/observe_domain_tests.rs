//! What `actserve ps` and `actserve logs` ask a server: stacks, services, commands.
use actserve::domain::observe::{
    Stack, logs_command, ps_command, resources_command, valid_service,
};

#[test]
fn resources_reads_cpu_memory_disk_and_the_memory_of_every_container() {
    let c = resources_command();
    for part in [
        "nproc",
        "free -m",
        "df -h /",
        "podman stats --no-stream",
        "uptime",
    ] {
        assert!(c.contains(part), "{part}: {c}");
    }
    assert!(!c.contains("sudo"), "read-only: {c}");
}

#[test]
fn the_known_stacks_are_api_cdn_and_edge() {
    assert_eq!(Stack::parse("api").unwrap(), Stack::Api);
    assert_eq!(Stack::parse("cdn").unwrap(), Stack::Cdn);
    assert_eq!(Stack::parse("edge").unwrap(), Stack::Edge);
}

#[test]
fn an_unknown_stack_is_a_validation_error_naming_it() {
    let e = Stack::parse("nope").unwrap_err().to_string();
    assert!(
        e.contains("nope") && e.contains("api") && e.contains("cdn") && e.contains("edge"),
        "{e}"
    );
}

#[test]
fn each_stack_has_its_directory_and_default_service() {
    assert_eq!(Stack::Api.dir(), "/opt/acteble/api");
    assert_eq!(Stack::Cdn.dir(), "/opt/acteble/cdn");
    assert_eq!(Stack::Edge.dir(), "/opt/acteble/edge");
    assert_eq!(Stack::Api.default_service(), "api");
    assert_eq!(Stack::Cdn.default_service(), "cdn_checkpoint");
    assert_eq!(Stack::Edge.default_service(), "caddy");
}

#[test]
fn the_edge_dir_is_the_one_edge_status_already_uses() {
    assert_eq!(Stack::Edge.dir(), actserve::domain::observe::EDGE_DIR);
}

#[test]
fn ps_lists_the_podman_containers_of_the_stack() {
    assert_eq!(
        ps_command(Stack::Cdn),
        "podman ps --filter name=systemd-acteble_cdn"
    );
    assert_eq!(
        ps_command(Stack::Api),
        "podman ps --filter name=systemd-acteble_api"
    );
    assert_eq!(
        ps_command(Stack::Edge),
        "podman ps --filter name=systemd-acteble_edge"
    );
}

#[test]
fn logs_follow_the_last_hundred_lines_of_the_unit_of_the_service() {
    assert_eq!(
        logs_command(Stack::Api, "api").unwrap(),
        "journalctl --user -u acteble_api-api.service -f -n 100"
    );
    assert_eq!(
        logs_command(Stack::Cdn, "cdn_checkpoint").unwrap(),
        "journalctl --user -u acteble_cdn-checkpoint.service -f -n 100"
    );
    assert_eq!(
        logs_command(Stack::Api, "pg_backup").unwrap(),
        "journalctl --user -u acteble_api-pg-backup.service -f -n 100"
    );
    assert_eq!(
        logs_command(Stack::Edge, "caddy").unwrap(),
        "journalctl --user -u acteble_edge-caddy.service -f -n 100"
    );
}

#[test]
fn a_service_name_that_could_break_the_quoting_is_refused() {
    for bad in ["", "a b", "a'b", "a;rm", "$(x)", "-f", "a/b", "é"] {
        assert!(!valid_service(bad), "{bad:?}");
        assert!(logs_command(Stack::Api, bad).is_err(), "{bad:?}");
    }
}

#[test]
fn compose_service_names_are_accepted() {
    for good in ["api", "cdn_checkpoint", "cdn-minio", "a.b", "A1"] {
        assert!(valid_service(good), "{good:?}");
    }
}

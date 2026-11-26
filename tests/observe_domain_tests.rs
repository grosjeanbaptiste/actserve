//! What `actserve ps` and `actserve logs` ask a server: stacks, services, commands.
use actserve::domain::observe::{Stack, logs_command, ps_command, valid_service};

#[test]
fn the_known_stacks_are_api_and_cdn() {
    assert_eq!(Stack::parse("api").unwrap(), Stack::Api);
    assert_eq!(Stack::parse("cdn").unwrap(), Stack::Cdn);
}

#[test]
fn an_unknown_stack_is_a_validation_error_naming_it() {
    let e = Stack::parse("nope").unwrap_err().to_string();
    assert!(
        e.contains("nope") && e.contains("api") && e.contains("cdn"),
        "{e}"
    );
}

#[test]
fn each_stack_has_its_directory_and_default_service() {
    assert_eq!(Stack::Api.dir(), "/opt/acteble/api");
    assert_eq!(Stack::Cdn.dir(), "/opt/acteble/cdn");
    assert_eq!(Stack::Api.default_service(), "api");
    assert_eq!(Stack::Cdn.default_service(), "cdn_checkpoint");
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

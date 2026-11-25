//! The Podman commands shared by the ported services.
use actserve::domain::podman::ghcr_wrapped;

#[test]
fn the_script_runs_logged_in_and_logs_out_whatever_its_result() {
    let w = ghcr_wrapped("echo hi");
    let login = w
        .find("podman login ghcr.io -u 'grosjeanbaptiste' --password-stdin")
        .unwrap();
    let script = w.find("(\necho hi\n)").unwrap();
    let logout = w.find("podman logout ghcr.io").unwrap();
    assert!(login < script && script < logout, "{w}");
    assert!(w.ends_with("exit $rc"), "{w}");
}

#[test]
fn a_failed_login_stops_before_the_script_runs() {
    let w = ghcr_wrapped("echo hi");
    assert!(w.contains("--password-stdin >/dev/null || exit 1\n"), "{w}");
}

#[test]
fn the_script_status_is_the_one_returned_not_the_logouts() {
    let w = ghcr_wrapped("x");
    assert!(
        w.contains(")\nrc=$?\npodman logout ghcr.io >/dev/null 2>&1\nexit $rc"),
        "{w}"
    );
}

#[test]
fn no_docker_command_is_left() {
    assert!(!ghcr_wrapped("x").contains("docker"));
}

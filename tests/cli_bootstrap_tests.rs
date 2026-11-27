//! `actserve bootstrap NAME` and `apply NAME` through the real binary, ssh replaced by a script.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn stage(name: &str, ssh_body: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("actserve-cb-{}-{name}", std::process::id()));
    let s = dir.join("servers/acteble");
    std::fs::create_dir_all(&s).unwrap();
    std::fs::write(
        dir.join("servers.txt"),
        "acteble|fake-target.invalid|host-1\n",
    )
    .unwrap();
    for (f, c) in [
        ("sshd.conf", "PermitRootLogin no\n"),
        ("ports.txt", "22/tcp\n"),
        ("dirs.txt", "/opt/a|admin\n"),
        ("stacks.txt", "cdn|run\napi|held\n"),
    ] {
        std::fs::write(s.join(f), c).unwrap();
    }
    let ssh = dir.join("ssh");
    std::fs::write(&ssh, format!("#!/bin/sh\n{ssh_body}\n")).unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, ssh)
}

fn run(dir: &Path, ssh: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_actserve"))
        .args(args)
        .env("ACTCONF_STATE_ROOT", dir)
        .env("ACTSERVE_SSH", ssh)
        // Nothing here may reach the network: scp and actesskey fail, and the local
        // edge and cdn directories do not exist, so a deploy stops before any call.
        .env("ACTSERVE_SCP", "/usr/bin/false")
        .env("ACTSERVE_ACTESSKEY", "/usr/bin/false")
        .env("ACTEBLE_EDGE_DIR", dir.join("no-edge"))
        .env("ACTEBLE_CDN_DIR", dir.join("no-cdn"))
        .output()
        .unwrap()
}

// Hostname answers host-1; `edge audit` answers $AUDIT_RC; everything else passes unless $FAIL.
const SSH: &str = r#"case "$*" in *" hostname") echo host-1; exit 0;; esac
case "$*" in *"edge audit"*) exit "${AUDIT_RC:-0}";; esac
cat > /dev/null
case "$*" in *"$FAIL"*) [ -n "$FAIL" ] && exit 1;; esac
exit 0"#;

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn a_conforming_server_prints_every_step_ending_with_the_audit_and_exits_0() {
    let (dir, ssh) = stage("ok", SSH);
    for verb in ["bootstrap", "apply"] {
        let out = run(&dir, &ssh, &[verb, "acteble"]);
        assert_eq!(out.status.code(), Some(0), "{verb}: {out:?}");
        let t = text(&out);
        assert!(t.starts_with("reach      ok\nharden sshd      ok\n"), "{t}");
        assert!(
            t.ends_with("stacks api       skipped (held back)\naudit      ok\n"),
            "{t}"
        );
    }
}

#[test]
fn a_deviation_in_the_audit_exits_1() {
    let (dir, ssh) = stage("dev", &format!("AUDIT_RC=1\n{SSH}"));
    let out = run(&dir, &ssh, &["bootstrap", "acteble"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).ends_with("audit      fail (deviation)\n"));
}

#[test]
fn a_failing_step_exits_1_and_nothing_after_it_is_printed() {
    let (dir, ssh) = stage("fail", &format!("FAIL=fail2ban\n{SSH}"));
    let out = run(&dir, &ssh, &["bootstrap", "acteble"]);
    assert_eq!(out.status.code(), Some(1));
    let t = text(&out);
    assert!(t.contains("harden fail2ban  fail ("), "{t}");
    assert!(!t.contains("runtime") && !t.contains("audit"), "{t}");
}

#[test]
fn a_dry_run_prints_no_audit_and_exits_0() {
    let (dir, ssh) = stage("dry", &format!("FAIL=podman\n{SSH}"));
    let out = run(&dir, &ssh, &["bootstrap", "acteble", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let t = text(&out);
    assert!(
        t.contains("runtime podman    would apply\n") && !t.contains("audit"),
        "{t}"
    );
}

#[test]
fn another_host_exits_3_and_bad_data_exits_1_before_any_step() {
    let (dir, ssh) = stage("wrong", "echo other-box");
    assert_eq!(
        run(&dir, &ssh, &["bootstrap", "acteble"]).status.code(),
        Some(3)
    );
    std::fs::write(dir.join("servers/acteble/dirs.txt"), "relative|admin\n").unwrap();
    let out = run(&dir, &ssh, &["bootstrap", "acteble"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("dirs.txt:1:"));
    assert_eq!(text(&out), "");
}

#[test]
fn usage_errors_and_unknown_names_exit_2() {
    let (dir, ssh) = stage("usage", SSH);
    for verb in ["bootstrap", "apply"] {
        for args in [
            vec![verb],
            vec![verb, "--dry-run"],
            vec![verb, "acteble", "--x"],
        ] {
            let out = run(&dir, &ssh, &args);
            assert_eq!(out.status.code(), Some(2), "{args:?}");
            let usage = format!("usage: actserve {verb} NAME [--dry-run]");
            assert!(String::from_utf8_lossy(&out.stderr).contains(&usage));
        }
        assert_eq!(run(&dir, &ssh, &[verb, "nope"]).status.code(), Some(2));
    }
}

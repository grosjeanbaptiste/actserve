//! `actserve edge NAME` and `actserve stacks NAME` through the real binary, ssh replaced by a script.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn stage(name: &str, ssh_body: &str, stacks: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("actserve-cs-{}-{name}", std::process::id()));
    std::fs::create_dir_all(dir.join("servers/acteble")).unwrap();
    std::fs::write(
        dir.join("servers.txt"),
        "acteble|fake-target.invalid|host-1\n",
    )
    .unwrap();
    std::fs::write(dir.join("servers/acteble/stacks.txt"), stacks).unwrap();
    let ssh = dir.join("ssh");
    std::fs::write(&ssh, format!("#!/bin/sh\n{ssh_body}\n")).unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, ssh)
}

fn run(dir: &Path, ssh: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_actserve"))
        .args(args)
        .env("DOTFILES_STATE_ROOT", dir)
        .env("DOTFILES_SSH", ssh)
        // Nothing here may reach the network: scp and actesskey fail, and the local
        // edge and cdn directories do not exist, so a deploy stops before any call.
        .env("DOTFILES_SCP", "/usr/bin/false")
        .env("ACTSERVE_ACTESSKEY", "/usr/bin/false")
        .env("ACTEBLE_EDGE_DIR", dir.join("no-edge"))
        .env("ACTEBLE_CDN_DIR", dir.join("no-cdn"))
        .output()
        .unwrap()
}

const SSH: &str = r#"case "$*" in *" hostname") echo host-1; exit 0;; esac
case "$*" in *"$FAIL"*) [ -n "$FAIL" ] && exit 1;; esac
exit 0"#;

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn an_edge_that_is_up_prints_ok_lines_and_exits_0() {
    let (dir, ssh) = stage("edge-ok", SSH, "cdn|run\n");
    let out = run(&dir, &ssh, &["edge", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(text(&out), "edge network   ok\nedge caddy     ok\n");
}

#[test]
fn a_missing_caddy_fails_naming_the_edge_files_the_deploy_needs() {
    let (dir, ssh) = stage(
        "edge-down",
        &format!("FAIL=service=caddy\n{SSH}"),
        "cdn|run\n",
    );
    let out = run(&dir, &ssh, &["edge", "acteble"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        text(&out).contains("edge caddy     fail (") && text(&out).contains("ACTEBLE_EDGE_DIR"),
        "{}",
        text(&out)
    );
}

#[test]
fn an_edge_dry_run_says_would_apply_and_exits_0() {
    let (dir, ssh) = stage(
        "edge-dry",
        &format!("FAIL=service=caddy\n{SSH}"),
        "cdn|run\n",
    );
    let out = run(&dir, &ssh, &["edge", "acteble", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(text(&out).contains("edge caddy     would apply\n"));
}

#[test]
fn stacks_print_a_line_per_declared_stack_with_held_ones_skipped() {
    let (dir, ssh) = stage("stacks", SSH, "cdn|run\napi|held\n");
    let out = run(&dir, &ssh, &["stacks", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out),
        "stacks cdn       ok\nstacks api       skipped (held back)\n"
    );
}

#[test]
fn a_bad_stacks_file_exits_1_naming_the_file_and_line() {
    let (dir, ssh) = stage("bad", SSH, "api|run\n");
    let out = run(&dir, &ssh, &["stacks", "acteble"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("stacks.txt:1:"));
}

#[test]
fn another_host_exits_3_for_both_steps() {
    let (dir, ssh) = stage("wrong", "echo other-box", "cdn|run\n");
    assert_eq!(run(&dir, &ssh, &["edge", "acteble"]).status.code(), Some(3));
    assert_eq!(
        run(&dir, &ssh, &["stacks", "acteble"]).status.code(),
        Some(3)
    );
}

#[test]
fn usage_errors_and_unknown_names_exit_2() {
    let (dir, ssh) = stage("usage", SSH, "cdn|run\n");
    for step in ["edge", "stacks"] {
        for args in [
            vec![step],
            vec![step, "--dry-run"],
            vec![step, "acteble", "--x"],
        ] {
            let out = run(&dir, &ssh, &args);
            assert_eq!(out.status.code(), Some(2), "{args:?}");
            let usage = format!("usage: actserve {step} NAME [--dry-run]");
            assert!(String::from_utf8_lossy(&out.stderr).contains(&usage));
        }
        let out = run(&dir, &ssh, &[step, "nope"]);
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
    }
}

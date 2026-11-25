//! `actserve runtime NAME [--dry-run]` through the real binary, ssh replaced by a script.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn stage(name: &str, ssh_body: &str, dirs: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("actserve-cr-{}-{name}", std::process::id()));
    std::fs::create_dir_all(dir.join("servers/acteble")).unwrap();
    std::fs::write(dir.join("servers.txt"), "acteble|api.acteble|host-1\n").unwrap();
    std::fs::write(dir.join("servers/acteble/docker-daemon.json"), "{}\n").unwrap();
    std::fs::write(dir.join("servers/acteble/dirs.txt"), dirs).unwrap();
    let ssh = dir.join("ssh");
    std::fs::write(&ssh, format!("#!/bin/sh\n{ssh_body}\n")).unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, ssh)
}

fn run(dir: &Path, ssh: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_actserve"))
        .args(args)
        .env("ACTCONF_STATE_ROOT", dir)
        .env("DOTFILES_SSH", ssh)
        .output()
        .unwrap()
}

const SSH: &str = r#"case "$*" in *" hostname") echo host-1; exit 0;; esac
cat > /dev/null
case "$*" in *"$FAIL"*) [ -n "$FAIL" ] && exit 1;; esac
exit 0"#;

#[test]
fn a_server_with_its_runtime_prints_ok_lines_and_exits_0() {
    let (dir, ssh) = stage("ok", SSH, "/opt/a|admin\n");
    let out = run(&dir, &ssh, &["runtime", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "runtime docker    ok\nruntime logs      ok\nruntime dirs      ok\n"
    );
}

#[test]
fn a_dry_run_says_would_apply() {
    let (dir, ssh) = stage("dry", &format!("FAIL=cmp\n{SSH}"), "/opt/a|admin\n");
    let out = run(&dir, &ssh, &["runtime", "acteble", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("runtime logs      would apply\n"));
}

#[test]
fn a_failing_item_exits_1() {
    let (dir, ssh) = stage("fail", &format!("FAIL=docker\n{SSH}"), "/opt/a|admin\n");
    let out = run(&dir, &ssh, &["runtime", "acteble"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("runtime docker    fail ("));
}

#[test]
fn another_host_exits_3() {
    let (dir, ssh) = stage("wrong", "echo other-box", "/opt/a|admin\n");
    assert_eq!(
        run(&dir, &ssh, &["runtime", "acteble"]).status.code(),
        Some(3)
    );
}

#[test]
fn a_bad_dirs_file_exits_1_naming_the_file() {
    let (dir, ssh) = stage("dirs", SSH, "relative|admin\n");
    let out = run(&dir, &ssh, &["runtime", "acteble"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("dirs.txt:1:"));
}

#[test]
fn usage_errors_and_unknown_names_exit_2() {
    let (dir, ssh) = stage("usage", SSH, "/opt/a|admin\n");
    for args in [
        &["runtime"][..],
        &["runtime", "--dry-run"],
        &["runtime", "acteble", "--x"],
    ] {
        let out = run(&dir, &ssh, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("usage: actserve runtime NAME [--dry-run]")
        );
    }
    let out = run(&dir, &ssh, &["runtime", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
}

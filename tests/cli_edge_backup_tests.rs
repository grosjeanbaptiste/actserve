//! `actserve edge-backup NAME [DIR]` through the real binary, ssh replaced by a script.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn stage(name: &str, ssh_body: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("actserve-eb-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("servers.txt"),
        "acteble|fake-target.invalid|host-1\n",
    )
    .unwrap();
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
        .env("HOME", dir.join("home"))
        .output()
        .unwrap()
}
const SSH: &str = r#"case "$*" in *" hostname") echo "${HOST:-host-1}"; exit 0;; esac
printf 'TARBYTES'; [ -n "$FAILTAR" ] && { echo "no caddy" >&2; exit 1; }; exit 0"#;
fn files(d: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(d)
        .map(|r| r.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default()
}

#[test]
fn a_backup_is_saved_under_the_given_directory_and_its_path_printed() {
    let (dir, ssh) = stage("ok", SSH);
    let target = dir.join("bk");
    let out = run(
        &dir,
        &ssh,
        &["edge-backup", "acteble", target.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let saved = files(&target);
    assert_eq!(saved.len(), 1);
    let name = saved[0].file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("acteble-edge-certs-") && name.ends_with(".tar.gz"),
        "{name}"
    );
    assert_eq!(std::fs::read(&saved[0]).unwrap(), b"TARBYTES");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(text, format!("==> Saved {}\n", saved[0].display()));
}

#[test]
fn without_a_directory_it_uses_backups_acteble_edge_under_home() {
    let (dir, ssh) = stage("home", SSH);
    let out = run(&dir, &ssh, &["edge-backup", "acteble"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(files(&dir.join("home/Backups/acteble-edge")).len(), 1);
}

#[test]
fn a_failed_transfer_exits_1_says_nothing_was_saved_and_leaves_no_file() {
    let (dir, ssh) = stage("fail", &format!("FAILTAR=1\n{SSH}"));
    let target = dir.join("bk");
    let out = run(
        &dir,
        &ssh,
        &["edge-backup", "acteble", target.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        err.contains("transfer failed, nothing saved") && err.contains("no caddy"),
        "{err}"
    );
    assert!(files(&target).is_empty());
}

#[test]
fn another_host_exits_3_and_downloads_nothing() {
    let (dir, ssh) = stage("wrong", &format!("HOST=other\n{SSH}"));
    let target = dir.join("bk");
    let out = run(
        &dir,
        &ssh,
        &["edge-backup", "acteble", target.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert!(!target.exists());
}

#[test]
fn an_unknown_name_and_bad_usage_exit_2() {
    let (dir, ssh) = stage("usage", SSH);
    for args in [
        vec!["edge-backup", "nope"],
        vec!["edge-backup"],
        vec!["edge-backup", "acteble", "a", "b"],
        vec!["edge-backup", "--oops"],
    ] {
        let out = run(&dir, &ssh, &args);
        assert_eq!(out.status.code(), Some(2), "{args:?} {out:?}");
    }
}

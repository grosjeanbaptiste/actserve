//! The real binary, a stand-in `ssh` and a throw-away state root.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Rig {
    dir: PathBuf,
}

impl Rig {
    /// `ssh_body` is the body of the fake ssh; `$*` holds what it was called with.
    fn new(name: &str, ssh_body: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("actserve-cli-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let server = dir.join("servers/acteble");
        std::fs::create_dir_all(&server).unwrap();
        std::fs::write(dir.join("servers.txt"), "acteble|ops@vps|cloud-1\n").unwrap();
        for (f, text) in [
            ("sshd.conf", "PasswordAuthentication no\n"),
            ("ports.txt", "22/tcp\n443/tcp\n"),
            ("dirs.txt", "/opt/acteble/edge|deploy\n"),
            ("stacks.txt", "cdn|run\napi|held\n"),
        ] {
            std::fs::write(server.join(f), text).unwrap();
        }
        let ssh = dir.join("ssh");
        std::fs::write(
            &ssh,
            format!(
                "#!/bin/sh\necho \"$*\" >> '{}/calls'\n{ssh_body}\n",
                dir.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_env(args, &[])
    }

    fn run_env(&self, args: &[&str], env: &[(&str, &Path)]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_actserve"));
        cmd.args(args)
            .env("ACTSERVE_STATE_ROOT", &self.dir)
            .env("ACTSERVE_SSH", self.dir.join("ssh"));
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.output().unwrap()
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.dir.join("calls")).unwrap_or_default()
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const HOST_OK: &str = r#"case "$*" in *hostname) echo cloud-1;; *) exit 0;; esac"#;

#[test]
fn status_prints_one_line_per_host_and_exits_0_when_all_answer() {
    let r = Rig::new("status-ok", HOST_OK);
    let o = r.run(&["status"]);
    assert_eq!(out(&o), "acteble: ok\n");
    assert_eq!(o.status.code(), Some(0));
}

#[test]
fn status_of_another_host_exits_1() {
    let r = Rig::new("status-wrong", "echo intruder");
    let o = r.run(&["status", "acteble"]);
    assert!(out(&o).contains("wrong-host"), "{}", out(&o));
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn the_dotfiles_variables_are_not_read() {
    let r = Rig::new("no-dotfiles-env", HOST_OK);
    let broken = r.dir.join("broken-ssh");
    std::fs::write(&broken, "#!/bin/sh\nexit 255\n").unwrap();
    std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o755)).unwrap();
    let o = r.run_env(&["status"], &[("DOTFILES_SSH", &broken)]);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));
}

#[test]
fn audit_passes_the_verdict_and_the_servers_report_through() {
    let r = Rig::new(
        "audit-dev",
        r#"case "$*" in *hostname) echo cloud-1;; *"edge audit"*) echo "ufw off"; exit 1;; esac"#,
    );
    let o = r.run(&["audit", "acteble"]);
    assert_eq!(out(&o), "ufw off\n");
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn audit_conforming_exits_0_and_unreadable_exits_3() {
    let ok = Rig::new(
        "audit-ok",
        r#"case "$*" in *hostname) echo cloud-1;; *) echo fine;; esac"#,
    );
    assert_eq!(ok.run(&["audit", "acteble"]).status.code(), Some(0));
    let bad = Rig::new(
        "audit-unreadable",
        r#"case "$*" in *hostname) echo cloud-1;; *) exit 255;; esac"#,
    );
    assert_eq!(bad.run(&["audit", "acteble"]).status.code(), Some(3));
}

#[test]
fn audit_without_a_name_is_a_usage_error_and_sends_nothing() {
    let r = Rig::new("audit-usage", HOST_OK);
    let o = r.run(&["audit"]);
    assert_eq!(o.status.code(), Some(2));
    assert_eq!(r.calls(), "");
}

#[test]
fn audit_of_an_unknown_server_fails_and_sends_nothing() {
    let r = Rig::new("audit-unknown", HOST_OK);
    let o = r.run(&["audit", "nope"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(r.calls(), "");
}

#[test]
fn apply_without_dry_run_is_refused_and_sends_nothing() {
    let r = Rig::new("apply-refused", HOST_OK);
    for args in [
        &["apply", "acteble"][..],
        &["apply"][..],
        &["apply", "--dry-run"][..],
    ] {
        assert_eq!(r.run(args).status.code(), Some(2), "{args:?}");
    }
    assert_eq!(r.calls(), "");
}

#[test]
fn apply_dry_run_of_a_conforming_server_exits_0() {
    let r = Rig::new("apply-ok", HOST_OK);
    let o = r.run(&["apply", "acteble", "--dry-run"]);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));
    assert!(out(&o).starts_with("reach      ok\n"), "{}", out(&o));
    assert!(
        out(&o).contains("stacks api       skipped (held back)"),
        "{}",
        out(&o)
    );
}

#[test]
fn apply_dry_run_names_what_would_change_and_exits_1() {
    let r = Rig::new(
        "apply-change",
        r#"case "$*" in *hostname) echo cloud-1;; *fail2ban*) exit 3;; *) exit 0;; esac"#,
    );
    let o = r.run(&["apply", "acteble", "--dry-run"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        out(&o).contains("harden fail2ban  would apply"),
        "{}",
        out(&o)
    );
}

#[test]
fn apply_dry_run_sends_the_declared_file_on_stdin() {
    let r = Rig::new(
        "apply-stdin",
        r#"case "$*" in *hostname) echo cloud-1;; *cmp*00-acteble.conf*) cat >> "$0.stdin"; exit 0;; *) exit 0;; esac"#,
    );
    r.run(&["apply", "acteble", "--dry-run"]);
    let got = std::fs::read_to_string(r.dir.join("ssh.stdin")).unwrap();
    assert_eq!(got, "PasswordAuthentication no\n");
}

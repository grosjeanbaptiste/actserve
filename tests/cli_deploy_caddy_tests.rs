//! `actserve deploy-caddy` through the real binary: ssh, git and actesskey are scripts.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct Env {
    dir: PathBuf,
}
impl Env {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("actserve-cad-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("api")).unwrap();
        std::fs::write(dir.join("servers.txt"), "acteble|api.acteble|host-1\n").unwrap();
        script(&dir.join("ssh"), SSH);
        script(&dir.join("git"), GIT);
        script(&dir.join("actesskey"), r#"printf 'value-of-%s\n' "$2""#);
        Self { dir }
    }
    fn run(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_actserve"));
        c.args(args)
            .env("DOTFILES_STATE_ROOT", &self.dir)
            .env("DOTFILES_SSH", self.dir.join("ssh"))
            .env("ACTSERVE_GIT", self.dir.join("git"))
            .env("ACTSERVE_ACTESSKEY", self.dir.join("actesskey"))
            .env("ACTEBLE_API_DIR", self.dir.join("api"))
            .env("LOG", self.dir.join("log"));
        for (k, v) in extra {
            c.env(k, v);
        }
        c.output().unwrap()
    }
    fn log(&self) -> String {
        std::fs::read_to_string(self.dir.join("log")).unwrap_or_default()
    }
}

/// Answers the hostname probe; logs every other call (arguments, then stdin).
const SSH: &str = r#"case "$*" in *" hostname") echo host-1; exit 0;; esac
echo "ssh $*" >> "$LOG"
cat >> "$LOG"
case "$*" in *"caddy reload"*) exit ${RELOAD_RC:-0};; esac
exit 0"#;
const GIT: &str = r#"echo "git $*" >> "$LOG"
if [ "$3" = show ]; then echo 'site-from-main'; fi
exit ${GIT_RC:-0}"#;

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn it_deploys_the_caddyfile_of_main_and_reloads() {
    let e = Env::new("ok");
    let o = e.run(&["deploy-caddy", "acteble"], &[]);
    assert_eq!(o.status.code(), Some(0), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("deploy-caddy reload"), "{}", out(&o));
    let log = e.log();
    assert!(log.contains("git -C"), "{log}");
    assert!(log.contains("site-from-main"), "{log}");
    assert!(log.contains("caddy reload"), "{log}");
    assert!(log.contains("rm -f '/dev/shm/acteble-api.env'"), "{log}");
}

#[test]
fn a_refused_reload_exits_1_and_still_removes_the_env_file() {
    let e = Env::new("reload");
    let o = e.run(&["deploy-caddy", "acteble"], &[("RELOAD_RC", "1")]);
    assert_eq!(o.status.code(), Some(1));
    assert!(e.log().contains("rm -f '/dev/shm/acteble-api.env'"));
}

#[test]
fn a_git_failure_exits_1_and_touches_no_server() {
    let e = Env::new("git");
    let o = e.run(&["deploy-caddy", "acteble"], &[("GIT_RC", "128")]);
    assert_eq!(o.status.code(), Some(1));
    assert!(!e.log().contains("ssh "), "{}", e.log());
}

#[test]
fn a_dry_run_runs_nothing() {
    let e = Env::new("dry");
    let o = e.run(&["deploy-caddy", "--dry-run", "acteble"], &[]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).contains("would apply"));
    assert_eq!(e.log(), "");
}

#[test]
fn another_host_exits_3() {
    let e = Env::new("wrong");
    script(&e.dir.join("ssh"), "echo other-host");
    let o = e.run(&["deploy-caddy", "acteble"], &[]);
    assert_eq!(o.status.code(), Some(3), "{}{}", out(&o), err(&o));
}

#[test]
fn an_unknown_name_exits_2_and_usage_errors_exit_2() {
    let e = Env::new("usage");
    assert_eq!(e.run(&["deploy-caddy", "nope"], &[]).status.code(), Some(2));
    assert_eq!(e.run(&["deploy-caddy"], &[]).status.code(), Some(2));
    assert_eq!(
        e.run(&["deploy-caddy", "--x", "acteble"], &[])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        e.run(&["deploy-caddy", "a", "b"], &[]).status.code(),
        Some(2)
    );
}

//! `actserve rotate` through the real binary: ssh, scp, actesskey and machine are scripts.
mod common;

use actserve::domain::api_podman::API_UNIT_FILES;
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
        let dir =
            std::env::temp_dir().join(format!("actserve-rot-cli-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("api")).unwrap();
        std::fs::write(
            dir.join("servers.txt"),
            "acteble|fake-target.invalid|host-1\n",
        )
        .unwrap();
        for f in API_UNIT_FILES {
            let path = dir.join("api").join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        std::fs::write(dir.join("prod.env"), common::full_api_env()).unwrap();
        script(&dir.join("ssh"), SSH);
        script(&dir.join("scp"), "echo \"scp $*\" >> \"$LOG\"");
        script(
            &dir.join("machine"),
            "echo \"machine $*\" >> \"$LOG\"; exit ${MACHINE_RC:-0}",
        );
        script(&dir.join("actesskey"), ACTESSKEY);
        Self { dir }
    }
    fn run(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_actserve"));
        c.args(args)
            .env("ACTCONF_STATE_ROOT", &self.dir)
            .env("DOTFILES_SSH", self.dir.join("ssh"))
            .env("DOTFILES_SCP", self.dir.join("scp"))
            .env("ACTSERVE_ACTESSKEY", self.dir.join("actesskey"))
            .env("ACTSERVE_MACHINE", self.dir.join("machine"))
            .env("ACTEBLE_API_DIR", self.dir.join("api"))
            .env("LOG", self.dir.join("log"))
            .env("KV", self.dir.join("kv"))
            .env("PROD_ENV", self.dir.join("prod.env"));
        for (k, v) in extra {
            c.env(k, v);
        }
        c.output().unwrap()
    }
    fn log(&self) -> String {
        std::fs::read_to_string(self.dir.join("log")).unwrap_or_default()
    }
    fn kv(&self) -> String {
        std::fs::read_to_string(self.dir.join("kv")).unwrap_or_default()
    }
}

/// Answers the hostname probe; logs every other call (arguments, then stdin).
const SSH: &str = r#"case "$*" in *" hostname") echo "${HOST:-host-1}"; exit 0;; esac
echo "ssh $*" >> "$LOG"
cat >> "$LOG"
case "$*" in *psql*) echo "psql: role missing" >&2; exit ${PSQL_RC:-0};; esac
exit 0"#;
const ACTESSKEY: &str = r#"case "$1" in
  get) if [ "$2" = acteble-api-prod ]; then cat "$PROD_ENV"; else printf 'value-of-%s\n' "$2"; fi;;
  kv-set) echo "actesskey kv-set $2" >> "$LOG"; cat >> "$KV";;
esac"#;

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn a_dry_run_prints_the_plan_and_changes_nothing() {
    let e = Env::new("dry");
    let out = e.run(&["rotate", "acteble", "all", "--dry-run"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        "rotate database  would apply\nrotate secret    would apply\nrotate persist   would apply\n\
         deploy prepare   would apply\ndeploy copy      would apply\ndeploy restart   would apply\n"
    );
    assert_eq!(e.log(), "");
    assert_eq!(e.kv(), "");
}

#[test]
fn without_yes_a_real_rotation_is_refused_before_any_call() {
    let e = Env::new("noyes");
    let out = e.run(&["rotate", "acteble", "jwt"], &[]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(text(&out.stderr).contains("--yes"), "{}", text(&out.stderr));
    assert_eq!((e.log().as_str(), e.kv().as_str()), ("", ""));
}

#[test]
fn bad_arguments_exit_2_and_an_unknown_target_exits_1() {
    let e = Env::new("args");
    for args in [
        vec!["rotate"],
        vec!["rotate", "acteble"],
        vec!["rotate", "acteble", "jwt", "extra", "--yes"],
        vec!["rotate", "acteble", "jwt", "--force"],
        vec!["rotate", "--yes", "jwt"],
        vec!["rotate", "-x", "jwt"],
        vec!["rotate", "acteble", "-x"],
    ] {
        let out = e.run(&args, &[]);
        assert_eq!(out.status.code(), Some(2), "{args:?} {out:?}");
        assert!(text(&out.stderr).contains("usage: actserve rotate"));
    }
    let out = e.run(&["rotate", "acteble", "redis", "--yes"], &[]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(text(&out.stderr).contains("jwt|postgres|minio|all"));
    assert_eq!(e.log(), "");
}

#[test]
fn a_jwt_rotation_writes_one_key_persists_and_redeploys_without_printing_it() {
    let e = Env::new("jwt");
    let out = e.run(&["rotate", "acteble", "jwt", "--yes"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        "rotate secret    applied\nrotate persist   applied\ndeploy prepare   applied\ndeploy copy      applied\ndeploy restart   applied\n"
    );
    let kv = e.kv();
    let (key, value) = kv.trim_end().split_once('\t').unwrap();
    assert_eq!(key, "APP_AUTH__JWT_SECRET");
    assert_eq!(value.len(), 88);
    let shown = format!("{}{}", text(&out.stdout), text(&out.stderr));
    assert!(!shown.contains(value));
    let log = e.log();
    assert!(log.contains("actesskey kv-set acteble-api-prod"), "{log}");
    assert!(log.contains("machine autocommit"), "{log}");
    assert!(!log.contains("psql"), "{log}");
    assert!(log.find("kv-set").unwrap() < log.find("machine autocommit").unwrap());
    assert!(log.find("machine autocommit").unwrap() < log.find("scp").unwrap());
}

#[test]
fn a_postgres_rotation_sends_the_same_new_password_to_the_database_and_the_secret() {
    let e = Env::new("pg");
    let out = e.run(&["rotate", "acteble", "postgres", "--yes"], &[]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let kv = e.kv();
    let (key, password) = kv.trim_end().split_once('\t').unwrap();
    assert_eq!(key, "POSTGRES_PASSWORD");
    assert_eq!(password.len(), 64);
    assert!(password.chars().all(|c| c.is_ascii_hexdigit()));
    let log = e.log();
    assert!(
        log.contains(&format!(
            "ALTER USER acteble_prod WITH PASSWORD '{password}';"
        )),
        "{log}"
    );
    assert!(log.find("ALTER USER").unwrap() < log.find("kv-set").unwrap());
    let args_only: String = log
        .lines()
        .filter(|l| l.starts_with("ssh "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!args_only.contains(password), "password in an ssh argument");
    assert!(!text(&out.stdout).contains(password));
}

#[test]
fn a_database_user_comes_from_the_environment() {
    let e = Env::new("user");
    let out = e.run(
        &["rotate", "acteble", "postgres", "--yes"],
        &[("ACTEBLE_DB_USER", "other_user")],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(e.log().contains("ALTER USER other_user WITH PASSWORD"));
}

#[test]
fn a_bad_database_user_is_refused_before_any_call() {
    let e = Env::new("baduser");
    let out = e.run(
        &["rotate", "acteble", "postgres", "--yes"],
        &[("ACTEBLE_DB_USER", "a'b")],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!((e.log().as_str(), e.kv().as_str()), ("", ""));
}

#[test]
fn a_refused_database_change_leaves_the_secret_alone_and_exits_1() {
    let e = Env::new("pgfail");
    let out = e.run(
        &["rotate", "acteble", "postgres", "--yes"],
        &[("PSQL_RC", "1")],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let shown = text(&out.stdout);
    assert!(
        shown.contains("psql: role missing") && shown.contains("secret untouched"),
        "{shown}"
    );
    assert_eq!(e.kv(), "");
    assert!(!e.log().contains("machine autocommit"));
    assert!(!e.log().contains("scp"));
}

#[test]
fn a_failed_persist_exits_1_after_the_redeploy_ran() {
    let e = Env::new("persist");
    let out = e.run(
        &["rotate", "acteble", "jwt", "--yes"],
        &[("MACHINE_RC", "1")],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(text(&out.stdout).contains("machine autocommit"));
    assert!(e.log().contains("scp"));
}

#[test]
fn a_wrong_host_exits_3_and_changes_nothing() {
    let e = Env::new("host");
    let out = e.run(&["rotate", "acteble", "all", "--yes"], &[("HOST", "other")]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert_eq!((e.log().as_str(), e.kv().as_str()), ("", ""));
}

#[test]
fn an_unknown_server_exits_2() {
    let e = Env::new("unknown");
    let out = e.run(&["rotate", "nope", "jwt", "--yes"], &[]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

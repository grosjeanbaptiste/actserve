//! Which program actserve runs for ssh: `$ACTSERVE_SSH`, never the dotfiles variable.
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `actserve status acteble` with PATH holding an `ssh` that logs "path" and answers wrong.
fn status(name: &str, vars: &[(&str, &str)]) -> (Output, String) {
    let dir = std::env::temp_dir().join(format!("actserve-env-{}-{name}", std::process::id()));
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("servers.txt"), "acteble|api.acteble|host-1\n").unwrap();
    script(&dir.join("bin/ssh"), "echo path >> \"$LOG\"; echo other");
    script(&dir.join("named"), "echo named >> \"$LOG\"; echo host-1");
    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap()
    );
    let mut c = Command::new(env!("CARGO_BIN_EXE_actserve"));
    c.args(["status", "acteble"])
        .env("ACTCONF_STATE_ROOT", &dir)
        .env("PATH", path)
        .env("LOG", dir.join("log"))
        .env_remove("ACTSERVE_SSH")
        .env_remove("DOTFILES_SSH");
    for (k, v) in vars {
        c.env(k, v.replace("DIR", dir.to_str().unwrap()));
    }
    let out = c.output().unwrap();
    (
        out,
        std::fs::read_to_string(dir.join("log")).unwrap_or_default(),
    )
}

#[test]
fn actserve_ssh_names_the_ssh_program() {
    let (out, log) = status("named", &[("ACTSERVE_SSH", "DIR/named")]);
    assert_eq!(log, "named\n", "{out:?}");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn the_dotfiles_variable_is_ignored() {
    let (_, log) = status("ignored", &[("DOTFILES_SSH", "DIR/named")]);
    assert_eq!(log, "path\n");
}

//! `GitCaddyfileSource` against a fake `git` script.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use actserve::adapters::outbound::system::git_caddyfile_source::GitCaddyfileSource;
use actserve::ports::outbound::caddyfile_source::CaddyfileSourcePort;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("actserve-git-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn fake(d: &Path, body: &str) -> PathBuf {
    let p = d.join("git");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[tokio::test]
async fn it_fetches_main_then_reads_its_caddyfile() {
    let d = dir("ok");
    let git = fake(
        &d,
        "echo \"$*\" >> \"$0.args\"\nif [ \"$3\" = show ]; then printf 'site {}\\n'; fi",
    );
    let text = GitCaddyfileSource::new(&git)
        .origin_main(Path::new("/repo"))
        .await
        .unwrap();
    assert_eq!(text, "site {}\n");
    assert_eq!(
        std::fs::read_to_string(format!("{}.args", git.display())).unwrap(),
        "-C /repo fetch --quiet origin main\n-C /repo show origin/main:Caddyfile\n"
    );
}

#[tokio::test]
async fn a_failing_fetch_stops_before_the_show_and_names_git() {
    let d = dir("fetch");
    let git = fake(
        &d,
        "echo \"$*\" >> \"$0.args\"\necho 'no network' >&2; exit 128",
    );
    let e = GitCaddyfileSource::new(&git)
        .origin_main(Path::new("/repo"))
        .await
        .unwrap_err();
    let m = format!("{e:?}");
    assert!(
        m.contains("git fetch origin main") && m.contains("no network"),
        "{m}"
    );
    assert_eq!(
        std::fs::read_to_string(format!("{}.args", git.display()))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[tokio::test]
async fn a_main_without_caddyfile_is_an_error() {
    let d = dir("show");
    let git = fake(
        &d,
        "if [ \"$3\" = show ]; then echo 'does not exist' >&2; exit 128; fi",
    );
    let e = GitCaddyfileSource::new(&git)
        .origin_main(Path::new("/repo"))
        .await
        .unwrap_err();
    let m = format!("{e:?}");
    assert!(
        m.contains("origin/main:Caddyfile") && m.contains("does not exist"),
        "{m}"
    );
}

#[tokio::test]
async fn a_caddyfile_that_is_not_utf8_is_an_error() {
    let d = dir("utf8");
    let git = fake(&d, "if [ \"$3\" = show ]; then printf '\\377\\376'; fi");
    let e = GitCaddyfileSource::new(&git)
        .origin_main(Path::new("/repo"))
        .await
        .unwrap_err();
    assert!(format!("{e:?}").contains("UTF-8"), "{e:?}");
}

#[tokio::test]
async fn a_missing_binary_is_an_error_naming_it() {
    let e = GitCaddyfileSource::new("/nonexistent/git")
        .origin_main(Path::new("/repo"))
        .await
        .unwrap_err();
    assert!(format!("{e:?}").contains("/nonexistent/git"), "{e:?}");
}

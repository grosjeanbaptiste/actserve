use actserve::domain::dirs_file::{DeclaredDir, parse_dirs};
use actserve::domain::errors::DomainError;
use actserve::domain::runtime::{
    DOCKER_APPLY, DOCKER_CHECK, LOGS_APPLY, LOGS_CHECK, dirs_apply_command, dirs_check_command,
};

fn dir(path: &str, owner: &str) -> DeclaredDir {
    DeclaredDir {
        path: path.into(),
        owner: owner.into(),
    }
}

fn problem(text: &str) -> String {
    match parse_dirs("dirs.txt", text).unwrap_err() {
        DomainError::ValidationError(m) => m,
        other => panic!("{other:?}"),
    }
}

#[test]
fn dirs_keep_file_order_and_skip_comments_blank_lines_and_padding() {
    let got = parse_dirs("dirs.txt", "# c\n\n  /opt/a|admin  \n/opt/a/b|_svc-1\n").unwrap();
    assert_eq!(got, [dir("/opt/a", "admin"), dir("/opt/a/b", "_svc-1")]);
}

#[test]
fn an_empty_file_declares_no_directory() {
    assert_eq!(parse_dirs("dirs.txt", "# only\n").unwrap(), []);
}

#[test]
fn every_bad_line_is_reported_with_file_and_line_in_one_pass() {
    let m = problem("/ok|admin\nrelative|admin\n/a/../b|admin\n/a|Root\n/a\n/a b|x\n/a|\n|x\n");
    for n in [2, 3, 4, 5, 6, 7, 8] {
        assert!(
            m.contains(&format!("dirs.txt:{n}:")),
            "line {n} missing in {m}"
        );
    }
    assert!(!m.contains("dirs.txt:1:"), "{m}");
    assert!(m.contains("ABSOLUTE-PATH|OWNER"), "{m}");
}

#[test]
fn a_trailing_dot_dot_component_is_refused_but_dots_inside_a_name_are_fine() {
    assert!(problem("/a/..|admin").contains("dirs.txt:1:"));
    assert_eq!(
        parse_dirs("dirs.txt", "/opt/a..b/c.d|admin").unwrap(),
        [dir("/opt/a..b/c.d", "admin")]
    );
}

#[test]
fn shell_metacharacters_in_a_path_or_owner_are_refused() {
    for line in ["/a;rm|x", "/a'b|x", "/a$(x)|x", "/a|x;y", "/a|x y", "/a|1x"] {
        assert!(problem(line).contains("dirs.txt:1:"), "{line}");
    }
}

#[test]
fn dirs_check_compares_every_owner_and_apply_creates_then_chowns_each() {
    let d = [dir("/opt/a", "admin"), dir("/opt/a/b", "svc")];
    assert_eq!(
        dirs_check_command(&d),
        r#"test "$(stat -c %U /opt/a)" = admin && test "$(stat -c %U /opt/a/b)" = svc"#
    );
    assert_eq!(
        dirs_apply_command(&d),
        "sudo -n sh -c '{ [ -d /opt/a ] || install -d -o admin -g admin /opt/a; } && chown admin:admin /opt/a && { [ -d /opt/a/b ] || install -d -o svc -g svc /opt/a/b; } && chown svc:svc /opt/a/b'"
    );
}

#[test]
fn the_fixed_commands_match_the_bash_tool() {
    assert_eq!(DOCKER_CHECK, "docker compose version > /dev/null 2>&1");
    assert_eq!(
        DOCKER_APPLY,
        "sudo -n sh -c 'DEBIAN_FRONTEND=noninteractive apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin'"
    );
    assert_eq!(LOGS_CHECK, "sudo -n cmp -s - /etc/docker/daemon.json");
    assert_eq!(
        LOGS_APPLY,
        "sudo -n sh -c 'mkdir -p /etc/docker && cat > /etc/docker/daemon.json.new && mv /etc/docker/daemon.json.new /etc/docker/daemon.json'"
    );
}

#[test]
fn the_root_directory_itself_is_refused() {
    assert!(problem("/|admin").contains("dirs.txt:1:"));
}

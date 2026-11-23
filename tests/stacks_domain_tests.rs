use actserve::domain::errors::DomainError;
use actserve::domain::stacks::{
    CADDY_CHECK, DeclaredStack, NETWORK_APPLY, NETWORK_CHECK, StackMode, cdn_check_command,
    parse_stacks,
};

fn problem(text: &str) -> String {
    match parse_stacks("stacks.txt", text).unwrap_err() {
        DomainError::ValidationError(m) => m,
        other => panic!("{other:?}"),
    }
}

fn stack(name: &str, mode: StackMode) -> DeclaredStack {
    DeclaredStack {
        name: name.into(),
        mode,
    }
}

#[test]
fn stacks_keep_file_order_and_skip_comments_blank_lines_and_padding() {
    let got = parse_stacks("stacks.txt", "# c\n\n cdn|run \napi|held\n").unwrap();
    assert_eq!(
        got,
        [stack("cdn", StackMode::Run), stack("api", StackMode::Held)]
    );
}

#[test]
fn an_empty_file_declares_no_stack() {
    assert_eq!(parse_stacks("stacks.txt", "# none\n").unwrap(), []);
}

#[test]
fn api_can_only_be_held_and_the_reason_is_given() {
    let m = problem("api|run\n");
    assert!(m.contains("stacks.txt:1:") && m.contains("held"), "{m}");
}

#[test]
fn unknown_names_unknown_modes_and_missing_modes_are_reported_in_one_pass() {
    let m = problem("cdn|run\nedge|run\ncdn|now\ncdn\n|run\nCDN|run\n");
    for n in [2, 3, 4, 5, 6] {
        assert!(m.contains(&format!("stacks.txt:{n}:")), "line {n} in {m}");
    }
    assert!(!m.contains("stacks.txt:1:"), "{m}");
    assert!(m.contains("the edge is step 4"), "{m}");
}

#[test]
fn the_commands_match_the_bash_tool() {
    assert_eq!(
        NETWORK_CHECK,
        "docker network inspect acteble_edge > /dev/null 2>&1"
    );
    assert_eq!(
        NETWORK_APPLY,
        "docker network create acteble_edge > /dev/null"
    );
    assert_eq!(
        CADDY_CHECK,
        r#"test -n "$(docker ps -q --filter label=com.docker.compose.project=acteble_edge --filter label=com.docker.compose.service=caddy)""#
    );
    assert_eq!(
        cdn_check_command(),
        r#"test -n "$(docker ps -q --filter label=com.docker.compose.project=acteble_cdn)""#
    );
}

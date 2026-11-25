//! The geo build: stages, the commands run on the server, the signing-key check.
use actserve::domain::geo_build::{
    GEO_BUILD_NAME, GeoStage, SIGNING_KEY, build_script, env_has_value, follow_command,
};

#[test]
fn the_four_stages_parse_and_print_back() {
    for s in ["catalog", "tree", "publish", "all"] {
        assert_eq!(GeoStage::parse(s).unwrap().as_str(), s);
    }
}

#[test]
fn another_stage_is_refused_naming_the_four() {
    for s in ["", "All", "build", "tree;rm", "--stage"] {
        let e = GeoStage::parse(s).unwrap_err().to_string();
        assert!(e.contains("catalog|tree|publish|all"), "{e}");
    }
}

#[test]
fn the_build_script_pulls_then_runs_detached_with_the_stage() {
    let s = build_script(GeoStage::Tree);
    assert!(s.starts_with("cd '/opt/acteble/cdn'\n"), "{s}");
    let run = "docker compose --env-file /dev/shm/acteble-cdn.env --profile tools";
    let pull = format!("{run} pull geo_builder");
    let up = format!("{run} run -d --name {GEO_BUILD_NAME} geo_builder --stage tree");
    assert!(s.contains(&format!("{pull} && {up}")), "{s}");
}

#[test]
fn the_build_name_is_the_one_of_the_bash_command() {
    assert_eq!(GEO_BUILD_NAME, "acteble-geo-build");
}

#[test]
fn following_streams_the_logs_waits_and_removes_the_container() {
    assert_eq!(
        follow_command(),
        "docker logs -f acteble-geo-build; rc=$(docker wait acteble-geo-build); docker rm acteble-geo-build >/dev/null; exit $rc"
    );
}

#[test]
fn a_set_signing_key_is_found() {
    assert!(env_has_value(
        "A=1\nCDN_MANIFEST_SIGNING_KEY=abc\nB=2\n",
        SIGNING_KEY
    ));
}

#[test]
fn a_missing_or_empty_signing_key_is_not_found() {
    assert!(!env_has_value("A=1\n", SIGNING_KEY));
    assert!(!env_has_value("CDN_MANIFEST_SIGNING_KEY=\n", SIGNING_KEY));
    assert!(!env_has_value(
        "# CDN_MANIFEST_SIGNING_KEY=abc\n",
        SIGNING_KEY
    ));
}

#[test]
fn a_key_that_only_ends_or_starts_like_the_wanted_one_does_not_count() {
    assert!(!env_has_value(
        "XCDN_MANIFEST_SIGNING_KEY=abc\n",
        SIGNING_KEY
    ));
    assert!(!env_has_value(
        "CDN_MANIFEST_SIGNING_KEY_OLD=abc\n",
        SIGNING_KEY
    ));
}

#[test]
fn only_the_first_line_of_the_key_is_what_counts() {
    assert!(env_has_value("CDN_MANIFEST_SIGNING_KEY=a=b\n", SIGNING_KEY));
    assert!(env_has_value("CDN_MANIFEST_SIGNING_KEY=abc", SIGNING_KEY));
}

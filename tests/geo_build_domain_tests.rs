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

const IMAGE: &str = "ghcr.io/acteble/acteble_geo:latest";

#[test]
fn the_script_sources_the_env_then_makes_the_signing_secret_then_pulls_then_runs() {
    let s = build_script(GeoStage::Tree);
    let at = |p: &str| s.find(p).unwrap_or_else(|| panic!("{p}: {s}"));
    let source = at("set -a && . /dev/shm/acteble-cdn.env && set +a");
    let secret = at(
        "printf %s \"$CDN_MANIFEST_SIGNING_KEY\" | podman secret create --replace acteble_cdn_manifest_signing_key - >/dev/null",
    );
    let pull = at(&format!("podman pull {IMAGE}"));
    let run = at(&format!("podman run -d --name {GEO_BUILD_NAME} "));
    assert!(source < secret && secret < pull && pull < run, "{s}");
}

#[test]
fn a_failed_secret_or_pull_stops_before_the_build_starts() {
    let s = build_script(GeoStage::All);
    assert!(s.contains("- >/dev/null || exit 1\n"), "{s}");
    assert!(
        s.contains(&format!("podman pull {IMAGE} && podman run")),
        "{s}"
    );
}

#[test]
fn the_container_is_confined_like_its_quadlet_unit() {
    let s = build_script(GeoStage::All);
    for flag in [
        "--network acteble_cdn",
        "--cap-drop ALL",
        "--security-opt no-new-privileges",
        "--memory 2800m",
        "--ulimit nofile=262144:262144",
        "-v acteble_cdn_geo_work:/srv/acteble_cdn/geo-out",
    ] {
        assert!(s.contains(flag), "{flag}: {s}");
    }
}

#[test]
fn the_store_and_signing_values_come_from_podman_secrets_never_an_argument() {
    let s = build_script(GeoStage::All);
    for secret in [
        "--secret acteble_cdn_minio_root_user,type=env,target=APP__STORAGE__MINIO__ACCESS_KEY",
        "--secret acteble_cdn_minio_root_password,type=env,target=APP__STORAGE__MINIO__SECRET_KEY",
        "--secret acteble_cdn_manifest_signing_key,type=env,target=GEO_MANIFEST_SIGNING_KEY",
        "--secret acteble_cdn_manifest_signing_key,type=env,target=CDN_MANIFEST_SIGNING_KEY",
    ] {
        assert!(s.contains(secret), "{secret}: {s}");
    }
}

#[test]
fn the_storage_settings_point_at_the_cdn_minio() {
    let s = build_script(GeoStage::All);
    for e in [
        "-e APP__STORAGE__MINIO__ENDPOINT=cdn-minio:9000",
        "-e APP__STORAGE__MINIO__BUCKET=acteble-files",
        "-e APP__STORAGE__MINIO__SECURE=false",
        "-e APP__STORAGE__MINIO__REGION=us-east-1",
    ] {
        assert!(s.contains(e), "{e}: {s}");
    }
}

#[test]
fn the_image_and_the_stage_end_the_run_command() {
    for st in [
        GeoStage::Catalog,
        GeoStage::Tree,
        GeoStage::Publish,
        GeoStage::All,
    ] {
        let s = build_script(st);
        assert!(
            s.ends_with(&format!("{IMAGE} --stage {}", st.as_str())),
            "{s}"
        );
    }
}

#[test]
fn the_script_has_no_docker_command() {
    assert!(!build_script(GeoStage::All).contains("docker"));
}

#[test]
fn the_build_name_is_the_one_of_the_bash_command() {
    assert_eq!(GEO_BUILD_NAME, "acteble-geo-build");
}

#[test]
fn following_streams_the_logs_waits_and_removes_the_container() {
    assert_eq!(
        follow_command(),
        "podman logs -f acteble-geo-build; rc=$(podman wait acteble-geo-build); podman rm acteble-geo-build >/dev/null; exit $rc"
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

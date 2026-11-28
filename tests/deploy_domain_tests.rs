//! The names of the API deploy and the image tags it accepts.
use actserve::domain::deploy::{API_ENV_PATH, API_SECRET, GHCR_SECRET, valid_image_tag};
use actserve::domain::secret::{valid_secret_name, valid_tmpfs_path};

#[test]
fn the_names_are_valid_secret_and_tmpfs_names() {
    assert!(valid_secret_name(API_SECRET) && valid_secret_name(GHCR_SECRET));
    assert!(valid_tmpfs_path(API_ENV_PATH));
    assert_eq!(API_SECRET, "acteble-api-prod");
    assert_eq!(GHCR_SECRET, "ghcr-read-packages");
}

#[test]
fn an_image_tag_that_could_break_the_quoting_is_refused() {
    for bad in ["", "a b", "a'b", "a;b", "$(x)", "-x", "a/b", ":latest"] {
        assert!(!valid_image_tag(bad), "{bad:?}");
    }
    for good in ["previous", "latest", "sha-0a1b2c", "v1.2.3", "A_b-c.d"] {
        assert!(valid_image_tag(good), "{good:?}");
    }
}

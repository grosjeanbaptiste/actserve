//! Base64 and hex of random bytes, pinned on the RFC 4648 vectors.
use actserve::domain::encoding::{base64, hex};

#[test]
fn base64_matches_the_rfc_4648_vectors() {
    for (raw, want) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64(raw.as_bytes()), want, "{raw}");
    }
}

#[test]
fn base64_uses_plus_and_slash_for_the_last_two_sextets() {
    assert_eq!(base64(&[0xfb, 0xff, 0xbf]), "+/+/");
}

#[test]
fn base64_of_64_bytes_is_88_characters_ending_in_two_pads() {
    let text = base64(&[0u8; 64]);
    assert_eq!(text.len(), 88);
    assert!(text.ends_with("=="));
}

#[test]
fn hex_is_lowercase_and_pads_each_byte_to_two_digits() {
    assert_eq!(hex(&[]), "");
    assert_eq!(hex(&[0x00, 0x0a, 0xff, 0x10]), "000aff10");
}

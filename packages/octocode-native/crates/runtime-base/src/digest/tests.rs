use super::json_sha256;
use octocode_test_support::json_fixture;

#[test]
fn digest_matches_the_display_serialization() {
    let value = json_fixture(r#"{"b": [1, "é"], "a": null}"#);
    assert_eq!(
        json_sha256(&value),
        hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
            value.to_string().as_bytes()
        ))
    );
}

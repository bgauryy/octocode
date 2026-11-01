//! Content digests for snapshot and evidence identities.

/// Hex [`octocode_engine::digest::json_sha256`] of `value` (the digest of
/// empty input when it cannot serialize).
pub(crate) fn json_sha256(value: &impl serde::Serialize) -> String {
    octocode_engine::digest::json_sha256(value)
        .map_or_else(|| octocode_engine::digest::sha256(&[]), hex::encode)
}

#[cfg(test)]
mod tests {
    use super::json_sha256;
    use serde_json::json;

    #[test]
    fn digest_matches_the_display_serialization() {
        let value = json!({"b":[1,"é"],"a":null});
        assert_eq!(
            json_sha256(&value),
            hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
                value.to_string().as_bytes()
            ))
        );
    }
}

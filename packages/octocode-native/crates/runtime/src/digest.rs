//! Content digests for snapshot and evidence identities.

/// Lowercase-hex SHA-256: the engine's content digest, so runtime and engine
/// graph identities hash alike.
pub(crate) use octocode_engine::digest::sha256;

/// Hex SHA-256 of `value`'s compact JSON serialization (empty input when it
/// cannot serialize).
pub(crate) fn json_sha256(value: &impl serde::Serialize) -> String {
    sha256(&serde_json::to_vec(value).unwrap_or_default())
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

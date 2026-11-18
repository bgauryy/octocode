//! Content digests for snapshot and evidence identities.

/// Hex [`octocode_engine::digest::json_sha256`] of `value` (the digest of
/// empty input when it cannot serialize).
pub fn json_sha256(value: &impl serde::Serialize) -> String {
    octocode_engine::digest::json_sha256(value)
        .map_or_else(|| octocode_engine::digest::sha256(&[]), hex::encode)
}

#[cfg(test)]
mod tests;

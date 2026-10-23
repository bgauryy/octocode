//! The one owner of a cached GitHub read's key: `<namespace>:<sha256 hex>`.
//! The digest input is part of the persisted cache contract, so both shapes
//! below keep their exact bytes.

use sha2::{Digest, Sha256};

use super::transport::RequestSpec;

fn finish(namespace: &str, digest: Sha256) -> String {
    format!("{namespace}:{}", hex::encode(digest.finalize()))
}

/// A key over named fields, each followed by a NUL so adjacent fields cannot
/// run together.
pub fn fields_key(namespace: &str, fields: &[&str]) -> String {
    let mut digest = Sha256::new();
    for field in fields {
        digest.update(field.as_bytes());
        digest.update([0]);
    }
    finish(namespace, digest)
}

/// A key over one request: its URL, a NUL, then its `Accept` media type when
/// it has one.
pub(crate) fn request_key(namespace: &str, spec: &RequestSpec) -> String {
    let mut digest = Sha256::new();
    digest.update(spec.url.as_str().as_bytes());
    digest.update([0]);
    if let Some(accept) = spec.headers.get(reqwest::header::ACCEPT) {
        digest.update(accept.as_bytes());
    }
    finish(namespace, digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha256_hex(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    #[test]
    fn fields_key_terminates_every_field_with_nul() {
        assert_eq!(
            fields_key("ns", &["a", "bc", ""]),
            format!("ns:{}", sha256_hex(b"a\0bc\0\0"))
        );
        assert_ne!(
            fields_key("ns", &["ab", "c"]),
            fields_key("ns", &["a", "bc"])
        );
    }

    #[test]
    fn request_key_hashes_url_nul_then_accept_without_a_trailing_nul() {
        let url = reqwest::Url::parse("https://api.github.com/repos/o/r/commits/abc").unwrap();
        let mut spec = RequestSpec::get(url);
        assert_eq!(
            request_key("ns", &spec),
            format!(
                "ns:{}",
                sha256_hex(b"https://api.github.com/repos/o/r/commits/abc\0")
            )
        );
        spec.headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github.diff"),
        );
        assert_eq!(
            request_key("ns", &spec),
            format!(
                "ns:{}",
                sha256_hex(
                    b"https://api.github.com/repos/o/r/commits/abc\0application/vnd.github.diff"
                )
            )
        );
    }
}

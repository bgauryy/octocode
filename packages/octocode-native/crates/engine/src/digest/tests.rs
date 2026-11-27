use super::{Nulls, canonical_json, fingerprint_json, json_sha256, sha256};
use serde_json::json;

/// The streamed JSON digest is the [`sha256`] of the bytes `to_vec`
/// builds, and the fingerprint's digest.
#[test]
fn the_json_digest_is_that_of_the_serialized_bytes() {
    for value in [
        json!(null),
        json!({"b": [1, "é"], "a": null, "t": "quote \" 中文 😀"}),
    ] {
        let digest = hex::encode(json_sha256(&value).expect("serializes"));
        assert_eq!(digest, sha256(&serde_json::to_vec(&value).unwrap()));
        assert_eq!(digest, fingerprint_json(&value).expect("serializes").digest);
    }
}

/// The streamed fingerprint is the digest and UTF-16 length of the
/// rendering `to_string` builds, whatever the value holds.
#[test]
fn a_json_fingerprint_is_that_of_the_rendered_string() {
    for value in [
        serde_json::json!(null),
        serde_json::json!({"a": [1, 2.5, true, null], "b": "quote \" tab \t é 中文 😀"}),
        serde_json::json!({"rows": (0..2000).map(|i| serde_json::json!({"i": i, "t": "😀é".repeat(i % 7)})).collect::<Vec<_>>()}),
    ] {
        let rendered = serde_json::to_string(&value).unwrap();
        let fingerprint = fingerprint_json(&value).unwrap();
        assert_eq!(fingerprint.digest, sha256(rendered.as_bytes()));
        assert_eq!(fingerprint.utf16_units, rendered.encode_utf16().count());
    }
    let invalid = std::collections::BTreeMap::from([((1, 2), "value")]);
    assert_eq!(fingerprint_json(&invalid), None);
}

#[test]
fn copy_hashed_reports_what_it_wrote() {
    let mut out = Vec::new();
    let (len, digest) = super::copy_hashed(&b"abc"[..], &mut out).expect("copy");
    assert_eq!((len, out.as_slice()), (3, &b"abc"[..]));
    assert_eq!(digest, sha256(b"abc"));
    let (len, digest) = super::copy_hashed(&b""[..], std::io::sink()).expect("empty");
    assert_eq!((len, digest), (0, sha256(b"")));
}

#[test]
fn keep_nulls_sorts_keys_and_keeps_explicit_nulls() {
    let value = canonical_json(json!({"b": null, "a": {"y": 1, "x": [null]}}), Nulls::Keep);
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        r#"{"a":{"x":[null],"y":1},"b":null}"#
    );
}

#[test]
fn dropping_nulls_makes_equal_identities_serialize_equally() {
    let a = canonical_json(
        json!({"b": 1, "a": {"y": [{"q": 1, "p": null}], "x": 2}}),
        Nulls::Drop,
    );
    let b = canonical_json(
        json!({"a": {"x": 2, "y": [{"q": 1}]}, "b": 1, "c": null}),
        Nulls::Drop,
    );
    assert_eq!(
        serde_json::to_vec(&a).unwrap(),
        serde_json::to_vec(&b).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        r#"{"a":{"x":2,"y":[{"q":1}]},"b":1}"#
    );
}

#[test]
fn sha256_matches_known_vectors() {
    // NIST/standard SHA-256 known-answer vectors — proves byte-for-byte
    // parity with the previous hand-rolled implementation and any external
    // consumer of these digests.
    assert_eq!(
        sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256(b"The quick brown fox jumps over the lazy dog"),
        "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
    );
}

#[test]
fn sha256_handles_multi_block_input() {
    // > 64 bytes forces multiple compression blocks; digest stays 64 hex chars.
    let input = "x".repeat(200);
    let out = sha256(input.as_bytes());
    assert_eq!(out.len(), 64);
    assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
}

//! The text armors the detector knows beyond PEM's five dashes, in every form
//! of each pattern: the ASCII form for all-ASCII text, the full form for the
//! rest, both through the prescan gate.

use super::*;

const BODY: &str = "UvImZaYMEtKJGF2VDuiBNgkWb2sRPReNbA/TkB/yOaGglfIPk5VlDPk4C47bIkpr";

fn hides_body(text: &str) {
    for haystack in [text.to_owned(), format!("caf\u{e9} {text} \u{e9}")] {
        let result = detect(&haystack, None);
        assert!(
            !result.sanitized.contains(BODY),
            "{haystack:?}: {}",
            result.sanitized
        );
        assert_no_pattern_matches(&result);
    }
}

#[test]
fn rfc4716_private_keys_are_redacted_with_or_without_encrypted() {
    for label in ["SSH2 ENCRYPTED PRIVATE KEY", "SSH2 PRIVATE KEY"] {
        for (open, close) in [
            (
                format!("---- BEGIN {label} ----"),
                format!("---- END {label} ----"),
            ),
            (
                format!("-----BEGIN {label}-----"),
                format!("-----END {label}-----"),
            ),
            (
                format!("----BEGIN {label}----"),
                format!("----END {label}----"),
            ),
            (
                format!("---- BEGIN\t{}\t----", label.replace(' ', "\t")),
                format!("---- END\t{}\t----", label.replace(' ', "\t")),
            ),
        ] {
            hides_body(&format!("{open}\nComment: \"fake\"\n{BODY}\n{close}\n"));
            hides_body(&format!("{open}\\nComment: x\\n{BODY}\\n{close}\\n"));
            hides_body(&format!(
                "{open}\r\nComment: \"fake\"\r\n{BODY}\r\n{close}\r\n"
            ));
        }
    }
}

#[test]
fn putty_keys_are_redacted_through_their_mac_line() {
    for version in [2, 3] {
        let key = format!(
            "PuTTY-User-Key-File-{version}: ssh-rsa\nEncryption: none\nComment: fake\nPublic-Lines: 1\nAAAA\nPrivate-Lines: 1\n{BODY}\nPrivate-MAC: 0123456789abcdef\nafter\n"
        );
        hides_body(&key);
        let result = detect(&key, None);
        assert!(
            !result.sanitized.contains("0123456789abcdef"),
            "{}",
            result.sanitized
        );
        assert!(
            result.sanitized.ends_with("after\n"),
            "{}",
            result.sanitized
        );
    }
}

/// Base64 of an RFC 4716 header and a fake body, and of its public twin.
#[test]
fn base64_of_an_rfc4716_header_is_a_private_key() {
    for encoded in [
        "LS0tLSBCRUdJTiBTU0gyIEVOQ1JZUFRFRCBQUklWQVRFIEtFWSAtLS0tCkNvbW1lbnQ6IHgKVXZJbVphWU1FdEtKR0YyVkR1aUJOZ2tXYjJzUlBSZU5iQS9Ua0IveU9hR2dsZklQazVWbERQazRDNDdiSWtwcgo=",
        "LS0tLSBCRUdJTiBTU0gyIFBSSVZBVEUgS0VZIC0tLS0KQ29tbWVudDogeApVdkltWmFZTUV0S0pHRjJWRHVpQk5na1diMnNSUFJlTmJBL1RrQi95T2FHZ2xmSVBrNVZsRFBrNEM0N2JJa3ByCg==",
        "LS0tLS1CRUdJTiBTU0gyIFBSSVZBVEUgS0VZLS0tLS0KVXZJbVphWU1FdEtKR0YyVkR1aUJOZ2tXYjJzUlBSZU5iQS9Ua0IveU9hR2dsZklQazVWbERQazRDNDdiSWtwcgo=",
    ] {
        let result = detect(&format!("data:\n  key: {encoded}\n"), None);
        assert!(
            !result.sanitized.contains(&encoded[..30]),
            "{}",
            result.sanitized
        );
    }
    let public = "LS0tLSBCRUdJTiBTU0gyIFBVQkxJQyBLRVkgLS0tLQpVdkltWmFZTUV0S0pHRjJWRHVpQk5na1diMnNSUFJlTmJBL1RrQi95T2FHZ2xmSVBrNVZsRFBrNEM0N2JJa3ByCg==";
    // Under a name that says nothing: a credential's name hides any opaque
    // value, whatever it encodes.
    let result = detect(&format!("data:\n  blob: {public}\n"), None);
    assert!(result.sanitized.contains(public), "{}", result.sanitized);
}

#[test]
fn rules_headings_and_public_armor_are_not_private_keys() {
    for text in [
        format!("----\n{BODY}\n----\n"),
        format!("---- text ----\n{BODY}\n---- more ----\n"),
        format!("| ---- | ---- |\n| {BODY} | a |\n"),
        format!("---- BEGIN SSH2 PUBLIC KEY ----\n{BODY}\n---- END SSH2 PUBLIC KEY ----\n"),
        format!("PuTTY-User-Key-File-2: ssh-rsa\n{BODY}\n"),
    ] {
        let result = detect(&text, None);
        assert!(result.sanitized.contains(BODY), "{}", result.sanitized);
    }
}

/// Retest R4: a key's header lines name it and hold none of it. A PuTTY file
/// shows its header lines and public lines, hiding its private lines and its
/// MAC's value; an armor shows its headers (PGP's `Version:`, PEM's
/// `Proc-Type:`, RFC 4716's continued `Comment:`) and hides its body.
#[test]
fn a_keys_header_lines_and_public_half_read_as_written() {
    const PUBLIC: &str = "AAAAB3NzaC1yc2EAAAADAQABAAABAQCfakepublicfakepublicfakepublic0";
    for version in [2, 3] {
        let derivation = if version == 3 {
            "Key-Derivation: Argon2id\nArgon2-Memory: 8192\nArgon2-Salt: 0123456789abcdef\n"
        } else {
            ""
        };
        let header = format!(
            "PuTTY-User-Key-File-{version}: ssh-rsa\nEncryption: none\nComment: rsa-key-20261112\nPublic-Lines: 2\n{PUBLIC}\nAAAA\n{derivation}Private-Lines: 2\n"
        );
        for eol in ["\n", "\r\n"] {
            let key = format!("{header}{BODY}\nUvIm\nPrivate-MAC: 9f8e7d6c5b4a39281706\nafter\n")
                .replace('\n', eol);
            let shown = detect(&key, None).sanitized;
            assert!(shown.starts_with(&header.replace('\n', eol)), "{shown}");
            assert!(!shown.contains(BODY) && !shown.contains("UvIm"), "{shown}");
            assert!(shown.contains("Private-MAC: [REDACTED-"), "{shown}");
            assert!(!shown.contains("9f8e7d6c5b4a39281706"), "{shown}");
            assert_eq!(shown.lines().count(), key.lines().count(), "{shown}");
        }
    }
    for (open, headers, close) in [
        (
            "-----BEGIN PGP PRIVATE KEY BLOCK-----",
            "Version: GnuPG v2\nComment: fabricated\n\n",
            "-----END PGP PRIVATE KEY BLOCK-----",
        ),
        (
            "-----BEGIN RSA PRIVATE KEY-----",
            "Proc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,0123ABCD\n\n",
            "-----END RSA PRIVATE KEY-----",
        ),
        (
            "---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----",
            "Comment: a long comment \\\nthat continues here\n",
            "---- END SSH2 ENCRYPTED PRIVATE KEY ----",
        ),
        (
            "-----BEGIN PGP SECRET KEY BLOCK-----",
            "Version: GnuPG v1\n\n",
            "-----END PGP SECRET KEY BLOCK-----",
        ),
    ] {
        let key = format!("{open}\n{headers}{BODY}\n{BODY}\n{close}\n");
        let shown = detect(&key, None).sanitized;
        assert!(
            shown.starts_with(&format!("{open}\n{headers}[REDACTED-")),
            "{shown}"
        );
        assert!(!shown.contains(BODY), "{shown}");
        assert!(shown.ends_with(&format!("{close}\n")), "{shown}");
    }
}

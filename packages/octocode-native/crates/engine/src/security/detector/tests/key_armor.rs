//! A private-key armor hides only a body shaped as key lines: code between
//! two armor literals is no body.

use super::*;

fn read(text: &str) -> String {
    replaced(text, &secret_spans_in(text, ValueForm::Config))
}

const BODY: &str = "MIIEpQIBAAKCAQEAunterminatedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[test]
fn armor_literals_in_two_statements_enclose_no_key() {
    let text = "const A = \"-----BEGIN RSA PRIVATE KEY-----\";\nconst FOOTER = \"-----END RSA PRIVATE KEY-----\";\n";
    assert_eq!(read(text), text);
    let wide = "a = \"-----BEGIN RSA PRIVATE KEY-----\"; b = c(\"x\"); d = \"-----END RSA PRIVATE KEY-----\";\n";
    assert_eq!(read(wide), wide);
    let open = "const A = \"-----BEGIN OPENSSH PRIVATE KEY-----\";\nconst x = 1;\nconst B = \"-----END OPENSSH PRIVATE KEY-----\";\n";
    assert_eq!(read(open), open);
}

#[test]
fn every_key_form_keeps_its_body_hidden() {
    let forms = [
        format!("-----BEGIN RSA PRIVATE KEY-----\n{BODY}\n{BODY}\n-----END RSA PRIVATE KEY-----\n"),
        format!(
            "k = \"-----BEGIN RSA PRIVATE KEY-----\\n{BODY}\\n-----END RSA PRIVATE KEY-----\\n\"\n"
        ),
        format!(
            "k = \"-----BEGIN RSA PRIVATE KEY-----\\n\" +\n  \"{BODY}\\n\" +\n  \"-----END RSA PRIVATE KEY-----\";\n"
        ),
        format!("# -----BEGIN RSA PRIVATE KEY-----\n# {BODY}\n# -----END RSA PRIVATE KEY-----\n"),
        format!("-----BEGIN RSA PRIVATE KEY----- {BODY} {BODY} -----END RSA PRIVATE KEY-----\n"),
        format!(
            "-----BEGIN RSA PRIVATE KEY-----\r\nProc-Type: 4,ENCRYPTED\r\n\r\n{BODY}\r\n-----END RSA PRIVATE KEY-----\r\n"
        ),
        format!("-----BEGIN OPENSSH PRIVATE KEY-----\n{BODY}\n-----END OPENSSH PRIVATE KEY-----\n"),
        format!(
            "-----BEGIN PGP SECRET KEY BLOCK-----\nVersion: x\n\n{BODY}\n=abcd\n-----END PGP SECRET KEY BLOCK-----\n"
        ),
        format!("---- BEGIN SSH2 PRIVATE KEY ----\n{BODY}\n---- END SSH2 PRIVATE KEY ----\n"),
        format!("-----BEGIN PRIVATE KEY-----\\n{BODY}\\n-----END PRIVATE KEY-----\\n"),
    ];
    for form in forms {
        let out = read(&form);
        assert!(!out.contains(&BODY[8..40]), "{form} -> {out}");
        assert!(out.contains("[REDACTED-"), "{form} -> {out}");
        assert!(out.contains("BEGIN"), "{out}");
    }
}

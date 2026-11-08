//! Credential assignments in dotenv files: any literal under a credential
//! name is a secret there, however short.

use super::*;

/// Paths of the dotenv class: `.env`, `.env.*` and `*.env`, in any case.
const DOTENV_PATHS: &[&str] = &[
    ".env",
    "services/chatbot/.env",
    ".env.local",
    "deploy/prod.env",
    "app\\.Env.Production",
];

/// Short values and `pass`/`pwd` names, quoted or not, each with the name and
/// operator it must keep, including values that read as a code identifier, a
/// plain word or a phrase of words, which a dotenv file holds as passwords
/// too. Every value is fake.
fn short_credentials() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("DB_PASS=Fake0Moo\n", "DB_PASS=", "Fake0Moo"),
        ("DB_PASS=CowSaysMoo\n", "DB_PASS=", "CowSaysMoo"),
        (
            "TLS_KEYSTORE_PASS=\"CowSaysMoo\"\n",
            "TLS_KEYSTORE_PASS=\"",
            "CowSaysMoo",
        ),
        (
            "PRIMARY_KEY=ResourceClaimName\n",
            "PRIMARY_KEY=",
            "ResourceClaimName",
        ),
        ("ADMIN_PWD='Moo' # dev\n", "ADMIN_PWD='", "Moo"),
        (
            "SESSION_TOKEN=\"cow says moo\"\n",
            "SESSION_TOKEN=\"",
            "cow says moo",
        ),
        (
            "TLS_KEYSTORE_PASS=fakepass1\n",
            "TLS_KEYSTORE_PASS=",
            "fakepass1",
        ),
        (
            "TEST_MYSQL_PASSWORD=password\n",
            "TEST_MYSQL_PASSWORD=",
            "password",
        ),
        ("API_PASSWORD=Fake!123\n", "API_PASSWORD=", "Fake!123"),
        ("DB_PWD=x7 # local\n", "DB_PWD=", "x7"),
        ("PASSWD=\"pw 1\"\n", "PASSWD=\"", "pw 1"),
        (
            "export SMTP_PASS='$fake9'\n",
            "export SMTP_PASS='",
            "$fake9",
        ),
        ("API_KEY=abc123\n", "API_KEY=", "abc123"),
        ("GITHUB_TOKEN = tok\n", "GITHUB_TOKEN = ", "tok"),
        ("MAIL_PASS=\"f\\\"q1\"\n", "MAIL_PASS=\"", "f\\\"q1"),
    ]
}

/// In a dotenv file a credential name says the value is one, whatever its
/// length: the value is redacted, its name, quotes and lines stay.
#[test]
fn dotenv_files_redact_short_values_under_credential_names() {
    for path in DOTENV_PATHS {
        for (content, kept, secret) in short_credentials() {
            let result = detect(content, Some(path));
            let shown = result.sanitized.replacen(kept, "", 1);
            assert!(
                result.sanitized.starts_with(kept) && !shown.contains(secret),
                "{path}: {content:?} -> {:?}",
                result.sanitized
            );
            assert_eq!(result.sanitized.lines().count(), content.lines().count());
            assert_no_pattern_matches(&result);
            let again = detect(&result.sanitized, Some(path));
            assert_eq!(again.sanitized, result.sanitized, "redaction is stable");
        }
    }
    let file = "DB_HOST=db\nDB_PASS=Fake0Moo\nAPI_PASSWORD=Fake!123\n";
    let result = detect(file, Some(".env"));
    assert!(
        result
            .sanitized
            .starts_with("DB_HOST=db\nDB_PASS=[REDACTED-")
    );
    assert!(!result.sanitized.contains("Fake0Moo") && !result.sanitized.contains("Fake!123"));
}

/// A dotenv file still keeps what is no credential: empty values, references
/// to other variables, placeholders, names whose credential word is another
/// word's part or not the name's last word, locators and comments.
#[test]
fn dotenv_files_keep_references_empty_values_and_other_names() {
    for content in [
        "DB_PASS=\n",
        "DB_PASS=\"\"\n",
        "DB_PASS=${DB_PASS_FROM_VAULT}\n",
        "DB_PASS=\"${DB_PASS_FROM_VAULT}\"\n",
        "DB_PASS=$OTHER\n",
        "API_TOKEN=\"[REDACTED-DOTENVFILECREDENTIAL]\"\n",
        "DB_PASSWORD_FILE=/run/secrets/db\n",
        "CLIENT_TOKEN=--token-file=/var/run/token\n",
    ] {
        let result = detect(content, Some(".env"));
        assert_eq!(result.sanitized, content);
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
}

/// Outside the dotenv class a short value is too like any setting to call a
/// credential: the length floor stays, though a `pass` name with a long value
/// is redacted there too.
#[test]
fn outside_dotenv_files_short_values_keep_the_length_floor() {
    for path in [
        Some("deploy.sh"),
        Some("conf/app.conf"),
        Some(".envrc.example"),
        None,
    ] {
        for (content, _, _) in short_credentials() {
            let result = detect(content, path);
            assert_eq!(result.sanitized, content, "{path:?}");
        }
    }
    for path in [
        Some("src/config.ts"),
        Some("src/settings.py"),
        Some("deploy.sh"),
        None,
    ] {
        for content in [
            "PRIMARY_KEY=ResourceClaimName\n",
            "const sortKey = \"createdAtTimestamp\";\n",
            "  apiKey: resourceClaimName\n",
            "api_key = default_api_key;\n",
        ] {
            let result = detect(content, path);
            assert_eq!(result.sanitized, content, "{path:?}");
            assert!(result.secrets_detected.is_empty(), "{path:?}: {content:?}");
        }
    }
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    for content in [
        format!("export DB_PASS={value}\n"),
        format!("TLS_KEYSTORE_PWD={value}\n"),
    ] {
        let result = detect(&content, Some("deploy.sh"));
        assert!(!result.sanitized.contains(&value[..16]), "{content:?}");
    }
}

/// Text that names no file is judged as any file: kept text, and a leaf
/// about a dotenv file, hide what a read of that file would.
#[test]
fn stored_text_and_dotenv_leaves_hide_short_dotenv_values() {
    for scope in [
        Scope::Stored(None),
        Scope::Leaf(".env"),
        Scope::Stored(Some("a.txt")),
    ] {
        for secret in ["Fake0Moo", "CowSaysMoo"] {
            let result = detect_in(&format!("DB_PASS={secret}\n"), scope);
            assert!(!result.sanitized.contains(secret), "{scope:?}");
        }
    }
    for secret in ["Fake0Moo", "CowSaysMoo"] {
        let content = format!("DB_PASS={secret}\n");
        assert_eq!(
            detect_in(&content, Scope::Leaf("src/a.rs")).sanitized,
            content
        );
    }
}

/// A dotenv text past what is held at once is judged in one pass over its
/// spans, with the same rule: its identifier-shaped passwords go too.
#[test]
fn long_dotenv_texts_redact_identifier_shaped_values() {
    let content =
        "DB_HOST=db\nDB_PASS=CowSaysMoo\nTLS_KEYSTORE_PASS=\"CowSaysMoo\"\nDB_USER=${USER}\n";
    let result = redact_within(content, Scope::File(".env"), 1).expect("redacted");
    assert!(
        !result.sanitized.contains("CowSaysMoo"),
        "{:?}",
        result.sanitized
    );
    assert!(
        result
            .sanitized
            .starts_with("DB_HOST=db\nDB_PASS=[REDACTED-")
    );
    assert!(result.sanitized.ends_with("DB_USER=${USER}\n"));
    assert_eq!(result.sanitized.lines().count(), content.lines().count());
}

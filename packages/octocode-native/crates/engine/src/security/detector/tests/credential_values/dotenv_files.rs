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
/// operator it must keep. Every value is fake.
fn short_credentials() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("DB_PASS=Fake0Moo\n", "DB_PASS=", "Fake0Moo"),
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
/// to other variables, names whose credential word is another word's part or
/// not the name's last word, identifier values, locators and comments.
#[test]
fn dotenv_files_keep_references_empty_values_and_other_names() {
    for content in [
        "DB_PASS=\n",
        "DB_PASS=\"\"\n",
        "DB_PASS=${DB_PASS_FROM_VAULT}\n",
        "DB_PASS=\"${DB_PASS_FROM_VAULT}\"\n",
        "DB_PASS=$OTHER\n",
        "PASSPORT_NUMBER=X1234\n",
        "BYPASS_CACHE=true\n",
        "PASSWORD_MIN_LENGTH=8\n",
        "TOKEN_TTL=3600\n",
        "PRIMARY_KEY=ResourceClaimName\n",
        "DB_PASSWORD_FILE=/run/secrets/db\n",
        "PWD=/Users/bgaryy/code\n",
        "# DB_PASS=commented\n",
        "parent.key = key;\n",
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
        Some(".envrc"),
        None,
    ] {
        for (content, _, _) in short_credentials() {
            let result = detect(content, path);
            assert_eq!(result.sanitized, content, "{path:?}");
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
        let result = detect_in("DB_PASS=Fake0Moo\n", scope);
        assert!(!result.sanitized.contains("Fake0Moo"), "{scope:?}");
    }
    assert_eq!(
        detect_in("DB_PASS=Fake0Moo\n", Scope::Leaf("src/a.rs")).sanitized,
        "DB_PASS=Fake0Moo\n"
    );
}

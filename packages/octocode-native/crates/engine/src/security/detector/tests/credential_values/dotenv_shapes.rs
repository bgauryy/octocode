//! Dotenv values as dotenv parsers read them (D6): any line end, any value
//! shape, any credential-named key, diff lines, and the whole dotenv name
//! class. Every value is fake.

use super::*;

/// Redacts each `secrets` of `content` read as `scope`, keeping its lines
/// and line ends, and redacts nothing more on a second pass.
fn hides_in(content: &str, scope: Scope<'_>, secrets: &[&str]) {
    let result = detect_in(content, scope);
    for secret in secrets {
        assert!(
            !result.sanitized.contains(secret),
            "{scope:?}: {secret:?} leaked: {content:?} -> {:?}",
            result.sanitized
        );
    }
    assert_eq!(
        result.sanitized.matches('\n').count(),
        content.matches('\n').count(),
        "{content:?} -> {:?}",
        result.sanitized
    );
    assert_eq!(
        result.sanitized.matches("\r\n").count(),
        content.matches("\r\n").count()
    );
    let again = detect_in(&result.sanitized, scope);
    assert_eq!(again.sanitized, result.sanitized, "redaction is stable");
}

fn hides(content: &str, path: &str, secrets: &[&str]) {
    hides_in(content, Scope::File(path), secrets);
}

fn keeps(content: &str, path: Option<&str>) {
    let result = detect(content, path);
    assert_eq!(result.sanitized, content, "{path:?}");
    assert!(result.secrets_detected.is_empty(), "{path:?}: {content:?}");
}

/// CRLF, and a last line without a line break, end a value as LF does.
#[test]
fn crlf_and_unterminated_lines_end_dotenv_values() {
    hides(
        "LONG_PASS=Abcdefghijkl1234567\r\nSHORT_PASS=Moo1\r\nQ_PASS=\"Moo2\"\r\nS_PASS='Moo3'\r\nC_PASS=Moo4 # dev\r\nlast_pass=Moo5",
        ".env",
        &[
            "Abcdefghijkl1234567",
            "Moo1",
            "Moo2",
            "Moo3",
            "Moo4",
            "Moo5",
        ],
    );
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    hides(
        &format!("export DB_PASSWORD={value}\r\nDB_HOST=db\r\n"),
        "deploy.sh",
        &[&value[..16]],
    );
    keeps(
        "DB_HOST=db\r\nDB_PASS=${VAULT_PASS}\r\nDB_USER=$USER\r\n",
        Some(".env"),
    );
}

/// A value is what a dotenv parser reads: unquoted up to a ` #` comment,
/// spaces and brackets included; quoted, across lines too; and any text
/// after a closing quote. A leading `$` reads another variable only in the
/// `$NAME` or `${NAME}` shape, and a bracketed value is no placeholder in a
/// dotenv file: only the tool's own markers stay.
#[test]
fn dotenv_values_are_read_whole_whatever_their_shape() {
    for (content, secrets) in [
        ("A9_PASS=correct horse\n", &["correct horse"][..]),
        ("D1_PASS=Cow Says Moo # x\n", &["Cow Says Moo"]),
        ("H_PASS=pass#word\n", &["pass#word"]),
        ("A1_PASS=Cow(Says)Moo\n", &["Cow(Says)Moo"]),
        ("A2_PASS={CowSaysMoo}\n", &["CowSaysMoo"]),
        ("A3_PASS=(CowSaysMoo)\n", &["CowSaysMoo"]),
        ("A4_PASS=[CowSaysMoo]x\n", &["CowSaysMoo"]),
        ("A7_PASS=$CowSaysMoo\n", &["CowSaysMoo"]),
        ("A8_PASS=\"$CowSaysMoo\"\n", &["CowSaysMoo"]),
        ("A6_PASS=${Cow}Says\n", &["Says"]),
        ("SMTP_PASS='$OTHER'\n", &["OTHER"]),
        (
            "O_PASS=\"line1Moo\nline2Moo\"\nDB_HOST=db\n",
            &["line1Moo", "line2Moo"],
        ),
        (
            "P_PASS='line1Moo\r\nline2Moo'\r\n",
            &["line1Moo", "line2Moo"],
        ),
        ("B6_PASS=\"CowSaysMoo\"x\n", &["CowSaysMoo"]),
        ("C9_PASS=\"\"CowSaysMoo\n", &["CowSaysMoo"]),
        ("B4_PASS=CowSaysMoo; rm\n", &["CowSaysMoo", "rm"]),
        ("B7_PASS='[CowSaysMoo]'\n", &["CowSaysMoo"]),
        ("B8_PASS=\"[CowSaysMoo]\"\n", &["CowSaysMoo"]),
        ("B9_PASS=[your-db-password]\n", &["your-db-password"]),
        (
            "U_PASS=\"unterminatedMoo\nDB_HOST=db\n",
            &["unterminatedMoo"],
        ),
    ] {
        hides(content, ".env", secrets);
    }
    let result = detect("D1_PASS=Cow Says Moo # x\n", Some(".env"));
    assert!(
        result.sanitized.ends_with(" # x\n"),
        "{:?}",
        result.sanitized
    );
    for content in [
        "DB_PASS=\n",
        "DB_PASS= # none\n",
        "DB_PASS=\"\"\n",
        "DB_PASS=''\n",
        "DB_PASS=${DB_PASS_FROM_VAULT}\n",
        "DB_PASS=$OTHER_PASS\n",
        "DB_PASS=\"$OTHER\" # vault\n",
        "API_TOKEN=[REDACTED-DOTENVFILECREDENTIAL]\n",
        "API_TOKEN=\"[REDACTED-DOTENVFILECREDENTIAL]\"\n",
    ] {
        keeps(content, Some(".env"));
    }
    for path in [Some("deploy.sh"), Some("src/config.ts"), None] {
        keeps("DB_PASS=[your-db-password]\n", path);
    }
}

/// In a dotenv file a key is credential-named when it contains a credential
/// word anywhere, in any case and with no separator (`APIKEY`, `DBPASS`):
/// no exclusion list, so `MONKEY` and `BYPASS` fail closed there. Outside
/// dotenv files the same names keep their values.
#[test]
fn dotenv_keys_name_credentials_by_any_word_they_contain() {
    for name in [
        "DB_PASS_PROD",
        "SECRET_KEY_BASE",
        "dbPassword",
        "db.password",
        "DB-PASS",
        "apiKey",
        "export MYSQLPASSWORD",
        "GITHUB_TOKENS",
        "APIKEY",
        "DBPASS",
        "KEYSTOREPASS",
        "ROOTPWD",
        "SECRETKEY",
        "ACCESSKEY",
        "APIKEY_LONG",
        "BYPASS_CACHE",
        "PASSPORT_NUMBER",
        "MONKEY_NAME",
        "KEYBOARD_LAYOUT",
    ] {
        for value in ["CowSaysMoo", "Abcdefghijkl1234567"] {
            hides(&format!("{name}={value}\n"), ".env", &[value]);
        }
    }
    hides(
        "PASSWORD_MIN_LENGTH=8\nTOKEN_TTL=3600\n",
        ".env",
        &["=8", "3600"],
    );
    keeps("DB_HOST=db\n", Some(".env"));
    for path in [Some("src/config.ts"), Some("settings.txt"), None] {
        for content in ["APIKEY=Moo4\n", "MONKEY_NAME=George\n", "DBPASS=Moo2\n"] {
            keeps(content, path);
        }
    }
}

/// In a dotenv file a path is no locator by its shape: only a key, or an
/// option, whose last word names a location keeps its value.
#[test]
fn dotenv_locators_are_named_by_their_key_or_option() {
    for (content, secret) in [
        ("DB_PASS=/hunter2\n", "hunter2"),
        ("X_TOKEN=~/Cow/Says\n", "Cow/Says"),
        ("EE_PASS=/correct/horse/battery\n", "correct/horse"),
        ("D3_TOKEN=--token=/Hunter\n", "Hunter"),
        ("PWD=/Users/bgaryy/code\n", "bgaryy"),
        ("SSH_KEY=~/.ssh/id_ed25519\n", "id_ed25519"),
    ] {
        hides(content, ".env", &[secret]);
    }
    for content in [
        "TOKEN_FILE=/run/token\n",
        "DB_PASSWORD_FILE=/run/secrets/db\n",
        "CLIENT_TOKEN=--token-file=/x\n",
        "KEY_PATH=./certs/k.pem\n",
        "SECRETS_DIR=/run/secrets\n",
        "GNUPG_KEY_HOME=~/.gnupg\n",
    ] {
        keeps(content, Some(".env"));
    }
}

/// A diff line of a dotenv file (`+`, `-` or space prefix) inside a hunk is
/// scanned like the line itself, in a read, a result leaf and stored text; a
/// long value is hidden in any patch. Outside a hunk a leading `-` is part
/// of the text (`-DMAX_TOKENS=4096`, a compiler flag), not a diff prefix.
#[test]
fn dotenv_diff_lines_are_scanned_like_the_line() {
    let patch = "@@ -1,3 +1,4 @@\n DB_HOST=db\n-DB_PASS=OldCowSaysMoo\n+DB_PASS=CowSaysMoo\n+export B_TOKEN=Short1\n DB_USER=root\n";
    for scope in [
        Scope::File(".env"),
        Scope::Leaf(".env"),
        Scope::Stored(None),
    ] {
        hides_in(patch, scope, &["CowSaysMoo", "Short1"]);
    }
    hides(
        "diff --git a/.env b/.env\n@@ -0,0 +1 @@\n+API_KEY=LongerSecretValue12345xyz\n",
        "change.diff",
        &["LongerSecretValue12345xyz"],
    );
    for scope in [Scope::Stored(None), Scope::Leaf(".env")] {
        let result = detect_in("-DB_PASS=Moo\n", scope);
        assert_eq!(result.sanitized, "-DB_PASS=Moo\n", "{scope:?}");
    }
    let flag = "-DMAX_TOKENS=4096 @@ -1 +1 @@\n";
    assert_eq!(detect_in(flag, Scope::Stored(None)).sanitized, flag);
}

/// The dotenv name class is one engine rule: runtime discovery's `.env`
/// family, backups included, and nothing else.
#[test]
fn dotenv_name_class_covers_the_env_family() {
    for path in [
        ".env",
        ".envrc",
        ".flaskenv",
        ".env~",
        ".env.bak",
        "prod.env.bak",
        "app/.ENV.Local",
        "deploy\\prod.env",
        ".envrc.orig",
    ] {
        assert!(crate::security::is_dotenv_name(path), "{path}");
        hides("DB_PASS=CowSaysMoo\n", path, &["CowSaysMoo"]);
    }
    for path in [
        "env.example",
        "src/env.rs",
        ".envoy",
        "deploy.sh",
        "a.environment",
    ] {
        assert!(!crate::security::is_dotenv_name(path), "{path}");
        keeps("DB_PASS=CowSaysMoo\n", Some(path));
    }
}

/// Which patterns treat every non-location value as a credential is
/// declared, not inferred: exactly the dotenv and properties file rules, each
/// gated by its own name class.
#[test]
fn file_class_value_test_is_declared_for_the_file_rules_only() {
    use crate::security::detector::credential_values::{VALUE_TESTS, ValueTest};
    use crate::security::patterns::{DOTENV_FILE_CONTEXT, PROPERTIES_FILE_CONTEXT};
    let file_class: Vec<(&str, Option<&str>)> = PATTERNS
        .iter()
        .zip(VALUE_TESTS.iter())
        .filter(|(_, test)| **test == ValueTest::FileClass)
        .map(|(pattern, _)| (pattern.name, pattern.file_context))
        .collect();
    assert_eq!(
        file_class,
        [
            ("propertiesFileCredential", Some(PROPERTIES_FILE_CONTEXT)),
            ("dotenvFileCredential", Some(DOTENV_FILE_CONTEXT)),
        ]
    );
}

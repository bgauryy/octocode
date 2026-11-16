//! A key that says it holds a credential, assigned a literal, has only its
//! value replaced; templates, placeholders and ordinary words stay.

use super::*;

fn read(text: &str) -> String {
    replaced(text, &secret_spans_in(text, ValueForm::Config))
}

fn read_as_code(text: &str) -> String {
    replaced(text, &secret_spans(text))
}

fn value_replaced(line: &str, secret: &str) {
    let out = read(line);
    assert!(!out.contains(secret), "{line} -> {out}");
    assert!(out.contains("[REDACTED-"), "{line} -> {out}");
    let label = &line[..line.find(secret).expect("secret in line")];
    assert!(out.starts_with(label), "{line} -> {out}");
}

/// `line` reads as written in code and in a config file.
fn untouched(line: &str) {
    assert_eq!(read(line), line, "{line}");
    assert_eq!(read_as_code(line), line, "{line}");
}

/// `line` is code, an expression after a label: it reads as written as code.
fn untouched_in_code(line: &str) {
    assert_eq!(read_as_code(line), line, "{line}");
}

#[test]
fn a_secret_named_key_has_only_its_value_replaced() {
    value_replaced("password = \"SuperSecretPw123\"", "SuperSecretPw123");
    value_replaced("db_password = 'hunter2x!'", "hunter2x!");
    value_replaced("password: s3cretYamlPw", "s3cretYamlPw");
    value_replaced("db.password=prop-secret-123", "prop-secret-123");
    value_replaced("password = ini-secret-456", "ini-secret-456");
    value_replaced("\"password\": \"Tr0ub4dor&3xK9\",", "Tr0ub4dor&3xK9");
    value_replaced("\"client_secret\": \"abc123def456\"", "abc123def456");
    value_replaced(
        "api_key: \"0123456789abcdef0123456789abcdef\"",
        "0123456789abcdef",
    );
    value_replaced(
        "SECRET_KEY=\"django-insecure-k3y9\"",
        "django-insecure-k3y9",
    );
    value_replaced("clientSecret: 'a1b2c3d4'", "a1b2c3d4");
    value_replaced("'pwd' => 'p4ssw0rd'", "p4ssw0rd");
    value_replaced("auth_token := abc123xyz9", "abc123xyz9");
    value_replaced("private_key = \"zz9-top-secret\"", "zz9-top-secret");
    value_replaced("password = hunter2hunter2hunter2", "hunter2hunter2hunter2");
    value_replaced(
        "secretKey: 'my-very-long-development-secret-value'",
        "my-very-long-development-secret-value",
    );
    value_replaced(
        "data:\n  token: c2VjcmV0LXZhbHVlLWluLWJhc2U2NA==",
        "c2VjcmV0LXZhbHVlLWluLWJhc2U2NA==",
    );
}

#[test]
fn the_text_around_the_value_reads_as_written() {
    let text = "const a = 1;\nconfig.password = \"hunter2x!\"; // note\nconst b = 2;\n";
    let out = read(text);
    assert!(
        out.starts_with("const a = 1;\nconfig.password = \""),
        "{out}"
    );
    assert!(out.ends_with("\"; // note\nconst b = 2;\n"), "{out}");
    let url = read("fetch(\"/x?token=abc123xyz&page=2\")");
    assert!(url.contains("&page=2"), "{url}");
    assert!(!url.contains("abc123xyz"), "{url}");
}

#[test]
fn templates_placeholders_and_ordinary_words_stay() {
    for line in [
        "password = getPassword()",
        "if (password) {",
        "password: str",
        "password: \"Password\"",
        "token: Token",
        "max_tokens: 100",
        "password = \"${DB_PASSWORD}\"",
        "password: $DB_PASSWORD",
        "password: {{ vault.pw }}",
        "password = <your-password>",
        "password = %s",
        "password: changeme",
        "secret: true",
        "token: null",
        "pwd = 1234",
        "password = ./secrets/pw.txt",
        "password = os.environ[\"PW\"]",
        "password == \"abc123\"",
        "password: [REDACTED-GITHUBTOKENS]",
        "password=$(cat /run/secrets/pw)",
        "const pwd = 12345;",
        "passwordHash: x",
    ] {
        untouched(line);
    }
    for line in [
        "secret_key = settings.secret_key",
        "token = req.headers.authorization",
        "password = userPass1",
        "token: abcdefgh12",
    ] {
        untouched_in_code(line);
    }
}

#[test]
fn ordinary_hashes_uuids_and_base64_stay() {
    let text = "sha: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nid = 550e8400-e29b-41d4-a716-446655440000\nlogo: iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==\nintegrity: sha512-Z2VuZXJhdGVkRmFrZUludGVncml0eUhhc2g=\n";
    assert_eq!(read(text), text);
}

#[test]
fn a_value_that_starts_like_a_word_in_the_list_is_still_a_value() {
    value_replaced("password: nobody123", "nobody123");
    value_replaced("password: yes42pw", "yes42pw");
}

#[test]
fn a_quoted_value_of_letters_is_a_value_and_a_label_word_is_not() {
    value_replaced("password = \"letmein\"", "letmein");
    value_replaced(
        "password = 'correcthorsebatterystaple'",
        "correcthorsebatterystaple",
    );
    value_replaced("{\"password\": \"sunshine\"}", "sunshine");
    value_replaced("\"PASSWORD\": \"mypassword\"", "mypassword");
    value_replaced(
        "client_secret = \"abcdefghijklmnopqrstuvwxyzABCDEF\"",
        "abcdefghijklmnopqrstuvwxyzABCDEF",
    );
    untouched("password: \"Password\"");
    untouched("token: \"token\"");
    untouched("secret = \"secret\"");
}

#[test]
fn a_value_before_a_closing_quote_is_a_value() {
    value_replaced(
        "const u = \"https://api.example.com/v1?token=FAKEurlTok123456\";",
        "FAKEurlTok123456",
    );
    value_replaced(
        "const u = 'x?api_key=FAKEurlKey123456&x=1';",
        "FAKEurlKey123456",
    );
}

#[test]
fn operator_literals_and_short_values_are_no_value() {
    for line in [
        "token = \"->\"",
        "token = \"==\"",
        "password = \"===\"",
        "secret=s1",
        "token=abc",
        "password: \"pw\"",
    ] {
        untouched(line);
    }
}

/// A bare value after a label is a value only in a config file; in code it
/// is an identifier. A quoted value is a value in both.
#[test]
fn a_bare_value_is_a_value_only_in_a_config_file() {
    for line in [
        "password=letmein1",
        "password = userPass1",
        "token: abcdefgh123",
        "auth_token := abc123xyz9",
        "db.password=prop-secret-123",
    ] {
        assert_eq!(read_as_code(line), line, "code shows {line}");
        let config = read(line);
        assert!(
            config.contains("[REDACTED-"),
            "config hides {line}: {config}"
        );
    }
    for line in [
        "password = \"userPass1\"",
        "token: 'abcdefgh123'",
        "{\"password\": \"letmein1\"}",
    ] {
        for out in [read(line), read_as_code(line)] {
            assert!(out.contains("[REDACTED-"), "{line} -> {out}");
            assert!(
                !out.contains("userPass1")
                    && !out.contains("abcdefgh123")
                    && !out.contains("letmein1"),
                "{out}"
            );
        }
    }
}

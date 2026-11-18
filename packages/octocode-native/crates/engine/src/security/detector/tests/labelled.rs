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

/// A URL query value is a literal: it is hidden in code and in a config file,
/// and the rest of the URL reads as written.
#[test]
fn a_url_query_value_is_hidden_in_code_too() {
    for (line, secret) in [
        ("fetch(\"/x?token=abc123xyz&page=2\")", "abc123xyz"),
        (
            "GET /v1/items?page=2&api_key=FAKEurlKey123456 HTTP/1.1",
            "FAKEurlKey123456",
        ),
        (
            "const u = `${base}/cb?access_token=s3cretT0ken#top`;",
            "s3cretT0ken",
        ),
        ("curl 'https://h/x?password=hunter2x&u=1'", "hunter2x"),
        (
            "\"download_url\": \"https://raw.example/README.md?token=ABTH2BQWERTYUIOP\"",
            "ABTH2BQWERTYUIOP",
        ),
    ] {
        for out in [read(line), read_as_code(line)] {
            assert!(!out.contains(secret), "{line} -> {out}");
            assert!(out.contains("[REDACTED-"), "{line} -> {out}");
        }
    }
    let kept = read_as_code("fetch(\"/x?token=abc123xyz&page=2\")");
    assert!(
        kept.contains("?token=") && kept.contains("&page=2"),
        "{kept}"
    );
    assert!(holds_credential_as_request("/x?token=abc123xyz&page=2"));
    assert!(holds_credential_as_request("password: \"hunter2x!\""));
}

/// A quoted label word is a key only when an assignment holds it: a ternary
/// branch is a value, a key in a mapping or `key = value` is a label.
#[test]
fn a_quoted_label_in_a_ternary_branch_is_no_key_but_an_assignment_is() {
    for line in [
        "input.type = secret ? 'password' : 'text';",
        "x = ok?'token':'secret'",
    ] {
        untouched(line);
    }
    for (line, secret) in [
        ("const o = { 'password': 'hunter2x!' };", "hunter2x!"),
        ("const o = { password: 'hunter2x!' };", "hunter2x!"),
        (
            "const o = ok ? 1 : { 'password': 'hunter2x!' };",
            "hunter2x!",
        ),
        ("password = 'hunter2x!'", "hunter2x!"),
        ("Example: secret='LEAKED' in a config", "LEAKED"),
    ] {
        for out in [read(line), read_as_code(line)] {
            assert!(!out.contains(secret), "{line} -> {out}");
        }
    }
}

/// A query key is hidden only when it is a known secret key: the bare label
/// or a listed prefix of it (RFC 6750 `access_token`, AWS's
/// `X-Amz-Security-Token`, `x-api-key`). A paging, sync or CSRF token is no
/// credential and reads as written.
#[test]
fn only_known_secret_query_keys_are_hidden() {
    for (line, secret) in [
        ("GET /v1?access_token=Abc123Def456 HTTP/1.1", "Abc123Def456"),
        ("GET /v1?a=1&token=Abc123Def456 HTTP/1.1", "Abc123Def456"),
        ("GET /v1?api_key=Abc123Def456 HTTP/1.1", "Abc123Def456"),
        ("GET /v1?apiKey=Abc123Def456 HTTP/1.1", "Abc123Def456"),
        ("GET /v1?x-api-key=Abc123Def456 HTTP/1.1", "Abc123Def456"),
        (
            "GET /v1?client_secret=Abc123Def456 HTTP/1.1",
            "Abc123Def456",
        ),
        ("GET /v1?password=Abc123Def456 HTTP/1.1", "Abc123Def456"),
        (
            "GET /v1?refresh_token=Abc123Def456 HTTP/1.1",
            "Abc123Def456",
        ),
        ("GET /v1?id_token=Abc123Def456 HTTP/1.1", "Abc123Def456"),
        (
            "GET /v1?X-Amz-Security-Token=Abc123Def456 HTTP/1.1",
            "Abc123Def456",
        ),
    ] {
        for out in [read(line), read_as_code(line)] {
            assert!(!out.contains(secret), "{line} -> {out}");
        }
    }
    for line in [
        "GET /v1?page_token=Abc123Def456 HTTP/1.1",
        "GET /v1?a=1&next_token=Abc123Def456 HTTP/1.1",
        "GET /v1?nextPageToken=Abc123Def456 HTTP/1.1",
        "GET /v1?pageToken=Abc123Def456 HTTP/1.1",
        "GET /v1?pagetoken=Abc123Def456 HTTP/1.1",
        "GET /v1?continuation_token=Abc123Def456 HTTP/1.1",
        "GET /v1?sync_token=Abc123Def456 HTTP/1.1",
        "GET /v1?csrf_token=Abc123Def456 HTTP/1.1",
    ] {
        assert_eq!(read_as_code(line), line, "{line}");
        assert_eq!(read(line), line, "{line}");
    }
    // A listed key later in the same URL is still hidden.
    let both = read_as_code("/x?page_token=Abc123Def456&access_token=Zyx987Wvu654");
    assert!(both.contains("page_token=Abc123Def456"), "{both}");
    assert!(!both.contains("Zyx987Wvu654"), "{both}");
}

/// Letters-only and short bare values are values in a config file; in code
/// they stay as written.
#[test]
fn a_config_value_of_letters_or_a_short_tail_is_hidden() {
    for line in [
        "password: letmein",
        "password: hunter2",
        "password: userPass12",
        "password=letmein1",
        "db_password: Summer2024",
        "secret: sunshine",
    ] {
        let config = read(line);
        assert!(
            config.contains("[REDACTED-"),
            "config hides {line}: {config}"
        );
        assert!(config.starts_with(&line[..line.find(['=', ':']).expect("operator")]));
        assert_eq!(read_as_code(line), line, "code shows {line}");
        assert!(!holds_credential_as_request(line), "{line} refused");
    }
}

/// An interpolation's literal default is the value: the default alone is
/// hidden. A placeholder default, a short one, an empty one and a lookup that
/// names a source stay.
#[test]
fn an_interpolation_default_is_the_value() {
    let out = read("spring.datasource.password=${MYSQL_PASS:petclinic}");
    assert_eq!(
        out,
        "spring.datasource.password=${MYSQL_PASS:[REDACTED-PASSWORDASSIGNMENT]}"
    );
    let shell = read_as_code("PASSWORD=${DB_PASS:-hunter2x}");
    assert!(
        shell.starts_with("PASSWORD=${DB_PASS:-[REDACTED-"),
        "{shell}"
    );
    assert!(shell.ends_with("}"), "{shell}");
    for line in [
        "spring.datasource.password=${MYSQL_PASS:changeme}",
        "spring.datasource.password=${MYSQL_PASS:<your-password>}",
        "spring.datasource.password=${MYSQL_PASS:pw}",
        "spring.datasource.password=${MYSQL_PASS:}",
        "password: ${DB_PASS:-}",
        "password: ${DB_PASS:-$OTHER}",
        "password: ${DB_PASS:true}",
        "token: ${env:API_HOME}",
        "token: ${vault:secret/data/app}",
    ] {
        untouched(line);
        assert!(!holds_credential_as_request(line), "{line} refused");
    }
}

#[test]
fn a_one_word_sql_placeholder_is_no_password() {
    untouched("ALTER ROLE r PASSWORD 'x';");
    let out = read("ALTER USER u PASSWORD 'S3cretPw!';");
    assert!(
        !out.contains("S3cretPw!") && out.contains("[REDACTED-"),
        "{out}"
    );
}

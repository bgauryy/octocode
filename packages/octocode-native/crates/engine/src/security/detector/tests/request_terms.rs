//! Which request strings hold a credential ([`holds_credential_as_request`]):
//! each rule hides the secret and runs the ordinary look-alike.

use super::*;

const PASSWORD: &str = "Tr0ub4dor&3xK9!zQ";
const GITHUB: &str = "ghp_x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";
const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
const OPENSSH_BODY: &str = "b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAABlwAAAAdzc2gtcn";

fn refused(term: &str) {
    assert!(holds_credential_as_request(term), "ran {term:?}");
}

fn runs(term: &str) {
    assert!(!holds_credential_as_request(term), "refused {term:?}");
}

#[test]
fn a_named_secret_is_refused_wherever_it_sits_in_a_sentence() {
    for term in [
        format!("password: {PASSWORD}"),
        format!("locate password: {PASSWORD}"),
        format!("the password: {PASSWORD}"),
        format!("equals password: {PASSWORD}"),
        format!("find password={PASSWORD} in config"),
        format!("Does the text contain password: {PASSWORD}?"),
        format!("use password: {PASSWORD} to log in"),
        format!("first line\nthen db_password = \"{PASSWORD}\", and more"),
        format!("password:\\s*{PASSWORD}"),
    ] {
        refused(&term);
    }
}

#[test]
fn a_credential_name_in_a_sentence_with_no_secret_runs() {
    for term in [
        "password: str",
        "token: Token",
        "api_key: str = None",
        "dispatchToken: string",
        "Does the function take a password: str parameter?",
        "find where password: string is declared",
        "locate token: Token in the client",
        "where is apiKey = process.env.API_KEY set",
        "find password = getpass.getpass() calls",
        "NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}",
        "list every sessionToken: Option<SessionToken> field",
        "the cacheKey: computeCacheKeyForRequest(request) line",
        "which publicKey: ssh-ed25519 lines are trusted",
        "checksum: 10c0/8f2b4a",
        "password: \"password\"",
        "apiKey: \"YOUR_API_KEY\"",
        "API_KEY=xxxxxxxxxxxxxxxx",
    ] {
        runs(term);
    }
}

#[test]
fn a_secret_spelled_as_a_regex_is_refused() {
    let escaped_dots = JWT.replace('.', "\\.");
    let one_member_class = format!("{}[I]", GITHUB.trim_end_matches('I'));
    let split = format!("{}(?:){}", &GITHUB[..20], &GITHUB[20..]);
    for term in [
        escaped_dots.as_str(),
        one_member_class.as_str(),
        "xoxb[\\-]2048576310[-]4096152620\\-AbCdEfGhIjKlMnOpQrStUvWx",
        split.as_str(),
        "xoxb\\-2048576310\\-4096152620\\-AbCdEfGhIjKlMnOpQrStUvWx",
    ] {
        refused(term);
    }
}

#[test]
fn a_regex_that_spells_no_secret_runs() {
    for term in [
        "ghp_.{36}",
        "ghp_[A-Za-z0-9]{36}",
        "eyJ[A-Za-z0-9_-]+\\.eyJ[A-Za-z0-9_-]+\\.",
        "password:\\s*str",
        "foo\\.bar\\(token: string\\)",
        "token\\s*[:=]\\s*[\"']",
        "\\bapi[_-]?key\\b",
        "[a]{3}(?:)x",
    ] {
        runs(term);
    }
}

#[test]
fn the_literal_form_reads_only_elements_of_one_fixed_character() {
    assert_eq!(literal_form("a\\.b\\-c\\/d").as_deref(), Some("a.b-c/d"));
    assert_eq!(literal_form("ab[c]d[\\.]").as_deref(), Some("abcd."));
    assert_eq!(literal_form("ab(?:)cd").as_deref(), Some("abcd"));
    assert_eq!(literal_form("a\\nb\\tc").as_deref(), Some("a\nb\tc"));
    for own in ["plain text", "a\\wb\\d\\s", "[ab]c[^d]", "(?:x)[]", "x\\"] {
        assert_eq!(literal_form(own), None, "{own:?}");
    }
}

#[test]
fn a_url_password_is_refused_and_a_placeholder_url_runs() {
    for term in [
        "postgres://admin:Tr0ub4dor3xK9zQ@db.internal:5432/app",
        "mysql://root:hunter2@db/app",
        "https://deploy:s3cr3t!@example.com",
        "redis://:Zq7xLm9Kp2Wv8Rt1@cache:6379",
    ] {
        refused(term);
    }
    for term in [
        "postgres://user:password@localhost:5432/db",
        "postgres://user:pass@localhost",
        "mysql://username:password@host/db",
        "https://user:pass@example.com",
        "redis://localhost:6379",
        "postgres://user:${DB_PASSWORD}@localhost/db",
    ] {
        runs(term);
    }
}

#[test]
fn a_key_body_is_refused_and_its_armor_line_alone_runs() {
    refused(OPENSSH_BODY);
    refused(&format!("Does it contain {OPENSSH_BODY}?"));
    refused(&format!(
        "-----BEGIN OPENSSH PRIVATE KEY-----\n{OPENSSH_BODY}\n-----END OPENSSH PRIVATE KEY-----"
    ));
    for term in [
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----END OPENSSH PRIVATE KEY-----",
        "-----BEGIN CERTIFICATE-----",
        "Does it contain -----BEGIN RSA PRIVATE KEY-----?",
        // Base64 with no format's header is a digest or an image.
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
        // An OpenSSH public key spells its own header.
        "AAAAC3NzaC1lZDI1NTE5AAAAIGtleWtleWtleWtleWtleWtleWtleWtleWtleWtleWtl",
        "b3BlbnNzaC1rZXktdjEA",
    ] {
        runs(term);
    }
}

/// The body's shape hides it in a file too, with no armor around it, and
/// only the lines that are base64 alone.
#[test]
fn an_openssh_body_with_no_armor_is_hidden_in_any_text() {
    let next = "WzHvuy3fEWJr5musirUr5JYe5CMpm8DI2mKvqGsxgG5oDTudHZGh0Z7tcHL5kVeqJy";
    let text = format!(
        "notes\n{OPENSSH_BODY}\n{next}\nlongIdentifierNameOfSomething = 1\nlogo: iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJ\n"
    );
    for path in [None, Some("notes.txt"), Some("src/a.ts")] {
        let result = detect(&text, path);
        assert_eq!(
            result.sanitized,
            "notes\n[REDACTED-OPENSSHPRIVATEKEY]\n\nlongIdentifierNameOfSomething = 1\nlogo: iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJ\n",
            "{path:?}"
        );
    }
}

/// A request string that is evidence keeps the text around a named secret.
#[test]
fn a_named_secret_is_hidden_alone_in_its_sentence() {
    let placeholder = "[REDACTED-CREDENTIALNAMEDVALUE]";
    for (text, hidden) in [
        (
            format!("Does the text contain password: {PASSWORD}?"),
            format!("Does the text contain password: {placeholder}?"),
        ),
        (
            format!("find password={PASSWORD} in config, then token: \"{PASSWORD}\"."),
            format!("find password={placeholder} in config, then token: \"{placeholder}\"."),
        ),
    ] {
        assert_eq!(hide_named_values(&text).as_deref(), Some(hidden.as_str()));
    }
    for plain in [
        "locate password: str",
        "token: Token and api_key: str = None",
    ] {
        assert_eq!(hide_named_values(plain), None, "{plain:?}");
    }
}

/// A token most of whose characters are written is that token, however a
/// regex spells the rest; a pattern that writes a prefix and leaves the value
/// to a class writes no token.
#[test]
fn a_secret_with_a_wildcard_tail_is_refused() {
    let most = &GITHUB[..GITHUB.len() - 2];
    let three_quarters = &GITHUB[..30];
    for term in [
        format!("{most}\\w\\w"),
        format!("{}.", &GITHUB[..GITHUB.len() - 1]),
        format!("{most}.."),
        format!("{most}.{{2}}"),
        format!("{most}[A-Za-z0-9]{{2}}"),
        format!("{most}\\w+"),
        format!("{most}\\w*"),
        format!("{most}[A-Za-z0-9]{{1,4}}"),
        format!("{most}\\S{{2,}}"),
        format!("find {most}\\w\\w in the logs"),
        format!("\\b{most}\\w\\w\\b"),
        format!("{three_quarters}.{{10}}"),
        format!("{three_quarters}\\w+"),
        format!("{}\\w{}", &GITHUB[..20], &GITHUB[21..]),
        format!("password:\\s*{}\\S\\S", &PASSWORD[..PASSWORD.len() - 2]),
        "postgres://admin:Tr0ub4dor3xK9z\\w@db.internal:5432/app".to_owned(),
    ] {
        refused(&term);
    }
}

#[test]
fn a_pattern_that_writes_little_of_a_secret_runs() {
    let under = &GITHUB[..29];
    for term in [
        "ghp_.{36}".to_owned(),
        "ghp_[A-Za-z0-9]{36}".to_owned(),
        "ghp_\\w+".to_owned(),
        "ghp_\\w{36}\\b".to_owned(),
        "gh[pousr]_[A-Za-z0-9]{36,}".to_owned(),
        "password:\\s*str".to_owned(),
        "password:\\s*\\S+".to_owned(),
        "password\\s*[:=]\\s*[\"'][^\"']{8,}[\"']".to_owned(),
        "token\\s*[:=]".to_owned(),
        "sk-[A-Za-z0-9]{20,}".to_owned(),
        "sk-ant-api03-[A-Za-z0-9_-]{93}AA".to_owned(),
        "AKIA[0-9A-Z]{16}".to_owned(),
        "xox[baprs]-[0-9]{10,13}-[0-9]{10,13}-[a-zA-Z0-9]{24}".to_owned(),
        "https://hooks\\.slack\\.com/services/T[A-Z0-9]{8}/B[A-Z0-9]{8}/[a-zA-Z0-9]{24}".to_owned(),
        "eyJ[A-Za-z0-9_-]{10,}\\.eyJ[A-Za-z0-9_-]{10,}\\.[A-Za-z0-9_-]+".to_owned(),
        "-----BEGIN [A-Z ]*PRIVATE KEY-----".to_owned(),
        "postgres://\\w+:\\w+@".to_owned(),
        "postgres://user:pass.@localhost".to_owned(),
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b".to_owned(),
        "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}".to_owned(),
        "123e4567-e89b-12d3-a456-426614174000".to_owned(),
        "\\$\\{[A-Z_]+\\}".to_owned(),
        "${GITHUB_TOKEN}".to_owned(),
        "src/security/.*/token_store\\.rs".to_owned(),
        "packages/octocode-native/crates/**/*.rs".to_owned(),
        "fn\\s+\\w+\\(token: &str\\)".to_owned(),
        "TODO|FIXME|XXX+".to_owned(),
        "[0-9a-f]{40}".to_owned(),
        "Bearer [A-Za-z0-9._~+/-]+=*".to_owned(),
        "Is the api_key optional? Does it default to None?".to_owned(),
        "What does foo.bar(token) return... and why?".to_owned(),
        format!("{under}.{{11}}"),
    ] {
        runs(&term);
    }
}

/// A sentence states a name's value with no operator; prose after the same
/// words is no value.
#[test]
fn a_secret_a_sentence_states_is_refused_and_prose_runs() {
    for term in [
        format!("the password is {PASSWORD}"),
        format!("The db_password is \"{PASSWORD}\"."),
        format!("check whether the API token IS {PASSWORD}, then stop"),
    ] {
        refused(&term);
    }
    for term in [
        "the password is required",
        "the token is expired",
        "the secret is stored in vault",
        "the password is password",
        "the password is hunter2",
        "the api key is optional",
        "the token is refreshed by refreshAccessToken()",
        "the secret is AWS_SECRET_ACCESS_KEY",
        "the token is process.env.GITHUB_TOKEN",
        "the key is computeCacheKeyForRequest(request)",
        "the token is dispatchToken",
        "the token is `SessionToken`",
        "the apiKey is undefined",
        "the password is <your-password>",
        "the password is ${DB_PASSWORD}",
        "the token is base64-encoded",
        "the key is user_id:session_id",
        "the token is JSON.parse(raw).access_token",
        "the cache key is sha256(path)",
        "the primary key is auto-incremented",
        "the token is 3 characters long",
        "the secret is in ~/.config/app/credentials.json",
        "the token is ghSearchCode",
        "whether the password is None",
        "the key is X-Api-Key",
        "the public key is ssh-ed25519",
        "why the token is HTTP-only",
        "the password is not logged",
    ] {
        runs(term);
    }
}

/// A value a config line's rule hides under a credential's name, at any
/// length, is refused in a request: the request and the file judge it
/// alike. A request clears only what names a type or a field (`password:
/// str`), a word of prose, or a placeholder.
#[test]
fn a_short_named_secret_is_refused_as_a_file_hides_it() {
    let short = "Hunter2Xq9vB7mK";
    let token = "Zq8Wv3Xr7Tn2Lp5";
    for (term, value) in [
        (format!("password: {short}"), short),
        (format!("password = {short}"), short),
        (format!("token: {token}"), token),
        (format!("api_key: {token}Ab1Cd9"), "Zq8Wv3Xr7Tn2Lp5Ab1Cd9"),
    ] {
        refused(&term);
        let file = detect(&term, None).sanitized;
        assert!(!file.contains(value), "a file shows {term:?}: {file:?}");
    }
    for term in [
        format!("the password is {short}"),
        format!("locate token: {token} in the client"),
        format!("Does the config hold password={short}?"),
    ] {
        refused(&term);
    }
    for term in [
        "password: str",
        "token: Token",
        "api_key: str = None",
        "dispatchToken: string",
        "the password is required",
        "the token is expired",
        "postgres://user:password@localhost",
        "password: hunter2",
        "token: v1alpha1",
        "key: utf8mb4",
        "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
    ] {
        runs(term);
    }
}

/// Under a `Sec-` field only the nonce the user agent fills runs, as a file
/// shows only that one.
#[test]
fn a_sec_field_with_no_nonce_is_refused() {
    refused("Sec-WebSocket-Key: Zq8Wv3Xr7Tn2Lp5Ab1Cd9Ef4Gh6Jk8Lm0Np2Qr4St9Uv");
    runs("Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==");
}

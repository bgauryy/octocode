//! Where a value starts and ends, and which values a request and a file
//! judge alike: a literal ends at its closing quote, an expression's
//! literals are each a value, a placed value's placeholder is none, and a
//! plain scalar ends with its token. Each rule hides the secret and keeps
//! the text before and after it, runs its look-alike, and a request refuses
//! what a file hides.

use super::*;

/// A generated key of a literal's full length and more.
const KEY: &str = "Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4";
/// A generated password with symbols.
const PASSWORD: &str = "Tr0ub4dor&3xK9!zQ";
/// A GitHub token's shape.
const GITHUB: &str = "ghp_x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";

/// That `content` loses each of `secrets` and nothing else in its file and
/// in the pathless net, and that a request holding it is refused.
fn hidden_alone(content: &str, path: &str, secrets: &[&str]) {
    for path in [Some(path), None] {
        assert_redacts_each(content, path, secrets);
    }
    assert!(holds_credential_as_request(content), "ran {content:?}");
}

/// That `content` stays as written in its file and in the pathless net, and
/// runs as a request.
fn shown_and_run(content: &str, path: &str) {
    for path in [Some(path), None] {
        assert_eq!(detect(content, path).sanitized, content, "{path:?}");
    }
    assert!(!holds_credential_as_request(content), "refused {content:?}");
}

/// D5: a literal in the expression a credential's name is assigned (a
/// ternary's arm, a fallback's operand, a template's text) is judged as the
/// value the name holds: an opaque one goes, alone, and a readable one, the
/// operators and the code around them stay.
#[test]
fn each_literal_of_an_assigned_expression_is_judged_as_the_named_value() {
    for (content, path) in [
        (
            format!("const apiKey = cond ? \"{KEY}\" : \"otherVisible3\";\n"),
            "src/a.ts",
        ),
        (
            format!("const apiKey = cond ? \"otherVisible3\" : \"{KEY}\";\n"),
            "src/a.ts",
        ),
        (
            format!("const apiKey = isProductionEnvironment ? \"{KEY}\" : \"otherVisible3\";\n"),
            "src/a.ts",
        ),
        (
            format!("const authToken = process.env.AUTH_TOKEN || \"{KEY}\";\n"),
            "src/a.ts",
        ),
        (
            format!("const secretKey = process.env.X ?? \"{KEY}\";\n"),
            "src/a.ts",
        ),
        (
            format!("api_key = \"otherVisible3\" if dev else \"{KEY}\"\n"),
            "src/a.py",
        ),
        (format!("const apiKey2 = `{KEY}${{suffix}}`;\n"), "src/a.ts"),
    ] {
        hidden_alone(&content, path, &[KEY]);
    }
    for (content, path) in [
        (
            "const apiKey = cond ? \"otherVisible3\" : \"fallbackValue\";\n",
            "src/a.ts",
        ),
        (
            "const token = cond ? readToken() : refreshToken();\n",
            "src/a.ts",
        ),
        ("const apiKey = `key-${id}`;\n", "src/a.ts"),
    ] {
        shown_and_run(content, path);
    }
}

/// D4: a literal's value ends at its closing quote. A piece joined after a
/// credential (`+ "tail"`, `+ name`) is the program's text and stays; two
/// pieces each a credential alone are each hidden; pieces no one of which is
/// a credential alone are judged joined, as one value.
#[test]
fn a_literal_value_ends_at_its_closing_quote() {
    for (content, path) in [
        (
            format!("const apiKey = \"{KEY}\" + \"tail-visible1\";\n"),
            "src/a.ts",
        ),
        (
            format!("const token = process.env.X || \"{KEY}\" + \"visible2\";\n"),
            "src/a.ts",
        ),
        (
            format!("api_key = \"{KEY}\" + suffix_visible5\n"),
            "src/a.py",
        ),
    ] {
        hidden_alone(&content, path, &[KEY]);
    }
    let (head, tail) = KEY.split_at(20);
    hidden_alone(
        &format!("const apiKey = \"{head}\" + \"{tail}\";\n"),
        "src/a.ts",
        &[head, tail],
    );
    let joined = detect(
        "const password = \"Fake0\" + \"Moo9Zed\";\n",
        Some("src/a.ts"),
    );
    assert!(
        !joined.sanitized.contains("Fake0") && !joined.sanitized.contains("Moo9Zed"),
        "{:?}",
        joined.sanitized
    );
}

/// D1: the value a pattern finds where a credential stands (the token after
/// `Bearer`, a URL's password) is judged once, the same in a request and in
/// a file: any value is a credential, letters alone too, and a placeholder a
/// document writes in its place is none.
#[test]
fn a_placed_value_is_judged_alike_in_a_request_and_a_file() {
    for (content, secret) in [
        (
            "Authorization: Bearer abcdefghijklmnopqrstuvwx\n",
            "abcdefghijklmnopqrstuvwx",
        ),
        ("mysql://root:hunterhunter@db/app\n", "hunterhunter"),
    ] {
        hidden_alone(content, "docs/api.txt", &[secret]);
    }
    for content in [
        "Authorization: Bearer YOUR_ACCESS_TOKEN_HERE\n",
        "Authorization: Bearer <token>\n",
        "Authorization: Bearer ${TOKEN}\n",
        "postgres://user:password@localhost\n",
        "url = postgres://user:password@localhost:5432/db\n",
        "https://user:pass@example.com\n",
    ] {
        shown_and_run(content, "docs/api.txt");
    }
}

/// D6: prose states a credential's value after `is`, `was`, `is:` or a bare
/// `:` or `=` after the credential's noun: an opaque value goes, the words
/// around it stay, and prose that states no value stays and runs.
#[test]
fn prose_states_a_value_in_its_common_forms() {
    for (content, path) in [
        (format!("// my password was {PASSWORD}\n"), "src/a.ts"),
        (format!("// the password is: {PASSWORD}\n"), "src/a.ts"),
        (
            format!("my password was {PASSWORD}, then I changed it\n"),
            "notes.txt",
        ),
        (format!("the password is: {PASSWORD}\n"), "notes.txt"),
        (format!("// the password = {PASSWORD}\n"), "src/a.ts"),
    ] {
        hidden_alone(&content, path, &[PASSWORD]);
    }
    for (content, path) in [
        ("// the password is required\n", "src/a.ts"),
        ("// the token was expired\n", "src/a.ts"),
        ("// my password was changed\n", "src/a.ts"),
        ("the password is: required\n", "notes.txt"),
        ("my password was changed yesterday\n", "notes.txt"),
    ] {
        shown_and_run(content, path);
    }
}

/// D7: a plain scalar whose first word is one opaque token ends with that
/// token: the text after a blank or a `;` is the line's own.
#[test]
fn a_plain_scalar_ends_with_its_token() {
    for (content, secret) in [
        (format!("Sec-Fake-Api-Key: {GITHUB}  tailJ\n"), GITHUB),
        (format!("Sec-Fake-Api-Key: {GITHUB}; visible10\n"), GITHUB),
        (format!("X-Custom: {GITHUB} tailL\n"), GITHUB),
        (format!("api_key: {KEY}  # rotated\n"), KEY),
    ] {
        hidden_alone(&content, "notes.txt", &[secret]);
    }
    let words = detect("password: correct horse 9\n", Some("config.yaml"));
    assert_eq!(
        words.sanitized, "password: [REDACTED-YAMLSECRETVALUE]\n",
        "a scalar of words is one value"
    );
}

/// D8: a type annotation and a documented placeholder after a credential's
/// name are no value, in a text file as in a request; the WebSocket nonce
/// under `Sec-WebSocket-Key` is the user agent's in a key/value form too.
#[test]
fn types_placeholders_and_the_handshake_nonce_are_no_credentials() {
    for content in [
        "password: str\n",
        "token: Token\n",
        "api_key: str = None\n",
        "dispatchToken: string\n",
        "session_token: Optional[str]\n",
        "secret: string | null\n",
    ] {
        shown_and_run(content, "notes.txt");
    }
    for (content, path) in [
        (
            "const headers = { \"Sec-WebSocket-Key\": \"dGhlIHNhbXBsZSBub25jZQ==\" };\n",
            "src/ws.ts",
        ),
        (
            "{\"Sec-WebSocket-Key\": \"dGhlIHNhbXBsZSBub25jZQ==\"}\n",
            "req.json",
        ),
    ] {
        shown_and_run(content, path);
    }
    // A value under the same names that is no type and no nonce stays hidden.
    for (content, secret) in [
        (format!("password: {PASSWORD}\n"), PASSWORD),
        ("token: Zq8Wv3Xr7Tn2Lp5\n".to_owned(), "Zq8Wv3Xr7Tn2Lp5"),
        (
            "{\"Sec-WebSocket-Key\": \"Zq8Wv3Xr7Tn2Lp5Ab1Cd9Ef4Gh6Jk8Lm0Np2Qr4St9Uv\"}\n"
                .to_owned(),
            "Zq8Wv3Xr7Tn2Lp5Ab1Cd9Ef4Gh6Jk8Lm0Np2Qr4St9Uv",
        ),
    ] {
        hidden_alone(&content, "notes.txt", &[secret]);
    }
}
